//! User settings, stored as JSON in `$XDG_CONFIG_HOME/reruns/settings.json`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clip_core::{
    Action, AudioSource, Binding, Codec, Encoder, RecorderConfig, VideoConfig, auto::Backend,
    game::IgnoredGame,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    pub launch_at_login: bool,
    /// When launched at login, stay in the tray instead of opening the window.
    pub start_in_tray: bool,
    /// Closing the window keeps Reruns running in the tray (otherwise quits).
    pub close_to_tray: bool,
    pub notify_on_save: bool,
    pub sound_on_save: bool,
    /// Loudness of the save sounds, in percent of the file's own level.
    pub sound_volume: u32,
    /// Played when a clip is saved or an edit renders.
    pub clip_sound: Sound,
    /// Played when a screenshot is taken. `None`: the built-in shutter,
    /// or the clip sound if that is a file of the user's (as it was
    /// before screenshots had a sound of their own).
    pub screenshot_sound: Option<Sound>,
    /// Windows: a popup over the game when recording starts and on a save.
    pub overlay: bool,
    /// Look for a new release now and then (builds that can update only).
    pub check_for_updates: bool,
    /// "Clipping <game>" on the user's Discord profile while recording.
    pub discord_presence: bool,

    // Recording
    pub replay_seconds: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: Codec,
    /// `None` picks the best working encoder for the codec.
    pub encoder: Option<Encoder>,
    pub cursor: bool,
    /// Capture straight into GPU memory when the driver allows; falls back
    /// to copying through system memory by itself. On by default on
    /// Windows (D3D11 is the native path), off on Linux.
    pub zero_copy: bool,
    /// Windows: tone-map HDR games through Desktop Duplication (captures
    /// the game window's area of the screen) when the display is in HDR.
    pub hdr_compatibility: bool,
    /// Windows, with HDR compatibility: capture the game window itself in
    /// HDR and tone-map it (experimental); falls back to the screen area.
    pub hdr_window_capture: bool,
    /// Apps marked "not a game"; never recorded.
    pub ignored_games: Vec<IgnoredGame>,

    // Audio
    pub desktop_audio: bool,
    /// Output whose sound is recorded (PipeWire node name / WASAPI id);
    /// `None` = the system default output, followed when it changes.
    pub desktop_device: Option<String>,
    pub mic: bool,
    /// PipeWire node name; `None` = system default input.
    pub mic_device: Option<String>,
    pub audio_bitrate_kbps: u32,

    /// Capture backend: `None` = KWin window capture when available, else the
    /// screen-cast portal. Applies on the next start of Reruns.
    pub capture_backend: Option<Backend>,

    /// Global "save clip" hotkey, e.g. `F8` or `Ctrl+Shift+S`.
    pub hotkey: String,
    /// Global "save screenshot" hotkey; empty leaves it unbound.
    pub screenshot_hotkey: String,
    /// Global "mark a moment" hotkey; empty leaves it unbound.
    pub marker_hotkey: String,
    pub clips_dir: PathBuf,
    /// Soft cap on the clips folder in GB; `None` = no limit.
    pub storage_limit_gb: Option<f64>,
    /// When enforcing the limit, never delete clips made in the trimmer.
    pub protect_edits: bool,
    /// When enforcing the limit, never delete starred clips.
    pub protect_favorites: bool,
    /// Deleted clips go to the desktop trash instead of being erased.
    pub use_trash: bool,
    /// Show the reserved ad slots. Not exposed in the UI; set it by hand to
    /// preview the layout.
    pub ads: bool,
}

/// A save sound: the one that ships with Reruns, or a file the user
/// imported (a copy in the config folder, see `system::import_sound`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Sound {
    #[default]
    Builtin,
    Custom {
        path: PathBuf,
        /// The picked file's name, for display (the copy has its own).
        #[serde(default)]
        name: String,
    },
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_in_tray: true,
            close_to_tray: true,
            notify_on_save: true,
            check_for_updates: true,
            discord_presence: true,
            sound_on_save: true,
            sound_volume: 100,
            clip_sound: Sound::Builtin,
            screenshot_sound: None,
            overlay: true,
            replay_seconds: 30,
            fps: 60,
            bitrate_kbps: 20_000,
            codec: Codec::H264,
            encoder: None,
            cursor: true,
            zero_copy: cfg!(windows),
            hdr_compatibility: false,
            hdr_window_capture: false,
            ignored_games: Vec::new(),
            desktop_audio: true,
            desktop_device: None,
            mic: false,
            mic_device: None,
            audio_bitrate_kbps: 160,
            capture_backend: None,
            hotkey: "F8".into(),
            screenshot_hotkey: "F9".into(),
            marker_hotkey: "F7".into(),
            clips_dir: clip_core::default_clips_dir(),
            storage_limit_gb: None,
            protect_edits: true,
            protect_favorites: true,
            use_trash: true,
            ads: false,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("reruns/settings.json")
    }

    /// Load settings, falling back to defaults if the file is missing.
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text, &path),
            Err(_) => Self::default(),
        }
    }

    /// Read `text` (the contents of `path`) leniently: a field that doesn't
    /// parse (a value another version wrote, a hand edit) falls back to its
    /// default and every other field is kept, rather than all of them
    /// going back to defaults and the next save overwriting the file. Text
    /// that isn't a JSON object at all is copied aside to
    /// `settings.json.bad` first, so it can still be recovered by hand.
    fn parse(text: &str, path: &Path) -> Self {
        let file = match serde_json::from_str::<Map<String, Value>>(text) {
            Ok(file) => file,
            Err(e) => {
                let bad = path.with_extension("json.bad");
                tracing::warn!(
                    "{} is unreadable ({e}); using defaults, the old file is kept as {}",
                    path.display(),
                    bad.display()
                );
                let _ = std::fs::copy(path, &bad);
                return Self::default();
            }
        };
        if let Ok(settings) = serde_json::from_value(Value::Object(file.clone())) {
            return settings;
        }
        let Ok(Value::Object(mut merged)) = serde_json::to_value(Self::default()) else {
            return Self::default();
        };
        // Field by field over the defaults; one that breaks parsing is
        // dropped again.
        for (key, value) in file {
            let previous = merged.insert(key.clone(), value);
            if serde_json::from_value::<Self>(Value::Object(merged.clone())).is_err() {
                tracing::warn!("settings: ignoring an invalid {key:?}; using its default");
                match previous {
                    Some(p) => merged.insert(key, p),
                    None => merged.remove(&key),
                };
            }
        }
        serde_json::from_value(Value::Object(merged)).unwrap_or_default()
    }

    /// Write the settings atomically: to a temporary file next to them,
    /// then moved over the old ones, so a crash or power cut mid-write
    /// can't leave a truncated file behind.
    pub fn save(&self) -> Result<()> {
        save_to(self, &Self::path())
    }

    pub fn recorder_config(&self) -> RecorderConfig {
        let mut audio = Vec::new();
        if self.desktop_audio {
            audio.push(AudioSource::Desktop(self.desktop_device.clone()));
        }
        if self.mic {
            audio.push(AudioSource::Mic(self.mic_device.clone()));
        }
        RecorderConfig {
            video: self.video_config(),
            encoder: self.encoder,
            replay_seconds: self.replay_seconds.clamp(5, 600),
            audio,
            audio_bitrate: (self.audio_bitrate_kbps.clamp(64, 320) * 1000) as i32,
            cursor: self.cursor,
            zero_copy: self.zero_copy,
            hdr_compat: self.hdr_compatibility,
            hdr_window: self.hdr_window_capture,
            ignored: self.ignored_games.clone(),
        }
    }

    /// The sound for a screenshot: the one picked for it, else the
    /// user's own clip sound, else the built-in one (for
    /// `system::Cue::Screenshot`, the shutter).
    pub fn screenshot_sound(&self) -> &Sound {
        self.screenshot_sound.as_ref().unwrap_or(&self.clip_sound)
    }

    /// Every global hotkey, bound or not.
    pub fn hotkey_bindings(&self) -> Vec<Binding> {
        vec![
            Binding::new(Action::SaveClip, self.hotkey.clone()),
            Binding::new(Action::Screenshot, self.screenshot_hotkey.clone()),
            Binding::new(Action::Marker, self.marker_hotkey.clone()),
        ]
    }

    pub fn storage_policy(&self) -> crate::storage::Policy {
        crate::storage::Policy {
            limit_bytes: self.storage_limit_gb.map(crate::storage::gb),
            protect_edits: self.protect_edits,
            protect_favorites: self.protect_favorites,
            use_trash: self.use_trash,
        }
    }

    pub fn video_config(&self) -> VideoConfig {
        VideoConfig {
            fps: self.fps.clamp(24, 480),
            bitrate_kbps: self.bitrate_kbps.clamp(2_000, 100_000),
            codec: self.codec,
        }
    }
}

fn save_to(settings: &Settings, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    clip_core::save::write_atomic(path, serde_json::to_string_pretty(settings)?.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("reruns-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_round_trip() {
        let text = serde_json::to_string(&Settings::default()).unwrap();
        let back = Settings::parse(&text, Path::new("unused.json"));
        assert_eq!(
            serde_json::to_value(back).unwrap(),
            serde_json::to_value(Settings::default()).unwrap()
        );
    }

    #[test]
    fn a_bad_field_keeps_the_others() {
        let text = r#"{
            "hotkey": "F10",
            "fps": "sixty",
            "codec": "h999",
            "replay_seconds": 120,
            "clips_dir": "/clips",
            "storage_limit_gb": 50.0,
            "some_future_setting": true
        }"#;
        let s = Settings::parse(text, Path::new("unused.json"));
        assert_eq!(s.hotkey, "F10");
        assert_eq!(s.replay_seconds, 120);
        assert_eq!(s.clips_dir, PathBuf::from("/clips"));
        assert_eq!(s.storage_limit_gb, Some(50.0));
        // The broken ones fall back to their defaults.
        assert_eq!(s.fps, Settings::default().fps);
        assert_eq!(s.codec, Settings::default().codec);
    }

    #[test]
    fn unreadable_file_is_kept_aside() {
        let dir = temp_dir("bad");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{\"hotkey\": \"F1").unwrap();
        let s = Settings::parse(&std::fs::read_to_string(&path).unwrap(), &path);
        assert_eq!(s.hotkey, Settings::default().hotkey);
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.bad")).unwrap(),
            "{\"hotkey\": \"F1"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saves_replace_the_file_whole() {
        let dir = temp_dir("save");
        let path = dir.join("settings.json");
        std::fs::write(&path, "old").unwrap();
        let settings = Settings {
            hotkey: "F12".into(),
            ..Default::default()
        };
        save_to(&settings, &path).unwrap();
        let back = Settings::parse(&std::fs::read_to_string(&path).unwrap(), &path);
        assert_eq!(back.hotkey, "F12");
        // Only the settings file is left: no temporary files.
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
