//! The clip library: `<clips_dir>/<game>/<timestamp>.mp4`, with thumbnails,
//! sprite sheets and durations cached in `$XDG_CACHE_HOME/reruns/thumbs/`.
//! Reading the library never decodes video; `generate` does that, from the
//! thumbnail pool.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    pub path: PathBuf,
    pub game: String,
    pub name: String,
    /// Unix time in milliseconds.
    pub created_ms: u64,
    pub size_bytes: u64,
    pub duration: Option<f64>,
    pub audio_tracks: usize,
    pub is_edit: bool,
    pub steam_app_id: Option<u32>,
    pub thumbnail: Option<PathBuf>,
    /// One row of evenly spaced frames for hover scrubbing.
    pub sprite: Option<PathBuf>,
    pub sprite_frames: usize,
    /// User-given name; the UI falls back to the game or time.
    pub title: Option<String>,
    pub favorite: bool,
    pub labels: Vec<String>,
    /// Moments marked while recording, in seconds from the start.
    pub markers: Vec<f64>,
    /// The first audio track is game and mic mixed (what players play);
    /// the sources' own tracks follow it, and are what the editor's levels
    /// set.
    pub mixed_audio: bool,
}

/// Saved next to each clip as `<clip>.mp4.json`. The folder stays the
/// source of truth, so anything the user adds to a clip lives here too.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClipMeta {
    pub game: String,
    pub steam_app_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Moments marked with the marker hotkey, in seconds from the start.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<f64>,
    /// The first audio track is every source mixed into one; the sources'
    /// own tracks follow (recordings with game audio and a mic).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mixed_audio: bool,
}

/// What the UI may change on a clip; `None` leaves a field alone.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaPatch {
    /// `Some("")` clears the title.
    pub title: Option<String>,
    pub favorite: Option<bool>,
    pub labels: Option<Vec<String>>,
}

const MAX_TITLE: usize = 120;
const MAX_LABEL: usize = 40;
const MAX_LABELS: usize = 20;

impl ClipMeta {
    /// Apply a patch, tidying free text: trimmed, length-capped, labels
    /// de-duplicated case-insensitively and kept in the order given.
    pub fn apply(&mut self, patch: MetaPatch) {
        if let Some(t) = patch.title {
            let t: String = t.trim().chars().take(MAX_TITLE).collect();
            self.title = (!t.is_empty()).then_some(t);
        }
        if let Some(f) = patch.favorite {
            self.favorite = f;
        }
        if let Some(labels) = patch.labels {
            let mut out: Vec<String> = Vec::new();
            for l in labels {
                let l: String = l.trim().chars().take(MAX_LABEL).collect();
                if l.is_empty() || out.iter().any(|o| o.eq_ignore_ascii_case(&l)) {
                    continue;
                }
                out.push(l);
                if out.len() == MAX_LABELS {
                    break;
                }
            }
            self.labels = out;
        }
    }
}

fn meta_path(clip: &Path) -> PathBuf {
    let mut p = clip.as_os_str().to_owned();
    p.push(".json");
    PathBuf::from(p)
}

/// Whole or not at all (a temporary file moved over the old one): the
/// storage limit reads `favorite` from here and must never catch it
/// half-written.
pub fn write_meta(clip: &Path, meta: &ClipMeta) -> Result<()> {
    clip_core::save::write_atomic(&meta_path(clip), serde_json::to_string(meta)?.as_bytes())?;
    Ok(())
}

/// Whether a clip is starred: `Some(false)` when it has no sidecar, `None`
/// when it has one that can't be read (in use, damaged), so a caller about
/// to delete it can play safe.
pub fn is_favorite(clip: &Path) -> Option<bool> {
    match std::fs::read_to_string(meta_path(clip)) {
        Ok(text) => serde_json::from_str::<ClipMeta>(&text)
            .ok()
            .map(|m| m.favorite),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

pub fn read_meta(clip: &Path) -> Option<ClipMeta> {
    serde_json::from_str(&std::fs::read_to_string(meta_path(clip)).ok()?).ok()
}

/// Change a clip's user metadata, refusing anything outside the library.
/// Returns the clip as the UI should now show it.
pub fn update_meta(clips_dir: &Path, path: &Path, patch: MetaPatch) -> Result<Clip> {
    let root = clips_dir.canonicalize().context("clips folder missing")?;
    let target = path.canonicalize().context("clip not found")?;
    if !target.starts_with(&root) || target.extension().is_none_or(|e| e != "mp4") {
        bail!("not a library clip: {}", path.display());
    }
    let mut meta = read_meta(&target).unwrap_or_else(|| ClipMeta {
        game: target
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..Default::default()
    });
    meta.apply(patch);
    write_meta(&target, &meta)?;
    let game = meta.game.clone();
    // Under the path the page knows it by: the canonical one can differ
    // (`\\?\C:\…` on Windows, a resolved symlink), and the page matches
    // clips by path.
    load(path, &game)
}

/// Give a rendered edit the same metadata as its source.
/// Give an edit of `from` covering `start..end` (seconds) the same
/// metadata, with the markers moved onto the edit's own timeline.
pub fn copy_meta(from: &Path, to: &Path, start: f64, end: f64) {
    if let Some(mut m) = read_meta(from) {
        m.markers = trim_markers(&m.markers, start, end);
        // An edit has one audio track, already mixed at the chosen levels.
        m.mixed_audio = false;
        let _ = write_meta(to, &m);
    }
}

/// The markers inside `start..=end`, relative to `start`.
fn trim_markers(markers: &[f64], start: f64, end: f64) -> Vec<f64> {
    markers
        .iter()
        .filter(|&&m| m >= start && m <= end)
        .map(|&m| m - start)
        .collect()
}

#[derive(Serialize, Deserialize)]
struct CachedInfo {
    duration: f64,
    audio_tracks: usize,
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("reruns/thumbs")
}

/// A clip file found on disk, without decoding it.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub game: String,
    pub size: u64,
    pub modified_ms: u64,
    pub is_edit: bool,
}

/// Every `<clips_dir>/<game>/*.mp4`, cheaply (metadata only).
pub fn scan(clips_dir: &Path) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let Ok(games) = std::fs::read_dir(clips_dir) else {
        return out;
    };
    for game in games.flatten() {
        let game_dir = game.path();
        if !game_dir.is_dir() {
            continue;
        }
        let game_name = game.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(&game_dir) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "mp4") {
                continue;
            }
            let Ok(meta) = f.metadata() else { continue };
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            out.push(FileEntry {
                is_edit: is_edit(&path),
                path,
                game: game_name.clone(),
                size: meta.len(),
                modified_ms,
            });
        }
    }
    out
}

/// Clips rendered by the trimmer are named `<stem>_edit.mp4` / `_editN.mp4`.
pub fn is_edit(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    stem.rsplit_once("_edit")
        .is_some_and(|(_, n)| n.chars().all(|c| c.is_ascii_digit()))
}

/// All clips, newest first. Reads only the filesystem and the cache; call
/// it off the async runtime all the same (one stat and sidecar per clip).
pub fn list(clips_dir: &Path) -> Vec<Clip> {
    let mut clips: Vec<Clip> = scan(clips_dir)
        .into_iter()
        .filter_map(|f| match load(&f.path, &f.game) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!("skipping {}: {e:#}", f.path.display());
                None
            }
        })
        .collect();
    clips.sort_by_key(|c| std::cmp::Reverse(c.created_ms));
    clips
}

/// Bumped whenever the generated images change shape or format.
const THUMB_VERSION: u32 = 3;

/// Cache files for a clip, keyed by path + size + mtime (so a rewritten
/// file gets new images) and the generator version. The image extension
/// follows the encoder this GStreamer has (WebP, or JPEG on Windows).
struct CacheFiles {
    thumb: PathBuf,
    sprite: PathBuf,
    info: PathBuf,
}

fn cache_files(path: &Path, size: u64, created_ms: u64, ext: &str) -> CacheFiles {
    let mut h = DefaultHasher::new();
    // Bump THUMB_VERSION when thumbnail generation changes.
    (THUMB_VERSION, path, size, created_ms).hash(&mut h);
    let key = format!("{:016x}", h.finish());
    let dir = cache_dir();
    CacheFiles {
        thumb: dir.join(format!("{key}.{ext}")),
        sprite: dir.join(format!("{key}.strip.{ext}")),
        info: dir.join(format!("{key}.json")),
    }
}

/// Extension the images are written with on this machine (cached: it needs
/// GStreamer's registry).
fn image_ext() -> &'static str {
    static EXT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    EXT.get_or_init(|| clip_core::media::ImageFormat::available().extension())
}

fn file_times(path: &Path) -> Result<(u64, u64)> {
    let meta = std::fs::metadata(path)?;
    let created_ms = meta.modified()?.duration_since(UNIX_EPOCH)?.as_millis() as u64;
    Ok((meta.len(), created_ms))
}

/// Describe one clip file from the filesystem and the cache. Never decodes
/// video: `thumbnail`, `sprite` and `duration` are `None` until `generate`
/// has run for this file.
pub fn load(path: &Path, game: &str) -> Result<Clip> {
    let (size_bytes, created_ms) = file_times(path)?;
    let files = cache_files(path, size_bytes, created_ms, image_ext());
    let info = std::fs::read_to_string(&files.info)
        .ok()
        .and_then(|s| serde_json::from_str::<CachedInfo>(&s).ok());
    let has_images = files.thumb.exists() && files.sprite.exists();

    let user = read_meta(path).unwrap_or_default();
    Ok(Clip {
        path: path.to_owned(),
        game: game.to_owned(),
        name: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        created_ms,
        size_bytes,
        duration: info.as_ref().map(|c| c.duration),
        audio_tracks: info.as_ref().map(|c| c.audio_tracks).unwrap_or(0),
        is_edit: is_edit(path),
        steam_app_id: user.steam_app_id,
        thumbnail: has_images.then(|| files.thumb.clone()),
        sprite: has_images.then(|| files.sprite.clone()),
        sprite_frames: clip_core::media::SPRITE_FRAMES,
        title: user.title,
        favorite: user.favorite,
        labels: user.labels,
        markers: user.markers,
        mixed_audio: user.mixed_audio,
    })
}

/// Whether `load` would come back complete for this file.
pub fn is_cached(clip: &Clip) -> bool {
    clip.thumbnail.is_some() && clip.duration.is_some()
}

/// Decode the clip and write its thumbnail, sprite sheet and info to the
/// cache. Slow (a second or two per clip); run it from the thumbnail pool.
pub fn generate(path: &Path) -> Result<()> {
    let (size_bytes, created_ms) = file_times(path)?;
    let info = clip_core::media::inspect(path)?;
    let files = cache_files(path, size_bytes, created_ms, info.format.extension());
    std::fs::create_dir_all(cache_dir())?;
    std::fs::write(&files.thumb, &info.thumbnail)?;
    std::fs::write(&files.sprite, &info.sprite)?;
    let c = CachedInfo {
        duration: info.duration.seconds_f64(),
        audio_tracks: info.audio_tracks,
    };
    std::fs::write(&files.info, serde_json::to_string(&c)?)?;
    Ok(())
}

/// Where to render an edit of `source`: next to it when it's in the library,
/// otherwise in `<clips_dir>/Edits/`. Never overwrites.
pub fn edit_path(clips_dir: &Path, source: &Path) -> PathBuf {
    let in_library = matches!(
        (clips_dir.canonicalize(), source.canonicalize()),
        (Ok(root), Ok(src)) if src.starts_with(&root)
    );
    let dir = match source.parent() {
        Some(p) if in_library => p.to_owned(),
        _ => clips_dir.join("Edits"),
    };
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    let mut n = 1;
    loop {
        let suffix = if n == 1 {
            "edit".to_string()
        } else {
            format!("edit{n}")
        };
        let p = dir.join(format!("{stem}_{suffix}.mp4"));
        if !p.exists() && !clip_core::save::partial_path(&p).exists() {
            return p;
        }
        n += 1;
    }
}

/// Delete a clip (to the desktop trash if `use_trash`), refusing anything
/// outside the clips folder.
pub fn delete(clips_dir: &Path, path: &Path, use_trash: bool) -> Result<()> {
    let root = clips_dir.canonicalize().context("clips folder missing")?;
    let target = path.canonicalize().context("clip not found")?;
    if !target.starts_with(&root) || target.extension().is_none_or(|e| e != "mp4") {
        bail!("not a clip: {}", path.display());
    }
    let _ = std::fs::remove_file(meta_path(&target));
    if use_trash {
        trash::delete(&target).with_context(|| format!("moving {} to trash", target.display()))?;
    } else {
        std::fs::remove_file(&target)?;
    }
    // Remove the game folder once it's empty.
    if let Some(dir) = target.parent()
        && dir != root
    {
        let _ = std::fs::remove_dir(dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_tidies_user_fields() {
        let mut m = ClipMeta::default();
        m.apply(MetaPatch {
            title: Some("  Ace  ".into()),
            favorite: Some(true),
            labels: Some(vec![
                "clutch".into(),
                " Clutch ".into(),
                "".into(),
                "funny".into(),
            ]),
        });
        assert_eq!(m.title.as_deref(), Some("Ace"));
        assert!(m.favorite);
        assert_eq!(m.labels, ["clutch", "funny"]);

        // Untouched fields stay; an empty title clears it.
        m.apply(MetaPatch {
            title: Some("".into()),
            ..Default::default()
        });
        assert_eq!(m.title, None);
        assert!(m.favorite);
        assert_eq!(m.labels, ["clutch", "funny"]);
    }

    #[test]
    fn meta_round_trips_without_noise() {
        let m = ClipMeta {
            game: "Hades".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"game":"Hades","steam_app_id":null}"#);
        let back: ClipMeta = serde_json::from_str(r#"{"game":"Hades"}"#).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn markers_follow_the_trim() {
        assert_eq!(trim_markers(&[1.0, 4.5, 9.0, 12.0], 4.0, 10.0), [0.5, 5.0]);
        assert!(trim_markers(&[1.0], 2.0, 3.0).is_empty());

        let dir = std::env::temp_dir().join(format!("reruns-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, out) = (dir.join("a.mp4"), dir.join("a_edit.mp4"));
        let m = ClipMeta {
            game: "Hades".into(),
            markers: vec![3.0, 8.0],
            mixed_audio: true,
            ..Default::default()
        };
        write_meta(&src, &m).unwrap();
        copy_meta(&src, &out, 2.0, 6.0);
        let copied = read_meta(&out).unwrap();
        assert_eq!(copied.game, "Hades");
        assert_eq!(copied.markers, [1.0]);
        // The recording starts with game and mic mixed; an edit has one
        // track, already mixed.
        std::fs::write(&src, b"x").unwrap();
        clip_core::init().unwrap(); // `load` asks GStreamer for the image format
        assert!(load(&src, "Hades").unwrap().mixed_audio);
        assert!(!copied.mixed_audio);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recognises_edits() {
        assert!(is_edit(Path::new("a/2026-10-28_15-01-16_edit.mp4")));
        assert!(is_edit(Path::new("a/2026-10-28_15-01-16_edit3.mp4")));
        assert!(!is_edit(Path::new("a/2026-10-28_15-01-16.mp4")));
        assert!(!is_edit(Path::new("a/my_editing_session.mp4")));
    }
}
