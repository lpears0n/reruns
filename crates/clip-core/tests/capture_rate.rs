//! The screen capture source must keep to the frame rate it is asked for.
//! On Windows' default 15.6 ms timer it doesn't: it wakes late, captures
//! twice in a frame slot and sends the old picture again (see
//! `clip_core::timer`). Needs a desktop to capture; skipped without one.
#![cfg(windows)]

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clip_core::timer::PreciseTimer;
use gstreamer as gst;
use gstreamer::prelude::*;

const FPS: i32 = 60;
const SECONDS: u64 = 5;

/// Capture the desktop for a few seconds; when each frame left the source.
fn capture() -> Option<Vec<Instant>> {
    let pipeline = gst::parse::launch(&format!(
        "d3d12screencapturesrc name=src capture-api=dxgi \
         ! video/x-raw(memory:D3D12Memory),framerate={FPS}/1 ! fakesink sync=false"
    ))
    .ok()?
    .downcast::<gst::Pipeline>()
    .unwrap();
    let frames = Arc::new(Mutex::new(Vec::new()));
    {
        let frames = frames.clone();
        let pad = pipeline.by_name("src")?.static_pad("src")?;
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            frames.lock().unwrap().push(Instant::now());
            gst::PadProbeReturn::Ok
        });
    }
    pipeline.set_state(gst::State::Playing).ok()?;
    let error = pipeline.bus()?.timed_pop_filtered(
        gst::ClockTime::from_seconds(SECONDS),
        &[gst::MessageType::Error],
    );
    let _ = pipeline.set_state(gst::State::Null);
    if error.is_some() {
        return None;
    }
    let frames = frames.lock().unwrap().clone();
    Some(frames)
}

/// Frames a second, and how many frames came within a quarter of a frame
/// of the one before (a second grab in the same slot).
fn measure(frames: &[Instant]) -> (f64, usize) {
    let span = frames[frames.len() - 1] - frames[0];
    let rushed = frames
        .windows(2)
        .filter(|w| w[1] - w[0] < Duration::from_secs(1) / (FPS as u32 * 4))
        .count();
    ((frames.len() - 1) as f64 / span.as_secs_f64(), rushed)
}

#[test]
fn the_capture_source_keeps_to_its_frame_rate() {
    clip_core::init().unwrap();
    let Some(coarse) = capture().filter(|f| f.len() > FPS as usize) else {
        eprintln!("no desktop to capture here; skipped");
        return;
    };
    let (fps, rushed) = measure(&coarse);
    // What this is about; not asserted, since another program may have
    // raised the system's timer already.
    eprintln!("default timer: {fps:.2} fps, {rushed} second grabs in a slot");

    let timer = PreciseTimer::hold();
    let precise = capture().expect("capture worked a moment ago");
    drop(timer);
    let (fps, rushed) = measure(&precise);
    eprintln!("precise timer: {fps:.2} fps, {rushed} second grabs in a slot");
    assert!((fps - f64::from(FPS)).abs() < 0.5, "{fps:.2} fps");
    assert!(rushed <= 2, "{rushed} second grabs in a slot");
}
