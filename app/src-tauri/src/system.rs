//! Desktop integration: notifications, the save sound, and autostart.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tauri::AppHandle;

use crate::settings::Sound;

/// Argument the autostart entry passes, so login launches can stay in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";

/// Show a desktop notification.
pub async fn notify(app: &AppHandle, summary: &str, body: &str) -> Result<()> {
    imp::notify(app, summary, body).await
}

/// What a sound is for; each has a built-in sound of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// A clip was saved, or an edit rendered.
    Clip,
    /// A screenshot was taken.
    Screenshot,
    /// The save that was just announced didn't work.
    Failed,
}

/// The built-in sounds, made by `scripts/make-sounds.mjs`: mono 16-bit PCM
/// WAV at 48 kHz, embedded so both platforms have them with nothing to
/// install. A two-note chime for clips (0.41 s), a small shutter for
/// screenshots (0.23 s), and the chime turned downwards for a failed save
/// (0.44 s), each with a short echo; all quiet enough to sit under a game.
const SAVE_SOUND: &[u8] = include_bytes!("../sounds/clip-saved.wav");
const SCREENSHOT_SOUND: &[u8] = include_bytes!("../sounds/screenshot.wav");
const FAILED_SOUND: &[u8] = include_bytes!("../sounds/save-failed.wav");

impl Cue {
    fn builtin(self) -> &'static [u8] {
        match self {
            Cue::Clip => SAVE_SOUND,
            Cue::Screenshot => SCREENSHOT_SOUND,
            Cue::Failed => FAILED_SOUND,
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Cue::Clip => "clip-saved.wav",
            Cue::Screenshot => "screenshot.wav",
            Cue::Failed => "save-failed.wav",
        }
    }
}

/// Play `sound` (for `Builtin`, the built-in sound of `cue`) at
/// `volume_percent` without blocking. A custom file that has gone missing
/// falls back to the built-in sound.
///
/// Plain 16-bit WAVs (every built-in sound, and such files of the user's)
/// go straight to the system's player from memory, which starts at once.
/// Anything else is played through GStreamer, which takes a moment to set
/// up but reads every common format.
pub fn play_sound(sound: &Sound, cue: Cue, volume_percent: u32) {
    let custom = match sound {
        Sound::Custom { path, .. } if path.is_file() => Some(path.clone()),
        Sound::Custom { path, .. } => {
            tracing::warn!(path = %path.display(), "custom sound is gone; using the built-in one");
            None
        }
        Sound::Builtin => None,
    };
    std::thread::spawn(move || {
        let quick = match &custom {
            Some(path) => std::fs::read(path)
                .ok()
                .and_then(|wav| scaled_pcm16(&wav, volume_percent)),
            None => scaled_pcm16(cue.builtin(), volume_percent),
        };
        if let Some(wav) = quick
            && imp::play_wav(wav)
        {
            return;
        }
        let path = custom.or_else(|| builtin_sound_file(cue));
        let result = match &path {
            Some(p) => clip_core::sound::play_blocking(p, volume_percent, None),
            None => Err(anyhow::anyhow!("can't write the built-in sound out")),
        };
        if let Err(e) = result {
            tracing::warn!("sound: {e:#}");
        }
    });
}

/// `wav` with every sample scaled to `volume_percent`, if it is a plain
/// 16-bit PCM RIFF WAVE (the only kind the system's player is given).
fn scaled_pcm16(wav: &[u8], volume_percent: u32) -> Option<Vec<u8>> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pcm16 = false;
    let mut at = 12;
    while at + 8 <= wav.len() {
        let id = &wav[at..at + 4];
        let len = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        let body = at + 8;
        let end = body.checked_add(len)?.min(wav.len());
        if id == b"fmt " && len >= 16 {
            let format = u16::from_le_bytes(wav[body..body + 2].try_into().ok()?);
            let bits = u16::from_le_bytes(wav[body + 14..body + 16].try_into().ok()?);
            pcm16 = format == 1 && bits == 16;
        } else if id == b"data" {
            if !pcm16 {
                return None;
            }
            let gain = f64::from(volume_percent.min(clip_core::sound::MAX_VOLUME_PERCENT)) / 100.0;
            let mut out = wav.to_vec();
            for sample in out[body..end].as_chunks_mut::<2>().0 {
                let v = f64::from(i16::from_le_bytes(*sample)) * gain;
                let v = v.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
                *sample = v.to_le_bytes();
            }
            return Some(out);
        }
        // Chunks are padded to an even length.
        at = body + len + (len & 1);
    }
    None
}

/// A built-in sound written out once per run, for GStreamer to read.
fn builtin_sound_file(cue: Cue) -> Option<PathBuf> {
    let path = dirs::cache_dir()?.join("reruns").join(cue.file_name());
    let current = std::fs::read(&path).is_ok_and(|b| b == cue.builtin());
    if !current {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, cue.builtin()).ok()?;
    }
    Some(path)
}

/// Where imported sounds are kept: the config folder, so they move with
/// the settings and survive the original file being moved or deleted.
pub fn sounds_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reruns/sounds")
}

/// Check `source` is a short sound and copy it in as `<slot>.<ext>`,
/// replacing an earlier import for the same slot. Returns the copy.
pub fn import_sound(source: &Path, slot: &str) -> Result<PathBuf> {
    import_sound_into(&sounds_dir(), source, slot)
}

fn import_sound_into(dir: &Path, source: &Path, slot: &str) -> Result<PathBuf> {
    clip_core::sound::validate(source)?;
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "audio".into());
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    // A new file name per import, so a sound playing from the old copy is
    // never overwritten underneath it; older imports for the slot go.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let dest = dir.join(format!("{slot}-{stamp}.{ext}"));
    std::fs::copy(source, &dest).with_context(|| format!("copying {}", source.display()))?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            let ours = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{slot}-")));
            if ours && p != dest {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(dest)
}

/// Add or remove the login entry for this executable.
pub fn set_autostart(enabled: bool) -> Result<()> {
    imp::set_autostart(enabled)
}

#[allow(dead_code)]
fn autostart_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart/reruns.desktop")
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;

    use anyhow::{Context, Result};
    use tauri::AppHandle;
    use zbus::zvariant::Value;

    use super::{AUTOSTART_ARG, autostart_file};

    /// GStreamer is the only player on Linux.
    pub fn play_wav(_wav: Vec<u8>) -> bool {
        false
    }

    /// Show a desktop notification (org.freedesktop.Notifications).
    pub async fn notify(_app: &AppHandle, summary: &str, body: &str) -> Result<()> {
        let conn = zbus::Connection::session().await?;
        let mut hints: HashMap<&str, Value> = HashMap::new();
        // We play our own sound; don't double up with the server's.
        hints.insert("suppress-sound", Value::Bool(true));
        hints.insert("desktop-entry", Value::Str("reruns".into()));
        conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Reruns",
                0u32,
                "media-record",
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                4000i32,
            ),
        )
        .await
        .context("notification service unavailable")?;
        Ok(())
    }

    /// Add or remove the XDG autostart entry for this executable.
    pub fn set_autostart(enabled: bool) -> Result<()> {
        let file = autostart_file();
        if !enabled {
            if file.exists() {
                std::fs::remove_file(&file)?;
            }
            return Ok(());
        }
        let exe = std::env::current_exe()?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            &file,
            format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=Reruns\n\
                 Comment=Instant replay game clipping\n\
                 Exec=\"{}\" {AUTOSTART_ARG}\n\
                 Icon=media-record\n\
                 X-GNOME-Autostart-enabled=true\n",
                exe.display()
            ),
        )
        .with_context(|| format!("writing {}", file.display()))
    }
}

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use tauri::AppHandle;
    use tauri_plugin_notification::NotificationExt;

    use super::AUTOSTART_ARG;

    /// Play a whole RIFF WAVE image through the system's own player and
    /// return at once. False if Windows wouldn't take it.
    pub fn play_wav(wav: Vec<u8>) -> bool {
        use std::sync::Mutex;
        use windows::{
            Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
            core::PCWSTR,
        };
        /// The sound being played: Windows reads it from our memory until
        /// it ends or another one starts.
        static PLAYING: Mutex<Vec<u8>> = Mutex::new(Vec::new());
        let mut playing = PLAYING.lock().unwrap();
        // SAFETY: with SND_MEMORY the "name" is a pointer to the image.
        // Starting a sound stops the one before it, so the old image is
        // only dropped (below) once Windows has let go of it, and the new
        // one lives in PLAYING until it is replaced the same way.
        let started = unsafe {
            PlaySoundW(
                PCWSTR(wav.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            )
            .as_bool()
        };
        if started {
            *playing = wav;
        }
        started
    }

    pub async fn notify(app: &AppHandle, summary: &str, body: &str) -> Result<()> {
        app.notification()
            .builder()
            .title(summary)
            .body(body)
            .show()
            .context("showing notification")
    }

    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, the per-user
    /// login entry that needs no admin rights.
    pub fn set_autostart(enabled: bool) -> Result<()> {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .context("opening the Run key")?;
        if enabled {
            let exe = std::env::current_exe()?;
            key.set_value("Reruns", &format!("\"{}\" {AUTOSTART_ARG}", exe.display()))
                .context("writing the Run entry")?;
        } else if key.get_value::<String, _>("Reruns").is_ok() {
            key.delete_value("Reruns")
                .context("removing the Run entry")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Cue, SAVE_SOUND, scaled_pcm16};

    /// 16-bit samples of a WAV made by scripts/make-sounds.mjs (a 44-byte
    /// header, then the data).
    fn samples(wav: &[u8]) -> Vec<i16> {
        wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| i16::from_le_bytes(*s))
            .collect()
    }

    /// Plain 16-bit PCM plays with the fewest plugins on any platform, and
    /// is what the quick player takes. Short and quiet: under a game.
    #[test]
    fn built_in_sounds_are_short_quiet_pcm_wavs() {
        for cue in [Cue::Clip, Cue::Screenshot, Cue::Failed] {
            let wav = cue.builtin();
            assert_eq!(&wav[0..4], b"RIFF", "{cue:?}");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(&wav[12..16], b"fmt ");
            assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "PCM");
            assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
            let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
            let riff_len = u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize;
            assert_eq!(riff_len + 8, wav.len());
            let samples = samples(wav);
            let seconds = samples.len() as f64 / f64::from(rate);
            assert!((0.1..=0.5).contains(&seconds), "{cue:?}: {seconds} s");
            let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
            // Between -16 and -7 dBFS.
            assert!((5_190..=14_640).contains(&peak), "{cue:?}: peak {peak}");
            // Ends in silence: no click when it stops.
            assert!(samples[samples.len() - 1].unsigned_abs() < 40, "{cue:?}");
        }
    }

    #[test]
    fn volume_scales_the_samples_and_nothing_else() {
        let half = scaled_pcm16(SAVE_SOUND, 50).unwrap();
        assert_eq!(half[..44], SAVE_SOUND[..44]);
        for (a, b) in samples(SAVE_SOUND).iter().zip(samples(&half)) {
            assert!((i32::from(*a) - 2 * i32::from(b)).abs() <= 1);
        }
        // Louder than the file allows is held at the limit, not wrapped.
        let loud = scaled_pcm16(SAVE_SOUND, 200).unwrap();
        let (peak, loud_peak) = (
            samples(SAVE_SOUND)
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap(),
            samples(&loud)
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap(),
        );
        assert_eq!(loud_peak, (peak * 2).min(32_767));
    }

    /// The system's player takes the built-in sounds from memory and
    /// returns at once. Silent (volume 0), and only where there is a
    /// sound device: `cargo test -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "needs a sound device"]
    fn windows_plays_a_built_in_sound_without_waiting() {
        for cue in [Cue::Clip, Cue::Screenshot, Cue::Failed] {
            let wav = scaled_pcm16(cue.builtin(), 0).unwrap();
            let started = std::time::Instant::now();
            assert!(super::imp::play_wav(wav), "{cue:?}");
            assert!(started.elapsed() < std::time::Duration::from_millis(100));
        }
    }

    #[test]
    fn only_plain_16_bit_wavs_take_the_quick_path() {
        assert!(scaled_pcm16(b"ID3 not a wav at all", 100).is_none());
        // The same file marked as 32-bit float (format 3).
        let mut float = SAVE_SOUND.to_vec();
        float[20] = 3;
        assert!(scaled_pcm16(&float, 100).is_none());
        // And as 8-bit.
        let mut eight = SAVE_SOUND.to_vec();
        eight[34] = 8;
        assert!(scaled_pcm16(&eight, 100).is_none());
    }

    #[test]
    fn imports_replace_the_previous_one_for_the_slot() {
        clip_core::init().unwrap();
        let dir = std::env::temp_dir().join(format!("reruns-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("beep.WAV");
        std::fs::write(&src, SAVE_SOUND).unwrap();
        let sounds = dir.join("sounds");
        let import = |p: &std::path::Path, slot| super::import_sound_into(&sounds, p, slot);

        let first = import(&src, "clip").unwrap();
        // Imports in the same millisecond would share a name.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = import(&src, "clip").unwrap();
        let other = import(&src, "screenshot").unwrap();
        assert!(!first.exists(), "the earlier clip import is removed");
        assert!(second.exists() && other.exists());
        assert_eq!(second.extension().unwrap(), "wav");
        assert_eq!(std::fs::read(&second).unwrap(), SAVE_SOUND);

        let text = dir.join("notes.txt");
        std::fs::write(&text, "no").unwrap();
        assert!(import(&text, "clip").is_err());
        assert!(second.exists(), "a failed import keeps the current sound");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
