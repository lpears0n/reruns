//! Audio capture → AAC pipeline branch. PipeWire on Linux (an output's
//! monitor for game audio), WASAPI on Windows (loopback of an output).

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource {
    /// Everything playing through an output device (game/desktop audio), by
    /// id, or through the default output (which it then follows).
    Desktop(Option<String>),
    /// An input device by id, or the default input.
    Mic(Option<String>),
}

impl AudioSource {
    pub fn name(&self) -> &'static str {
        match self {
            AudioSource::Desktop(_) => "desktop",
            AudioSource::Mic(_) => "mic",
        }
    }

    fn device(&self) -> Option<&str> {
        match self {
            AudioSource::Desktop(d) | AudioSource::Mic(d) => d.as_deref(),
        }
    }

    fn kind(&self) -> DeviceKind {
        match self {
            AudioSource::Desktop(_) => DeviceKind::Output,
            AudioSource::Mic(_) => DeviceKind::Input,
        }
    }
}

/// AAC so clips play everywhere (browsers, Discord, the webview).
const AAC_ENCODERS: [&str; 2] = ["fdkaacenc", "avenc_aac"];

/// An audio device the user can pick: an input for the mic track, or an
/// output whose sound is recorded as the game track.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    /// Stable device id (PipeWire node name / WASAPI device id); pass to
    /// [`AudioSource`].
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceKind {
    Input,
    Output,
}

/// Audio inputs (microphones, line-in). Output monitors and loopbacks,
/// which some systems list as inputs too, are left out.
pub fn input_devices() -> Vec<AudioDevice> {
    devices(DeviceKind::Input)
}

/// Audio outputs (speakers, headsets, virtual sinks) that can be recorded.
pub fn output_devices() -> Vec<AudioDevice> {
    devices(DeviceKind::Output)
}

fn devices(kind: DeviceKind) -> Vec<AudioDevice> {
    let class = match kind {
        DeviceKind::Input => "Audio/Source",
        DeviceKind::Output => "Audio/Sink",
    };
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some(class), None);
    if monitor.start().is_err() {
        return vec![];
    }
    let mut devices: Vec<AudioDevice> = monitor
        .devices()
        .into_iter()
        .filter_map(|d| {
            let props = d.properties()?;
            #[cfg(target_os = "linux")]
            let id = {
                let id = props.get::<String>("node.name").ok()?;
                let media_class = props.get::<String>("media.class").unwrap_or_default();
                // Sink monitors also show up as sources.
                if media_class != class || id.ends_with(".monitor") {
                    return None;
                }
                id
            };
            #[cfg(not(target_os = "linux"))]
            let id = {
                // WASAPI lists a loopback "source" for every output.
                if props
                    .get::<bool>("wasapi2.device.loopback")
                    .unwrap_or(false)
                {
                    return None;
                }
                props
                    .get::<String>("device.id")
                    .or_else(|_| props.get::<String>("device.strid"))
                    .ok()?
            };
            Some(AudioDevice {
                id,
                name: d.display_name().to_string(),
            })
        })
        .collect();
    monitor.stop();
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

/// Add `source → 48 kHz stereo → AAC → aacparse` to `pipeline` and return
/// the `aacparse` element for the caller to link onwards.
pub fn add_audio_branch(
    pipeline: &gst::Pipeline,
    source: &AudioSource,
    bitrate: i32,
) -> Result<gst::Element> {
    let raw = add_audio_source(pipeline, source)?;
    add_aac_branch(pipeline, &raw, bitrate)
}

/// Open `source` (falling back to the default device like the recorder
/// does) as 48 kHz stereo, ending in a `tee` that [`add_aac_branch`] and
/// [`add_audio_mix`] each take a branch from.
pub fn add_audio_source(pipeline: &gst::Pipeline, source: &AudioSource) -> Result<gst::Element> {
    add_raw_source(pipeline, make_source(&present(source))?)
}

/// `src → audioconvert → audioresample → 48 kHz stereo → tee`.
pub fn add_raw_source(pipeline: &gst::Pipeline, src: gst::Element) -> Result<gst::Element> {
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let resample = gst::ElementFactory::make("audioresample").build()?;
    let caps = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("rate", 48_000i32)
                .field("channels", 2i32)
                .build(),
        )
        .build()?;
    let tee = gst::ElementFactory::make("tee").build()?;
    let chain = [src, convert, resample, caps, tee.clone()];
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    Ok(tee)
}

/// A leaky queue that holds up to half a second: a branch that stalls
/// drops its own oldest audio instead of holding up the source (and with
/// it every other branch of its tee).
fn leaky_queue() -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("queue")
        .property(
            "max-size-time",
            gst::ClockTime::from_mseconds(500).nseconds(),
        )
        .property("max-size-buffers", 0u32)
        .property("max-size-bytes", 0u32)
        .property_from_str("leaky", "downstream")
        .build()?)
}

/// One track: a branch of `tee` (from [`add_audio_source`]) encoded to
/// AAC. Returns the `aacparse` at its end.
pub fn add_aac_branch(
    pipeline: &gst::Pipeline,
    tee: &gst::Element,
    bitrate: i32,
) -> Result<gst::Element> {
    let queue = leaky_queue()?;
    let enc = aac_encoder(bitrate)?;
    let parse = gst::ElementFactory::make("aacparse").build()?;
    let chain = [queue.clone(), enc, parse.clone()];
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    tee.link(&queue)?;
    Ok(parse)
}

/// One track of every source mixed together (game and mic in one), from
/// a branch of each `tee`. Clips put it first, because players (and the
/// app's own viewer) only play a file's first audio track; the sources'
/// own tracks follow it for the editor's levels. Returns the `aacparse`
/// at its end.
pub fn add_audio_mix(
    pipeline: &gst::Pipeline,
    tees: &[gst::Element],
    bitrate: i32,
) -> Result<gst::Element> {
    let mixer = gst::ElementFactory::make("audiomixer").build()?;
    pipeline.add(&mixer)?;
    for tee in tees {
        // Each input converted on its own: the mixer takes one format, and
        // the sources may deliver different ones.
        let queue = leaky_queue()?;
        let convert = gst::ElementFactory::make("audioconvert").build()?;
        pipeline.add_many([&queue, &convert])?;
        gst::Element::link_many([&queue, &convert, &mixer])?;
        tee.link(&queue)?;
    }
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let caps = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("rate", 48_000i32)
                .field("channels", 2i32)
                .build(),
        )
        .build()?;
    let queue = leaky_queue()?;
    let enc = aac_encoder(bitrate)?;
    let parse = gst::ElementFactory::make("aacparse").build()?;
    let chain = [mixer, convert, caps, queue, enc, parse.clone()];
    pipeline.add_many(&chain[1..])?;
    gst::Element::link_many(&chain)?;
    Ok(parse)
}

/// The capture element the recorder would open for `source` (falling back
/// to the default device the same way), for the level meters.
pub fn meter_source(source: &AudioSource) -> Result<gst::Element> {
    make_source(&present(source))
}

/// `source`, or the default device of its kind if the one it names isn't
/// connected right now (an unplugged headset must not stop the recording).
fn present(source: &AudioSource) -> AudioSource {
    let Some(id) = source.device() else {
        return source.clone();
    };
    if devices(source.kind()).iter().any(|d| d.id == id) {
        return source.clone();
    }
    tracing::warn!(
        track = source.name(),
        device = id,
        "audio device not connected; recording the default instead"
    );
    match source {
        AudioSource::Desktop(_) => AudioSource::Desktop(None),
        AudioSource::Mic(_) => AudioSource::Mic(None),
    }
}

#[cfg(target_os = "linux")]
fn make_source(source: &AudioSource) -> Result<gst::Element> {
    let mut src = gst::ElementFactory::make("pipewiresrc").name(source.name());
    if let AudioSource::Desktop(_) = source {
        // Capture an output's monitor: the targeted sink, or without a
        // target the default sink, following it when the default changes.
        src = src.property(
            "stream-properties",
            gst::Structure::builder("props")
                .field("stream.capture.sink", "true")
                .build(),
        );
    }
    let src = src.build()?;
    // After the stream properties: on an older pipewiresrc the target joins them.
    if let Some(device) = source.device() {
        crate::capture::set_pipewire_target(&src, device);
    }
    Ok(src)
}

#[cfg(not(target_os = "linux"))]
fn make_source(source: &AudioSource) -> Result<gst::Element> {
    // wasapi2src: shared-mode WASAPI, no exclusive grabs. Not low-latency:
    // that leaves a 20 ms ring buffer (2 × 10 ms), so any stall of the
    // reading thread over 20 ms, routine while a game loads the CPU, drops
    // audio ("Can't record audio fast enough"). The default holds 200 ms.
    let mut src = gst::ElementFactory::make("wasapi2src")
        .name(source.name())
        .property("low-latency", false);
    if let AudioSource::Desktop(_) = source {
        // Loopback of a render device: everything playing through it. With
        // no device named, the default one.
        src = src.property("loopback", true);
    }
    if let Some(device) = source.device() {
        src = src.property("device", device);
    }
    src.build()
        .context("wasapi2src missing (GStreamer wasapi2 plugin)")
}

fn aac_encoder(bitrate: i32) -> Result<gst::Element> {
    let name = AAC_ENCODERS
        .into_iter()
        .find(|n| gst::ElementFactory::find(n).is_some())
        .context("no AAC encoder found (install gst-plugins-bad or gst-libav)")?;
    let el = gst::ElementFactory::make(name).build()?;
    el.set_property("bitrate", bitrate);
    Ok(el)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_inputs_without_monitors() {
        crate::init().unwrap();
        let devices = input_devices();
        for d in &devices {
            println!("in: {} — {}", d.id, d.name);
        }
        assert!(devices.iter().all(|d| !d.id.ends_with(".monitor")));
        for d in output_devices() {
            println!("out: {} — {}", d.id, d.name);
        }
    }

    #[test]
    fn missing_devices_fall_back_to_the_default() {
        crate::init().unwrap();
        let gone = "Reruns-test-device-that-does-not-exist";
        assert_eq!(
            present(&AudioSource::Desktop(Some(gone.into()))),
            AudioSource::Desktop(None)
        );
        assert_eq!(
            present(&AudioSource::Mic(Some(gone.into()))),
            AudioSource::Mic(None)
        );
        // Defaults stay as they are.
        assert_eq!(
            present(&AudioSource::Desktop(None)),
            AudioSource::Desktop(None)
        );
    }
}
