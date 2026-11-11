//! Always-on recorder: capture → encode → replay buffer, with save on demand.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
};

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use serde::{Deserialize, Serialize};

use crate::{
    audio::{AudioSource, add_aac_branch, add_audio_mix, add_audio_source},
    capture::{CapturePath, CaptureTarget, FRAME_QUEUE, VideoConfig, add_video_branch},
    encoder::Encoder,
    pacing::PacketMonitor,
    ring::{ReplayBuffer, running_time_buffer},
    save::write_mp4,
    timer::PreciseTimer,
};
#[cfg(target_os = "linux")]
use crate::{capture::PipewireTarget, kwin::KwinStream, portal::ScreenSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderConfig {
    pub video: VideoConfig,
    /// `None` probes for the best working encoder.
    pub encoder: Option<Encoder>,
    pub replay_seconds: u32,
    pub audio: Vec<AudioSource>,
    /// AAC bitrate per audio track, bits/s.
    #[serde(default = "default_audio_bitrate")]
    pub audio_bitrate: i32,
    /// Draw the mouse cursor into the video.
    #[serde(default = "yes")]
    pub cursor: bool,
    /// Try the zero-copy (GPU memory) capture path before the copy path.
    /// Default: on where it is the native path (Windows D3D11), off on
    /// Linux, where KWin's frame delivery was measured to be the bottleneck
    /// and the GL import path did no better.
    #[serde(default = "zero_copy_default")]
    pub zero_copy: bool,
    /// Windows: on an HDR display, capture through Desktop Duplication
    /// with tone mapping (the game window's area of the screen) so HDR
    /// games look right in clips. Off: window capture, HDR highlights clip.
    #[serde(default)]
    pub hdr_compat: bool,
    /// Windows, with `hdr_compat`: try capturing the game window itself in
    /// HDR first (experimental), before the screen-area capture.
    #[serde(default)]
    pub hdr_window: bool,
    /// Apps the user said aren't games; never recorded.
    #[serde(default)]
    pub ignored: Vec<crate::game::IgnoredGame>,
}

fn zero_copy_default() -> bool {
    cfg!(windows)
}

/// A saved clip: its file and the marks inside it, in seconds from its
/// start.
#[derive(Debug, Clone)]
pub struct SavedClip {
    pub path: PathBuf,
    pub markers: Vec<f64>,
    /// Its first audio track is the sources mixed together (game and mic),
    /// the one players play; the sources' own tracks follow.
    pub mixed_audio: bool,
}

/// The marks within `start ..= start + duration`, in seconds from `start`,
/// in order.
fn markers_in(
    marks: &[gst::ClockTime],
    start: gst::ClockTime,
    duration: gst::ClockTime,
) -> Vec<f64> {
    let mut out: Vec<f64> = marks
        .iter()
        .filter(|&&m| m >= start && m <= start + duration)
        .map(|&m| (m - start).seconds_f64())
        .collect();
    out.sort_by(f64::total_cmp);
    out
}

/// When to restart a recording that failed, shared by every platform's
/// auto-recorder: the first retry at once, then backing off, with the
/// count reset by a recording that ran a good while.
#[derive(Debug, Default)]
pub struct RestartPolicy {
    failures: u32,
    retry_at: Option<std::time::Instant>,
    /// Why the last attempt failed, for the status shown to the user.
    reason: Option<String>,
}

/// A game whose recording failed and is waiting to be restarted: shown in
/// place of "waiting for a game", which would hide that capture is broken.
#[derive(Debug, Clone)]
pub struct Retrying {
    pub game: crate::game::Game,
    /// The last failure, as logged.
    pub reason: String,
    /// How long until the next attempt.
    pub retry_in: std::time::Duration,
}

/// A recording that ran this long before failing was healthy: its failure
/// starts the back-off afresh.
pub const HEALTHY_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

impl RestartPolicy {
    /// Count a failure (of a recording that ran `healthy_for`, or of a
    /// start: zero) and return how long to wait before the next attempt.
    pub fn failed(&mut self, healthy_for: std::time::Duration) -> std::time::Duration {
        if healthy_for >= HEALTHY_AFTER {
            self.failures = 0;
        }
        self.failures += 1;
        let delay = restart_backoff(self.failures);
        self.retry_at = Some(std::time::Instant::now() + delay);
        delay
    }

    /// Remember why the last attempt failed.
    pub fn set_reason(&mut self, reason: impl Into<String>) {
        self.reason = Some(reason.into());
    }

    /// Why the last attempt failed, while failures are being counted.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Time left before the next attempt is due.
    pub fn retry_in(&self) -> std::time::Duration {
        self.retry_at
            .map(|t| t.saturating_duration_since(std::time::Instant::now()))
            .unwrap_or_default()
    }

    /// Whether an attempt may be made now.
    pub fn due(&self) -> bool {
        self.retry_at.is_none_or(|t| std::time::Instant::now() >= t)
    }

    /// Failures in a row so far.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Forget the failures (the game closed).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How long to wait before restart attempt `failures` (1-based) of a
/// recording that keeps failing: the first at once (the next scan), then
/// 5 s, 15 s, 30 s, and a minute from then on.
pub fn restart_backoff(failures: u32) -> std::time::Duration {
    let secs = match failures {
        0 | 1 => 0,
        2 => 5,
        3 => 15,
        4 => 30,
        _ => 60,
    };
    std::time::Duration::from_secs(secs)
}

/// A newest frame older than this at save time gets a log line.
const STALE: gst::ClockTime = gst::ClockTime::from_seconds(2);

/// How long the zero-copy pipeline gets to deliver a first frame before it
/// is judged not to work on this machine.
const ZERO_COPY_PROBE: std::time::Duration = std::time::Duration::from_secs(4);

fn default_audio_bitrate() -> i32 {
    160_000
}

fn yes() -> bool {
    true
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            video: VideoConfig::default(),
            encoder: None,
            replay_seconds: 30,
            audio: vec![AudioSource::Desktop(None)],
            audio_bitrate: default_audio_bitrate(),
            cursor: true,
            zero_copy: zero_copy_default(),
            hdr_compat: false,
            hdr_window: false,
            ignored: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub encoder: Encoder,
    /// Which capture path is in use.
    pub path: CapturePath,
    pub buffered_seconds: f64,
    pub replay_seconds: u32,
    pub error: Option<String>,
}

/// What is being captured. Keeps the underlying session alive.
pub enum CaptureSource {
    /// A screen or window picked through the xdg portal.
    #[cfg(target_os = "linux")]
    Portal(ScreenSource),
    /// A window streamed by KWin directly (no picker).
    #[cfg(target_os = "linux")]
    Kwin(KwinStream),
    /// The game's window, through Windows Graphics Capture.
    #[cfg(windows)]
    Screen(crate::win::Target),
}

impl CaptureSource {
    fn target(&self) -> CaptureTarget {
        match self {
            #[cfg(target_os = "linux")]
            CaptureSource::Portal(s) => CaptureTarget::Pipewire(s.target()),
            #[cfg(target_os = "linux")]
            CaptureSource::Kwin(s) => {
                CaptureTarget::Pipewire(PipewireTarget::Serial(s.pipewire_serial()))
            }
            #[cfg(windows)]
            CaptureSource::Screen(t) => match (t.hdr_window, t.hdr_monitor) {
                (Some(hdr), _) => CaptureTarget::HdrWindow {
                    hwnd: t.window,
                    cursor: t.cursor,
                    hdr,
                },
                (None, Some(monitor)) => CaptureTarget::HdrRegion {
                    hwnd: t.window,
                    monitor,
                    cursor: t.cursor,
                },
                (None, None) => CaptureTarget::Window {
                    hwnd: t.window,
                    cursor: t.cursor,
                },
            },
        }
    }

    async fn close(self) -> Result<()> {
        match self {
            #[cfg(target_os = "linux")]
            CaptureSource::Portal(s) => s.close().await,
            #[cfg(target_os = "linux")]
            CaptureSource::Kwin(_) => Ok(()),
            #[cfg(windows)]
            CaptureSource::Screen(_) => Ok(()),
        }
    }
}

pub struct Recorder {
    pipeline: gst::Pipeline,
    buffer: Arc<Mutex<ReplayBuffer>>,
    /// Moments the user marked, on the recording's running-time timeline.
    marks: Mutex<Vec<gst::ClockTime>>,
    encoder: Encoder,
    path: CapturePath,
    replay_seconds: u32,
    /// Audio track names ("desktop", "mic"), in buffer order, for the log.
    audio_names: Vec<&'static str>,
    error: Arc<Mutex<Option<String>>>,
    pacing: Arc<Pacing>,
    /// Frame-accurate sleeps for the capture source, while this lives.
    _timer: PreciseTimer,
    stop: Arc<AtomicBool>,
    bus_thread: Option<JoinHandle<()>>,
    // Dropped last: pipewiresrc borrows its fd.
    source: Option<CaptureSource>,
}

impl Recorder {
    /// Start buffering `source`.
    pub async fn start(cfg: &RecorderConfig, source: CaptureSource) -> Result<Self> {
        let encoder = match cfg.encoder {
            Some(e) => e,
            None => {
                let codec = cfg.video.codec;
                tokio::task::spawn_blocking(move || Encoder::probe(codec))
                    .await?
                    .with_context(|| format!("no working {codec:?} encoder found"))?
            }
        };

        let mut candidates = Vec::new();
        if cfg.zero_copy && encoder.gpu_memory().is_some() {
            candidates.push(CapturePath::ZeroCopy);
        }
        candidates.push(CapturePath::Copy);

        let target = source.target();
        let mut last_err = None;
        for (i, path) in candidates.iter().copied().enumerate() {
            let last = i + 1 == candidates.len();
            let built = match Pipeline::build(cfg, encoder, target, path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(?path, "can't build capture pipeline: {e:#}");
                    last_err = Some(e);
                    continue;
                }
            };
            if let Err(e) = built
                .pipeline
                .set_state(gst::State::Playing)
                .context("starting capture pipeline")
            {
                built.teardown().await;
                last_err = Some(e);
                continue;
            }
            // The copy path is the known-good one; only the zero-copy path
            // has to prove itself with a first frame.
            if !last && !built.probe(ZERO_COPY_PROBE).await {
                let why = built
                    .error
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "no frames arrived".into());
                tracing::warn!(?path, "zero-copy capture failed, falling back: {why}");
                built.teardown().await;
                last_err = Some(anyhow::anyhow!(why));
                continue;
            }
            tracing::info!(
                ?encoder,
                ?path,
                codec = ?cfg.video.codec,
                fps = cfg.video.fps,
                bitrate_kbps = cfg.video.bitrate_kbps,
                replay_seconds = cfg.replay_seconds,
                audio = ?cfg.audio,
                "recorder started"
            );
            return Ok(Self {
                pipeline: built.pipeline,
                buffer: built.buffer,
                marks: Mutex::new(Vec::new()),
                encoder,
                path,
                replay_seconds: cfg.replay_seconds,
                audio_names: audio_track_names(&cfg.audio),
                error: built.error,
                pacing: built.pacing,
                _timer: built.timer,
                stop: built.stop,
                bus_thread: Some(built.bus_thread),
                source: Some(source),
            });
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no capture path available")))
    }

    pub fn status(&self) -> Status {
        Status {
            encoder: self.encoder,
            path: self.path,
            buffered_seconds: self.buffer.lock().unwrap().buffered().seconds_f64(),
            replay_seconds: self.replay_seconds,
            error: self.error.lock().unwrap().clone(),
        }
    }

    /// Token to persist so the next start skips the screen picker.
    pub fn restore_token(&self) -> Option<&str> {
        match self.source.as_ref()? {
            #[cfg(target_os = "linux")]
            CaptureSource::Portal(s) => s.restore_token(),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    /// Change the replay length of the running recording. Shorter takes
    /// effect at once; longer fills as the game goes on.
    pub fn set_replay_seconds(&mut self, seconds: u32) {
        if seconds == self.replay_seconds {
            return;
        }
        self.replay_seconds = seconds;
        self.buffer
            .lock()
            .unwrap()
            .set_window(gst::ClockTime::from_seconds(seconds.into()));
        tracing::info!(seconds, "replay length changed");
    }

    /// The error that stopped capture, if the pipeline failed after it
    /// started (the replay buffer then holds nothing newer than the
    /// failure).
    pub fn failure(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    /// How long ago the newest buffered frame was captured. Long when the
    /// game stops drawing (minimized), or capture failed.
    pub fn frame_age(&self) -> Option<gst::ClockTime> {
        let now = self.pipeline.current_running_time()?;
        let newest = self.buffer.lock().unwrap().newest_video_end()?;
        Some(now.saturating_sub(newest))
    }

    /// Mark this moment (a marker hotkey press). Returns how far into the
    /// replay buffer it is, in seconds. Marks older than the buffer are
    /// forgotten.
    pub fn mark(&self) -> Result<f64> {
        let now = self
            .pipeline
            .current_running_time()
            .context("the recording hasn't started")?;
        let (oldest, window) = {
            let b = self.buffer.lock().unwrap();
            (b.video.oldest_pts(), b.window())
        };
        let mut marks = self.marks.lock().unwrap();
        let floor = oldest.unwrap_or(now).min(now.saturating_sub(window));
        marks.retain(|&m| m >= floor);
        marks.push(now);
        Ok(now.saturating_sub(oldest.unwrap_or(now)).seconds_f64())
    }

    /// Save the last `seconds` (default: the whole buffer) to `path`, with
    /// the marks that fall inside it.
    pub async fn save(&self, seconds: Option<u32>, path: &Path) -> Result<SavedClip> {
        if let Some(age) = self.frame_age()
            && age > STALE
        {
            tracing::warn!(
                %age,
                failure = self.failure().as_deref().unwrap_or("none"),
                "saving footage that ends this long ago: no new frames since \
                 (window minimized, game not drawing, or capture failed)"
            );
        }
        let duration = seconds
            .map(|s| gst::ClockTime::from_seconds(s.into()))
            .unwrap_or_else(|| self.buffer.lock().unwrap().window());
        let (clip, audio_stats) = {
            let buffer = self.buffer.lock().unwrap();
            let stats: Vec<_> = buffer.audio.iter().map(|t| t.stats()).collect();
            (buffer.snapshot(duration)?, stats)
        };
        self.log_audio(&audio_stats, Some((clip.start, clip.start + clip.duration)));
        self.pacing.log_clip(clip.start);
        // The recording so far, too: one that ends without a clean stop
        // (the app quit mid-game) would otherwise leave no totals.
        self.log_pacing();
        let markers = markers_in(&self.marks.lock().unwrap(), clip.start, clip.duration);
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            write_mp4(&clip, &path)?;
            tracing::info!(
                path = %path.display(),
                duration = %clip.duration,
                markers = markers.len(),
                "saved clip"
            );
            Ok(SavedClip {
                path,
                markers,
                mixed_audio: clip.mixed,
            })
        })
        .await?
    }

    /// Write the newest buffered frame to `path` as PNG.
    pub async fn screenshot(&self, path: &Path) -> Result<PathBuf> {
        let gop = self.buffer.lock().unwrap().latest_gop()?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::screenshot::write_png(&gop, &path)?;
            tracing::info!(path = %path.display(), "saved screenshot");
            Ok(path)
        })
        .await?
    }

    /// The recording so far: what `videorate` did to the source's frames,
    /// and the repeats and holes it can't see.
    fn log_pacing(&self) {
        if let Some(rate) = self.pipeline.by_name("rate") {
            let frames_in = rate.property::<u64>("in");
            // What the source really delivers, against the rate asked for.
            let source_fps = self
                .pipeline
                .current_running_time()
                .filter(|t| !t.is_zero())
                .map(|t| frames_in as f64 / t.seconds_f64());
            tracing::info!(
                frames_in,
                out = rate.property::<u64>("out"),
                duplicated = rate.property::<u64>("duplicate"),
                dropped = rate.property::<u64>("drop"),
                source_fps = source_fps.map(|f| (f * 100.0).round() / 100.0),
                clock = ?self.pipeline.clock().map(|c| c.name()),
                "video pacing"
            );
        }
        self.pacing.log_session();
    }

    /// One line per audio track: what arrived and what is buffered. With
    /// a clip's running-time range, a track that won't be in the clip is a
    /// warning that says why (it is otherwise left out silently).
    fn log_audio(
        &self,
        stats: &[crate::ring::TrackStats],
        clip: Option<(gst::ClockTime, gst::ClockTime)>,
    ) {
        for (name, s) in self.audio_names.iter().zip(stats) {
            match clip.and_then(|(start, end)| s.missing_reason(start, end)) {
                Some(why) => tracing::warn!(
                    track = name,
                    received = s.received,
                    buffered = s.buffered,
                    "audio track left out of the clip: {why}"
                ),
                None => tracing::info!(
                    track = name,
                    received = s.received,
                    no_pts = s.no_pts,
                    buffered = s.buffered,
                    oldest = ?s.oldest,
                    newest = ?s.newest,
                    "audio"
                ),
            }
        }
    }

    pub async fn stop(mut self) -> Result<()> {
        let stats: Vec<_> = self
            .buffer
            .lock()
            .unwrap()
            .audio
            .iter()
            .map(|t| t.stats())
            .collect();
        self.log_audio(&stats, None);
        self.log_pacing();
        self.pipeline.set_state(gst::State::Null)?;
        release_capture(&self.pipeline);
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.bus_thread.take() {
            let _ = tokio::task::spawn_blocking(move || t.join()).await;
        }
        match self.source.take() {
            Some(s) => s.close().await,
            None => Ok(()),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        release_capture(&self.pipeline);
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The audio tracks' names in buffer (and clip) order: "mix" first when
/// there is more than one source, as `Pipeline::build` lays them out.
fn audio_track_names(sources: &[AudioSource]) -> Vec<&'static str> {
    let mix = (sources.len() > 1).then_some("mix");
    mix.into_iter()
        .chain(sources.iter().map(AudioSource::name))
        .collect()
}

/// Stop capture that lives outside GStreamer's own elements (Windows' HDR
/// window capture runs a Windows Graphics Capture session with FP16 frames
/// on the GPU) as soon as the pipeline stops, rather than whenever the last
/// reference to its element goes: a failed attempt that left one running
/// kept costing GPU memory, and after a few retries NVENC couldn't open a
/// session at all.
fn release_capture(pipeline: &gst::Pipeline) {
    #[cfg(windows)]
    if let Some(src) = pipeline.by_name("screen") {
        crate::win::hdr_window::stop(&src);
    }
    #[cfg(not(windows))]
    let _ = pipeline;
}

/// What `videorate`'s counters can't see (see [`crate::pacing`]): pictures
/// the source repeated, and frames lost after the rate was fixed. Counted
/// from the encoded packets and the frame queue; no frame is looked at.
struct Pacing {
    packets: Mutex<PacketMonitor>,
    /// Times the frame queue was full when a frame arrived; each one drops
    /// its oldest frame, which leaves a hole in the timeline.
    queue_overruns: AtomicU64,
}

impl Pacing {
    fn new(fps: u32) -> Self {
        Self {
            packets: Mutex::new(PacketMonitor::new(f64::from(fps))),
            queue_overruns: AtomicU64::new(0),
        }
    }

    /// A clip that starts at `start`: the repeats and holes inside it.
    fn log_clip(&self, start: gst::ClockTime) {
        let clip = self.packets.lock().unwrap().tally().since(start.nseconds());
        tracing::info!(
            judder_percent = (clip.judder_percent() * 100.0).round() / 100.0,
            repeated = clip.repeated,
            freezes = clip.repeat_runs,
            longest_freeze = clip.longest_repeat,
            missing = clip.missing,
            holes = clip.holes,
            "clip pacing"
        );
    }

    /// The whole recording, when it stops.
    fn log_session(&self) {
        let all = self.packets.lock().unwrap().tally().totals();
        tracing::info!(
            judder_percent = (all.judder_percent() * 100.0).round() / 100.0,
            frames = all.frames,
            repeated = all.repeated,
            freezes = all.repeat_runs,
            longest_freeze = all.longest_repeat,
            missing = all.missing,
            holes = all.holes,
            still = all.still,
            pauses = all.pauses,
            queue_overruns = self.queue_overruns.load(Ordering::Relaxed),
            "frame pacing"
        );
    }
}

/// A built capture pipeline that hasn't proven itself yet.
struct Pipeline {
    pipeline: gst::Pipeline,
    buffer: Arc<Mutex<ReplayBuffer>>,
    error: Arc<Mutex<Option<String>>>,
    pacing: Arc<Pacing>,
    timer: PreciseTimer,
    stop: Arc<AtomicBool>,
    bus_thread: JoinHandle<()>,
}

impl Pipeline {
    fn build(
        cfg: &RecorderConfig,
        encoder: Encoder,
        target: CaptureTarget,
        path: CapturePath,
    ) -> Result<Self> {
        let window = gst::ClockTime::from_seconds(cfg.replay_seconds.into());
        // Before the source starts: it paces itself by sleeping.
        let timer = PreciseTimer::hold();
        let pipeline = gst::Pipeline::new();
        // Audio tracks in clip order: with more than one source, all of them
        // mixed into one first (players only play the first audio track),
        // then each source's own.
        let tees = cfg
            .audio
            .iter()
            .map(|src| add_audio_source(&pipeline, src))
            .collect::<Result<Vec<_>>>()?;
        let mixed = tees.len() > 1;
        let mut tracks = Vec::new();
        if mixed {
            tracks.push(("mix", add_audio_mix(&pipeline, &tees, cfg.audio_bitrate)?));
        }
        for (src, tee) in cfg.audio.iter().zip(&tees) {
            tracks.push((
                src.name(),
                add_aac_branch(&pipeline, tee, cfg.audio_bitrate)?,
            ));
        }
        let buffer = ReplayBuffer::new(window, tracks.len());
        let buffer = Arc::new(Mutex::new(if mixed { buffer.with_mix() } else { buffer }));

        let parse = add_video_branch(&pipeline, target, encoder, &cfg.video, path)?;
        let pacing = Arc::new(Pacing::new(cfg.video.fps));
        if let Some(queue) = pipeline.by_name(FRAME_QUEUE) {
            let pacing = pacing.clone();
            queue.connect("overrun", false, move |_| {
                pacing.queue_overruns.fetch_add(1, Ordering::Relaxed);
                None
            });
        }
        let buf = buffer.clone();
        let mut keyframes = KeyframeGuard::default();
        let packets = pacing.clone();
        link_appsink(&pipeline, &parse, "video-sink", move |caps, buffer| {
            let keyframe = !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
            let ask = buffer.pts().is_some_and(|pts| {
                packets
                    .packets
                    .lock()
                    .unwrap()
                    .packet(pts.nseconds(), buffer.size(), keyframe);
                keyframes.on_packet(pts, keyframe)
            });
            let mut b = buf.lock().unwrap();
            if let Some(caps) = caps {
                b.video.set_caps(caps);
            }
            b.push_video(buffer);
            ask
        })?;

        for (i, (name, parse)) in tracks.into_iter().enumerate() {
            let buf = buffer.clone();
            link_appsink(
                &pipeline,
                &parse,
                &format!("{name}-sink"),
                move |caps, buffer| {
                    let mut b = buf.lock().unwrap();
                    if let Some(caps) = caps {
                        b.audio[i].set_caps(caps);
                    }
                    b.push_audio(i, buffer);
                    false
                },
            )?;
        }

        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let bus_thread = spawn_bus_watch(&pipeline, error.clone(), stop.clone())?;
        Ok(Self {
            pipeline,
            buffer,
            error,
            pacing,
            timer,
            stop,
            bus_thread,
        })
    }

    /// True once a video frame has been buffered without an error; false if
    /// an error is posted or nothing arrives within `timeout`.
    async fn probe(&self, timeout: std::time::Duration) -> bool {
        let started = std::time::Instant::now();
        while started.elapsed() < timeout {
            if self.error.lock().unwrap().is_some() {
                return false;
            }
            if self.buffer.lock().unwrap().video.oldest_pts().is_some() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        false
    }

    async fn teardown(self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        release_capture(&self.pipeline);
        self.stop.store(true, Ordering::Relaxed);
        let _ = tokio::task::spawn_blocking(move || self.bus_thread.join()).await;
    }
}

/// Terminate `tail` in an appsink that hands each buffer to `on_buffer`,
/// along with the caps whenever they change.
/// Link `tail` to an appsink that hands every encoded packet (with running-
/// time timestamps, and the caps when they change) to `on_buffer`. When
/// `on_buffer` returns true, a keyframe is requested from the encoder.
fn link_appsink(
    pipeline: &gst::Pipeline,
    tail: &gst::Element,
    name: &str,
    mut on_buffer: impl FnMut(Option<gst::Caps>, gst::Buffer) -> bool + Send + 'static,
) -> Result<()> {
    let mut last_caps: Option<gst::Caps> = None;
    let sink = gst_app::AppSink::builder()
        .name(name)
        // Pull as fast as possible; the ring keeps capture timestamps.
        .sync(false)
        .callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let Some(buffer) = running_time_buffer(&sample) else {
                        return Ok(gst::FlowSuccess::Ok);
                    };
                    let caps = sample
                        .caps_owned()
                        .filter(|c| last_caps.as_ref() != Some(c));
                    if let Some(c) = &caps {
                        last_caps = Some(c.clone());
                    }
                    if on_buffer(caps, buffer) {
                        // Travels upstream through the parser to the encoder.
                        sink.send_event(
                            gstreamer_video::UpstreamForceKeyUnitEvent::builder()
                                .all_headers(true)
                                .build(),
                        );
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        )
        .build();
    pipeline.add(&sink)?;
    tail.link(&sink)?;
    Ok(())
}

/// The replay buffer can only trim, and a clip can only start, at a
/// keyframe. An encoder that stops sending them (one was seen to on
/// Windows: a clip came out 78 s long with 30 s asked for) would make the
/// buffer, and memory, grow without bound. This watches the packets and
/// asks for a keyframe when none has come for [`MAX_KEYFRAME_GAP`].
#[derive(Debug, Default)]
struct KeyframeGuard {
    /// Newest keyframe, or the first packet while none has been seen.
    since: Option<gst::ClockTime>,
    /// When a keyframe was last asked for and hasn't come yet.
    asked: Option<gst::ClockTime>,
    warned: bool,
}

/// Longest stretch without a keyframe before one is requested. Encoders
/// are set to one a second; this only fires when one ignores that.
const MAX_KEYFRAME_GAP: gst::ClockTime = gst::ClockTime::from_seconds(2);

impl KeyframeGuard {
    /// Note a video packet; true when a keyframe should be requested now.
    fn on_packet(&mut self, pts: gst::ClockTime, keyframe: bool) -> bool {
        if keyframe {
            self.since = Some(pts);
            self.asked = None;
            return false;
        }
        let since = *self.since.get_or_insert(pts);
        if pts.saturating_sub(since) < MAX_KEYFRAME_GAP {
            return false;
        }
        // Give a request a second to be answered before repeating it.
        if self
            .asked
            .is_some_and(|a| pts.saturating_sub(a) < gst::ClockTime::from_seconds(1))
        {
            return false;
        }
        if !self.warned {
            self.warned = true;
            tracing::warn!(
                gap = %pts.saturating_sub(since),
                "the encoder sent no keyframe for this long; asking for them \
                 (the replay buffer can only trim at keyframes)"
            );
        }
        self.asked = Some(pts);
        true
    }
}

fn spawn_bus_watch(
    pipeline: &gst::Pipeline,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let bus = pipeline.bus().context("pipeline has no bus")?;
    Ok(std::thread::Builder::new()
        .name("reruns-bus".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) else {
                    continue;
                };
                match msg.view() {
                    gst::MessageView::Error(e) => {
                        let text = format!(
                            "{} from {:?}: {:?}",
                            e.error(),
                            msg.src().map(|s| s.path_string()),
                            e.debug()
                        );
                        tracing::error!("{text}");
                        // Keep the first: later errors are knock-ons (an
                        // encoder that won't open shows up again as a
                        // "not-negotiated" upstream), and the restart logic
                        // needs the cause to know what to blame.
                        error.lock().unwrap().get_or_insert(text);
                    }
                    gst::MessageView::Warning(w) => {
                        tracing::warn!(
                            "{} from {:?}",
                            w.error(),
                            msg.src().map(|s| s.path_string())
                        );
                    }
                    _ => {}
                }
            }
        })?)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn s(secs: f64) -> gst::ClockTime {
        gst::ClockTime::from_nseconds((secs * 1e9) as u64)
    }

    #[test]
    fn restart_policy_remembers_why_and_when() {
        use std::time::Duration;
        let mut p = RestartPolicy::default();
        assert_eq!(p.reason(), None);
        p.failed(Duration::from_secs(3));
        p.set_reason("Failed to open session");
        p.failed(Duration::ZERO);
        assert_eq!(p.reason(), Some("Failed to open session"));
        // The second failure in a row waits 5 s.
        let left = p.retry_in();
        assert!(
            left > Duration::from_secs(4) && left <= Duration::from_secs(5),
            "{left:?}"
        );
        p.reset();
        assert_eq!(p.reason(), None);
        assert_eq!(p.retry_in(), Duration::ZERO);
    }

    #[test]
    fn restart_policy_backs_off_and_resets_after_a_healthy_run() {
        use std::time::Duration;
        let mut p = RestartPolicy::default();
        assert!(p.due());
        assert_eq!(p.failed(Duration::ZERO), Duration::ZERO);
        assert!(p.due(), "first retry at once");
        assert_eq!(p.failed(Duration::from_secs(3)), Duration::from_secs(5));
        assert!(!p.due(), "then it waits");
        assert_eq!(p.failed(Duration::ZERO), Duration::from_secs(15));
        assert_eq!(p.failures(), 3);
        // A recording that ran a minute starts over.
        assert_eq!(p.failed(HEALTHY_AFTER), Duration::ZERO);
        assert_eq!(p.failures(), 1);
        p.reset();
        assert!(p.due());
        assert_eq!(p.failures(), 0);
    }

    #[test]
    fn markers_inside_the_clip_are_kept_relative_to_its_start() {
        let marks = [s(5.0), s(12.5), s(20.0), s(31.0)];
        // A clip from 10 s, 20 s long: 12.5 and 20 fall inside.
        assert_eq!(markers_in(&marks, s(10.0), s(20.0)), [2.5, 10.0]);
        assert!(markers_in(&marks, s(40.0), s(5.0)).is_empty());
        // Inclusive at both ends.
        assert_eq!(markers_in(&marks, s(5.0), s(7.5)), [0.0, 7.5]);
    }

    #[test]
    fn restarts_back_off_and_level_out() {
        let d: Vec<u64> = (1..=7).map(|n| restart_backoff(n).as_secs()).collect();
        assert_eq!(d, [0, 5, 15, 30, 60, 60, 60]);
    }

    #[test]
    fn guard_asks_after_two_seconds_and_not_again_for_one() {
        let mut g = KeyframeGuard::default();
        assert!(!g.on_packet(s(0.0), true));
        assert!(!g.on_packet(s(1.9), false));
        assert!(g.on_packet(s(2.0), false), "2 s without a keyframe");
        assert!(!g.on_packet(s(2.5), false), "already asked");
        assert!(g.on_packet(s(3.0), false), "asks again after a second");
        assert!(!g.on_packet(s(3.1), true));
        assert!(!g.on_packet(s(4.5), false), "the keyframe reset the clock");
        // A stream that starts mid-GOP counts from its first packet.
        let mut g = KeyframeGuard::default();
        assert!(!g.on_packet(s(10.0), false));
        assert!(g.on_packet(s(12.0), false));
    }

    /// x264 with keyframes effectively off, through the real appsink path:
    /// with the guard asking, keyframes keep coming.
    fn keyframes_from_encoder_without_gop(ask: bool) -> usize {
        crate::init().unwrap();
        let pipeline = gst::parse::launch(
            "videotestsrc num-buffers=300 ! video/x-raw,width=160,height=120,framerate=30/1 \
             ! videoconvert ! x264enc name=enc key-int-max=100000 tune=zerolatency \
             speed-preset=ultrafast option-string=scenecut=0 ! h264parse name=parse",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let parse = pipeline.by_name("parse").unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let mut guard = KeyframeGuard::default();
        {
            let count = count.clone();
            link_appsink(&pipeline, &parse, "video-sink", move |_, buffer| {
                let key = !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
                if key {
                    count.fetch_add(1, Ordering::Relaxed);
                }
                let wants = guard.on_packet(buffer.pts().unwrap(), key);
                ask && wants
            })
            .unwrap();
        }
        pipeline.set_state(gst::State::Playing).unwrap();
        let msg = pipeline
            .bus()
            .unwrap()
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(30),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .expect("encoding timed out");
        assert!(matches!(msg.view(), gst::MessageView::Eos(_)), "{msg:?}");
        pipeline.set_state(gst::State::Null).unwrap();
        count.load(Ordering::Relaxed)
    }

    #[test]
    fn guard_gets_keyframes_from_an_encoder_that_stops_sending_them() {
        assert_eq!(keyframes_from_encoder_without_gop(false), 1, "baseline");
        // 10 s of video: the first keyframe, then one about every 2 s.
        let n = keyframes_from_encoder_without_gop(true);
        assert!((4..=6).contains(&n), "{n} keyframes");
    }
}
