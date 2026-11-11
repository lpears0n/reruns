//! Capture, encoding and instant-replay buffering for Reruns.

pub mod audio;
pub mod backend;
pub mod capture;
pub mod encoder;
pub mod export;
pub mod game;
pub mod hdr;
pub mod launchers;
pub mod media;
pub mod meter;
pub mod pacing;
pub mod recorder;
pub mod ring;
pub mod save;
pub mod screenshot;
pub mod sound;
pub mod tonemap;

// Linux: Wayland compositors (KWin direct, or the screen-cast portal),
// PipeWire, desktop hotkey services.
#[cfg(target_os = "linux")]
pub mod auto;
#[cfg(target_os = "linux")]
pub mod display;
pub mod hotkey;
#[cfg(target_os = "linux")]
pub mod kwin;
#[cfg(target_os = "linux")]
pub mod portal;

// Windows: Windows Graphics Capture, WASAPI, window walking.
#[cfg(windows)]
pub mod win;
#[cfg(windows)]
pub use win::auto;
#[cfg(windows)]
pub use win::display;

pub use audio::{AudioDevice, AudioSource, add_audio_branch, input_devices, output_devices};
pub use backend::Backend;
#[cfg(target_os = "linux")]
pub use capture::PipewireTarget;
pub use capture::{CapturePath, CaptureTarget, VideoConfig, add_video_branch};
pub use encoder::{Codec, Encoder, EncoderFallback};
pub use gstreamer as gst;
pub use hdr::HdrDisplay;
pub use hotkey::{Action, Binding};
#[cfg(target_os = "linux")]
pub use portal::ScreenSource;
pub use recorder::{CaptureSource, Recorder, RecorderConfig, SavedClip, Status};
pub use ring::{Clip, ReplayBuffer};
pub use save::{clip_path, default_clips_dir, screenshot_path, write_mp4};
pub use screenshot::write_png;

/// Initialise GStreamer. Safe to call more than once.
pub fn init() -> anyhow::Result<()> {
    gstreamer::init()?;
    Ok(())
}
