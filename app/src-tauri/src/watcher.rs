//! Watch the clips folder so changes made outside the app (a file manager,
//! a sync tool, the CLI) reach the UI without polling.

use std::{path::Path, time::Duration};

use anyhow::Result;
use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{EventKind, RecommendedWatcher, RecursiveMode},
};
use tauri::{AppHandle, Emitter};

/// Emitted (with no payload of note) when clips or their sidecars change.
pub const LIBRARY_CHANGED: &str = "library-changed";

pub type Handle = Debouncer<RecommendedWatcher, RecommendedCache>;

/// Start watching `dir` recursively. Events are debounced so a save (file,
/// sidecar, folder) arrives as one notification. The handler runs on the
/// debouncer's own thread, never on the async runtime.
pub fn watch(app: AppHandle, dir: &Path) -> Result<Handle> {
    std::fs::create_dir_all(dir)?;
    let mut debouncer = new_debouncer(
        Duration::from_millis(300),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let relevant = events.iter().any(|e| {
                    !matches!(e.kind, EventKind::Access(_))
                        && e.paths.iter().any(|p| is_library_file(p))
                });
                if relevant {
                    let _ = app.emit(LIBRARY_CHANGED, "watcher");
                }
            }
            Err(errors) => {
                for e in errors {
                    tracing::debug!("watching clips folder: {e}");
                }
            }
        },
    )?;
    debouncer.watch(dir, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// Clips and their sidecars; also folders, which appear and vanish with
/// the first and last clip of a game.
fn is_library_file(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()) {
        Some("mp4" | "png") => true,
        Some("json") => p
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.ends_with(".mp4")),
        None => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_out_library_files() {
        assert!(is_library_file(Path::new("/c/Game/2026.mp4")));
        assert!(is_library_file(Path::new("/c/Game/2026.mp4.json")));
        assert!(is_library_file(Path::new("/c/Game/2026.png")));
        assert!(is_library_file(Path::new("/c/Game")));
        assert!(!is_library_file(Path::new("/c/Game/notes.txt")));
        assert!(!is_library_file(Path::new("/c/Game/other.json")));
    }
}
