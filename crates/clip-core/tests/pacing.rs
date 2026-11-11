//! The judder analyzer against clips with known faults: a moving picture
//! encoded with x264 into MP4, with some frames repeated and some left out.

use std::path::{Path, PathBuf};

use clip_core::pacing::{Event, analyze};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

const WIDTH: usize = 320;
const HEIGHT: usize = 180;
const FPS: u64 = 60;

/// A noisy picture with a bright square at `step`: every step changes it, so
/// two pictures are equal only if their steps are.
fn picture(step: usize) -> Vec<u8> {
    // Noise that changes with every step behind it, so a new picture costs
    // the encoder real bits, as game footage does, and a repeat almost none.
    let mut seed = (step as u32).wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut data: Vec<u8> = (0..WIDTH * HEIGHT)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            20 + (seed >> 26) as u8
        })
        .collect();
    let x0 = (step * 6) % (WIDTH - 48);
    let y0 = (step * 2) % (HEIGHT - 48);
    for row in data.chunks_mut(WIDTH).skip(y0).take(48) {
        row[x0..x0 + 48].fill(220);
    }
    data
}

/// Encode `frames` (slot on the 60 fps timeline, picture step) to an MP4.
fn encode(path: &Path, frames: &[(u64, usize)]) {
    clip_core::init().unwrap();
    let pipeline = gst::parse::launch(
        // 4:2:0 like the recorder's clips: hardware decoders refuse the
        // monochrome stream x264 makes from grey input.
        "appsrc name=src format=time ! videoconvert ! video/x-raw,format=I420 \
         ! x264enc tune=zerolatency speed-preset=ultrafast pass=quant quantizer=12 key-int-max=60 bframes=0 \
         ! h264parse ! mp4mux ! filesink name=out",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    // A property, not part of the launch string, which eats backslashes.
    pipeline
        .by_name("out")
        .unwrap()
        .set_property("location", path.to_str().unwrap());
    let src = pipeline
        .by_name("src")
        .unwrap()
        .downcast::<gst_app::AppSrc>()
        .unwrap();
    src.set_caps(Some(
        &gst::Caps::builder("video/x-raw")
            .field("format", "GRAY8")
            .field("width", WIDTH as i32)
            .field("height", HEIGHT as i32)
            .field("framerate", gst::Fraction::new(FPS as i32, 1))
            .build(),
    ));
    pipeline.set_state(gst::State::Playing).unwrap();
    let frame = gst::ClockTime::SECOND / FPS;
    for &(slot, step) in frames {
        let mut buffer = gst::Buffer::from_mut_slice(picture(step));
        {
            let b = buffer.get_mut().unwrap();
            b.set_pts(frame * slot);
            b.set_duration(frame);
        }
        src.push_buffer(buffer).unwrap();
    }
    src.end_of_stream().unwrap();
    let msg = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(60),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .expect("encoding the test clip timed out");
    assert!(matches!(msg.view(), gst::MessageView::Eos(_)), "{msg:?}");
    pipeline.set_state(gst::State::Null).unwrap();
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("reruns-pacing-{}-{name}.mp4", std::process::id()))
}

#[test]
fn a_smooth_clip_scores_zero() {
    let path = temp("smooth");
    let frames: Vec<_> = (0..240).map(|i| (i, i as usize)).collect();
    encode(&path, &frames);
    let report = analyze(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!((report.fps - 60.0).abs() < 0.5, "{report:?}");
    assert_eq!(report.totals.frames, 240);
    assert_eq!(report.totals.judder_percent(), 0.0, "{report:?}");
    assert!(report.events.is_empty());
}

#[test]
fn repeats_and_holes_are_found_where_they_are() {
    let path = temp("faults");
    let mut frames = Vec::new();
    let mut step = 0;
    for slot in 0..300u64 {
        // Two frames left out of the timeline.
        if (200..202).contains(&slot) {
            step += 1;
            continue;
        }
        // The picture of slot 99 shown three more times.
        if !(100..103).contains(&slot) {
            step += 1;
        }
        frames.push((slot, step));
    }
    encode(&path, &frames);
    let report = analyze(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let t = report.totals;
    assert_eq!(t.frames, 298, "{report:?}");
    assert_eq!((t.repeat_runs, t.repeated, t.longest_repeat), (1, 3, 3));
    assert_eq!((t.holes, t.missing), (1, 2));
    assert_eq!(t.still, 0);
    // (3 repeated + 2 missing) of 300 slots.
    assert!((t.judder_percent() - 5.0 / 3.0).abs() < 0.01, "{t:?}");

    let frame = 1_000_000_000 / FPS;
    let near = |at: u64, slot: u64| at.abs_diff(slot * frame) < frame / 2;
    assert!(
        matches!(report.events[..], [
            Event::Repeat { at_ns: r, frames: 3 },
            Event::Hole { at_ns: h, missing: 2 },
        ] if near(r, 99) && near(h, 199)),
        "{:?}",
        report.events
    );

    // The recorder's size-only guess sees the same faults in this clip.
    let p = report.packets.expect("an MP4 has packets to read");
    assert_eq!((p.repeated, p.missing), (3, 2), "{p:?}");
}
