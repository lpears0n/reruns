//! Frame pacing: how evenly a recording's pictures advance.
//!
//! Two things make a clip judder while its timestamps look fine or its
//! frame count looks right:
//!
//! - **repeats**: the same picture sent again for a few frames (the capture
//!   source had nothing new), a short freeze in the middle of motion;
//! - **holes**: frames missing from the timeline (dropped after the frame
//!   rate was fixed), which a player shows as a held frame.
//!
//! [`Tally`] counts both from a stream of "frame at this time, picture
//! changed or not". [`PacketMonitor`] feeds it while recording, from
//! encoded packet sizes alone (a repeated picture encodes to almost
//! nothing), so it costs the capture path nothing. [`analyze`] feeds it
//! from a saved clip's decoded pictures, which is the ground truth the
//! size guess is checked against.

use std::{collections::VecDeque, path::Path};

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// A run of unchanged pictures up to this long is a repeat; longer is a
/// scene that really stood still (a menu, a loading screen).
const MAX_REPEAT_NS: u64 = 250_000_000;

/// A gap longer than this is a pause, not a hole: the recorder leaves
/// them when a game stops drawing (see `capture::make_rate`).
const PAUSE_NS: u64 = 1_000_000_000;

/// Most events kept for [`Tally::events`] and [`Tally::since`].
const MAX_EVENTS: usize = 4096;

/// Something that broke even pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `missing` frames' worth of time with no frame, starting at `at_ns`.
    Hole { at_ns: u64, missing: u32 },
    /// The picture at `at_ns` stayed for `frames` more frames.
    Repeat { at_ns: u64, frames: u32 },
}

impl Event {
    pub fn at_ns(&self) -> u64 {
        match *self {
            Event::Hole { at_ns, .. } | Event::Repeat { at_ns, .. } => at_ns,
        }
    }
}

/// Counts over a stretch of frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub frames: u64,
    /// Gaps in the timeline, and the frames that should have filled them.
    pub holes: u64,
    pub missing: u64,
    /// Short freezes, and the frames that only repeated a picture.
    pub repeat_runs: u64,
    pub repeated: u64,
    /// Longest short freeze, in frames.
    pub longest_repeat: u32,
    /// Frames of scenes that stood still (not judder).
    pub still: u64,
    /// Gaps longer than a second (the game wasn't drawing; not judder).
    pub pauses: u64,
}

impl Totals {
    /// Share of the timeline that was a repeated or missing frame, in
    /// percent. The budget is under 0.5 (docs/DESIGN.md §5).
    pub fn judder_percent(&self) -> f64 {
        let slots = self.frames + self.missing;
        if slots == 0 {
            return 0.0;
        }
        (self.missing + self.repeated) as f64 * 100.0 / slots as f64
    }
}

/// Counts repeats and holes from frames in presentation order.
#[derive(Debug, Clone)]
pub struct Tally {
    frame_ns: u64,
    totals: Totals,
    last_pts: Option<u64>,
    /// Unchanged pictures in a row so far: (time of the picture they
    /// repeat, how many).
    run: Option<(u64, u32)>,
    events: VecDeque<Event>,
    /// Pauses: (time of the last frame before, length).
    pauses: VecDeque<(u64, u64)>,
}

impl Tally {
    pub fn new(fps: f64) -> Self {
        Self {
            frame_ns: (1e9 / fps.max(1.0)) as u64,
            totals: Totals::default(),
            last_pts: None,
            run: None,
            events: VecDeque::new(),
            pauses: VecDeque::new(),
        }
    }

    /// One frame at `pts_ns`. `still`: its picture is the previous one's.
    pub fn frame(&mut self, pts_ns: u64, still: bool) {
        self.totals.frames += 1;
        if let Some(last) = self.last_pts {
            let step = pts_ns.saturating_sub(last);
            if step > PAUSE_NS {
                self.totals.pauses += 1;
                if self.pauses.len() == MAX_EVENTS {
                    self.pauses.pop_front();
                }
                self.pauses.push_back((last, step));
            } else if step > self.frame_ns * 3 / 2 {
                let missing = ((step + self.frame_ns / 2) / self.frame_ns).saturating_sub(1);
                self.totals.holes += 1;
                self.totals.missing += missing;
                self.push(Event::Hole {
                    at_ns: last,
                    missing: missing as u32,
                });
            }
        }
        if still {
            let (_, n) = self.run.get_or_insert((self.last_pts.unwrap_or(pts_ns), 0));
            *n += 1;
        } else {
            self.close_run();
        }
        self.last_pts = Some(pts_ns);
    }

    /// Count the run of unchanged pictures that just ended.
    fn close_run(&mut self) {
        let Some((at_ns, frames)) = self.run.take() else {
            return;
        };
        if u64::from(frames) * self.frame_ns <= MAX_REPEAT_NS {
            self.totals.repeat_runs += 1;
            self.totals.repeated += u64::from(frames);
            self.totals.longest_repeat = self.totals.longest_repeat.max(frames);
            self.push(Event::Repeat { at_ns, frames });
        } else {
            self.totals.still += u64::from(frames);
        }
    }

    fn push(&mut self, event: Event) {
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Everything so far. A run of unchanged pictures still going on isn't
    /// counted until it ends (call [`Tally::finish`] at the end of a file).
    pub fn totals(&self) -> Totals {
        self.totals
    }

    /// The end of the stream: count a run that was still open.
    pub fn finish(&mut self) -> Totals {
        self.close_run();
        self.totals
    }

    /// The newest events, oldest first (at most a few thousand are kept).
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    /// Repeats and holes from `start_ns` on: what a clip that starts there
    /// contains. `frames` is estimated from the time covered, less the
    /// pauses in it (no frames are expected there).
    pub fn since(&self, start_ns: u64) -> Totals {
        let mut t = Totals::default();
        for e in self.events.iter().filter(|e| e.at_ns() >= start_ns) {
            match *e {
                Event::Hole { missing, .. } => {
                    t.holes += 1;
                    t.missing += u64::from(missing);
                }
                Event::Repeat { frames, .. } => {
                    t.repeat_runs += 1;
                    t.repeated += u64::from(frames);
                    t.longest_repeat = t.longest_repeat.max(frames);
                }
            }
        }
        let paused: u64 = self
            .pauses
            .iter()
            .filter(|p| p.0 >= start_ns)
            .map(|p| p.1)
            .sum();
        let covered = self
            .last_pts
            .unwrap_or(start_ns)
            .saturating_sub(start_ns)
            .saturating_sub(paused);
        t.frames = (covered / self.frame_ns.max(1)).saturating_sub(t.missing);
        t
    }
}

/// Sizes remembered for the median a packet is compared against.
const SIZE_WINDOW: usize = 128;
/// Packets seen before any is judged.
const SIZE_WARMUP: usize = 32;
/// A non-keyframe under a quarter of the median size carries no new
/// picture. Measured on five games' clips (NVENC, 20–50 Mb/s) with
/// `RERUNS_JUDDER_SIZES=1 judder`: 95% of repeats were under 25–42% of the
/// median, and 95% of new pictures over 42–50%.
const TINY_DIVISOR: u64 = 4;

/// [`Tally`] fed from encoded packets while recording: a non-keyframe far
/// smaller than its neighbours is taken for a repeated picture. A guess
/// (a still scene looks the same, and is sorted out by its length), cheap
/// enough for the capture path: no frame is looked at.
#[derive(Debug, Clone)]
pub struct PacketMonitor {
    tally: Tally,
    sizes: [u32; SIZE_WINDOW],
    seen: usize,
    median: u32,
}

impl PacketMonitor {
    pub fn new(fps: f64) -> Self {
        Self {
            tally: Tally::new(fps),
            sizes: [0; SIZE_WINDOW],
            seen: 0,
            median: 0,
        }
    }

    pub fn packet(&mut self, pts_ns: u64, size: usize, keyframe: bool) {
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let still = !keyframe
            && self.seen >= SIZE_WARMUP
            && u64::from(size) * TINY_DIVISOR < u64::from(self.median);
        self.tally.frame(pts_ns, still);
        if keyframe {
            return;
        }
        self.sizes[self.seen % SIZE_WINDOW] = size;
        self.seen += 1;
        // The median moves slowly; refresh it now and then, not per frame.
        if self.seen.is_multiple_of(SIZE_WARMUP) {
            let n = self.seen.min(SIZE_WINDOW);
            let mut sorted = self.sizes;
            let sorted = &mut sorted[..n];
            sorted.sort_unstable();
            self.median = sorted[n / 2];
        }
    }

    pub fn tally(&self) -> &Tally {
        &self.tally
    }

    pub fn finish(&mut self) -> Totals {
        self.tally.finish()
    }
}

/// What [`analyze`] found in a clip.
#[derive(Debug, Clone)]
pub struct Report {
    /// Frame rate, from the usual step between frames.
    pub fps: f64,
    /// From the decoded pictures: the truth.
    pub totals: Totals,
    /// Repeats and holes, oldest first, timed from the start of the clip.
    pub events: Vec<Event>,
    /// What the recorder's own counters ([`PacketMonitor`]) make of the
    /// same clip, to check the guess against `totals`. `None` if the file
    /// isn't MP4.
    pub packets: Option<Totals>,
    /// Frames that looked unchanged in a slow scene: too little was moving
    /// around them to tell a repeat from a picture that barely changed.
    /// Not counted as judder.
    pub unclear: u64,
    /// Share of the clip's frames, in percent, with motion clear enough
    /// to judge. A low number means the score says little.
    pub clear_motion_percent: f64,
}

/// Size the pictures are compared at: small enough to be quick, wide
/// enough that a moving crosshair still shows.
const PROBE_WIDTH: usize = 344;
const PROBE_HEIGHT: usize = 144;

/// Two pictures whose pixels differ by less than this on average (of 255)
/// are the same picture. A repeat decodes to exactly the frame before it;
/// the slowest real motion measured was well over 0.5.
const SAME_PICTURE: f64 = 0.1;

/// A short freeze counts only when the pictures on both sides of it
/// differ by at least this much: in a slow scene (the owner's Deadlock
/// clips: a median difference of 0.25–0.66) ordinary frames dip under
/// [`SAME_PICTURE`] and would be taken for repeats.
const CLEAR_MOTION: f64 = 0.5;

/// Which frames repeat the one before, from each frame's difference to
/// it (`f64::INFINITY` for the first). Short runs of unchanged pictures
/// without clear motion on both sides are left out and counted, as the
/// second value; runs longer than `max_run` frames are still scenes and
/// are kept for [`Tally`] to sort out.
fn judge(differences: &[f64], max_run: usize) -> (Vec<bool>, u64) {
    let mut still: Vec<bool> = differences.iter().map(|&d| d < SAME_PICTURE).collect();
    let clear = |i: usize| {
        differences
            .get(i)
            .is_some_and(|d| d.is_finite() && *d >= CLEAR_MOTION)
    };
    let mut unclear = 0;
    let mut i = 0;
    while i < still.len() {
        if !still[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < still.len() && still[i] {
            i += 1;
        }
        let moving = start > 0 && clear(start - 1) && clear(i);
        if i - start <= max_run && !moving {
            still[start..i].fill(false);
            unclear += (i - start) as u64;
        }
    }
    (still, unclear)
}

/// Measure a saved clip's pacing: decode every frame, compare each with
/// the one before, and count repeats and holes.
pub fn analyze(path: &Path) -> Result<Report> {
    let frames = decoded_frames(path)?;
    if frames.len() < 2 {
        bail!("{} has no video to measure", path.display());
    }
    let fps = usual_fps(frames.iter().map(|f| f.0));
    let start = frames[0].0;
    let differences: Vec<f64> = frames.iter().map(|f| f.1).collect();
    let max_run = (MAX_REPEAT_NS as f64 * fps / 1e9) as usize;
    let (still, unclear) = judge(&differences, max_run);
    let mut tally = Tally::new(fps);
    for (&(pts, _), &still) in frames.iter().zip(&still) {
        tally.frame(pts - start, still);
    }
    let totals = tally.finish();
    let clear = differences.iter().filter(|&&d| d >= CLEAR_MOTION).count();
    let packets = packets(path).ok().filter(|p| p.len() >= 2).map(|p| {
        let start = p[0].0;
        let mut monitor = PacketMonitor::new(fps);
        for &(pts, size, keyframe) in &p {
            monitor.packet(pts.saturating_sub(start), size, keyframe);
        }
        monitor.finish()
    });
    Ok(Report {
        fps,
        totals,
        events: tally.events().copied().collect(),
        packets,
        unclear,
        clear_motion_percent: clear as f64 * 100.0 / frames.len() as f64,
    })
}

/// For tuning [`PacketMonitor`]'s guess against the truth: every
/// non-keyframe's size as a share of the clip's median one, and whether
/// its picture was a repeat.
pub fn size_shares(path: &Path) -> Result<Vec<(f64, bool)>> {
    let frames = decoded_frames(path)?;
    let packets = packets(path)?;
    if frames.len() != packets.len() {
        bail!(
            "{} frames decoded from {} packets",
            frames.len(),
            packets.len()
        );
    }
    let mut sizes: Vec<usize> = packets.iter().filter(|p| !p.2).map(|p| p.1).collect();
    sizes.sort_unstable();
    let median = *sizes.get(sizes.len() / 2).context("no frames")? as f64;
    Ok(frames
        .iter()
        .zip(&packets)
        .filter(|(_, p)| !p.2)
        .map(|(f, p)| (p.1 as f64 / median.max(1.0), f.1 < SAME_PICTURE))
        .collect())
}

/// Frames per second from a short step between timestamps: the tenth
/// percentile, not the median, which a clip that dropped most of its
/// frames would pass off as a lower frame rate with no holes. (A clip
/// that dropped the same frames throughout can't be told from one
/// recorded at that rate: the file's own rate comes from these steps.)
fn usual_fps(pts: impl Iterator<Item = u64>) -> f64 {
    let pts: Vec<u64> = pts.collect();
    let mut steps: Vec<u64> = pts
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]))
        .filter(|&s| s > 0)
        .collect();
    if steps.is_empty() {
        return 60.0;
    }
    steps.sort_unstable();
    1e9 / steps[steps.len() / 10] as f64
}

/// Every frame's timestamp and how much its picture differs from the one
/// before (mean per pixel, of 255; infinite for the first frame).
fn decoded_frames(path: &Path) -> Result<Vec<(u64, f64)>> {
    // A URI needs the whole path; one given on a command line may not be.
    let uri = gst::glib::filename_to_uri(std::path::absolute(path)?, None)?;
    let pipeline = gst::parse::launch(&format!(
        "uridecodebin uri=\"{uri}\" ! videoconvert ! videoscale \
         ! video/x-raw,format=GRAY8,width={PROBE_WIDTH},height={PROBE_HEIGHT} \
         ! appsink name=sink sync=false max-buffers=4"
    ))?
    .downcast::<gst::Pipeline>()
    .unwrap();
    let sink = pipeline
        .by_name("sink")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    let result = (|| {
        pipeline.set_state(gst::State::Playing)?;
        let mut frames = Vec::new();
        let mut previous: Option<Vec<u8>> = None;
        while let Some(sample) = next_sample(&pipeline, &sink)? {
            let Some(buffer) = sample.buffer() else {
                continue;
            };
            let Some(pts) = buffer.pts() else { continue };
            let map = buffer.map_readable()?;
            let picture = luma(map.as_slice());
            let difference = previous
                .as_deref()
                .map_or(f64::INFINITY, |p| mean_difference(p, &picture));
            frames.push((pts.nseconds(), difference));
            previous = Some(picture);
        }
        Ok(frames)
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result
}

/// The next sample, or `None` at the end of the stream. A pipeline that
/// fails, or delivers nothing for ten seconds, is an error rather than a
/// wait that never ends.
fn next_sample(pipeline: &gst::Pipeline, sink: &gst_app::AppSink) -> Result<Option<gst::Sample>> {
    if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        return Ok(Some(sample));
    }
    if let Some(msg) = pipeline
        .bus()
        .and_then(|b| b.pop_filtered(&[gst::MessageType::Error]))
        && let gst::MessageView::Error(e) = msg.view()
    {
        bail!("{} ({})", e.error(), e.debug().unwrap_or_default());
    }
    if sink.is_eos() {
        return Ok(None);
    }
    bail!("the file stopped giving frames")
}

/// The picture's rows without the padding GStreamer adds to each (GRAY8
/// rows are padded to 4 bytes).
fn luma(data: &[u8]) -> Vec<u8> {
    let stride = PROBE_WIDTH.next_multiple_of(4);
    data.chunks(stride)
        .take(PROBE_HEIGHT)
        .flat_map(|row| &row[..PROBE_WIDTH.min(row.len())])
        .copied()
        .collect()
}

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
        .sum();
    sum as f64 / a.len() as f64
}

/// The video track's packets as they are in the file: timestamp, size,
/// keyframe. MP4 only.
fn packets(path: &Path) -> Result<Vec<(u64, usize, bool)>> {
    let pipeline = gst::Pipeline::new();
    // The path is set as a property: a launch string eats backslashes.
    let src = gst::ElementFactory::make("filesrc")
        .property("location", path.to_str().context("path isn't UTF-8")?)
        .build()?;
    let demux = gst::ElementFactory::make("qtdemux").build()?;
    let sink = gst_app::AppSink::builder().sync(false).build();
    pipeline.add_many([&src, &demux, sink.upcast_ref()])?;
    src.link(&demux)?;
    {
        let sink = sink.clone();
        demux.connect_pad_added(move |_, pad| {
            let video = pad
                .current_caps()
                .or_else(|| Some(pad.query_caps(None)))
                .and_then(|c| c.structure(0).map(|s| s.name().starts_with("video/")))
                .unwrap_or(false);
            let target = sink.static_pad("sink").unwrap();
            if video && !target.is_linked() {
                let _ = pad.link(&target);
            }
        });
    }
    let result = (|| {
        pipeline.set_state(gst::State::Playing)?;
        let mut out = Vec::new();
        while let Some(sample) = next_sample(&pipeline, &sink)? {
            let Some(buffer) = sample.buffer() else {
                continue;
            };
            let Some(pts) = buffer.pts() else { continue };
            out.push((
                pts.nseconds(),
                buffer.size(),
                !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT),
            ));
        }
        // Files are in decode order; with B-frames that isn't display order.
        out.sort_by_key(|p| p.0);
        Ok(out)
    })();
    let _ = pipeline.set_state(gst::State::Null);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: u64 = 1_000_000_000 / 60;

    /// Feed `n` frames from `*t`, one frame apart.
    fn run(tally: &mut Tally, t: &mut u64, n: u32, still: bool) {
        for _ in 0..n {
            tally.frame(*t, still);
            *t += FRAME;
        }
    }

    #[test]
    fn even_motion_has_no_judder() {
        let mut tally = Tally::new(60.0);
        let mut t = 0;
        run(&mut tally, &mut t, 600, false);
        let totals = tally.finish();
        assert_eq!(totals.frames, 600);
        assert_eq!(totals.judder_percent(), 0.0);
        assert_eq!(tally.events().count(), 0);
    }

    #[test]
    fn short_freezes_are_repeats_and_long_ones_are_still_scenes() {
        let mut tally = Tally::new(60.0);
        let mut t = 0;
        run(&mut tally, &mut t, 100, false);
        run(&mut tally, &mut t, 3, true);
        run(&mut tally, &mut t, 100, false);
        // Two seconds of a menu.
        run(&mut tally, &mut t, 120, true);
        run(&mut tally, &mut t, 77, false);
        let totals = tally.finish();
        assert_eq!(totals.repeat_runs, 1);
        assert_eq!(totals.repeated, 3);
        assert_eq!(totals.longest_repeat, 3);
        assert_eq!(totals.still, 120);
        assert_eq!(
            tally.events().copied().collect::<Vec<_>>(),
            [Event::Repeat {
                at_ns: 99 * FRAME,
                frames: 3
            }]
        );
        assert!((totals.judder_percent() - 0.75).abs() < 1e-9, "{totals:?}");
    }

    #[test]
    fn missing_frames_are_holes_and_long_gaps_are_pauses() {
        let mut tally = Tally::new(60.0);
        let mut t = 0;
        run(&mut tally, &mut t, 50, false);
        // Two frames dropped.
        t += 2 * FRAME;
        run(&mut tally, &mut t, 50, false);
        // The game was minimized for five seconds.
        t += 5_000_000_000;
        run(&mut tally, &mut t, 98, false);
        let totals = tally.finish();
        assert_eq!((totals.holes, totals.missing, totals.pauses), (1, 2, 1));
        // A clip across the pause holds the frames around it, not the
        // five seconds' worth nobody drew.
        assert_eq!(tally.since(0).frames, 196);
        assert_eq!(
            tally.events().next(),
            Some(&Event::Hole {
                at_ns: 49 * FRAME,
                missing: 2
            })
        );
        assert!((totals.judder_percent() - 1.0).abs() < 1e-9, "{totals:?}");
    }

    #[test]
    fn a_freeze_at_the_end_counts_once_finished() {
        let mut tally = Tally::new(60.0);
        let mut t = 0;
        run(&mut tally, &mut t, 10, false);
        run(&mut tally, &mut t, 2, true);
        assert_eq!(tally.totals().repeated, 0);
        assert_eq!(tally.finish().repeated, 2);
    }

    #[test]
    fn since_counts_only_what_a_clip_would_contain() {
        let mut tally = Tally::new(60.0);
        let mut t = 0;
        run(&mut tally, &mut t, 60, false);
        run(&mut tally, &mut t, 4, true);
        run(&mut tally, &mut t, 60, false);
        let clip_start = t;
        run(&mut tally, &mut t, 60, false);
        t += FRAME;
        run(&mut tally, &mut t, 30, false);
        run(&mut tally, &mut t, 2, true);
        run(&mut tally, &mut t, 30, false);
        let clip = tally.since(clip_start);
        assert_eq!((clip.holes, clip.missing), (1, 1));
        assert_eq!((clip.repeat_runs, clip.repeated), (1, 2));
        assert_eq!(tally.totals().repeated, 6);
    }

    #[test]
    fn tiny_packets_between_normal_ones_are_repeats() {
        let mut monitor = PacketMonitor::new(60.0);
        let mut t = 0;
        let mut packet = |m: &mut PacketMonitor, size: usize, key: bool| {
            m.packet(t, size, key);
            t += FRAME;
        };
        for i in 0..300 {
            // A keyframe a second, like the recorder's encoders.
            let key = i % 60 == 0;
            let tiny = (200..203).contains(&i);
            let size = match (key, tiny) {
                (true, _) => 400_000,
                (_, true) => 1_500,
                // Real frames vary a lot; none comes near 4% of the median.
                _ => 60_000 + (i % 7) * 15_000,
            };
            packet(&mut monitor, size, key);
        }
        let totals = monitor.finish();
        assert_eq!((totals.repeat_runs, totals.repeated), (1, 3), "{totals:?}");
        assert_eq!(totals.still, 0);
    }

    #[test]
    fn freezes_count_only_with_clear_motion_around_them() {
        let inf = f64::INFINITY;
        // Fast motion, a two-frame freeze, fast motion again.
        let fast = [inf, 3.0, 3.0, 0.01, 0.02, 3.0, 3.0];
        assert_eq!(
            judge(&fast, 15),
            (vec![false, false, false, true, true, false, false], 0)
        );
        // A slow scene: frames dip under the threshold on their own.
        let slow = [inf, 0.2, 0.3, 0.05, 0.2, 0.08, 0.3];
        assert_eq!(judge(&slow, 15), (vec![false; 7], 2));
        // Motion on one side only (the clip starts or ends in the freeze).
        let edge = [inf, 0.01, 0.01, 3.0, 3.0, 0.02];
        assert_eq!(judge(&edge, 15), (vec![false; 6], 3));
        // A scene that stood still is left for the tally, slow or not.
        let menu = [inf, 0.2, 0.0, 0.0, 0.0, 0.0, 0.2];
        assert_eq!(
            judge(&menu, 3),
            (vec![false, false, true, true, true, true, false], 0)
        );
    }

    #[test]
    fn fps_comes_from_the_usual_step() {
        let pts = [0, FRAME, 2 * FRAME, 5 * FRAME, 6 * FRAME, 7 * FRAME];
        assert!((usual_fps(pts.into_iter()) - 60.0).abs() < 0.01);
        // Most frames dropped: still 60, so the drops count as holes.
        let pts = (0..40u64).map(|i| (i * 2 - u64::from(i % 4 == 0 && i > 0)) * FRAME);
        assert!((usual_fps(pts) - 60.0).abs() < 0.01);
    }

    #[test]
    fn identical_pictures_differ_by_nothing() {
        let a = vec![10u8; 100];
        let mut b = a.clone();
        assert_eq!(mean_difference(&a, &b), 0.0);
        b[0] = 110;
        assert_eq!(mean_difference(&a, &b), 1.0);
    }
}
