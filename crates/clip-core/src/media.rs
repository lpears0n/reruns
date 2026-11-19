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

/// Covers stay sharp on a 480 px card at 2x display scaling.
pub const THUMB_WIDTH: i32 = 960;
/// Scrubbing stays lightweight, independently of the full-size cover.
pub const SPRITE_FRAME_WIDTH: i32 = 160;
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

/// Decode the frame about one second in and `SPRITE_FRAMES` more spread
/// across the clip. Pick a detailed, visible cover from those same samples;
/// no extra seeks, and only the best full-size frame is retained.
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
        let mut thumb = grab(at)?;
        let mut best = cover_score(&thumb);
        let mut strip: Vec<Frame> = Vec::with_capacity(SPRITE_FRAMES);
        for i in 0..SPRITE_FRAMES {
            // Centre of each tenth, so the last frame isn't the (often black) end.
            let t = duration.mul_div_floor(2 * i as u64 + 1, 2 * SPRITE_FRAMES as u64);
            let f = grab(t.unwrap_or(gst::ClockTime::ZERO))?;
            strip.push(sprite_frame(&f));
            let score = cover_score(&f);
            if score > best {
                best = score;
                thumb = f;
            }
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
    still(path, None)
}

/// The video's frame at `at` seconds, exactly (not the keyframe before it),
/// like a clip thumbnail: the cover the user picked in the viewer.
pub fn thumbnail_at(path: &Path, at: f64) -> Result<Vec<u8>> {
    still(
        path,
        Some(gst::ClockTime::from_nseconds((at.max(0.0) * 1e9) as u64)),
    )
}

fn still(path: &Path, at: Option<gst::ClockTime>) -> Result<Vec<u8>> {
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
        if let Some(at) = at {
            pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at)?;
            wait_preroll(&pipeline)?;
        }
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

/// Area-average downscale; a larger cover never makes the strip larger.
fn sprite_frame(f: &Frame) -> Frame {
    let w = (SPRITE_FRAME_WIDTH as usize).min(f.width);
    let h = ((f.height * w + f.width / 2) / f.width).max(1);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let (y0, y1) = (y * f.height / h, (y + 1) * f.height / h);
        for x in 0..w {
            let (x0, x1) = (x * f.width / w, (x + 1) * f.width / w);
            let mut sum = [0u32; 3];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let p = (yy * f.width + xx) * 3;
                    for (c, value) in sum.iter_mut().enumerate() {
                        *value += u32::from(f.rgb[p + c]);
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            for value in sum {
                rgb.push((value / count) as u8);
            }
        }
    }
    Frame {
        width: w,
        height: h,
        rgb,
    }
}

/// A bounded luminance sample rewards detail across the image, while
/// penalising black loading screens, sparse logos and solid flashes. It
/// cannot recognise gameplay semantically; a user-picked cover always wins
/// in the library. Equal scores keep the earlier candidate deterministically.
fn cover_score(f: &Frame) -> u64 {
    let (cols, rows) = (f.width.min(64), f.height.min(36));
    let (mut sum, mut squares, mut visible, mut detail) = (0u64, 0u64, 0u64, 0u64);
    for y in 0..rows {
        let mut previous = None;
        for x in 0..cols {
            let p = ((y * f.height / rows) * f.width + x * f.width / cols) * 3;
            let luma = (54 * u64::from(f.rgb[p])
                + 183 * u64::from(f.rgb[p + 1])
                + 19 * u64::from(f.rgb[p + 2]))
                / 256;
            sum += luma;
            squares += luma * luma;
            visible += u64::from((16..=244).contains(&luma));
            if let Some(last) = previous {
                detail += luma.abs_diff(last);
            }
            previous = Some(luma);
        }
    }
    let count = (cols * rows) as u64;
    let variance = (squares * count - sum * sum) / (count * count);
    (variance + 16 * detail / count) * visible * visible / (count * count)
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
    fn cover_scoring_prefers_visible_detail_over_loading_frames() {
        let frame = |pixel: &dyn Fn(usize, usize) -> u8| Frame {
            width: 64,
            height: 36,
            rgb: (0..36)
                .flat_map(|y| (0..64).flat_map(move |x| [pixel(x, y); 3]))
                .collect(),
        };
        let scene = frame(&|x, y| 30 + ((x * 11 + y * 7) % 130) as u8);
        let dark_scene = frame(&|x, y| 18 + ((x * 11 + y * 7) % 35) as u8);
        let loading = frame(&|x, y| {
            if (28..36).contains(&x) && (16..20).contains(&y) {
                220
            } else {
                3
            }
        });
        for blank in [0, 6, 90, 255] {
            assert!(cover_score(&scene) > cover_score(&frame(&|_, _| blank)));
        }
        assert!(cover_score(&scene) > cover_score(&loading));
        assert!(cover_score(&dark_scene) > cover_score(&loading));
    }

    #[test]
    fn sharp_covers_keep_the_preview_strip_small() {
        let cover = Frame {
            width: THUMB_WIDTH as usize,
            height: 540,
            rgb: vec![90; THUMB_WIDTH as usize * 540 * 3],
        };
        let previews: Vec<_> = (0..SPRITE_FRAMES).map(|_| sprite_frame(&cover)).collect();
        let strip = concat_row(&previews);
        assert_eq!((strip.width, strip.height), (1600, 90));
        assert_eq!(strip.rgb.len(), 432_000);
        assert!(strip.rgb.iter().all(|&value| value == 90));
    }

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
