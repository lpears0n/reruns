//! Render an edited clip: trim to a range and mix the audio tracks at chosen
//! levels into one track (so the result plays the same everywhere),
//! optionally fitted under a file size (Discord's upload limits).

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde::Serialize;

use crate::{
    capture::VideoConfig,
    encoder::{Codec, Encoder},
    media,
};

#[derive(Debug, Clone)]
pub struct Edit {
    pub start: gst::ClockTime,
    pub end: gst::ClockTime,
    /// Linear gain per source audio track, in file order (1.0 = unchanged).
    /// Missing entries default to 1.0.
    pub volumes: Vec<f64>,
    /// Fit the file under this many bytes: the bitrate comes from the
    /// length, and the picture is scaled down (and to 30 fps) when the
    /// budget is too small for the full size. `None` keeps full quality.
    pub max_bytes: Option<u64>,
    /// Crop to a vertical 9:16 frame (TikTok, Shorts, Reels): where the
    /// frame sits across a wider source, 0 = left edge, 1 = right edge.
    pub vertical: Option<f64>,
    /// Play a range slower, inside `start..end`.
    pub slow: Option<Slow>,
}

/// A slow-motion range: `from..to` (source time) plays `factor` times
/// slower. Frames keep their order and are spread out; the source's frame
/// rate sets how smooth it is (a 120 fps recording at 2× still shows 60).
/// Its audio is left out (silent) rather than dragged down in pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slow {
    pub from: gst::ClockTime,
    pub to: gst::ClockTime,
    pub factor: f64,
}

impl Slow {
    /// Slowest allowed: a quarter speed.
    pub const MAX_FACTOR: f64 = 4.0;

    /// The range clamped to `start..end` and the factor to 1–4; `None` if
    /// nothing is left to slow.
    pub fn within(self, start: gst::ClockTime, end: gst::ClockTime) -> Option<Self> {
        let from = self.from.clamp(start, end);
        let to = self.to.clamp(start, end);
        let factor = self.factor.clamp(1.0, Self::MAX_FACTOR);
        (to > from && factor > 1.0).then_some(Self { from, to, factor })
    }

    pub fn contains(&self, t: gst::ClockTime) -> bool {
        t >= self.from && t < self.to
    }

    /// How much longer the output gets.
    pub fn added(&self) -> gst::ClockTime {
        scale(self.to - self.from, self.factor - 1.0)
    }

    /// Where source time `t` lands in the output (same timeline, stretched).
    pub fn map(&self, t: gst::ClockTime) -> gst::ClockTime {
        if t < self.from {
            t
        } else if t < self.to {
            self.from + scale(t - self.from, self.factor)
        } else {
            t + self.added()
        }
    }
}

fn scale(t: gst::ClockTime, by: f64) -> gst::ClockTime {
    gst::ClockTime::from_nseconds((t.nseconds() as f64 * by).round() as u64)
}

impl Edit {
    /// The slow range, if any, clamped to the trim.
    fn slow_range(&self) -> Option<Slow> {
        self.slow.and_then(|s| s.within(self.start, self.end))
    }

    /// How long the rendered file is: the trim, plus what slow motion adds.
    pub fn output_duration(&self) -> gst::ClockTime {
        (self.end - self.start)
            + self
                .slow_range()
                .map_or(gst::ClockTime::ZERO, |s| s.added())
    }
}

/// The 9:16 window of a `w`×`h` picture at `x` (0 = left, 1 = right):
/// `(left, right, top, bottom)` pixels to crop, all even. A picture already
/// narrower than 9:16 loses top and bottom instead, centred.
pub fn vertical_crop(w: u32, h: u32, x: f64) -> (u32, u32, u32, u32) {
    let even = |v: f64| ((v / 2.0).floor() as u32) * 2;
    if w as u64 * 16 >= h as u64 * 9 {
        let cw = even(h as f64 * 9.0 / 16.0).max(2);
        let left = even((w - cw) as f64 * x.clamp(0.0, 1.0));
        (left, w - cw - left, 0, 0)
    } else {
        let ch = even(w as f64 * 16.0 / 9.0).max(2);
        let top = even((h - ch) as f64 / 2.0);
        (0, 0, top, h - ch - top)
    }
}

/// Retime one decoded stream for `slow`: buffers move to their place on the
/// stretched timeline (video frames also last longer in the slow range,
/// audio there is dropped), and the segment's end moves with them so
/// nothing downstream clips the extra length.
fn retime(pad: &gst::Pad, slow: Slow, audio: bool) {
    pad.add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |_, info| {
            // Audio inside the slow range is left out. Taken out of the probe
            // here (rather than returning Drop, which made GStreamer release
            // a buffer gstreamer-rs had already taken).
            let skip = audio
                && matches!(&info.data, Some(gst::PadProbeData::Buffer(b))
                    if b.pts().is_some_and(|pts| slow.contains(pts)));
            if skip {
                info.data = None;
                info.flow_res = Ok(gst::FlowSuccess::Ok);
                return gst::PadProbeReturn::Handled;
            }
            match &mut info.data {
                Some(gst::PadProbeData::Buffer(buffer)) => {
                    let Some(pts) = buffer.pts() else {
                        return gst::PadProbeReturn::Ok;
                    };
                    let b = buffer.make_mut();
                    b.set_pts(slow.map(pts));
                    b.set_dts(gst::ClockTime::NONE);
                    if slow.contains(pts)
                        && let Some(d) = b.duration()
                    {
                        b.set_duration(scale(d, slow.factor));
                    }
                }
                Some(gst::PadProbeData::Event(event))
                    if event.type_() == gst::EventType::Segment =>
                {
                    if let gst::EventView::Segment(e) = event.view()
                        && let Ok(segment) = e.segment().clone().downcast::<gst::ClockTime>()
                    {
                        let mut segment = segment;
                        if let Some(stop) = segment.stop() {
                            segment.set_stop(slow.map(stop));
                        }
                        if let Some(d) = segment.duration() {
                            segment.set_duration(d + slow.added());
                        }
                        *event = gst::event::Segment::new(&segment);
                    }
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        },
    );
}

const AAC_BITRATE: i32 = 192_000;

/// How a size-limited export is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SizePlan {
    pub video_kbps: u32,
    pub audio_kbps: u32,
    /// Picture size class: a 16:9 picture this tall, or the same number of
    /// pixels at other shapes. Never scaled up.
    pub max_height: u32,
    /// Frame rate to drop to; `None` keeps the source's.
    pub fps: Option<u32>,
}

/// Share of the byte budget given to the streams; the rest covers the
/// container and encoder overshoot.
const SIZE_MARGIN: f64 = 0.92;
const MIN_VIDEO_KBPS: u32 = 200;
const MIN_AUDIO_KBPS: u32 = 64;
/// Bits per pixel per frame a size class needs to look clean; lower
/// rates step down to the next class.
const MIN_BPP: f64 = 0.05;
/// Size classes, best first: (height, fps to drop to).
const LADDER: [(u32, Option<u32>); 7] = [
    (2160, None),
    (1440, None),
    (1080, None),
    (720, None),
    (720, Some(30)),
    (480, Some(30)),
    (360, Some(30)),
];

/// Bitrates and picture size to fit `seconds` of video (with audio, if
/// `audio`) under `max_bytes`. Errors when it can't fit at a watchable
/// rate, saying how long a clip can be.
pub fn plan_size(max_bytes: u64, seconds: f64, audio: bool) -> Result<SizePlan> {
    if seconds <= 0.0 {
        bail!("nothing to render");
    }
    let total_kbps = (max_bytes as f64 * 8.0 * SIZE_MARGIN / seconds / 1000.0) as u32;
    let audio_kbps = match (audio, total_kbps) {
        (false, _) => 0,
        (true, t) if t >= 1500 => 128,
        (true, t) if t >= 600 => 96,
        _ => MIN_AUDIO_KBPS,
    };
    let video_kbps = total_kbps.saturating_sub(audio_kbps);
    if video_kbps < MIN_VIDEO_KBPS {
        let floor = MIN_VIDEO_KBPS + if audio { MIN_AUDIO_KBPS } else { 0 };
        let longest = max_bytes as f64 * 8.0 * SIZE_MARGIN / (floor as f64 * 1000.0);
        bail!(
            "too long for {}; trim to under {}",
            mb(max_bytes),
            minutes(longest)
        );
    }
    let (max_height, fps) = LADDER
        .iter()
        .copied()
        .find(|&(h, fps)| {
            let pixels = h as f64 * h as f64 * 16.0 / 9.0;
            video_kbps as f64 * 1000.0 >= pixels * fps.unwrap_or(60) as f64 * MIN_BPP
        })
        .unwrap_or(LADDER[LADDER.len() - 1]);
    Ok(SizePlan {
        video_kbps,
        audio_kbps,
        max_height,
        fps,
    })
}

fn mb(bytes: u64) -> String {
    if bytes < 1_000_000 {
        format!("{} kB", bytes / 1000)
    } else {
        format!("{} MB", (bytes as f64 / 1e6).round())
    }
}

fn minutes(seconds: f64) -> String {
    let s = seconds.floor() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The picture size for a `src_w`×`src_h` source under `max_height`'s
/// pixel budget: the same shape, even sides, never larger than the source.
pub fn scaled_size(src_w: u32, src_h: u32, max_height: u32) -> (u32, u32) {
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    let budget = max_height as f64 * max_height as f64 * 16.0 / 9.0;
    let pixels = src_w as f64 * src_h as f64;
    let factor = if pixels > budget {
        (budget / pixels).sqrt()
    } else {
        1.0
    };
    (even(src_w as f64 * factor), even(src_h as f64 * factor))
}

/// Progress reporting and cancellation for a render.
#[derive(Default)]
pub struct Control {
    /// Called with the fraction done (0..=1) a few times a second.
    pub on_progress: Option<Box<dyn Fn(f64) + Send>>,
    /// Set to stop; the partial output is removed and `render` returns an
    /// error whose message contains "cancelled".
    pub cancel: Option<Arc<AtomicBool>>,
}

/// Re-encode `input[start..end]` to `output` as H.264 (edits are for sharing,
/// so they favour compatibility). `encoder` must support H.264. Blocks until
/// done.
pub fn render(
    input: &Path,
    output: &Path,
    edit: &Edit,
    encoder: Encoder,
    cfg: &VideoConfig,
) -> Result<()> {
    render_with(input, output, edit, encoder, cfg, Control::default())
}

/// [`render`], reporting progress and honouring cancellation.
pub fn render_with(
    input: &Path,
    output: &Path,
    edit: &Edit,
    encoder: Encoder,
    cfg: &VideoConfig,
    control: Control,
) -> Result<()> {
    let cfg = VideoConfig {
        codec: Codec::H264,
        ..cfg.clone()
    };
    if edit.end <= edit.start {
        bail!("end must be after start");
    }
    let audio_tracks = media::inspect(input)?.audio_tracks;
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Every pass renders under a temporary name; the edit only appears
    // once it is done (and, with a size limit, fits).
    let partial = crate::save::partial_path(output);
    let Some(max_bytes) = edit.max_bytes else {
        let result = render_once(
            input,
            &partial,
            edit,
            encoder,
            &cfg,
            audio_tracks,
            None,
            &control,
        );
        return crate::save::finish_partial(output, result);
    };
    let seconds = edit.output_duration().seconds_f64();
    let mut plan = plan_size(max_bytes, seconds, audio_tracks > 0)?;
    // One pass with a rate from the length usually lands under; encoders
    // can overshoot on busy footage, so a file that came out too big is
    // redone at a rate scaled down by how much it missed.
    for _ in 0..3 {
        let cfg = VideoConfig {
            bitrate_kbps: plan.video_kbps,
            fps: plan.fps.map_or(cfg.fps, |f| f.min(cfg.fps)),
            ..cfg.clone()
        };
        // A failed or cancelled pass removes its file; a too-big one from
        // before was only ever the temporary file, so nothing is left.
        render_once(
            input,
            &partial,
            edit,
            encoder,
            &cfg,
            audio_tracks,
            Some(plan),
            &control,
        )?;
        let size = match std::fs::metadata(&partial) {
            Ok(m) => m.len(),
            Err(e) => return crate::save::finish_partial(output, Err(e.into())),
        };
        if size <= max_bytes {
            tracing::info!(size, max_bytes, ?plan, "fitted export");
            return crate::save::finish_partial(output, Ok(()));
        }
        let scaled = plan.video_kbps as f64 * max_bytes as f64 / size as f64 * 0.95;
        tracing::info!(
            size,
            max_bytes,
            ?plan,
            "export too big; redoing at {scaled:.0} kb/s"
        );
        if scaled < MIN_VIDEO_KBPS as f64 {
            break;
        }
        plan.video_kbps = scaled as u32;
    }
    crate::save::discard(&partial);
    bail!("couldn't fit in {}; trim it shorter", mb(max_bytes))
}

/// One render into `output` (removed if it fails); `plan` scales the
/// picture and sets the audio rate.
#[allow(clippy::too_many_arguments)]
fn render_once(
    input: &Path,
    output: &Path,
    edit: &Edit,
    encoder: Encoder,
    cfg: &VideoConfig,
    audio_tracks: usize,
    plan: Option<SizePlan>,
    control: &Control,
) -> Result<()> {
    let pipeline = gst::Pipeline::new();
    let dec = gst::ElementFactory::make("uridecodebin")
        .property("uri", gst::glib::filename_to_uri(input, None)?)
        .build()?;
    let mux = gst::ElementFactory::make("mp4mux")
        .property("faststart", true)
        .build()?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", output)
        .build()?;
    pipeline.add_many([&dec, &mux, &sink])?;
    mux.link(&sink)?;

    // Video: decoded frames → (scaled to the plan) → encoder → mux.
    let vqueue = gst::ElementFactory::make("queue").build()?;
    let mut vchain = vec![
        vqueue.clone(),
        gst::ElementFactory::make("videoconvert").build()?,
    ];
    // Vertical: the 9:16 window is cut first, so the size plan below scales
    // the cropped picture. Its edges are set once the source size is known.
    let crop = match edit.vertical {
        Some(_) => {
            let crop = gst::ElementFactory::make("videocrop").build()?;
            vchain.push(crop.clone());
            Some(crop)
        }
        None => None,
    };
    let slow = edit.slow_range();
    if let Some(slow) = slow {
        retime(&vqueue.static_pad("sink").unwrap(), slow, false);
    }
    // The size is set once the decoder says how big the source is.
    let scale_caps = match plan {
        Some(plan) => {
            let caps = gst::ElementFactory::make("capsfilter").build()?;
            vchain.push(gst::ElementFactory::make("videoscale").build()?);
            if plan.fps.is_some() {
                vchain.push(gst::ElementFactory::make("videorate").build()?);
            }
            vchain.push(caps.clone());
            Some((caps, plan))
        }
        None => None,
    };
    vchain.extend(encoder.upload_chain()?);
    vchain.push(encoder.build(cfg)?);
    vchain.push(
        gst::ElementFactory::make("capsfilter")
            .property("caps", cfg.codec.caps())
            .build()?,
    );
    vchain.push(cfg.codec.make_parser()?);
    pipeline.add_many(&vchain)?;
    gst::Element::link_many(&vchain)?;
    vchain.last().unwrap().link(&mux)?;

    // Audio: every source track → volume → one mixer → AAC → mux. Only built
    // when there's audio; a mixer with no inputs would never finish.
    let mixer = if audio_tracks > 0 {
        let mixer = gst::ElementFactory::make("audiomixer").build()?;
        let chain = [
            mixer.clone(),
            gst::ElementFactory::make("audioconvert").build()?,
            gst::ElementFactory::make("audioresample").build()?,
            gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("audio/x-raw")
                        .field("rate", 48_000i32)
                        .field("channels", 2i32)
                        .build(),
                )
                .build()?,
            aac_encoder(plan.map_or(AAC_BITRATE, |p| p.audio_kbps as i32 * 1000))?,
            gst::ElementFactory::make("aacparse").build()?,
        ];
        pipeline.add_many(&chain)?;
        gst::Element::link_many(&chain)?;
        chain[5].link(&mux)?;
        Some(mixer)
    } else {
        None
    };

    let link_error: Arc<Mutex<Option<String>>> = Arc::default();
    {
        let pipeline = pipeline.downgrade();
        let volumes = edit.volumes.clone();
        let vertical = edit.vertical;
        let link_error = link_error.clone();
        let next_audio = Mutex::new(0usize);
        dec.connect_pad_added(move |_, pad| {
            let Some(pipeline) = pipeline.upgrade() else {
                return;
            };
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            let Some(name) = caps.structure(0).map(|s| s.name().to_string()) else {
                return;
            };
            let result = (|| -> Result<()> {
                if name.starts_with("video/") {
                    let s = caps.structure(0).context("video caps")?;
                    let (mut w, mut h) = (
                        s.get::<i32>("width")? as u32,
                        s.get::<i32>("height")? as u32,
                    );
                    if let (Some(crop), Some(x)) = (&crop, vertical) {
                        let (l, r, t, b) = vertical_crop(w, h, x);
                        crop.set_property("left", l as i32);
                        crop.set_property("right", r as i32);
                        crop.set_property("top", t as i32);
                        crop.set_property("bottom", b as i32);
                        (w, h) = (w - l - r, h - t - b);
                    }
                    if let Some((filter, plan)) = &scale_caps {
                        let (w, h) = scaled_size(w, h, plan.max_height);
                        let mut out = gst::Caps::builder("video/x-raw")
                            .field("width", w as i32)
                            .field("height", h as i32)
                            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1));
                        if let Some(fps) = plan.fps {
                            out = out.field("framerate", gst::Fraction::new(fps as i32, 1));
                        }
                        filter.set_property("caps", out.build());
                    }
                    pad.link(&vqueue.static_pad("sink").unwrap())?;
                } else if name.starts_with("audio/")
                    && let Some(mixer) = &mixer
                {
                    let i = {
                        let mut n = next_audio.lock().unwrap();
                        *n += 1;
                        *n - 1
                    };
                    let gain = volumes.get(i).copied().unwrap_or(1.0).clamp(0.0, 4.0);
                    // audiorate fills what slow motion leaves out with silence.
                    let chain = [
                        gst::ElementFactory::make("queue").build()?,
                        gst::ElementFactory::make("audioconvert").build()?,
                        gst::ElementFactory::make("audioresample").build()?,
                        gst::ElementFactory::make("audiorate").build()?,
                        gst::ElementFactory::make("volume")
                            .property("volume", gain)
                            .build()?,
                    ];
                    pipeline.add_many(&chain)?;
                    gst::Element::link_many(&chain)?;
                    chain[4].link(mixer)?;
                    if let Some(slow) = slow {
                        retime(&chain[0].static_pad("sink").unwrap(), slow, true);
                    }
                    for e in &chain {
                        e.sync_state_with_parent()?;
                    }
                    pad.link(&chain[0].static_pad("sink").unwrap())?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                *link_error.lock().unwrap() = Some(format!("{e:#}"));
            }
        });
    }

    let result = (|| -> Result<()> {
        pipeline.set_state(gst::State::Paused)?;
        wait(&pipeline, gst::MessageType::AsyncDone)?;
        pipeline.seek(
            1.0,
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::SeekType::Set,
            edit.start,
            gst::SeekType::Set,
            edit.end,
        )?;
        wait(&pipeline, gst::MessageType::AsyncDone)?;
        pipeline.set_state(gst::State::Playing)?;
        run(&pipeline, edit, control)
    })();
    let _ = pipeline.set_state(gst::State::Null);
    let result = match link_error.lock().unwrap().take() {
        Some(e) => result.and(Err(anyhow::anyhow!(e))),
        None => result,
    };
    if result.is_err() {
        crate::save::discard(output);
    }
    result
}

/// How long a render may take before it is taken for stuck: ten minutes,
/// or ten times the output's length for long ones (a ten-minute replay
/// encoded in software, or in slow motion, takes a good while longer than
/// ten minutes on a slower PC).
fn render_time_limit(output: gst::ClockTime) -> Duration {
    Duration::from_secs(600).max(Duration::from_nanos(output.nseconds()) * 10)
}

/// Play to EOS, reporting position as progress and stopping on cancel.
fn run(pipeline: &gst::Pipeline, edit: &Edit, control: &Control) -> Result<()> {
    let bus = pipeline.bus().context("no bus")?;
    let span = edit.output_duration().nseconds().max(1) as f64;
    let started = std::time::Instant::now();
    let limit = render_time_limit(edit.output_duration());
    loop {
        if control
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            bail!("render cancelled");
        }
        match bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(200),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) {
            Some(msg) => match msg.view() {
                gst::MessageView::Error(e) => bail!(
                    "{} from {:?}: {:?}",
                    e.error(),
                    msg.src().map(|s| s.path_string()),
                    e.debug()
                ),
                _ => {
                    if let Some(f) = &control.on_progress {
                        f(1.0);
                    }
                    return Ok(());
                }
            },
            None => {
                if started.elapsed() > limit {
                    bail!("timed out rendering");
                }
                if let Some(f) = &control.on_progress
                    && let Some(pos) = pipeline.query_position::<gst::ClockTime>()
                {
                    let done = pos.saturating_sub(edit.start).nseconds() as f64 / span;
                    f(done.clamp(0.0, 1.0));
                }
            }
        }
    }
}

fn wait(pipeline: &gst::Pipeline, until: gst::MessageType) -> Result<()> {
    let bus = pipeline.bus().context("no bus")?;
    match bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(600),
        &[until, gst::MessageType::Error],
    ) {
        Some(msg) => match msg.view() {
            gst::MessageView::Error(e) => bail!(
                "{} from {:?}: {:?}",
                e.error(),
                msg.src().map(|s| s.path_string()),
                e.debug()
            ),
            _ => Ok(()),
        },
        None => bail!("timed out rendering"),
    }
}

fn aac_encoder(bitrate: i32) -> Result<gst::Element> {
    let name = ["fdkaacenc", "avenc_aac"]
        .into_iter()
        .find(|n| gst::ElementFactory::find(n).is_some())
        .context("no AAC encoder found")?;
    let el = gst::ElementFactory::make(name).build()?;
    el.set_property("bitrate", bitrate);
    Ok(el)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    #[test]
    fn long_renders_get_time_in_proportion() {
        let secs = |s: u64| gst::ClockTime::from_seconds(s);
        assert_eq!(render_time_limit(secs(30)), Duration::from_secs(600));
        // A 10-minute replay at quarter speed: 40 minutes of output.
        assert_eq!(render_time_limit(secs(2400)), Duration::from_secs(24_000));
    }

    #[test]
    fn short_clips_keep_a_high_class() {
        // 10 MB for 10 s: ~7.2 Mb/s, enough for 1080p60.
        let p = plan_size(10 * MB, 10.0, true).unwrap();
        assert_eq!(p.audio_kbps, 128);
        assert_eq!((p.max_height, p.fps), (1080, None));
        assert!(p.video_kbps > 6000, "{p:?}");
    }

    #[test]
    fn longer_clips_step_down_in_size_and_rate() {
        // 10 MB for 30 s: ~2.3 Mb/s of video: 720p at 30 fps.
        let p = plan_size(10 * MB, 30.0, true).unwrap();
        assert_eq!((p.max_height, p.fps), (720, Some(30)));
        // 10 MB for 60 s: ~1.1 Mb/s: 480p, with less audio.
        let p = plan_size(10 * MB, 60.0, true).unwrap();
        assert_eq!((p.max_height, p.fps), (480, Some(30)));
        assert_eq!(p.audio_kbps, 96);
    }

    #[test]
    fn the_plan_fits_the_budget() {
        for secs in [5.0, 30.0, 90.0, 240.0] {
            let p = plan_size(10 * MB, secs, true).unwrap();
            let bytes = (p.video_kbps + p.audio_kbps) as f64 * 1000.0 / 8.0 * secs;
            assert!(
                bytes <= 10.0 * MB as f64 * SIZE_MARGIN + 1000.0,
                "{secs}: {bytes}"
            );
        }
    }

    #[test]
    fn too_long_says_how_long_fits() {
        let e = plan_size(10 * MB, 600.0, true).unwrap_err().to_string();
        assert_eq!(e, "too long for 10 MB; trim to under 4:38");
        // Without audio, all of it goes to the picture.
        assert_eq!(plan_size(10 * MB, 300.0, false).unwrap().audio_kbps, 0);
    }

    #[test]
    fn vertical_crop_takes_a_9_16_window() {
        // 1920x1080: a 606x1080 window (even), sliding left to right.
        assert_eq!(vertical_crop(1920, 1080, 0.0), (0, 1314, 0, 0));
        assert_eq!(vertical_crop(1920, 1080, 1.0), (1314, 0, 0, 0));
        let (l, r, _, _) = vertical_crop(1920, 1080, 0.5);
        assert_eq!((l % 2, 1920 - l - r), (0, 606));
        // Ultrawide keeps its full height.
        let (l, r, t, b) = vertical_crop(3440, 1440, 0.5);
        assert_eq!((3440 - l - r, t + b), (810, 0));
        // Already portrait: trims top and bottom, centred.
        assert_eq!(vertical_crop(720, 1600, 0.3), (0, 0, 160, 160));
    }

    #[test]
    fn slow_motion_stretches_the_range_and_shifts_the_rest() {
        let s = |v: u64| gst::ClockTime::from_mseconds(v);
        let slow = Slow {
            from: s(1000),
            to: s(2000),
            factor: 2.0,
        };
        assert_eq!(slow.map(s(500)), s(500));
        assert_eq!(slow.map(s(1500)), s(2000));
        assert_eq!(slow.map(s(2000)), s(3000));
        assert_eq!(slow.map(s(2500)), s(3500));
        // Clamped to the trim, and nothing to do at 1x.
        assert_eq!(
            slow.within(s(1500), s(3000)),
            Some(Slow {
                from: s(1500),
                to: s(2000),
                factor: 2.0
            })
        );
        assert_eq!(
            Slow {
                factor: 1.0,
                ..slow
            }
            .within(s(0), s(5000)),
            None
        );
        assert_eq!(
            Slow {
                factor: 9.0,
                ..slow
            }
            .within(s(0), s(5000))
            .unwrap()
            .factor,
            4.0
        );
    }

    #[test]
    fn scaling_keeps_the_shape_and_pixel_budget() {
        // 16:9 down to 720p.
        assert_eq!(scaled_size(1920, 1080, 720), (1280, 720));
        // Never up.
        assert_eq!(scaled_size(1280, 720, 1080), (1280, 720));
        // Ultrawide: the same pixels as 1280x720, the same shape.
        let (w, h) = scaled_size(3440, 1440, 720);
        assert_eq!((w % 2, h % 2), (0, 0));
        assert!((w as f64 / h as f64 - 3440.0 / 1440.0).abs() < 0.01);
        assert!(w * h <= 1280 * 720 + 4000, "{w}x{h}");
    }
}
