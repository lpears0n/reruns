//! Mux a [`Clip`] snapshot into an MP4 file.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::{
    encoder::Codec,
    ring::{Clip, ClipTrack},
};

/// Where a file is written until it is complete: `<file>.part`, next to
/// it. With faststart the muxer writes nothing until the very end, so the
/// final name would otherwise sit there empty for the whole save or render,
/// and the library, folder watcher, thumbnailer and storage limit (which
/// all look for `.mp4`) would pick up a clip that isn't there yet.
pub fn partial_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

/// Finish a file written to [`partial_path`]`(path)`: on success move it
/// into place (replacing what was there), otherwise remove it. A finished
/// file that can't be moved is kept under its temporary name rather than
/// lost, and the error says where.
pub fn finish_partial(path: &Path, result: Result<()>) -> Result<()> {
    let partial = partial_path(path);
    if let Err(e) = result {
        discard(&partial);
        return Err(e);
    }
    rename_retrying(&partial, path).with_context(|| {
        format!(
            "moving {} into place (it is kept as {})",
            path.display(),
            partial.display()
        )
    })
}

/// Remove a file that won't be finished, waiting out a process that has
/// it open for now (see [`remove_retrying`]). One still held after that is
/// left behind, with a warning.
pub(crate) fn discard(path: &Path) {
    if let Err(e) = remove_retrying(path) {
        tracing::warn!(path = %path.display(), "couldn't remove an unfinished file: {e}");
    }
}

/// How long a rename or removal keeps trying while Windows reports the
/// file busy.
const BUSY_PATIENCE: Duration = Duration::from_secs(5);

/// `std::fs::rename`, replacing `to`. On Windows a file that was just
/// written is often held open for a moment by antivirus or the search
/// indexer, which fails a rename (not a write) with a sharing violation;
/// those are retried for a few seconds.
pub fn rename_retrying(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_busy(|| std::fs::rename(from, to))
}

/// `std::fs::remove_file`, retried like [`rename_retrying`]: deleting is
/// refused the same way while another process has the file open. Besides
/// antivirus, that includes any process started while GStreamer had the
/// file open: it opens files through the C runtime, whose handles are
/// inherited, so the child holds the file until it exits. A file that
/// isn't there counts as removed.
pub fn remove_retrying(path: &Path) -> std::io::Result<()> {
    match retry_busy(|| std::fs::remove_file(path)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Run `op` until it succeeds, fails for another reason, or the file has
/// been busy for [`BUSY_PATIENCE`].
fn retry_busy(mut op: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let deadline = Instant::now() + BUSY_PATIENCE;
    let mut wait = Duration::from_millis(10);
    loop {
        match op() {
            Err(e) if busy(&e) && Instant::now() < deadline => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(500));
            }
            result => return result,
        }
    }
}

/// A Windows error that means another process has the file open for now:
/// access denied, sharing violation, lock violation.
fn busy(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Write `contents` to `path` whole: to a temporary file next to it, then
/// moved over it. A reader never sees it half-written and a crash can't
/// leave it truncated.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Unique per write: two writes at once mustn't share a file.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(tmp);
    let result = std::fs::write(&tmp, contents).and_then(|()| rename_retrying(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write `clip` to `path` as MP4 (faststart). Blocks until the file is done;
/// `path` only appears once it is complete.
pub fn write_mp4(clip: &Clip, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }

    let pipeline = gst::Pipeline::new();
    let mux = gst::ElementFactory::make("mp4mux")
        .property("faststart", true)
        .build()?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", partial_path(path))
        .build()?;
    pipeline.add_many([&mux, &sink])?;
    mux.link(&sink)?;

    let codec = Codec::from_caps(&clip.video.caps).context("unknown video codec")?;
    let mut srcs = vec![add_track(&pipeline, &mux, &clip.video, codec.parser())?];
    for track in &clip.audio {
        srcs.push(add_track(&pipeline, &mux, track, "aacparse")?);
    }

    // The appsrcs are unbounded, so every packet can be queued up front;
    // the buffers are refcounted, not copied.
    let tracks = std::iter::once(&clip.video).chain(&clip.audio);
    for (src, track) in srcs.iter().zip(tracks) {
        for buf in &track.buffers {
            src.push_buffer(buf.clone())?;
        }
        src.end_of_stream()?;
    }

    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("pipeline has no bus")?;
    let result = match bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(60),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    ) {
        Some(msg) => match msg.view() {
            gst::MessageView::Eos(_) => Ok(()),
            gst::MessageView::Error(e) => Err(anyhow::anyhow!(
                "{} from {:?}: {:?}",
                e.error(),
                msg.src().map(|s| s.path_string()),
                e.debug()
            )),
            _ => unreachable!(),
        },
        None => Err(anyhow::anyhow!("timed out writing {}", path.display())),
    };
    // Stopped before the rename: the sink closes the file (Windows can't
    // move a file that is still open).
    let stopped = pipeline.set_state(gst::State::Null).map(drop);
    finish_partial(path, result.and(stopped.map_err(Into::into)))
}

fn add_track(
    pipeline: &gst::Pipeline,
    mux: &gst::Element,
    track: &ClipTrack,
    parser: &str,
) -> Result<gst_app::AppSrc> {
    if track.buffers.is_empty() {
        bail!("empty track");
    }
    let src = gst_app::AppSrc::builder()
        .caps(&track.caps)
        .format(gst::Format::Time)
        .max_bytes(0)
        .build();
    let parse = gst::ElementFactory::make(parser).build()?;
    pipeline.add_many([src.upcast_ref(), &parse])?;
    src.link(&parse)?;
    parse.link(mux)?;
    Ok(src)
}

/// `<root>/<folder>/<YYYY-MM-DD_HH-MM-SS>.mp4`, adding a suffix if taken.
pub fn clip_path(root: &Path, folder: &str) -> PathBuf {
    media_path(root, folder, "mp4")
}

/// `<root>/<folder>/<YYYY-MM-DD_HH-MM-SS>.png`, next to the folder's clips.
pub fn screenshot_path(root: &Path, folder: &str) -> PathBuf {
    media_path(root, folder, "png")
}

fn media_path(root: &Path, folder: &str, ext: &str) -> PathBuf {
    let dir = root.join(sanitize(folder));
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut path = dir.join(format!("{stamp}.{ext}"));
    let mut n = 2;
    while path.exists() || partial_path(&path).exists() {
        path = dir.join(format!("{stamp}_{n}.{ext}"));
        n += 1;
    }
    path
}

/// Default clips folder: `$XDG_VIDEOS_DIR/Reruns`.
pub fn default_clips_dir() -> PathBuf {
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Reruns")
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    let s = s.trim().trim_start_matches('.');
    if s.is_empty() {
        "Desktop".into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_replace_whole_and_leave_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("reruns-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["clip.mp4.json"], "no temporary files left");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sanitizes_folder_names() {
        assert_eq!(sanitize("Counter-Strike 2"), "Counter-Strike 2");
        assert_eq!(sanitize("a/b"), "a_b");
        assert_eq!(sanitize("../x"), "_x");
        assert_eq!(sanitize("  "), "Desktop");
    }
}
