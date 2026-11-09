//! In-memory instant-replay buffer of encoded packets.
//!
//! Each track keeps a window of encoded buffers (`gst::Buffer`s are
//! refcounted, so snapshots share memory with the ring). Eviction only ever
//! cuts at a keyframe, so the oldest retained packet can always start a file.
//! Audio packets are all keyframes, so the same logic serves both.

use std::collections::VecDeque;

use anyhow::{Context, Result};
use gstreamer as gst;

/// One elementary stream in the buffer.
#[derive(Debug, Default)]
pub struct Track {
    caps: Option<gst::Caps>,
    packets: VecDeque<gst::Buffer>,
    /// Every packet ever offered, kept or not: tells "nothing arrived"
    /// apart from "arrived but evicted" when a track comes out empty.
    received: u64,
    /// Of those, packets dropped for having no timestamp.
    no_pts: u64,
}

impl Track {
    pub fn set_caps(&mut self, caps: gst::Caps) {
        self.caps = Some(caps);
    }

    /// Append a packet. Packets without a PTS can't be placed and are dropped.
    pub fn push(&mut self, buffer: gst::Buffer) {
        self.received += 1;
        if buffer.pts().is_some() {
            self.packets.push_back(buffer);
        } else {
            self.no_pts += 1;
        }
    }

    /// What the track holds now, for the log.
    pub fn stats(&self) -> TrackStats {
        TrackStats {
            received: self.received,
            no_pts: self.no_pts,
            buffered: self.packets.len(),
            oldest: self.oldest_pts(),
            newest: self.newest_end(),
            has_caps: self.caps.is_some(),
        }
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    fn newest_end(&self) -> Option<gst::ClockTime> {
        let last = self.packets.back()?;
        Some(last.pts()? + last.duration().unwrap_or(gst::ClockTime::ZERO))
    }

    pub fn oldest_pts(&self) -> Option<gst::ClockTime> {
        self.packets.front()?.pts()
    }

    /// Index of the last keyframe at or before `t`, falling back to the first
    /// keyframe if every keyframe is later than `t`.
    fn keyframe_at_or_before(&self, t: gst::ClockTime) -> Option<usize> {
        let mut found = None;
        for (i, p) in self.packets.iter().enumerate() {
            if !is_keyframe(p) {
                continue;
            }
            if p.pts()? <= t || found.is_none() {
                found = Some(i);
            }
            if p.pts()? > t {
                break;
            }
        }
        found
    }

    /// Drop packets that start before `t`.
    pub fn evict_before(&mut self, t: gst::ClockTime) {
        let keep = self
            .packets
            .iter()
            .position(|p| p.pts().is_some_and(|pts| pts >= t))
            .unwrap_or(self.packets.len());
        self.packets.drain(..keep);
    }

    /// Drop packets so that roughly `window` of history remains, cutting only
    /// at a keyframe. At least `window` is kept whenever it's available.
    pub fn evict(&mut self, window: gst::ClockTime) {
        let Some(end) = self.newest_end() else { return };
        let Some(cutoff) = end.checked_sub(window) else {
            return;
        };
        // Only cut at a keyframe that is itself at or before the cutoff;
        // never fall back to a later one, which would shrink the window.
        let cut = self
            .packets
            .iter()
            .take_while(|p| p.pts().is_some_and(|pts| pts <= cutoff))
            .enumerate()
            .filter(|(_, p)| is_keyframe(p))
            .map(|(i, _)| i)
            .last();
        if let Some(cut) = cut {
            self.packets.drain(..cut);
        }
    }
}

/// The sample's buffer with PTS/DTS converted to running time, so tracks from
/// different branches share one timeline. Raw PTS can't be compared: e.g.
/// x264enc offsets its output by 1000h and compensates in the segment.
pub fn running_time_buffer(sample: &gst::Sample) -> Option<gst::Buffer> {
    let mut buffer = sample.buffer_owned()?;
    let Some(segment) = sample
        .segment()
        .and_then(|s| s.downcast_ref::<gst::ClockTime>())
    else {
        return Some(buffer);
    };
    let pts = segment.to_running_time(buffer.pts());
    let dts = segment.to_running_time(buffer.dts()).or(pts);
    let b = buffer.make_mut();
    b.set_pts(pts);
    b.set_dts(dts);
    Some(buffer)
}

fn is_keyframe(buffer: &gst::BufferRef) -> bool {
    !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT)
}

/// A track's state for the log: how much arrived, how much is kept, and
/// the running-time range it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackStats {
    pub received: u64,
    pub no_pts: u64,
    pub buffered: usize,
    pub oldest: Option<gst::ClockTime>,
    pub newest: Option<gst::ClockTime>,
    pub has_caps: bool,
}

impl TrackStats {
    /// Why a track with these stats would be left out of a clip covering
    /// `start..end`, or `None` if it would be in it.
    pub fn missing_reason(&self, start: gst::ClockTime, end: gst::ClockTime) -> Option<String> {
        if self.received == 0 {
            return Some("no audio packets have arrived from the device".into());
        }
        if self.no_pts == self.received {
            return Some(format!("all {} packets had no timestamp", self.received));
        }
        if !self.has_caps {
            return Some("packets arrived without a format".into());
        }
        match (self.oldest, self.newest) {
            (Some(a), Some(b)) if b <= start || a >= end => Some(format!(
                "its timestamps ({a}..{b}) don't overlap the video's ({start}..{end})"
            )),
            (None, _) | (_, None) => Some(format!(
                "{} packets arrived but none are buffered: their timestamps fall \
                 before the oldest video frame ({start})",
                self.received
            )),
            _ => None,
        }
    }
}

/// How far past the requested start a keyframe may sit and still be used.
const SNAP_TOLERANCE: gst::ClockTime = gst::ClockTime::from_mseconds(1);

/// The replay buffer: one video track plus any number of audio tracks.
#[derive(Debug)]
pub struct ReplayBuffer {
    window: gst::ClockTime,
    pub video: Track,
    pub audio: Vec<Track>,
    /// `audio[0]` is every source mixed into one track (see
    /// [`ReplayBuffer::with_mix`]).
    mix_first: bool,
}

impl ReplayBuffer {
    pub fn new(window: gst::ClockTime, audio_tracks: usize) -> Self {
        Self {
            window,
            video: Track::default(),
            audio: (0..audio_tracks).map(|_| Track::default()).collect(),
            mix_first: false,
        }
    }

    /// Mark `audio[0]` as the sources mixed into one track, so clips can
    /// say whether they start with it ([`Clip::mixed`]).
    pub fn with_mix(mut self) -> Self {
        self.mix_first = true;
        self
    }

    pub fn window(&self) -> gst::ClockTime {
        self.window
    }

    /// Change how much history is kept. A shorter window drops the excess
    /// at once; a longer one fills as new frames arrive.
    pub fn set_window(&mut self, window: gst::ClockTime) {
        self.window = window;
        self.video.evict(window);
        if let Some(start) = self.video.oldest_pts() {
            for t in &mut self.audio {
                t.evict_before(start);
            }
        }
    }

    pub fn push_video(&mut self, buffer: gst::Buffer) {
        self.video.push(buffer);
        self.video.evict(self.window);
    }

    pub fn push_audio(&mut self, track: usize, buffer: gst::Buffer) {
        let video_start = self.video.oldest_pts();
        let window = self.window;
        let t = &mut self.audio[track];
        t.push(buffer);
        // Video keeps up to a GOP beyond the window (it cuts at keyframes),
        // so trim audio to match it rather than to the window, or clips
        // would start with a gap in the audio.
        match video_start {
            Some(start) => t.evict_before(start),
            None => t.evict(window),
        }
    }

    /// Where the newest buffered video ends (running time), if any.
    pub fn newest_video_end(&self) -> Option<gst::ClockTime> {
        self.video.newest_end()
    }

    /// How much video is currently buffered.
    pub fn buffered(&self) -> gst::ClockTime {
        match (self.video.oldest_pts(), self.video.newest_end()) {
            (Some(a), Some(b)) => b.saturating_sub(a),
            _ => gst::ClockTime::ZERO,
        }
    }

    /// The newest GOP (last keyframe up to the newest packet), video only:
    /// just enough to decode the most recent frame, for screenshots.
    pub fn latest_gop(&self) -> Result<Clip> {
        let mut clip = self.snapshot(gst::ClockTime::ZERO)?;
        clip.audio.clear();
        Ok(clip)
    }

    /// Copy out the last `duration` (or everything, if less is buffered) as a
    /// clip starting on a video keyframe, with all timestamps rebased to 0.
    /// Buffer memory is shared with the ring, not copied.
    pub fn snapshot(&self, duration: gst::ClockTime) -> Result<Clip> {
        let video = &self.video;
        let caps = video.caps.clone().context("no video received yet")?;
        let end = video.newest_end().context("no video received yet")?;
        // Timestamps carry rounding noise (1/30 s isn't a whole number of
        // nanoseconds), so a keyframe sitting a hair after the exact point
        // still counts; otherwise it would cost a whole GOP of extra footage.
        let want = end.saturating_sub(duration) + SNAP_TOLERANCE;
        let start_idx = video
            .keyframe_at_or_before(want)
            .context("no keyframe buffered yet")?;
        let start = video.packets[start_idx].pts().unwrap();

        let video_track = ClipTrack {
            caps,
            buffers: rebase(video.packets.range(start_idx..), start, end),
        };
        let mut mixed = false;
        let audio = self
            .audio
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let caps = t.caps.clone()?;
                let buffers = rebase(t.packets.iter(), start, end);
                if buffers.is_empty() {
                    return None;
                }
                mixed |= i == 0 && self.mix_first;
                Some(ClipTrack { caps, buffers })
            })
            .collect();

        Ok(Clip {
            video: video_track,
            audio,
            mixed,
            duration: end - start,
            start,
        })
    }
}

/// Packets with `start <= pts < end`, shifted so `start` becomes 0.
fn rebase<'a>(
    packets: impl Iterator<Item = &'a gst::Buffer>,
    start: gst::ClockTime,
    end: gst::ClockTime,
) -> Vec<gst::Buffer> {
    packets
        .filter(|p| p.pts().is_some_and(|pts| pts >= start && pts < end))
        .map(|p| {
            // Shallow copy: new metadata, shared memory.
            let mut b = p.copy();
            let bm = b.make_mut();
            bm.set_pts(p.pts().map(|t| t - start));
            bm.set_dts(p.dts().or(p.pts()).map(|t| t.saturating_sub(start)));
            b
        })
        .collect()
}

/// A self-contained snapshot ready to be muxed.
#[derive(Debug)]
pub struct Clip {
    pub video: ClipTrack,
    pub audio: Vec<ClipTrack>,
    /// `audio[0]` is the sources mixed into one track; the sources' own
    /// tracks follow it.
    pub mixed: bool,
    pub duration: gst::ClockTime,
    /// Where the clip began on the recording's (running-time) timeline,
    /// before its timestamps were rebased to 0.
    pub start: gst::ClockTime,
}

#[derive(Debug)]
pub struct ClipTrack {
    pub caps: gst::Caps,
    pub buffers: Vec<gst::Buffer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> gst::ClockTime {
        gst::ClockTime::from_mseconds(v)
    }

    fn packet(pts_ms: u64, dur_ms: u64, key: bool) -> gst::Buffer {
        let mut b = gst::Buffer::from_slice([0u8; 4]);
        let bm = b.get_mut().unwrap();
        bm.set_pts(ms(pts_ms));
        bm.set_dts(ms(pts_ms));
        bm.set_duration(ms(dur_ms));
        if !key {
            bm.set_flags(gst::BufferFlags::DELTA_UNIT);
        }
        b
    }

    /// 10 fps video with a keyframe every second, from `from_ms` to `to_ms`.
    fn fill_video(buf: &mut ReplayBuffer, from_ms: u64, to_ms: u64) {
        for t in (from_ms..to_ms).step_by(100) {
            buf.push_video(packet(t, 100, t % 1000 == 0));
        }
    }

    fn pts_ms(b: &gst::Buffer) -> u64 {
        b.pts().unwrap().mseconds()
    }

    /// A clip only says it starts with the mix when the mix track made it
    /// into the clip: with it missing, the first audio track is a source's
    /// own, and the editor and export must treat it as one.
    #[test]
    fn clip_starts_with_the_mix_only_if_it_has_one() {
        crate::init().unwrap();
        let caps = || gst::Caps::new_empty_simple("audio/mpeg");
        let mut buf = ReplayBuffer::new(ms(10_000), 2).with_mix();
        buf.video
            .set_caps(gst::Caps::new_empty_simple("video/x-h264"));
        fill_video(&mut buf, 0, 2000);
        buf.audio[1].set_caps(caps());
        for t in (0..2000).step_by(20) {
            buf.push_audio(1, packet(t, 20, true));
        }
        let clip = buf.snapshot(ms(10_000)).unwrap();
        assert_eq!(clip.audio.len(), 1);
        assert!(!clip.mixed, "no mix track: the first is a source's own");

        buf.audio[0].set_caps(caps());
        for t in (0..2000).step_by(20) {
            buf.push_audio(0, packet(t, 20, true));
        }
        let clip = buf.snapshot(ms(10_000)).unwrap();
        assert_eq!(clip.audio.len(), 2);
        assert!(clip.mixed);
        // A buffer that was never marked has no mix.
        let mut plain = ReplayBuffer::new(ms(10_000), 1);
        plain
            .video
            .set_caps(gst::Caps::new_empty_simple("video/x-h264"));
        fill_video(&mut plain, 0, 1000);
        plain.audio[0].set_caps(caps());
        plain.push_audio(0, packet(0, 20, true));
        assert!(!plain.snapshot(ms(10_000)).unwrap().mixed);
    }

    /// The three ways an audio track can look when a clip is saved: never
    /// fed, fed on a timeline before the video's (all evicted), or fine.
    #[test]
    fn explains_why_audio_is_missing_from_a_clip() {
        let mut buf = new_buf(5, 3);
        fill_video(&mut buf, 100_000, 110_000);
        // Track 1: audio stamped ~100 s earlier than the video.
        for t in (0..10_000).step_by(20) {
            buf.push_audio(1, packet(t, 20, true));
        }
        // Track 2: audio on the video's timeline.
        for t in (100_000..110_000).step_by(20) {
            buf.push_audio(2, packet(t, 20, true));
        }
        let clip = buf.snapshot(gst::ClockTime::from_seconds(5)).unwrap();
        let (start, end) = (clip.start, clip.start + clip.duration);
        let stats: Vec<_> = buf.audio.iter().map(|t| t.stats()).collect();

        let never = stats[0].missing_reason(start, end).unwrap();
        assert!(never.contains("no audio packets have arrived"), "{never}");

        assert_eq!(stats[1].received, 500);
        assert_eq!(stats[1].buffered, 0);
        let early = stats[1].missing_reason(start, end).unwrap();
        assert!(early.contains("before the oldest video frame"), "{early}");

        assert_eq!(stats[2].missing_reason(start, end), None);
        // Only the healthy track made it into the clip.
        assert_eq!(clip.audio.len(), 1);
    }

    fn new_buf(window_s: u64, audio: usize) -> ReplayBuffer {
        gst::init().unwrap();
        let mut buf = ReplayBuffer::new(gst::ClockTime::from_seconds(window_s), audio);
        buf.video
            .set_caps(gst::Caps::new_empty_simple("video/x-h264"));
        for t in &mut buf.audio {
            t.set_caps(gst::Caps::new_empty_simple("audio/mpeg"));
        }
        buf
    }

    #[test]
    fn evicts_whole_gops_and_keeps_window() {
        let mut buf = new_buf(3, 0);
        fill_video(&mut buf, 0, 10_000);
        // Newest end is 10.0s, cutoff 7.0s → oldest kept keyframe is 7.0s.
        assert_eq!(buf.video.oldest_pts(), Some(ms(7000)));
        assert!(is_keyframe(buf.video.packets.front().unwrap()));
        assert_eq!(buf.buffered(), ms(3000));
        assert_eq!(buf.newest_video_end(), Some(ms(10_000)));
    }

    #[test]
    fn never_shrinks_below_window() {
        let mut buf = new_buf(3, 0);
        // Ends at 10.5s → cutoff 7.5s; 8.0s would leave only 2.5s, keep 7.0s.
        fill_video(&mut buf, 0, 10_500);
        assert_eq!(buf.video.oldest_pts(), Some(ms(7000)));
        assert!(buf.buffered() >= ms(3000));
    }

    #[test]
    fn eviction_without_keyframe_keeps_everything() {
        let mut buf = new_buf(1, 0);
        for t in (0..5000).step_by(100) {
            buf.push_video(packet(t, 100, false));
        }
        assert_eq!(buf.video.len(), 50);
    }

    #[test]
    fn drops_packets_without_pts() {
        let mut buf = new_buf(3, 0);
        buf.push_video(gst::Buffer::from_slice([0u8; 4]));
        assert!(buf.video.is_empty());
    }

    #[test]
    fn snapshot_starts_on_keyframe_and_rebases() {
        let mut buf = new_buf(30, 1);
        fill_video(&mut buf, 5000, 20_000);
        for t in (5000..20_000).step_by(20) {
            buf.push_audio(0, packet(t, 20, true));
        }
        // Last 4.5s → wanted start 15.5s → keyframe 15.0s.
        let clip = buf.snapshot(ms(4500)).unwrap();
        let v = &clip.video.buffers;
        assert!(is_keyframe(&v[0]));
        assert_eq!(pts_ms(&v[0]), 0);
        assert_eq!(v[0].dts(), Some(gst::ClockTime::ZERO));
        assert_eq!(pts_ms(v.last().unwrap()), 4900);
        assert_eq!(v.len(), 50);
        assert_eq!(clip.duration, ms(5000));
        assert_eq!(
            clip.start,
            ms(15_000),
            "the clip began at the 15 s keyframe"
        );

        let a = &clip.audio[0].buffers;
        assert_eq!(pts_ms(&a[0]), 0);
        assert_eq!(pts_ms(a.last().unwrap()), 4980);
    }

    #[test]
    fn snapshot_longer_than_buffer_returns_everything() {
        let mut buf = new_buf(30, 0);
        fill_video(&mut buf, 2000, 5000);
        let clip = buf.snapshot(gst::ClockTime::from_seconds(60)).unwrap();
        assert_eq!(clip.video.buffers.len(), 30);
        assert_eq!(clip.duration, ms(3000));
    }

    #[test]
    fn snapshot_skips_leading_delta_frames() {
        let mut buf = new_buf(30, 0);
        for t in (0..500).step_by(100) {
            buf.push_video(packet(t, 100, false));
        }
        fill_video(&mut buf, 1000, 3000);
        let clip = buf.snapshot(gst::ClockTime::from_seconds(60)).unwrap();
        assert!(is_keyframe(&clip.video.buffers[0]));
        assert_eq!(clip.video.buffers.len(), 20);
    }

    #[test]
    fn snapshot_drops_audio_outside_video_range() {
        let mut buf = new_buf(30, 2);
        fill_video(&mut buf, 1000, 3000);
        // Audio track 0 starts early and runs late; track 1 is empty.
        for t in (0..4000).step_by(20) {
            buf.push_audio(0, packet(t, 20, true));
        }
        let clip = buf.snapshot(gst::ClockTime::from_seconds(60)).unwrap();
        assert_eq!(clip.audio.len(), 1, "empty audio tracks are omitted");
        let a = &clip.audio[0].buffers;
        assert_eq!(pts_ms(&a[0]), 0);
        assert!(pts_ms(a.last().unwrap()) < 2000);
    }

    #[test]
    fn snapshot_shares_memory() {
        let mut buf = new_buf(30, 0);
        fill_video(&mut buf, 0, 1000);
        let clip = buf.snapshot(gst::ClockTime::from_seconds(1)).unwrap();
        let orig = buf.video.packets[0].peek_memory(0);
        let snap = clip.video.buffers[0].peek_memory(0);
        assert_eq!(orig.as_ptr(), snap.as_ptr());
        // The ring's own timestamps are untouched.
        assert_eq!(buf.video.packets[0].pts(), Some(gst::ClockTime::ZERO));
    }

    #[test]
    fn audio_is_trimmed_to_oldest_video_not_window() {
        let mut buf = new_buf(3, 1);
        // Interleave like a live pipeline; video ends at 10.5s and keeps
        // 7.0s.. (3.5s), so audio must reach back to 7.0s too.
        for t in (0..10_500).step_by(20) {
            if t % 100 == 0 {
                buf.push_video(packet(t, 100, t % 1000 == 0));
            }
            buf.push_audio(0, packet(t, 20, true));
        }
        assert_eq!(buf.video.oldest_pts(), Some(ms(7000)));
        assert_eq!(buf.audio[0].oldest_pts(), Some(ms(7000)));
        let clip = buf.snapshot(gst::ClockTime::from_seconds(60)).unwrap();
        assert_eq!(pts_ms(&clip.audio[0].buffers[0]), 0);
    }

    #[test]
    fn latest_gop_is_the_last_keyframe_onwards_without_audio() {
        let mut buf = new_buf(30, 1);
        fill_video(&mut buf, 0, 10_500);
        for t in (0..10_500).step_by(20) {
            buf.push_audio(0, packet(t, 20, true));
        }
        let clip = buf.latest_gop().unwrap();
        let v = &clip.video.buffers;
        assert!(is_keyframe(&v[0]));
        assert_eq!(v.len(), 5, "10.0s keyframe, then 10.1..10.4");
        assert_eq!(pts_ms(&v[0]), 0);
        assert_eq!(clip.duration, ms(500));
        assert!(clip.audio.is_empty());
    }

    #[test]
    fn window_changes_apply_to_a_running_buffer() {
        let mut buf = new_buf(30, 1);
        for t in (0..20_000).step_by(20) {
            if t % 100 == 0 {
                buf.push_video(packet(t, 100, t % 1000 == 0));
            }
            buf.push_audio(0, packet(t, 20, true));
        }
        assert_eq!(
            buf.buffered(),
            ms(20_000),
            "all 20 s kept under a 30 s window"
        );

        // Shorter: trimmed right away, audio with it.
        buf.set_window(gst::ClockTime::from_seconds(5));
        assert_eq!(buf.video.oldest_pts(), Some(ms(15_000)));
        assert_eq!(buf.audio[0].oldest_pts(), Some(ms(15_000)));
        let clip = buf.snapshot(buf.window()).unwrap();
        assert_eq!(clip.duration, ms(5000));

        // Longer: nothing comes back, and new frames are kept up to it.
        buf.set_window(gst::ClockTime::from_seconds(60));
        assert_eq!(buf.buffered(), ms(5000));
        fill_video(&mut buf, 20_000, 30_000);
        assert_eq!(buf.buffered(), ms(15_000));
    }

    #[test]
    fn snapshot_without_video_fails() {
        let buf = new_buf(30, 0);
        assert!(buf.snapshot(gst::ClockTime::from_seconds(5)).is_err());
    }
}
