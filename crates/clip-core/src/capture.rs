//! Screen capture → encoder pipeline branch.
//!
//! Two ways to get frames from PipeWire to the encoder:
//!
//! * **Zero-copy** (`CapturePath::ZeroCopy`): the compositor's DMA-BUF is
//!   imported straight into GPU memory (`glupload` for NVENC, `vapostproc`
//!   for VA-API) and encoded there. Nothing crosses to system memory, so a
//!   game saturating the GPU no longer starves the capture of bandwidth.
//! * **Copy** (`CapturePath::Copy`): frames arrive in system memory and the
//!   encoder's upload chain moves them. Works everywhere; costs a copy per
//!   frame and roughly a third of the frame rate under load.
//!
//! The recorder tries zero-copy first and falls back if negotiation fails.

#[cfg(target_os = "linux")]
use std::os::fd::RawFd;

use anyhow::{Context, Result, bail};
use gstreamer as gst;
use gstreamer::prelude::*;
use serde::{Deserialize, Serialize};

use crate::encoder::{Codec, Encoder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub fps: u32,
    pub bitrate_kbps: u32,
    #[serde(default)]
    pub codec: Codec,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            fps: 60,
            bitrate_kbps: 20_000,
            codec: Codec::H264,
        }
    }
}

/// Where `pipewiresrc` reads frames from.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub enum PipewireTarget {
    /// A portal-provided PipeWire remote and node id. The fd must stay open
    /// for as long as the pipeline runs.
    Remote { fd: RawFd, node_id: u32 },
    /// An object serial on the session's own PipeWire daemon (KWin streams).
    Serial(u64),
}

/// What the video source captures, per platform.
#[derive(Debug, Clone, Copy)]
pub enum CaptureTarget {
    #[cfg(target_os = "linux")]
    Pipewire(PipewireTarget),
    /// The game's window on Windows, by `HWND`. Only the window is
    /// recorded, never the rest of the screen.
    #[cfg(windows)]
    Window { hwnd: isize, cursor: bool },
    /// HDR compatibility (Windows): the game window's area of its HDR
    /// monitor, through DXGI Desktop Duplication with tone mapping. Unlike
    /// `Window`, anything drawn on top of the game in that area is
    /// recorded too.
    #[cfg(windows)]
    HdrRegion {
        hwnd: isize,
        monitor: isize,
        cursor: bool,
    },
    /// HDR window capture (Windows, experimental): the game's window itself
    /// in 16-bit float, tone-mapped on the GPU for its display. Only the
    /// window is recorded, as with `Window`.
    #[cfg(windows)]
    HdrWindow {
        hwnd: isize,
        cursor: bool,
        hdr: crate::win::HdrWindow,
    },
}

/// How frames travel from the compositor to the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapturePath {
    /// DMA-BUF import into GPU memory; encoder reads it in place.
    ZeroCopy,
    /// Through system memory.
    Copy,
}

/// Add `pipewiresrc → constant fps → (even size) → encoder → parser` to
/// `pipeline` and return the parser for the caller to link onwards. The
/// parser repeats stream headers on every keyframe so any GOP can start a
/// file.
pub fn add_video_branch(
    pipeline: &gst::Pipeline,
    target: CaptureTarget,
    encoder: Encoder,
    cfg: &VideoConfig,
    path: CapturePath,
) -> Result<gst::Element> {
    if path == CapturePath::ZeroCopy && encoder.gpu_memory().is_none() {
        bail!("{encoder:?} has no zero-copy path");
    }
    #[cfg(windows)]
    if let CaptureTarget::HdrRegion {
        hwnd,
        monitor,
        cursor,
    } = target
    {
        return add_hdr_branch(pipeline, hwnd, monitor, cursor, encoder, cfg, path);
    }
    #[cfg(windows)]
    if let CaptureTarget::HdrWindow { .. } = target {
        return add_hdr_window_branch(pipeline, target, encoder, cfg, path);
    }
    let src = make_source(target, cfg, path)?;

    let rate = make_rate()?;
    let queue = make_queue()?;
    let enc = encoder.build(cfg)?;
    // Windows: capture on the encoder's own GPU. Window capture otherwise
    // makes its device on the system's default adapter, which on a PC with
    // integrated graphics can be the other GPU: every frame would then go
    // through system memory to reach the encoder.
    #[cfg(windows)]
    if let Some(index) = enc
        .find_property("adapter-luid")
        .and_then(|_| enc.property_value("adapter-luid").get::<i64>().ok())
        .and_then(crate::win::display::adapter_index)
        && src.has_property("adapter")
    {
        tracing::info!(adapter = index, "capturing on the encoder's GPU");
        src.set_property("adapter", index as i32);
    }
    let enc_caps = gst::ElementFactory::make("capsfilter")
        .property("caps", cfg.codec.caps())
        .build()?;
    let parse = cfg.codec.make_parser()?;

    // The size the encoder sees, fixed for the whole recording. Frames of
    // any other size are scaled to it, letterboxed, so a game window
    // resizing or switching modes never makes the encoder renegotiate:
    // that renegotiation failed ("not-negotiated") and killed the capture
    // on the owner's machine. Windows knows the window's size up front;
    // elsewhere (`None`) it is locked to the first frame's size.
    #[cfg(windows)]
    let fixed = fixed_output_size(target);
    #[cfg(not(windows))]
    let fixed: Option<(i32, i32)> = None;
    if let Some((w, h)) = fixed {
        tracing::info!(width = w, height = h, "encoding at a fixed size");
    }

    let mut chain = vec![src.clone()];
    // The Windows source paces itself to the frame rate it negotiates, and
    // left alone it settles on 30 fps (videorate would then duplicate every
    // other frame). Ask it for the recording rate.
    #[cfg(windows)]
    let source_rate = |s: &mut gst::StructureRef| {
        s.set("framerate", gst::Fraction::new(cfg.fps as i32, 1));
    };
    // pipewiresrc sends frames on damage at a variable rate; videorate makes
    // it constant, so the source itself isn't constrained.
    #[cfg(not(windows))]
    let source_rate = |_: &mut gst::StructureRef| {};
    match path {
        CapturePath::Copy => {
            #[cfg(windows)]
            chain.push(capsfilter(raw_caps(None, source_rate))?);
            // Window streams can have odd sizes, which 4:2:0 encoders reject;
            // trim the odd row/column whenever the size changes.
            let crop = gst::ElementFactory::make("videocrop").build()?;
            {
                let crop = crop.clone();
                on_caps(&src, move |w, h| {
                    crop.set_property("right", w & 1);
                    crop.set_property("bottom", h & 1);
                })?;
            }
            let rate_caps = capsfilter(raw_caps(None, |s| {
                s.set("framerate", gst::Fraction::new(cfg.fps as i32, 1));
            }))?;
            chain.extend([crop, rate, rate_caps, queue]);
            let [scale, size] = fixed_size_scaler(fixed)?;
            if fixed.is_none() {
                lock_size_at_first_caps(&src, &size, None)?;
            }
            chain.extend([scale, size]);
            chain.extend(encoder.upload_chain()?);
        }
        CapturePath::ZeroCopy => {
            let memory = encoder.gpu_memory().unwrap();
            // Only GPU memory leaves the source; a system-memory offer must
            // fail here so the recorder can fall back instead of silently
            // copying.
            let gpu = capsfilter(raw_caps(Some(SOURCE_GPU_MEMORY), source_rate))?;
            let rate_caps = capsfilter(raw_caps(Some(SOURCE_GPU_MEMORY), |s| {
                s.set("framerate", gst::Fraction::new(cfg.fps as i32, 1));
            }))?;
            chain.extend([gpu, rate, rate_caps, queue]);
            let convert = encoder.gpu_chain()?;
            for c in &convert {
                // Letterbox rather than stretch when the shape changes.
                if c.has_property("add-borders") {
                    c.set_property("add-borders", true);
                }
            }
            chain.extend(convert);
            // The GPU converter also scales: everything goes out at the fixed
            // (or first) size, even for 4:2:0 encoders.
            let size = capsfilter(size_caps(Some(memory), fixed))?;
            if fixed.is_none() {
                lock_size_at_first_caps(&src, &size, Some(memory))?;
            }
            chain.push(size);
        }
    }
    chain.push(enc);
    chain.push(enc_caps);
    chain.push(parse.clone());
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    Ok(parse)
}

/// System-memory frames of any size out at one size, letterboxed
/// (passthrough while they already are that size). `None`: no size yet;
/// [`lock_size_at_first_caps`] sets it.
fn fixed_size_scaler(size: Option<(i32, i32)>) -> Result<[gst::Element; 2]> {
    let scale = gst::ElementFactory::make("videoscale")
        .property("add-borders", true)
        .build()?;
    Ok([scale, capsfilter(size_caps(None, size))?])
}

/// `video/x-raw` (with `memory`) at `size`, square pixels; any size if
/// `None`.
fn size_caps(memory: Option<&str>, size: Option<(i32, i32)>) -> gst::Caps {
    raw_caps(memory, |s| {
        if let Some((w, h)) = size {
            s.set("width", w);
            s.set("height", h);
            s.set("pixel-aspect-ratio", gst::Fraction::new(1, 1));
        }
    })
}

/// Fix `size` (a capsfilter after a scaler) to the source's first frame
/// size, rounded down to even; later size changes are scaled to it.
fn lock_size_at_first_caps(
    src: &gst::Element,
    size: &gst::Element,
    memory: Option<&'static str>,
) -> Result<()> {
    let size = size.clone();
    let locked = std::sync::atomic::AtomicBool::new(false);
    on_caps(src, move |w, h| {
        if locked.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let (w, h) = (w & !1, h & !1);
        tracing::info!(width = w, height = h, "encoding at a fixed size");
        size.set_property("caps", size_caps(memory, Some((w, h))));
    })
}

/// Name of the frame queue in front of the encoder, for the recorder to
/// count what it drops.
pub(crate) const FRAME_QUEUE: &str = "frames";

/// The frame queue in front of the encoder: a few frames, dropping the
/// oldest when the encoder falls behind rather than stalling capture.
fn make_queue() -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("queue")
        .name(FRAME_QUEUE)
        .property("max-size-buffers", 4u32)
        .property("max-size-time", 0u64)
        .property("max-size-bytes", 0u32)
        .property_from_str("leaky", "downstream")
        .build()?)
}

/// HDR compatibility: `d3d12screencapturesrc` duplicates the HDR monitor
/// in 16-bit float and tone-maps it to SDR (Reinhard, using Windows' SDR
/// white level), cropped to the game window's area. `d3d12convert` scales
/// it to the fixed size and converts it for the encoder, and
/// `d3d12download` hands it over as D3D11 textures, so the frame never
/// leaves the GPU and the usual hardware encoders take it. GPU only: the
/// copy path would move 16-bit frames through system memory.
#[cfg(windows)]
fn add_hdr_branch(
    pipeline: &gst::Pipeline,
    hwnd: isize,
    monitor: isize,
    cursor: bool,
    encoder: Encoder,
    cfg: &VideoConfig,
    path: CapturePath,
) -> Result<gst::Element> {
    if path != CapturePath::ZeroCopy {
        bail!("HDR capture only runs on the GPU path");
    }
    let memory = encoder
        .gpu_memory()
        .with_context(|| format!("{encoder:?} takes no GPU frames"))?;
    let target = CaptureTarget::HdrRegion {
        hwnd,
        monitor,
        cursor,
    };
    let (w, h) = fixed_output_size(target).context("can't size the game window")?;
    tracing::info!(
        width = w,
        height = h,
        "encoding at a fixed size (HDR capture)"
    );

    let src = gst::ElementFactory::make("d3d12screencapturesrc")
        .name("screen")
        .property("monitor-handle", monitor as u64)
        .property("show-cursor", cursor)
        .build()
        .context("d3d12screencapturesrc missing (GStreamer d3d12 plugin)")?;
    if src.has_property("capture-api") {
        src.set_property_from_str("capture-api", "dxgi");
    }
    if src.has_property("tonemap") {
        src.set_property_from_str("tonemap", "reinhard");
    }
    if let Some((x, y, cw, ch)) = window_crop(hwnd, monitor) {
        src.set_property("crop-x", x);
        src.set_property("crop-y", y);
        src.set_property("crop-width", cw);
        src.set_property("crop-height", ch);
    }

    const D3D12: &str = "memory:D3D12Memory";
    let rate_of = |s: &mut gst::StructureRef| {
        s.set("framerate", gst::Fraction::new(cfg.fps as i32, 1));
    };
    let convert = gst::ElementFactory::make("d3d12convert")
        .build()
        .context("d3d12convert missing (GStreamer d3d12 plugin)")?;
    if convert.has_property("add-borders") {
        convert.set_property("add-borders", true);
    }
    let download = gst::ElementFactory::make("d3d12download")
        .build()
        .context("d3d12download missing (GStreamer d3d12 plugin)")?;
    let parse = cfg.codec.make_parser()?;
    let chain = [
        src,
        capsfilter(raw_caps(Some(D3D12), rate_of))?,
        make_rate()?,
        capsfilter(raw_caps(Some(D3D12), rate_of))?,
        make_queue()?,
        convert,
        capsfilter(raw_caps(Some(D3D12), |s| {
            s.set("width", w);
            s.set("height", h);
            s.set("pixel-aspect-ratio", gst::Fraction::new(1, 1));
        }))?,
        download,
        capsfilter(raw_caps(Some(memory), |_| {}))?,
        encoder.build(cfg)?,
        gst::ElementFactory::make("capsfilter")
            .property("caps", cfg.codec.caps())
            .build()?,
        parse.clone(),
    ];
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    Ok(parse)
}

/// HDR window capture: the game's window in FP16 through our own Windows
/// Graphics Capture session, tone-mapped by a shader into D3D11 frames
/// (see `win::hdr_window`), then the usual GPU chain into the encoder.
/// The source shares its D3D11 device with every element through the
/// pipeline's context, so frames go to the encoder without a copy.
#[cfg(windows)]
fn add_hdr_window_branch(
    pipeline: &gst::Pipeline,
    target: CaptureTarget,
    encoder: Encoder,
    cfg: &VideoConfig,
    path: CapturePath,
) -> Result<gst::Element> {
    let CaptureTarget::HdrWindow { hwnd, cursor, hdr } = target else {
        bail!("not an HDR window target");
    };
    if path != CapturePath::ZeroCopy {
        bail!("HDR window capture only runs on the GPU path");
    }
    let memory = encoder
        .gpu_memory()
        .with_context(|| format!("{encoder:?} takes no GPU frames"))?;
    if memory != crate::win::hdr_window::MEMORY {
        bail!("{encoder:?} doesn't take D3D11 frames");
    }
    let (w, h) = fixed_output_size(target).context("can't size the game window")?;
    tracing::info!(
        width = w,
        height = h,
        "encoding at a fixed size (HDR window)"
    );
    let (src, context) = crate::win::hdr_window::source(
        hwnd,
        cursor,
        &hdr.display,
        hdr.adapter_luid,
        (w as u32, h as u32),
    )?;
    let rate_of = |s: &mut gst::StructureRef| {
        s.set("framerate", gst::Fraction::new(cfg.fps as i32, 1));
    };
    let parse = cfg.codec.make_parser()?;
    let mut chain = vec![
        src,
        make_rate()?,
        capsfilter(raw_caps(Some(memory), rate_of))?,
        make_queue()?,
    ];
    // BGRA to what the encoder takes, on the GPU.
    chain.extend(encoder.gpu_chain()?);
    chain.extend([
        capsfilter(size_caps(Some(memory), Some((w, h))))?,
        encoder.build(cfg)?,
        gst::ElementFactory::make("capsfilter")
            .property("caps", cfg.codec.caps())
            .build()?,
        parse.clone(),
    ]);
    pipeline.add_many(&chain)?;
    gst::Element::link_many(&chain)?;
    // Every D3D11 element uses the source's device.
    pipeline.set_context(&context);
    Ok(parse)
}

/// The game window's client area in its monitor's coordinates, for the
/// duplication crop; `None` when it covers the whole monitor (fullscreen)
/// or can't be read.
#[cfg(windows)]
fn window_crop(hwnd: isize, monitor: isize) -> Option<(u32, u32, u32, u32)> {
    use windows::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, HMONITOR, MONITORINFO},
        UI::WindowsAndMessaging::GetClientRect,
    };
    let hwnd = HWND(hwnd as *mut _);
    let mut client = RECT::default();
    let mut origin = POINT::default();
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: plain queries; stale handles just fail.
    unsafe {
        GetClientRect(hwnd, &mut client).ok()?;
        ClientToScreen(hwnd, &mut origin).ok().ok()?;
        GetMonitorInfoW(HMONITOR(monitor as *mut _), &mut info)
            .ok()
            .ok()?;
    }
    let m = info.rcMonitor;
    let x = (origin.x - m.left).max(0);
    let y = (origin.y - m.top).max(0);
    let w = (client.right - client.left).min(m.right - m.left - x);
    let h = (client.bottom - client.top).min(m.bottom - m.top - y);
    if w <= 0 || h <= 0 {
        return None;
    }
    let full = x == 0 && y == 0 && w == m.right - m.left && h == m.bottom - m.top;
    (!full).then_some((x as u32, y as u32, (w & !1) as u32, (h & !1) as u32))
}

/// `videorate`, which turns the source's frames into a constant rate.
/// When the source pauses (Windows: a window only yields frames while the
/// game draws), the gap is not filled with copies: on resume that would be
/// a burst of hundreds of duplicate frames at once for the encoder. Pauses
/// longer than a second stay gaps in the replay buffer.
fn make_rate() -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("videorate")
        .name("rate")
        .property(
            "max-duplication-time",
            gst::ClockTime::from_seconds(1).nseconds(),
        )
        .build()?)
}

/// The size to encode a Windows game window at: its client area when
/// recording starts, or its monitor's size if the window is tiny then
/// (minimized, or a splash screen), rounded down to even numbers for 4:2:0
/// encoders.
#[cfg(windows)]
fn fixed_output_size(target: CaptureTarget) -> Option<(i32, i32)> {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::GetClientRect,
    };
    let (CaptureTarget::Window { hwnd, .. }
    | CaptureTarget::HdrRegion { hwnd, .. }
    | CaptureTarget::HdrWindow { hwnd, .. }) = target;
    let hwnd = HWND(hwnd as *mut _);
    let mut r = RECT::default();
    // SAFETY: plain queries on a window handle; a stale one just fails.
    let (w, h) = unsafe {
        if GetClientRect(hwnd, &mut r).is_ok() && r.right - r.left >= 640 && r.bottom - r.top >= 360
        {
            (r.right - r.left, r.bottom - r.top)
        } else {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return None;
            }
            let m = info.rcMonitor;
            (m.right - m.left, m.bottom - m.top)
        }
    };
    (w >= 2 && h >= 2).then_some((w & !1, h & !1))
}

/// The GPU memory the platform's capture source hands out.
#[cfg(target_os = "linux")]
const SOURCE_GPU_MEMORY: &str = "memory:DMABuf";
#[cfg(windows)]
const SOURCE_GPU_MEMORY: &str = "memory:D3D11Memory";

/// The capture source element for this platform.
#[cfg(target_os = "linux")]
fn make_source(
    target: CaptureTarget,
    cfg: &VideoConfig,
    path: CapturePath,
) -> Result<gst::Element> {
    let CaptureTarget::Pipewire(target) = target;
    let src = gst::ElementFactory::make("pipewiresrc")
        .name("screen")
        .property("do-timestamp", true);
    let src = match target {
        PipewireTarget::Remote { fd, node_id } => {
            src.property("fd", fd).property("path", node_id.to_string())
        }
        PipewireTarget::Serial(_) => src,
    }
    .build()?;
    if let PipewireTarget::Serial(serial) = target {
        set_pipewire_target(&src, &serial.to_string());
    }
    // Compositors only send frames on damage; resend so static scenes
    // still produce video. Newer pipewiresrc only (videorate covers the
    // gaps without it), so set only where it exists.
    crate::encoder::set(&src, "keepalive-time", 1000 / cfg.fps as i32);
    if path == CapturePath::ZeroCopy && src.has_property("always-copy") {
        // Copying can't map a DMA-BUF; it would hand out empty buffers.
        src.set_property("always-copy", false);
    }
    Ok(src)
}

/// Point a `pipewiresrc` at `target` (a node name, or an object serial).
/// `target-object` is a newer property; a pipewiresrc without it gets the
/// target in the stream's own properties instead (`target.object`, and
/// `node.target`, which older session managers read). Never set blindly:
/// the element builder panics on a property the element lacks.
#[cfg(target_os = "linux")]
pub(crate) fn set_pipewire_target(src: &gst::Element, target: &str) {
    if src.has_property("target-object") {
        src.set_property("target-object", target);
    } else if src.has_property("stream-properties") {
        let props = src.property::<Option<gst::Structure>>("stream-properties");
        src.set_property("stream-properties", with_target(props, target));
    } else {
        tracing::warn!(
            target,
            "this pipewiresrc can't be pointed at a device; recording the default"
        );
    }
}

/// `props` (or empty ones) plus the stream properties that pick `target`.
#[cfg(target_os = "linux")]
fn with_target(props: Option<gst::Structure>, target: &str) -> gst::Structure {
    let mut props = props.unwrap_or_else(|| gst::Structure::new_empty("props"));
    props.set("target.object", target);
    props.set("node.target", target);
    props
}

/// `d3d11screencapturesrc` on the game's window, through Windows Graphics
/// Capture: only that window is recorded (like Medal), so notifications,
/// a second monitor or anything on top of the game stay out of clips.
/// Frames arrive as D3D11 textures (zero-copy into every Windows hardware
/// encoder) or as system memory for the copy path. WGC doesn't hook the
/// game's swap chain, and capturing a window rather than a monitor lets a
/// fullscreen game keep presenting straight to the display.
#[cfg(windows)]
fn make_source(
    target: CaptureTarget,
    _cfg: &VideoConfig,
    _path: CapturePath,
) -> Result<gst::Element> {
    let CaptureTarget::Window { hwnd, cursor } = target else {
        bail!("HDR capture is built by its own branch");
    };
    // A window handle selects WGC window capture by itself.
    let src = gst::ElementFactory::make("d3d11screencapturesrc")
        .name("screen")
        .property("window-handle", hwnd as u64)
        .property("show-cursor", cursor)
        .build()
        .context("d3d11screencapturesrc missing (GStreamer d3d11 plugin)")?;
    // The client area only: no title bar or borders on a windowed game.
    if src.has_property("window-capture-mode") {
        src.set_property_from_str("window-capture-mode", "client");
    }
    if src.has_property("show-border") {
        src.set_property("show-border", false);
    }
    Ok(src)
}

/// `video/x-raw` with an optional memory feature (`memory:DMABuf`,
/// `memory:GLMemory`, …); `None` means system memory. `set` adds fields.
fn raw_caps(feature: Option<&str>, set: impl FnOnce(&mut gst::StructureRef)) -> gst::Caps {
    let mut structure = gst::Structure::new_empty("video/x-raw");
    set(structure.as_mut());
    let mut caps = gst::Caps::new_empty();
    caps.get_mut()
        .unwrap()
        .append_structure_full(structure, feature.map(|f| gst::CapsFeatures::new([f])));
    caps
}

fn capsfilter(caps: gst::Caps) -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("capsfilter")
        .property("caps", caps)
        .build()?)
}

/// Call `f(width, height)` from the source pad whenever the stream's caps
/// change (window resizes).
fn on_caps(src: &gst::Element, f: impl Fn(i32, i32) + Send + Sync + 'static) -> Result<()> {
    src.static_pad("src")
        .context("pipewiresrc has no src pad")?
        .add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            if let Some(gst::PadProbeData::Event(ev)) = &info.data
                && let gst::EventView::Caps(c) = ev.view()
                && let Some(s) = c.caps().structure(0)
            {
                let w = s.get::<i32>("width").unwrap_or(0);
                let h = s.get::<i32>("height").unwrap_or(0);
                if w > 0 && h > 0 {
                    f(w, h);
                }
            }
            gst::PadProbeReturn::Ok
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use gstreamer_video as gst_video;

    use super::*;

    /// Without `target-object`, the target joins the stream properties
    /// already set (desktop audio's `stream.capture.sink`).
    #[cfg(target_os = "linux")]
    #[test]
    fn older_pipewiresrc_gets_the_target_as_stream_properties() {
        crate::init().unwrap();
        let existing = gst::Structure::builder("props")
            .field("stream.capture.sink", "true")
            .build();
        let props = with_target(Some(existing), "alsa_output.usb-headset");
        assert_eq!(props.get::<&str>("stream.capture.sink").unwrap(), "true");
        assert_eq!(
            props.get::<&str>("target.object").unwrap(),
            "alsa_output.usb-headset"
        );
        assert_eq!(
            props.get::<&str>("node.target").unwrap(),
            "alsa_output.usb-headset"
        );
        assert!(with_target(None, "42").has_field("target.object"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn newer_pipewiresrc_gets_target_object() {
        crate::init().unwrap();
        let Ok(src) = gst::ElementFactory::make("pipewiresrc").build() else {
            eprintln!("no pipewiresrc here; skipped");
            return;
        };
        set_pipewire_target(&src, "alsa_input.usb-mic");
        if src.has_property("target-object") {
            assert_eq!(
                src.property::<Option<String>>("target-object").as_deref(),
                Some("alsa_input.usb-mic")
            );
        } else {
            let props = src
                .property::<Option<gst::Structure>>("stream-properties")
                .unwrap();
            assert_eq!(
                props.get::<&str>("target.object").unwrap(),
                "alsa_input.usb-mic"
            );
        }
    }

    /// 1 s of 30 fps frames, a 3 s pause, 1 s more: the pause must not come
    /// out as ~90 filler frames.
    #[test]
    fn long_pauses_are_not_filled_with_copies() {
        use gstreamer_app as gst_app;
        crate::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst_app::AppSrc::builder()
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", 16i32)
                    .field("height", 16i32)
                    .field("framerate", gst::Fraction::new(0, 1))
                    .build(),
            )
            .format(gst::Format::Time)
            .build();
        let rate = make_rate().unwrap();
        let caps = capsfilter(raw_caps(None, |s| {
            s.set("framerate", gst::Fraction::new(30, 1))
        }))
        .unwrap();
        let sink = gst_app::AppSink::builder().sync(false).build();
        pipeline
            .add_many([src.upcast_ref(), &rate, &caps, sink.upcast_ref()])
            .unwrap();
        gst::Element::link_many([src.upcast_ref(), &rate, &caps, sink.upcast_ref()]).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();

        let frame = gst::ClockTime::from_nseconds(1_000_000_000 / 30);
        let times = (0..30u64)
            .map(|i| frame * i)
            .chain((0..30u64).map(|i| gst::ClockTime::from_seconds(4) + frame * i));
        for t in times {
            let mut b = gst::Buffer::from_slice(vec![0u8; 16 * 16 * 3 / 2]);
            let bm = b.get_mut().unwrap();
            bm.set_pts(t);
            bm.set_duration(frame);
            src.push_buffer(b).unwrap();
        }
        src.end_of_stream().unwrap();
        let mut out = 0;
        while sink
            .try_pull_sample(gst::ClockTime::from_seconds(5))
            .is_some()
        {
            out += 1;
        }
        pipeline.set_state(gst::State::Null).unwrap();
        // 60 real frames, give or take videorate's edge frames; a filled
        // gap would be ~150.
        assert!((55..=70).contains(&out), "{out} frames out");
    }

    /// A source that changes size mid-stream (a game window switching
    /// resolution) still reaches the encoder at one size, and every frame
    /// gets through.
    #[test]
    fn size_changes_are_scaled_to_the_fixed_size() {
        use gstreamer_app as gst_app;
        crate::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst_app::AppSrc::builder().format(gst::Format::Time).build();
        let [scale, size] = fixed_size_scaler(Some((320, 180))).unwrap();
        let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let enc = gst::ElementFactory::make("x264enc")
            .property_from_str("tune", "zerolatency")
            .build()
            .unwrap();
        let sink = gst_app::AppSink::builder().sync(false).build();
        let chain = [
            src.upcast_ref(),
            &scale,
            &size,
            &convert,
            &enc,
            sink.upcast_ref(),
        ];
        pipeline.add_many(chain).unwrap();
        gst::Element::link_many(chain).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();

        let frame = gst::ClockTime::from_nseconds(1_000_000_000 / 30);
        let mut n = 0u64;
        for (w, h) in [(320u32, 180u32), (400, 300), (250, 250), (1280, 720)] {
            src.set_caps(Some(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", w as i32)
                    .field("height", h as i32)
                    .field("framerate", gst::Fraction::new(30, 1))
                    .build(),
            ));
            for _ in 0..10 {
                let len = gst_video::VideoInfo::builder(gst_video::VideoFormat::I420, w, h)
                    .build()
                    .unwrap()
                    .size();
                let mut b = gst::Buffer::from_slice(vec![16u8; len]);
                let bm = b.get_mut().unwrap();
                bm.set_pts(frame * n);
                bm.set_duration(frame);
                src.push_buffer(b).unwrap();
                n += 1;
            }
        }
        src.end_of_stream().unwrap();
        let mut out = 0;
        let mut sizes = std::collections::HashSet::new();
        while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
            let st = s.caps().unwrap().structure(0).unwrap().to_owned();
            sizes.insert((
                st.get::<i32>("width").unwrap(),
                st.get::<i32>("height").unwrap(),
            ));
            out += 1;
        }
        let errors = pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error]);
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(errors.is_none(), "{errors:?}");
        assert_eq!(out, 40, "every frame encoded");
        assert_eq!(sizes, [(320, 180)].into(), "one size reached the encoder");
    }

    /// Linux: the size is locked to the first frame's (rounded to even),
    /// and later sizes are scaled to it.
    #[test]
    fn size_is_locked_at_the_first_frame() {
        use gstreamer_app as gst_app;
        crate::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst_app::AppSrc::builder().format(gst::Format::Time).build();
        let [scale, size] = fixed_size_scaler(None).unwrap();
        lock_size_at_first_caps(src.upcast_ref(), &size, None).unwrap();
        let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let enc = gst::ElementFactory::make("x264enc")
            .property_from_str("tune", "zerolatency")
            .build()
            .unwrap();
        let sink = gst_app::AppSink::builder().sync(false).build();
        let chain = [
            src.upcast_ref(),
            &scale,
            &size,
            &convert,
            &enc,
            sink.upcast_ref(),
        ];
        pipeline.add_many(chain).unwrap();
        gst::Element::link_many(chain).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();

        let frame = gst::ClockTime::from_nseconds(1_000_000_000 / 30);
        let mut n = 0u64;
        // An odd first size: locked to 322x180.
        for (w, h) in [(323u32, 181u32), (640, 360), (200, 300)] {
            src.set_caps(Some(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", w as i32)
                    .field("height", h as i32)
                    .field("framerate", gst::Fraction::new(30, 1))
                    .build(),
            ));
            for _ in 0..10 {
                let len = gst_video::VideoInfo::builder(gst_video::VideoFormat::I420, w, h)
                    .build()
                    .unwrap()
                    .size();
                let mut b = gst::Buffer::from_slice(vec![16u8; len]);
                let bm = b.get_mut().unwrap();
                bm.set_pts(frame * n);
                bm.set_duration(frame);
                src.push_buffer(b).unwrap();
                n += 1;
            }
        }
        src.end_of_stream().unwrap();
        let mut sizes = std::collections::HashSet::new();
        let mut out = 0;
        while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
            let st = s.caps().unwrap().structure(0).unwrap().to_owned();
            sizes.insert((
                st.get::<i32>("width").unwrap(),
                st.get::<i32>("height").unwrap(),
            ));
            out += 1;
        }
        let errors = pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error]);
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(errors.is_none(), "{errors:?}");
        assert_eq!(out, 30, "every frame encoded");
        assert_eq!(sizes, [(322, 180)].into(), "locked to the first size, even");
    }

    #[test]
    fn caps_carry_memory_features() {
        crate::init().unwrap();
        let c = raw_caps(Some("memory:DMABuf"), |s| {
            s.set("framerate", gst::Fraction::new(60, 1))
        });
        assert_eq!(
            c.to_string(),
            "video/x-raw(memory:DMABuf), framerate=(fraction)60/1"
        );
        let c = raw_caps(None, |_| {});
        assert_eq!(c.to_string(), "video/x-raw");
    }
}
