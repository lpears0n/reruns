//! Inspect saved clips: duration, audio track count, a WebP thumbnail and a
//! sprite sheet of evenly spaced frames for hover scrubbing.
//!
//! One decode pipeline produces every frame at thumbnail size; the sprite
//! frames are downscaled from those in software, and both images are encoded
//! through a tiny `appsrc → webpenc` pipeline.

use std::path::Path;

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

/// Thumbnail width; cards are shown at most ~320 px wide at 1x.
pub const THUMB_WIDTH: i32 = 320;
/// Each sprite frame is half the thumbnail size.
pub const SPRITE_FRAME_WIDTH: i32 = THUMB_WIDTH / 2;
/// Frames in the sprite sheet, laid out in one row.
pub const SPRITE_FRAMES: usize = 10;

/// Image format of the thumbnail and sprite. WebP where the `webp` plugin
/// exists (Linux distributions); the Windows GStreamer packages ship no
/// WebP encoder, so JPEG there. Screenshots are PNG (lossless).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Webp,
    Jpeg,
    Png,
}

impl ImageFormat {
    /// Whichever lossy encoder this GStreamer has, for thumbnails.
    pub fn available() -> ImageFormat {
        if gst::ElementFactory::find("webpenc").is_some() {
            ImageFormat::Webp
        } else {
            ImageFormat::Jpeg
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Webp => "webp",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
        }
    }
}

pub struct ClipInfo {
    pub duration: gst::ClockTime,
    pub audio_tracks: usize,
    pub format: ImageFormat,
    /// `THUMB_WIDTH` wide, aspect preserved.
    pub thumbnail: Vec<u8>,
    /// `SPRITE_FRAMES` frames of `SPRITE_FRAME_WIDTH` side by side.
    pub sprite: Vec<u8>,
}

pub(crate) struct Frame {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// Tightly packed RGB.
    pub(crate) rgb: Vec<u8>,
}

/// Decode the frame about one second in (or the first, for shorter clips)
/// for the thumbnail, and `SPRITE_FRAMES` more spread across the clip.
pub fn inspect(path: &Path) -> Result<ClipInfo> {
    let uri = gst::glib::filename_to_uri(path, None)?;
    let pipeline = gst::parse::launch(&format!(
        "uridecodebin uri=\"{uri}\" name=dec ! videoconvert ! videoscale \
         ! video/x-raw,format=RGB,width={THUMB_WIDTH},pixel-aspect-ratio=1/1 \
         ! appsink name=sink max-buffers=1"
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();
    // Audio pads are left unlinked (uridecodebin only needs video here), but
    // counted.
    let audio = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let audio = audio.clone();
        pipeline
            .by_name("dec")
            .unwrap()
            .connect_pad_added(move |_, pad| {
                if pad
                    .current_caps()
                    .or_else(|| Some(pad.query_caps(None)))
                    .and_then(|c| c.structure(0).map(|s| s.name().starts_with("audio/")))
                    .unwrap_or(false)
                {
                    audio.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            });
    }
    let sink = pipeline
        .by_name("sink")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();

    let result = (|| {
        pipeline.set_state(gst::State::Paused)?;
        wait_preroll(&pipeline)?;
        let duration = pipeline
            .query_duration::<gst::ClockTime>()
            .context("clip has no duration")?;

        let grab = |at: gst::ClockTime| -> Result<Frame> {
            pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, at)?;
            wait_preroll(&pipeline)?;
            let sample = sink
                .try_pull_preroll(gst::ClockTime::from_seconds(5))
                .context("no frame decoded")?;
            frame_of(&sample)
        };

        let at = gst::ClockTime::from_seconds(1).min(duration / 2);
        let thumb = grab(at)?;
        let mut strip: Vec<Frame> = Vec::with_capacity(SPRITE_FRAMES);
        for i in 0..SPRITE_FRAMES {
            // Centre of each tenth, so the last frame isn't the (often black) end.
            let t = duration.mul_div_floor(2 * i as u64 + 1, 2 * SPRITE_FRAMES as u64);
            let f = grab(t.unwrap_or(gst::ClockTime::ZERO))?;
            strip.push(halve(&f));
        }
        let sprite = concat_row(&strip);
        let format = ImageFormat::available();
        Ok(ClipInfo {
            duration,
            audio_tracks: audio.load(std::sync::atomic::Ordering::Relaxed),
            format,
            thumbnail: encode_image(&thumb, format, 82)?,
            sprite: encode_image(&sprite, format, 70)?,
        })
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result
}

/// A still image (a screenshot) scaled to `THUMB_WIDTH`, encoded as
/// `ImageFormat::available()` like clip thumbnails.
pub fn image_thumbnail(path: &Path) -> Result<Vec<u8>> {
    let uri = gst::glib::filename_to_uri(path, None)?;
    let pipeline = gst::parse::launch(&format!(
        "uridecodebin uri=\"{uri}\" ! videoconvert ! videoscale \
         ! video/x-raw,format=RGB,width={THUMB_WIDTH},pixel-aspect-ratio=1/1 \
         ! appsink name=sink max-buffers=1"
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();
    let sink = pipeline
        .by_name("sink")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    let result = (|| {
        pipeline.set_state(gst::State::Paused)?;
        wait_preroll(&pipeline)?;
        let sample = sink
            .try_pull_preroll(gst::ClockTime::from_seconds(5))
            .context("no image decoded")?;
        encode_image(&frame_of(&sample)?, ImageFormat::available(), 82)
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result
}

/// Copy a sample's RGB frame into tightly packed rows (GStreamer pads rows).
pub(crate) fn frame_of(sample: &gst::Sample) -> Result<Frame> {
    let caps = sample.caps().context("frame has no caps")?;
    let info = gst_video::VideoInfo::from_caps(caps)?;
    let buf = sample.buffer().context("empty frame")?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buf, &info)
        .context("unreadable frame")?;
    let (width, height) = (info.width() as usize, info.height() as usize);
    let stride = frame.plane_stride()[0] as usize;
    let data = frame.plane_data(0).context("no plane data")?;
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        rgb.extend_from_slice(&data[y * stride..y * stride + width * 3]);
    }
    Ok(Frame { width, height, rgb })
}

/// 2×2 box downscale.
fn halve(f: &Frame) -> Frame {
    let (w, h) = (f.width / 2, f.height / 2);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let p = |yy: usize, xx: usize| f.rgb[(yy * f.width + xx) * 3 + c] as u32;
                let sum = p(2 * y, 2 * x)
                    + p(2 * y, 2 * x + 1)
                    + p(2 * y + 1, 2 * x)
                    + p(2 * y + 1, 2 * x + 1);
                rgb.push((sum / 4) as u8);
            }
        }
    }
    Frame {
        width: w,
        height: h,
        rgb,
    }
}

/// Frames side by side (all the same size).
fn concat_row(frames: &[Frame]) -> Frame {
    let (fw, h) = (frames[0].width, frames[0].height);
    let width = fw * frames.len();
    let mut rgb = vec![0u8; width * h * 3];
    for (i, f) in frames.iter().enumerate() {
        for y in 0..h {
            let dst = (y * width + i * fw) * 3;
            rgb[dst..dst + fw * 3].copy_from_slice(&f.rgb[y * fw * 3..(y + 1) * fw * 3]);
        }
    }
    Frame {
        width,
        height: h,
        rgb,
    }
}

/// Encode one RGB frame. `quality` is ignored for PNG.
pub(crate) fn encode_image(f: &Frame, format: ImageFormat, quality: u32) -> Result<Vec<u8>> {
    let encoder = match format {
        ImageFormat::Webp => format!("webpenc quality={quality}"),
        ImageFormat::Jpeg => format!("jpegenc quality={quality}"),
        ImageFormat::Png => "pngenc".to_owned(),
    };
    let pipeline = gst::parse::launch(&format!(
        "appsrc name=src is-live=false format=time \
         caps=video/x-raw,format=RGB,width={},height={},framerate=0/1 \
         ! videoconvert ! {encoder} ! appsink name=sink",
        f.width, f.height
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();
    let src = pipeline
        .by_name("src")
        .unwrap()
        .downcast::<gst_app::AppSrc>()
        .unwrap();
    let sink = pipeline
        .by_name("sink")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    let result = (|| {
        pipeline.set_state(gst::State::Playing)?;
        let mut buf = gst::Buffer::from_slice(f.rgb.clone());
        buf.get_mut().unwrap().set_pts(gst::ClockTime::ZERO);
        src.push_buffer(buf).context("pushing frame")?;
        src.end_of_stream().context("ending stream")?;
        let sample = sink
            .try_pull_sample(gst::ClockTime::from_seconds(5))
            .context("no image encoded")?;
        Ok(sample
            .buffer()
            .context("empty image")?
            .map_readable()?
            .to_vec())
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result
}

fn wait_preroll(pipeline: &gst::Pipeline) -> Result<()> {
    let bus = pipeline.bus().context("no bus")?;
    match bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(10),
        &[gst::MessageType::AsyncDone, gst::MessageType::Error],
    ) {
        Some(msg) => match msg.view() {
            gst::MessageView::AsyncDone(_) => Ok(()),
            gst::MessageView::Error(e) => bail!("{}", e.error()),
            _ => unreachable!(),
        },
        None => bail!("timed out opening clip"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshots_get_card_sized_thumbnails() {
        crate::init().unwrap();
        // A 1280×720 PNG, as a screenshot would be written.
        let frame = Frame {
            width: 1280,
            height: 720,
            rgb: vec![90; 1280 * 720 * 3],
        };
        let dir = std::env::temp_dir().join(format!("reruns-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("shot.png");
        std::fs::write(&png, encode_image(&frame, ImageFormat::Png, 0).unwrap()).unwrap();

        let thumb = image_thumbnail(&png).unwrap();
        let magic: &[u8] = match ImageFormat::available() {
            ImageFormat::Webp => b"RIFF",
            _ => &[0xff, 0xd8],
        };
        assert!(thumb.starts_with(magic), "not a thumbnail image");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
