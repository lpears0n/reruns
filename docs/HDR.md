# HDR

Status and plan for HDR games. Windows first; Linux (KWin HDR) later.

## Where it stands

- **Recording is SDR only.** The Windows capture element,
  `d3d11screencapturesrc` (GStreamer 1.26), hands over 8-bit BGRA in
  both its WGC and DXGI paths (the DXGI path even carries a FIXME about
  10-bit). The encoder writes 8-bit H.264/HEVC/AV1 and nothing in the
  pipeline carries colour-space metadata.
- **On an HDR display, HDR games clip.** Windows converts the HDR
  composition to the 8-bit capture format without tone mapping, so
  highlights above SDR white blow out and colours can look off. SDR
  games on an HDR desktop come out roughly right.
- **Detection exists** (this is step 1 below): when recording starts,
  Reruns checks whether the game's monitor runs in HDR (DXGI output
  colour space BT.2020/PQ), reads Windows' SDR white level and the
  display's peak brightness, logs them, and shows an "HDR display" row
  under Settings → Recording.

## What GStreamer offers (checked against the 1.26, 1.28 and main sources)

- `d3d11screencapturesrc`: BGRA only, WGC and DXGI. No HDR.
- `d3d12screencapturesrc`: `{ BGRA, RGBA64_LE }`. On an HDR monitor its
  **DXGI** path duplicates in `R16G16B16A16_FLOAT` and tone-maps to SDR
  (`tonemap` = linear or Reinhard, using the SDR white level), output
  labelled sRGB. Its **WGC** path is still `B8G8R8A8`, so window capture
  gets no HDR. Unchanged on main.

So stock elements give HDR-correct output only for whole-monitor DXGI
capture, and Reruns records only the game's window (owner's decision:
like Medal, nothing else on screen ends up in clips).

## HDR compatibility (Settings → Recording)

A toggle, off by default. When it is on and the game's display is in
HDR, recording uses `d3d12screencapturesrc` through DXGI Desktop
Duplication, cropped to the game window's area, with Reinhard tone
mapping at Windows' SDR white level; `d3d12convert` scales it to the
fixed encode size and `d3d12download` hands it to the usual D3D11
encoders, so it stays on the GPU. The trade-off, stated in the UI: it
records the game's area of the screen, so a notification or overlay
drawn on top of the game in that area can appear in clips. If that
capture can't start, recording falls back to window capture (SDR,
highlights clipped). Not yet run on an HDR machine.

## HDR window capture (experimental)

Under HDR compatibility, a second toggle: **Capture the window in HDR**,
off by default. It is step 2 of the plan below, built. When it is on and
the display is in HDR, recording tries it first, then the screen-area
capture above, then plain window capture.

- `win/hdr_window.rs` runs its own Windows Graphics Capture session on
  the game window with an `R16G16B16A16Float` frame pool (scRGB).
- Each frame goes through one D3D11 pass (`tonemap::SHADER`):
  - scale so the display's SDR white is 1.0;
  - leave everything below 75% of SDR white as it is;
  - roll the rest off with an extended-Reinhard curve that meets the
    knee smoothly and reaches 1.0 exactly at the display's peak;
  - apply it to the brightest channel, so hues hold;
  - encode with the sRGB curve.
- The pass also crops to the client area (no title bar) and letterboxes
  into the fixed encode size.
- It draws straight into BGRA textures from GStreamer's own D3D11 buffer
  pool, on a GStreamer D3D11 device that the whole pipeline shares
  through its context. `d3d11convert` and the hardware encoder take the
  frames without a copy.
- `tonemap.rs` holds the same curve on the CPU, with tests: below-knee
  identity, peak → 1.0, a smooth knee, rising throughout, hue kept, and
  the constant-buffer layout.

Safety:

- GStreamer's D3D11 helper library is loaded at run time, so a missing
  function only disables this capture; it can't break the build or the
  app.
- If the capture can't start, or fails while recording, Reruns uses
  the screen-area capture for the rest of the session.
- Not yet run on an HDR machine. The shader is compiled by Windows when
  the capture starts (`D3DCompile`), so a shader error also just falls
  back.

## When the SDR white level can't be read

Seen twice on the owner's PC (2026-11-14, 03:46 and 03:54 local, game
minimized, most likely the display asleep): Windows reported the display
in HDR but neither its SDR white level nor its peak. The recorder used
to log that as "80 nits" as if it were measured. Now an unread level is
0 (like an unread peak), logged as a warning, shown in Settings as
"SDR white unknown yet, 80 nits assumed", and asked again on every scan
(2 s). Once Windows answers, the status takes the real level, and an HDR
window capture (which baked the guess into its shader) restarts with it.
The screen-area capture needs no restart: GStreamer re-reads the level
each time Desktop Duplication reopens, which it does when the display
comes back. Not reproduced on demand: the display answered 240 / 456
nits on every read while awake.

## Window-only HDR (checked 2026-11-14)

- GStreamer main's `d3d12screencapturesrc` WGC path
  (`gstd3d12graphicscapture.cpp`) still makes a `B8G8R8A8` frame pool and
  has no HDR handling; its HDR tone mapping is DXGI-only. Nothing stock
  captures one window in HDR.
- OBS does window-only HDR through the same route as ours: WGC with FP16
  frames, converted with the SDR white level.
- Hooking the game's swap chain (OBS Game Capture) is window-only and
  HDR-capable but injects into the game: anti-cheat risk, not for Reruns.
- So the window-only path is ours, already built: "Capture the window in
  HDR" above. Its one logged real attempt (2026-11-05) **started**
  ("HDR window capture started (FP16, tone-mapped on the GPU)") and then
  died on NVENC "Failed to open session", the driver problem fixed in
  1.1.x, not on the capture. Next: one session with it on, on the
  owner's HDR display, then a clip compared with the screen-area capture.
  If it holds, make it the HDR compatibility default.

## Plan

1. **Detect and report** (done). Gives the owner's machine a way to say
   whether it is affected, and gives step 2 its parameters.
2. **Tone-mapped SDR from HDR games, window capture kept** (built,
   experimental; see above). Reruns's own
   WGC window capture (Rust, `windows` crate) requesting
   `R16G16B16A16Float` frames (scRGB, linear, BT.709 primaries, 1.0 =
   80 nits), a D3D11 pixel shader that scales by the SDR white level,
   tone-maps highlights (Reinhard or a filmic curve with the display's
   peak as the knee), converts to sRGB and writes BGRA, then hands the
   texture to GStreamer as D3D11 memory (frames from GStreamer's own
   D3D11 buffer pool, rendered into in place) so the existing zero-copy
   chain into NVENC/AMF/QSV is unchanged. Only used when the display is in
   HDR; SDR displays keep `d3d11screencapturesrc`. This is what Medal
   does, and the result plays correctly everywhere (Discord, phones).
3. **Optional HDR10 recording.** Same FP16 capture; the shader writes
   BT.2020 primaries with the PQ curve into P010; HEVC Main10 or AV1
   10-bit with mastering-display and content-light metadata
   (`nvd3d11h265enc` and the AMF/QSV elements take these from caps);
   `mp4mux` writes `colr`. The app's player and thumbnails then need to
   tone-map for SDR screens. Off by default: most places clips are
   shared show HDR video washed out.

Per-frame cost of step 2 on the GPU is one full-screen shader pass,
the same order as the `d3d11convert` the zero-copy path already runs.

## Open questions

- What WGC's 8-bit capture of an HDR game actually looks like on the
  owner's machine (blown out vs washed out): a clip from an HDR game
  with HDR on answers it.
- Whether `nvd3d11h265enc` accepts mastering-display caps on the
  bundled 1.26 (needed only for step 3).
