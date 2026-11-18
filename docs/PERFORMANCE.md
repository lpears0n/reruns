# Reruns performance rules and budgets

The app runs next to a game and must never cost it frames. These rules are
binding for UI and backend work. (Sections keep the numbers they had in the
old design guide, which code comments still cite as §3–§5.) Visual design
lives in `docs/DESIGN.md`.

---

## 3. Frontend performance

The webview on Linux is WebKitGTK. It is a capable engine with two
well-documented weak spots, and both are things the current UI does.

### 3.1 WebKitGTK rules

1. **No `backdrop-filter` on anything repeated, or over anything that
   scrolls.** Multiple shipped Tauri apps measured it: a list scrolling at
   50–57 fps dropped to 14 fps with glass panels on Intel; folder
   navigation went from ~450 ms to ~60 ms after removing a few blurred
   controls. Two uses are allowed, each one element at a time over a
   still page: a card's actions, which carry the blur only while the card
   is hovered, and the lightbox's backdrop. Neither has been measured on
   WebKitGTK yet; if the lightbox is slow there, drop its blur first (the
   88% fill stands without it).
2. **No blurred `box-shadow` on repeated elements.** Modrinth found most of
   the frame time in WebKitGTK went to the blur filter while painting a
   shadow, and replaced it with a 1 px border. Keep one shadow token for
   overlays only.
3. **Make sure GPU compositing is actually on.** Luminous discovered it had
   `WEBKIT_DISABLE_COMPOSITING_MODE` and `WEBKIT_DISABLE_DMABUF_RENDERER`
   set unconditionally as an old AppImage workaround; removing them took
   theme-change frames from ~500 ms to ~27 ms and idle CPU from 83% to <1%.
   Tauri's own Linux graphics page says the same: never ship those
   variables unconditionally. Reruns does not set them. If NVIDIA users
   report blank windows, the first fix is `__NV_DISABLE_EXPLICIT_SYNC=1`
   (no cost), then the DMABUF variable, only for that user.
4. **Scrolling over fixed blurred/shadowed layers is the worst case.** Our
   selection toolbar and toasts are fixed over a scrolling grid: hairline
   plus solid fill, one shadow, no blur.
5. **Perpetual CSS animation keeps the compositor awake.** Remove
   `animate-ping`.
6. **`<video>` decode:** WebKitGTK plays video through GStreamer and uses
   VA-API only when the `va` plugin is present; NVIDIA has no VA-API, so
   playback of 4K60 HEVC/AV1 clips is software-decoded. Keep the trimmer
   preview at display size, and add the proxy-transcode fallback from the
   roadmap for clips the webview reports as dropping frames (we already
   log `getVideoPlaybackQuality`).

### 3.2 Rendering budget

- 60 fps while scrolling the grid on an Intel iGPU; measured with
  `getVideoPlaybackQuality` for video and a `requestAnimationFrame` frame
  timer in dev builds.
- Library of 2 000 clips: first paint < 150 ms after the data arrives,
  scroll without long tasks > 50 ms.
- Idle CPU < 1% with the window open and nothing playing.

How to meet it:

- **Virtualise the grid above ~300 cards.** Svelte 5 has no built-in
  windowing; `@humanspeak/svelte-virtual-list` or a 60-line own
  implementation (row height is known: cards are 16:9 plus 44 px). Below
  that, `content-visibility: auto` with `contain-intrinsic-size` on each
  card lets the engine skip offscreen layout and paint for free.
- **Keep large arrays out of deep `$state`.** A Svelte issue shows 10k-item
  proxied arrays freezing the app. `allClips` should be `$state.raw` and
  replaced wholesale, with filtered/sorted views as `$derived`.
- **Images:** covers at 960 px wide for large cards at desktop scaling,
  WebP (JPEG where the encoder is unavailable), `loading="lazy"`,
  `decoding="async"`, explicit `width`/`height` so nothing reflows when
  they land. The cover is selected from the existing decode samples with a
  bounded luminance/detail score; manual cover timestamps stay authoritative.
  Keep sprite frames independently sized at 160 px so larger covers do not
  multiply strip memory. Cache versions invalidate lower-resolution covers.
- **Hover previews** are the right idea but should start after 450 ms
  (done), be limited to one at a time (done by hover), and should use a
  low-bitrate proxy or a sprite sheet rather than the full clip, since a
  100 Mbps HEVC clip is heavy to decode for a 300 px preview. Sprite
  sheets are what every video platform uses: one pass with GStreamer,
  10 frames at 160×90, one WebP, scrub by `background-position`.
- **Ambient lighting:** one static gradient layer per library/viewer, and
  a radial gradient on the hovered or focused card. No blur filters,
  per-card `will-change`, continuous animation or video-frame sampling.
  Extract colour from a 24 × 24 copy of an image once, cache by image URL,
  and defer card sampling until its first hover/focus. Reuse that colour
  through scrubbing, playback and returning to the library.

### 3.3 IPC and data flow

- **Events, not polling.** Status polls every 2 s, usage every 15 s, the
  library every 60 s. Each poll serialises the whole list over IPC and
  re-renders. The backend already knows when things change: emit
  `status-changed` on every transition (it does), emit `usage-changed`
  after save/delete/enforce, and run a `notify` watcher on the clips
  folder (debounced 300 ms with `notify-debouncer-full`) so external
  changes arrive as `library-changed` within a second. Then remove all
  three intervals.
- **Send deltas for big lists.** `list_clips` returns every clip every
  time. Add `list_clips_since(generation)` or send `clip-added` /
  `clip-removed` events with one `Clip`; a full list only on first load.
  Tauri's guidance is to paginate or stream large payloads, since IPC goes
  through the webview's fetch path.
- **Images through the asset protocol** (already), never base64 through
  commands.
- **Coalesce** bursts: a save triggers `clip-saved`, `library-changed`,
  `usage-changed`; the frontend should debounce re-renders to one frame.

### 3.4 Startup

- Target: window visible with the last library state in < 400 ms on a warm
  cache. Means: cache the last `list_clips` result as JSON in the app data
  dir, render it immediately, reconcile when the real list arrives.
  Encoder probing (test-encodes) must never gate the first paint; it does
  not today, keep it that way.
- Fonts: both Inter and Archivo are bundled; subset to Latin and preload
  the two weights used, or the first paint flashes fallback type.

---

## 4. Backend performance

### 4.1 Capture: zero-copy path

This is the single biggest performance item and the README's known
stutter. Today every frame goes PipeWire → system memory → encoder. The
measured gap, from a project doing exactly our pipeline on an RTX 3060 at
2560×1600: **DMA-BUF → `glupload` → `nvh264enc` held 59.7 fps; system
memory → `cudaupload` → `nvh264enc` managed 37.4 fps.** The copy is the
bottleneck under load, not the encoder.

Plan, by vendor, all inside the existing GStreamer pipeline:

- **NVIDIA:** `pipewiresrc` negotiating `video/x-raw(memory:DMABuf)` →
  `glupload` → `glcolorconvert` → `nvh264enc` / `nvav1enc` (nvcodec accepts
  GL memory via CUDA-GL interop). Fallback to today's path if caps fail.
- **AMD / Intel:** `pipewiresrc` (DMA-BUF) → `vapostproc` → `vah264enc`
  etc. `vapostproc` imports DMA-BUF directly; this is the chain FluxCast
  ships. Known pitfall: `pipewiresrc always-copy=true` yields buffers with
  no memory on DMA-BUF portals, so it must stay `false` on this path.
- **Keep the ring buffer as is.** It stores encoded packets, which are
  small; zero-copy only changes the capture→encode leg.
- Verify with `GST_DEBUG=GST_CAPS:5` that no `videoconvert` sneaks in, and
  measure dropped frames at the encoder (`nvh264enc` exposes QoS stats).

### 4.2 Library index and thumbnails

- `list_clips` stats every file and reads every sidecar on each call, and
  generates missing thumbnails inline, so a first launch with 500 clips
  does 500 decodes before the UI shows anything.
  - Return the list immediately with `thumbnail: null` for anything
    uncached, then generate thumbnails on a bounded worker pool (2 threads;
    GStreamer decode is itself threaded) and emit `thumbnail-ready` per
    clip. The UI already handles a missing thumbnail.
  - Cache the parsed list (path, mtime, size, duration, tracks, meta) in
    one `index.json` or a small SQLite file in the cache dir, keyed by
    mtime+size like the thumbnails are now. Startup then reads one file
    instead of N+N.
- Thumbnails: WebP, 320×180, quality 80, ~8 KB each. Sprite sheet
  alongside for hover scrub. Generate both in one decode pass.
- Watch the clips folder with `notify` (inotify) on a blocking thread, not
  the Tokio runtime, with a 300 ms debounce; the UI never polls.

### 4.3 Threads and the runtime

- Anything that touches the filesystem or GStreamer stays on
  `spawn_blocking` (already the pattern). Audit `list_games` and
  `get_capabilities` for the same.
- Encoder probing results are cached per machine in the config dir with
  the driver version, so the test-encode runs once per driver update, not
  per launch.
- Storage enforcement walks the folder after every save; use the index
  instead of a walk.

### 4.4 Playback

- Serve range requests (done) and `Content-Length`; add `Cache-Control:
  immutable` since clip files never change in place.
- For the trimmer, seek accuracy matters more than throughput: encode
  clips with a 1 s GOP (done), and for `,`/`.` frame stepping request
  `video.requestVideoFrameCallback` where available to land on real frame
  boundaries rather than 1/60 s guesses.

---

## 5. Measurable budgets

| Metric | Budget | How measured |
|---|---|---|
| Capture frame drops under full GPU load | < 0.5% over 60 s | encoder QoS counters, logged |
| Library scroll frame time (p95) | ≤ 16.7 ms | rAF timer in dev builds |
| First paint with cached library | < 400 ms | `performance.now()` at mount, logged |
| 2 000-clip list render | < 150 ms | same |
| Idle CPU, window open | < 1% | `top` |
| Memory, 2 000 clips, 30 s buffer at 50 Mbps | < 450 MB | `smem` |
| Hover preview start | < 300 ms after the 450 ms dwell | manual |
| Any UI animation | ≤ 200 ms | code review |
