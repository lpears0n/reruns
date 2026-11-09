//! End-to-end save path without the portal: test sources → encoders → ring
//! buffer → MP4, then verify the file with ffprobe.

use std::{path::Path, process::Command};

use clip_core::{Encoder, ReplayBuffer, VideoConfig, ring::running_time_buffer, write_mp4};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// First AAC encoder on this machine, in the recorder's own preference
/// order (Debian builds GStreamer without fdk-aac; Arch has it).
fn aac_encoder() -> &'static str {
    ["fdkaacenc", "avenc_aac", "voaacenc"]
        .into_iter()
        .find(|n| gst::ElementFactory::find(n).is_some())
        .expect("an AAC encoder (gst-libav or fdk-aac) is required")
}

/// Encode `seconds` of test video (and audio) into a ring with `window`.
fn fill_ring(seconds: u64, window: u64) -> ReplayBuffer {
    clip_core::init().unwrap();
    let fps = 30;
    let aac = aac_encoder();
    let pipeline = gst::parse::launch(&format!(
        "videotestsrc num-buffers={n} is-live=false ! video/x-raw,width=320,height=240,framerate={fps}/1,format=BGRx \
         ! videoconvert ! x264enc name=enc ! video/x-h264,profile=high ! h264parse config-interval=-1 ! appsink name=v sync=false \
         audiotestsrc num-buffers={an} samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 \
         ! audioconvert ! {aac} ! aacparse ! appsink name=a sync=false",
        n = seconds * fps,
        an = seconds * 48_000 / 1024,
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    // Same encoder settings as the recorder (1s GOP, no B-frames).
    let cfg = VideoConfig {
        fps: fps as u32,
        bitrate_kbps: 2_000,
        ..Default::default()
    };
    let real = Encoder::X264.build(&cfg).unwrap();
    let enc = pipeline.by_name("enc").unwrap();
    for p in ["key-int-max", "bitrate"] {
        enc.set_property_from_value(p, &real.property_value(p));
    }
    enc.set_property_from_str("tune", "zerolatency");

    let mut ring = ReplayBuffer::new(gst::ClockTime::from_seconds(window), 1);
    let v = pipeline
        .by_name("v")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    let a = pipeline
        .by_name("a")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();

    // Pull both sinks in lockstep so neither stalls the other.
    let (mut v_done, mut a_done) = (false, false);
    while !(v_done && a_done) {
        if !v_done {
            match v.try_pull_sample(gst::ClockTime::from_mseconds(10)) {
                Some(s) => {
                    ring.video.set_caps(s.caps_owned().unwrap());
                    ring.push_video(running_time_buffer(&s).unwrap());
                }
                None => v_done = v.is_eos(),
            }
        }
        if !a_done {
            match a.try_pull_sample(gst::ClockTime::from_mseconds(10)) {
                Some(s) => {
                    ring.audio[0].set_caps(s.caps_owned().unwrap());
                    ring.push_audio(0, running_time_buffer(&s).unwrap());
                }
                None => a_done = a.is_eos(),
            }
        }
    }
    pipeline.set_state(gst::State::Null).unwrap();
    ring
}

fn ffprobe(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", entries, "-of", "csv=p=0"])
        .arg(path)
        .output()
        .expect("ffprobe not installed");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Remove a test's folder. An ffprobe or ffmpeg that another test starts
/// while one of this test's pipelines has a file open inherits a handle to
/// it (GStreamer opens files through the C runtime, whose handles are
/// inherited), and on Windows the file can't be deleted until that process
/// exits; so a folder that won't go is retried for a few seconds.
fn remove_test_dir(dir: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match std::fs::remove_dir_all(dir) {
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            result => return result.unwrap(),
        }
    }
}

#[test]
fn saves_last_n_seconds_as_mp4() {
    let ring = fill_ring(8, 5);
    assert!(ring.buffered() >= gst::ClockTime::from_seconds(5));

    let clip = ring.snapshot(gst::ClockTime::from_seconds(3)).unwrap();
    let dir = std::env::temp_dir().join(format!("reruns-test-{}", std::process::id()));
    let path = dir.join("clip.mp4");
    write_mp4(&clip, &path).unwrap();
    // Written under a temporary name and moved into place when complete.
    assert!(
        !clip_core::save::partial_path(&path).exists(),
        "no .part left behind"
    );

    let streams = ffprobe(&path, "stream=codec_type,codec_name,profile,start_time");
    let lines: Vec<_> = streams.lines().collect();
    assert_eq!(lines.len(), 2, "{streams}");
    assert!(lines[0].starts_with("h264,High,video"), "{streams}");
    assert!(lines[1].starts_with("aac,LC,audio"), "{streams}");

    let duration: f64 = ffprobe(&path, "format=duration").trim().parse().unwrap();
    assert!((2.9..=3.2).contains(&duration), "duration {duration}");

    let info = clip_core::media::inspect(&path).unwrap();
    assert!((2.9..=3.2).contains(&info.duration.seconds_f64()));
    let magic: &[u8] = match info.format {
        clip_core::media::ImageFormat::Webp => b"RIFF",
        clip_core::media::ImageFormat::Jpeg => &[0xFF, 0xD8],
        clip_core::media::ImageFormat::Png => b"\x89PNG",
    };
    assert!(
        info.thumbnail.starts_with(magic),
        "thumbnail is not {:?}",
        info.format
    );
    assert!(
        info.sprite.starts_with(magic),
        "sprite is not {:?}",
        info.format
    );

    assert_eq!(info.audio_tracks, 1);

    // Trim 0.5s..2.0s with the audio halved.
    let edited = dir.join("edit.mp4");
    let edit = clip_core::export::Edit {
        start: gst::ClockTime::from_mseconds(500),
        end: gst::ClockTime::from_mseconds(2000),
        volumes: vec![0.5],
        max_bytes: None,
        vertical: None,
        slow: None,
    };
    let cfg = VideoConfig {
        fps: 30,
        bitrate_kbps: 2_000,
        ..Default::default()
    };
    clip_core::export::render(&path, &edited, &edit, Encoder::X264, &cfg).unwrap();
    assert!(
        !clip_core::save::partial_path(&edited).exists(),
        "no .part left behind"
    );
    let streams = ffprobe(&edited, "stream=codec_name,codec_type");
    assert_eq!(
        streams.lines().collect::<Vec<_>>(),
        ["h264,video", "aac,audio"],
        "{streams}"
    );
    let duration: f64 = ffprobe(&edited, "format=duration").trim().parse().unwrap();
    assert!(
        (1.4..=1.6).contains(&duration),
        "edited duration {duration}"
    );

    remove_test_dir(&dir);
}

/// A size-limited export lands under the limit, scaled down and at 30 fps
/// when the budget calls for it.
#[test]
fn fits_an_export_under_a_size_limit() {
    clip_core::init().unwrap();
    let dir = std::env::temp_dir().join(format!("reruns-fit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.mp4");
    // Four seconds of 720p60 with audio, at a generous rate.
    let aac = aac_encoder();
    let pipeline = gst::parse::launch(&format!(
        "videotestsrc num-buffers=240 pattern=ball ! video/x-raw,width=1280,height=720,framerate=60/1 \
         ! videoconvert ! x264enc bitrate=8000 key-int-max=60 speed-preset=ultrafast ! h264parse ! mp4mux name=m \
         ! filesink name=out \
         audiotestsrc num-buffers=188 samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 \
         ! audioconvert ! {aac} ! aacparse ! m.",
    ))
    .unwrap()
    .downcast::<gst::Bin>()
    .unwrap();
    // Set as a property, not in the launch string: the launch syntax eats
    // the backslashes of a Windows path.
    pipeline
        .by_name("out")
        .unwrap()
        .set_property("location", source.to_str().unwrap());
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    bus.timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos])
        .expect("source rendered");
    pipeline.set_state(gst::State::Null).unwrap();

    // 3 s in 300 kB: ~640 kb/s of video, the 480p/30 fps class.
    let max_bytes = 300_000;
    let plan = clip_core::export::plan_size(max_bytes, 3.0, true).unwrap();
    assert_eq!((plan.max_height, plan.fps), (480, Some(30)));
    let out = dir.join("fitted.mp4");
    let edit = clip_core::export::Edit {
        start: gst::ClockTime::from_mseconds(500),
        end: gst::ClockTime::from_mseconds(3500),
        volumes: vec![1.0],
        max_bytes: Some(max_bytes),
        vertical: None,
        slow: None,
    };
    let cfg = VideoConfig {
        fps: 60,
        bitrate_kbps: 20_000,
        ..Default::default()
    };
    clip_core::export::render(&source, &out, &edit, Encoder::X264, &cfg).unwrap();

    let size = std::fs::metadata(&out).unwrap().len();
    assert!(size <= max_bytes, "{size} bytes");
    let video = ffprobe(&out, "stream=width,height,r_frame_rate");
    assert_eq!(video.lines().next().unwrap(), "854,480,30/1", "{video}");
    let duration: f64 = ffprobe(&out, "format=duration").trim().parse().unwrap();
    assert!((2.9..=3.1).contains(&duration), "duration {duration}");

    // Cancelled (here before the first pass is done): neither the edit nor
    // its temporary file is left; every pass renders under the temporary
    // name, so a failure on a later pass can't leave an earlier one either.
    let cancelled = dir.join("cancelled.mp4");
    let control = clip_core::export::Control {
        on_progress: None,
        cancel: Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            true,
        ))),
    };
    let err =
        clip_core::export::render_with(&source, &cancelled, &edit, Encoder::X264, &cfg, control)
            .unwrap_err()
            .to_string();
    assert!(err.contains("cancelled"), "{err}");
    assert!(!cancelled.exists());
    assert!(!clip_core::save::partial_path(&cancelled).exists());

    // Far too small: refused up front, with how long would fit.
    let edit = clip_core::export::Edit {
        max_bytes: Some(20_000),
        ..edit
    };
    let err = clip_core::export::render(&source, &dir.join("no.mp4"), &edit, Encoder::X264, &cfg)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("too long for 20 kB"), "{err}");
    assert!(!dir.join("no.mp4").exists());

    remove_test_dir(&dir);
}

/// Vertical 9:16 with a 2x slow-motion range: the picture is the cropped
/// window, and the file is longer by what the slow range adds.
#[test]
fn renders_vertical_with_slow_motion() {
    clip_core::init().unwrap();
    let dir = std::env::temp_dir().join(format!("reruns-vertical-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.mp4");
    let aac = aac_encoder();
    let pipeline = gst::parse::launch(&format!(
        "videotestsrc num-buffers=240 pattern=ball ! video/x-raw,width=1280,height=720,framerate=60/1 \
         ! videoconvert ! x264enc bitrate=4000 key-int-max=60 speed-preset=ultrafast ! h264parse ! mp4mux name=m \
         ! filesink name=out \
         audiotestsrc num-buffers=188 samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 \
         ! audioconvert ! {aac} ! aacparse ! m.",
    ))
    .unwrap()
    .downcast::<gst::Bin>()
    .unwrap();
    pipeline
        .by_name("out")
        .unwrap()
        .set_property("location", source.to_str().unwrap());
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    bus.timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos])
        .expect("source rendered");
    pipeline.set_state(gst::State::Null).unwrap();

    let ms = gst::ClockTime::from_mseconds;
    let edit = clip_core::export::Edit {
        start: ms(500),
        end: ms(3500),
        volumes: vec![1.0],
        max_bytes: None,
        vertical: Some(0.25),
        slow: Some(clip_core::export::Slow {
            from: ms(1500),
            to: ms(2500),
            factor: 2.0,
        }),
    };
    assert_eq!(edit.output_duration(), ms(4000));
    let out = dir.join("vertical.mp4");
    let cfg = VideoConfig {
        fps: 60,
        bitrate_kbps: 4_000,
        ..Default::default()
    };
    clip_core::export::render(&source, &out, &edit, Encoder::X264, &cfg).unwrap();

    let video = ffprobe(&out, "stream=width,height");
    assert_eq!(video.lines().next().unwrap(), "404,720", "{video}");
    let streams = ffprobe(&out, "stream=codec_type");
    assert!(streams.contains("audio"), "{streams}");
    let duration: f64 = ffprobe(&out, "format=duration").trim().parse().unwrap();
    assert!((3.85..=4.15).contains(&duration), "duration {duration}");
    // The video itself is stretched, not just padded out by the audio.
    let video_secs: f64 = ffprobe(&out, "stream=duration")
        .lines()
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        (3.85..=4.15).contains(&video_secs),
        "video lasts {video_secs}"
    );

    remove_test_dir(&dir);
}

/// Every codec the machine can hardware-encode survives ring → MP4.
#[test]
fn saves_every_supported_codec() {
    use clip_core::Codec;

    clip_core::init().unwrap();
    for codec in Codec::ALL {
        let Some(encoder) = Encoder::probe(codec) else {
            println!("{codec:?}: no encoder, skipped");
            continue;
        };
        let cfg = VideoConfig {
            fps: 60,
            bitrate_kbps: 100_000,
            codec,
        };
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 180i32)
            .property_from_str("pattern", "ball")
            .build()
            .unwrap();
        let caps = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "BGRx")
                    .field("width", 1280i32)
                    .field("height", 720i32)
                    .field("framerate", gst::Fraction::new(60, 1))
                    .build(),
            )
            .build()
            .unwrap();
        let sink = gst_app::AppSink::builder().sync(false).build();
        let mut chain = vec![src, caps];
        chain.extend(encoder.upload_chain().unwrap());
        chain.push(encoder.build(&cfg).unwrap());
        chain.push(
            gst::ElementFactory::make("capsfilter")
                .property("caps", codec.caps())
                .build()
                .unwrap(),
        );
        chain.push(codec.make_parser().unwrap());
        chain.push(sink.clone().upcast());
        pipeline.add_many(&chain).unwrap();
        gst::Element::link_many(&chain).unwrap();

        let mut ring = ReplayBuffer::new(gst::ClockTime::from_seconds(30), 0);
        pipeline.set_state(gst::State::Playing).unwrap();
        while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
            ring.video.set_caps(s.caps_owned().unwrap());
            ring.push_video(running_time_buffer(&s).unwrap());
        }
        pipeline.set_state(gst::State::Null).unwrap();

        let clip = ring.snapshot(gst::ClockTime::from_seconds(2)).unwrap();
        let path =
            std::env::temp_dir().join(format!("reruns-codec-{}-{codec:?}.mp4", std::process::id()));
        write_mp4(&clip, &path).unwrap();
        let probe = ffprobe(&path, "stream=codec_name,width,height");
        let expected = match codec {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Av1 => "av1",
        };
        assert_eq!(
            probe.trim(),
            format!("{expected},1280,720"),
            "{codec:?} via {encoder:?}"
        );
        println!("{codec:?} via {encoder:?}: ok");
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn screenshot_is_the_newest_frame_as_png() {
    let ring = fill_ring(3, 5);
    let gop = ring.latest_gop().unwrap();
    assert!(gop.audio.is_empty());
    // 30 fps, 1 s GOPs: the newest GOP is the last whole second.
    assert_eq!(gop.video.buffers.len(), 30, "newest GOP");

    let dir = std::env::temp_dir().join(format!("reruns-shot-{}", std::process::id()));
    let path = dir.join("shot.png");
    clip_core::write_png(&gop, &path).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"\x89PNG"), "not a PNG");
    let probe = ffprobe(&path, "stream=codec_name,width,height");
    assert_eq!(probe.trim(), "png,320,240", "{probe}");

    // The ring itself is untouched: a clip can still be saved after.
    let clip = ring.snapshot(gst::ClockTime::from_seconds(2)).unwrap();
    write_mp4(&clip, &dir.join("after.mp4")).unwrap();
    remove_test_dir(&dir);
}

/// Loudness of audio track `n` (0-based) of `path`: ffmpeg's mean volume,
/// in dB.
fn mean_volume(path: &Path, n: usize) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(path)
        .args([
            "-map",
            &format!("0:a:{n}"),
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ])
        .output()
        .expect("ffmpeg not installed");
    let log = String::from_utf8_lossy(&out.stderr);
    log.lines()
        .find_map(|l| l.split("mean_volume:").nth(1))
        .and_then(|v| v.trim().trim_end_matches("dB").trim().parse().ok())
        .unwrap_or_else(|| panic!("no mean_volume for track {n}: {log}"))
}

/// Record `seconds` of live video with two live tones as game and mic,
/// through the recorder's own audio branches (the mix first, then each
/// source's track), and return everything as a clip. With `stall_mixer`
/// (after, for), the mixer's output thread is held up once for that long,
/// as a mixer that hangs would; the returned flag says it happened.
fn record_game_and_mic(
    seconds: u64,
    stall_mixer: Option<(std::time::Duration, std::time::Duration)>,
) -> (clip_core::ring::Clip, bool) {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use clip_core::audio::{add_aac_branch, add_audio_mix, add_raw_source};

    clip_core::init().unwrap();
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true ! video/x-raw,width=160,height=120,framerate=30/1 \
         ! videoconvert ! x264enc tune=zerolatency key-int-max=30 speed-preset=ultrafast \
         ! video/x-h264,profile=high ! h264parse config-interval=-1 ! appsink name=v sync=false",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    // Two tones at the same level and different pitches: mixed, they come
    // out about 3 dB louder than either.
    let tone = |freq: f64| {
        gst::ElementFactory::make("audiotestsrc")
            .property("is-live", true)
            .property("freq", freq)
            .property("volume", 0.3)
            .build()
            .unwrap()
    };
    let game = add_raw_source(&pipeline, tone(440.0)).unwrap();
    let mic = add_raw_source(&pipeline, tone(1000.0)).unwrap();
    let tracks = [
        add_audio_mix(&pipeline, &[game.clone(), mic.clone()], 160_000).unwrap(),
        add_aac_branch(&pipeline, &game, 160_000).unwrap(),
        add_aac_branch(&pipeline, &mic, 160_000).unwrap(),
    ];

    let ring = Arc::new(Mutex::new(
        ReplayBuffer::new(gst::ClockTime::from_seconds(30), tracks.len()).with_mix(),
    ));
    // Every sink feeds the ring as the recorder's do.
    let feed = |sink: &gst_app::AppSink, track: Option<usize>| {
        let ring = ring.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let s = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let mut r = ring.lock().unwrap();
                    let caps = s.caps_owned().unwrap();
                    let buffer = running_time_buffer(&s).unwrap();
                    match track {
                        None => {
                            r.video.set_caps(caps);
                            r.push_video(buffer);
                        }
                        Some(i) => {
                            r.audio[i].set_caps(caps);
                            r.push_audio(i, buffer);
                        }
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
    };
    let v = pipeline
        .by_name("v")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();
    feed(&v, None);
    for (i, parse) in tracks.iter().enumerate() {
        let sink = gst_app::AppSink::builder().sync(false).build();
        pipeline.add(&sink).unwrap();
        parse.link(&sink).unwrap();
        feed(&sink, Some(i));
    }
    let stalled = Arc::new(AtomicBool::new(false));
    if let Some((after, length)) = stall_mixer {
        let mixer = pipeline
            .iterate_elements()
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.factory().is_some_and(|f| f.name() == "audiomixer"))
            .expect("the mix branch has an audiomixer");
        let started = std::time::Instant::now();
        let stalled = stalled.clone();
        mixer
            .static_pad("src")
            .unwrap()
            .add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                if started.elapsed() >= after && !stalled.swap(true, Ordering::SeqCst) {
                    std::thread::sleep(length);
                }
                gst::PadProbeReturn::Ok
            });
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    pipeline.set_state(gst::State::Null).unwrap();

    let clip = ring
        .lock()
        .unwrap()
        .snapshot(gst::ClockTime::from_seconds(30))
        .unwrap();
    (clip, stalled.load(Ordering::SeqCst))
}

/// Game and mic, two live sources, through the recorder's own audio
/// branches: the clip's first audio track has both mixed (players only
/// play the first), and each source keeps its own track after it for the
/// editor.
#[test]
fn mixes_game_and_mic_into_the_first_audio_track() {
    let (clip, _) = record_game_and_mic(4, None);
    assert!(clip.mixed, "the clip says it starts with the mix");
    assert_eq!(clip.audio.len(), 3);
    let dir = std::env::temp_dir().join(format!("reruns-mix-{}", std::process::id()));
    let path = dir.join("clip.mp4");
    write_mp4(&clip, &path).unwrap();

    let streams = ffprobe(&path, "stream=codec_type");
    assert_eq!(
        streams.lines().filter(|l| *l == "audio").count(),
        3,
        "{streams}"
    );
    let (mix, game, mic) = (
        mean_volume(&path, 0),
        mean_volume(&path, 1),
        mean_volume(&path, 2),
    );
    eprintln!("mean volume: mix {mix} dB, game {game} dB, mic {mic} dB");
    // A 0.3 sine is about -13.5 dB; both together about -10.5 dB.
    assert!((game - mic).abs() < 1.5, "game {game} dB, mic {mic} dB");
    assert!(
        (2.0..4.5).contains(&(mix - game)),
        "the mix ({mix} dB) should carry both game ({game} dB) and mic ({mic} dB)"
    );

    // An edit mixes the sources' own tracks with the mix left out (the app
    // sends it a level of 0): game and mic once, not twice.
    let edited = dir.join("edit.mp4");
    let edit = clip_core::export::Edit {
        start: gst::ClockTime::from_mseconds(500),
        end: gst::ClockTime::from_mseconds(2500),
        volumes: vec![0.0, 1.0, 1.0],
        max_bytes: None,
        vertical: None,
        slow: None,
    };
    let cfg = VideoConfig {
        fps: 30,
        bitrate_kbps: 2_000,
        ..Default::default()
    };
    clip_core::export::render(&path, &edited, &edit, Encoder::X264, &cfg).unwrap();
    let both = mean_volume(&edited, 0);
    eprintln!("edit mean volume: {both} dB");
    assert!((both - mix).abs() < 1.5, "edit {both} dB, mix {mix} dB");
    remove_test_dir(&dir);
}

/// The longest step between consecutive packets of `track`.
fn longest_gap(track: &clip_core::ring::ClipTrack) -> gst::ClockTime {
    track
        .buffers
        .windows(2)
        .map(|w| w[1].pts().unwrap() - w[0].pts().unwrap())
        .max()
        .unwrap_or(gst::ClockTime::ZERO)
}

/// A mixer that hangs (here for 2 s, four times what its queues hold)
/// costs only mix audio: game and mic keep their own tracks whole, because
/// every branch into and out of the mixer sits behind a leaky queue
/// instead of holding up the sources. Then mixing carries on.
#[test]
fn a_stalled_mixer_never_holds_up_the_source_tracks() {
    let after = std::time::Duration::from_secs(1);
    let length = std::time::Duration::from_secs(2);
    let (clip, stalled) = record_game_and_mic(5, Some((after, length)));
    assert!(stalled, "the mixer was held up");
    assert_eq!(clip.audio.len(), 3);
    for (name, track) in [("game", &clip.audio[1]), ("mic", &clip.audio[2])] {
        let gap = longest_gap(track);
        let first = track.buffers.first().unwrap().pts().unwrap();
        let last = track.buffers.last().unwrap().pts().unwrap();
        eprintln!("{name}: longest gap {gap}, covers {first}..{last}");
        // AAC packets are ~21 ms apart; a source held up by the mixer would
        // show the stall here.
        assert!(
            gap < gst::ClockTime::from_mseconds(100),
            "{name} track has a {gap} gap"
        );
        assert!(
            last - first > gst::ClockTime::from_seconds(4),
            "{name} track covers only {first}..{last}"
        );
    }
    // The mix came back after the stall.
    let mix_last = clip.audio[0].buffers.last().unwrap().pts().unwrap();
    eprintln!(
        "mix: longest gap {}, last packet {mix_last}",
        longest_gap(&clip.audio[0])
    );
    assert!(
        mix_last > gst::ClockTime::from_nseconds((after + length).as_nanos() as u64),
        "no mix audio after the stall (last at {mix_last})"
    );
}
