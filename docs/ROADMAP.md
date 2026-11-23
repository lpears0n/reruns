# Reruns roadmap

Where Reruns goes from "works on my KDE box" to a clipping app people
choose over ShadowPlay, Medal and Steam Game Recording: on Linux first, then
Windows. This is a working document; strike things out rather than deleting
them so the reasoning stays.

Research notes behind the choices are in [Sources](#sources) at the end.

## v2.0.0 checklist

Added 2026-11-18. 2.0.0 is the redesign (`codex/reruns-design-refinement`:
the rr identity, the library and clip workspace, Settings → Appearance →
Theme) plus Reruns Setup (`branded-installer`, built on the redesign). Tag
it when every box under "Must have" is ticked.

**Must have**

- [x] The 1.2.x fixes are on `main`: high-tier AV1 saves
  (`fix/av1-clip-save`; saves failed with `not-negotiated` on the owner's
  PC on 2026-11-18), audio devices whose id changed
  (`fix-renamed-audio-device`), and the log folder button
  (`open-log-folder`). AV1 came in through #40; the other two were
  applied to `main` directly.
- [x] The redesign is on `main` through a PR from a branch that follows
  `AGENTS.md` (#41, `feature/library-redesign`). Setup comes with it.
- [ ] Seen working in the real window on Windows (WebView2) and on Linux
  (WebKitGTK, KDE): library, clip workspace, trim, Copy clip, Save clip,
  Settings and themes. No redesign pass has run on Linux yet.
- [ ] Reruns Setup on a clean Windows install: fresh install, a PC
  without WebView2 (it hands over to the NSIS pages), Update from 1.2.x,
  Reinstall, uninstall. The in-app updater goes from 1.2.x to 2.0.0.
- [ ] Setup and the NSIS installer are code-signed
  ([RELEASING.md](RELEASING.md), Windows code signing), so SmartScreen
  doesn't block the first download.
- [ ] A 1.x library survives the upgrade: favorites, titles, tags,
  markers, edits, screenshots, `settings.json` and ignored apps. Known
  loss: the library's grouping preference resets (`grouping` became
  `groupBy`).
- [ ] Capture within budget on the HDR path: repeats plus holes under 0.5%
  (`examples/judder`, PresentMon running) on Rainbow Six Siege, Dead by
  Daylight and Deadlock. On 2026-11-18, Deadlock clips (HDR compatibility,
  AV1, 120 fps) scored 0.9–13%. Test "Capture the window in HDR" first and
  make it the default if it measures smooth ([HDR.md](HDR.md)).

**Should have** (changes stored data, so it belongs in a major version)

- [ ] A stable id in each clip's `.mp4.json` sidecar, so later APIs stop
  identifying clips by absolute path. Clips without one get it the first
  time they are listed. This is the first step toward watching clips from
  a phone.

**Not in 2.0.0**

- Watching clips from a phone over Wi-Fi: 2.x, after the stable ids and a
  library index.
- Montage, crop and GIF export: [DESIGN.md](DESIGN.md) keeps Reruns from
  growing into an editor.
- Per-application audio (Phase 3), replays longer than RAM allows, and
  wlroots capture: 2.x or later.

## 1. What we are competing with

| | ShadowPlay / NVIDIA App | Medal | Steam Game Recording | Reruns today |
|---|---|---|---|---|
| Platforms | Windows, NVIDIA only | Windows, mobile | Windows, Linux, Deck | Linux (Wayland) |
| Instant replay | yes | yes | yes (background mode) | yes |
| Hardware encode | NVENC | NVENC / AMF / QSV, CPU | yes | NVENC / VA-API / x264 |
| Codecs | H.264 / HEVC / AV1 | H.264 / HEVC / AV1 | H.264 | H.264 / HEVC / AV1 |
| Game detection | yes | yes, plus event detection | Steam games only | Steam app id / Wine exe |
| Event markers | no | game events (kills, wins) | timeline: achievements, screenshots, dev SDK | no |
| Editor | trim | trim, slow-mo, zoom, overlays, multi-track mix | trim, export | trim, multi-track mix |
| Sharing | file | instant upload, links, privacy, mobile sync | Steam share | "Show in folder" |
| Library | basic | search, labels, sort, cloud | per-game timeline | per-game, search, sort |
| Prompt-free window capture on Wayland | n/a | n/a | overlay-based | KDE only |

The gap that matters: Steam's recorder is free, cross-platform and already
on Linux. Reruns wins by being lighter, desktop-wide (not just Steam games),
GPU-encoded at native refresh rates, and by having the best editor and
library of the three. Medal's social layer is a different business; we can
match its *local* feature set without a backend.

## 2. Design principles (what "attractive" means here)

Taken from current dark-theme guidance and the apps gamers already use:

1. **Dark grey, never pure black, with elevation by tint.** Surfaces step
   from `#0d0d0f` (canvas) to `#17181b` (panel) to `#1e1f23` (raised); depth
   comes from lighter surfaces, not drop shadows. Already true; keep it.
2. **One desaturated accent, user-selectable.** The accent is the only
   saturated colour on screen. Bright orange on dark can glare, so the
   palette offers six accents (Appearance settings) and every accent-coloured
   surface derives from the one CSS variable.
3. **Dark mode is its own design, not an inversion.** There is no light
   theme and there will not be one until the app has a reason; gaming
   tools (Steam, Medal, Discord) are dark-only and users expect it.
4. **Meet WCAG AA contrast.** Body text `zinc-100` on panel is ~14:1,
   muted text `#8b8d96` on canvas is ~5.6:1. Any new colour must stay above
   4.5:1 for text and 3:1 for icons.
5. **Respect the OS.** Reduced-motion is honoured automatically
   (`prefers-reduced-motion`) with an in-app override, window size and
   position are remembered, and the system file manager, trash and
   notification service are used rather than reimplemented.
6. **Keyboard first.** Every viewer action has a key and the list is one
   keystroke away (`?`). Library search is `Ctrl+F`, selection is `Esc` to
   cancel and `Delete` to delete.
7. **Undo over confirm.** Destructive actions happen immediately in the UI
   and can be undone for six seconds, instead of a modal asking "Are you
   sure?". Clips also go to the desktop trash by default.
8. **Content is the hero.** Game art fills the top of the Games page,
   thumbnails fill cards, previews play on hover. Chrome stays muted so the
   clips carry the colour.

## 3. Done in this pass

- Library: search (`Ctrl+F`), sort (newest / oldest / longest / largest),
  game filter, selection mode with bulk delete and a size readout.
- Clip cards play a muted preview on hover (Medal's "quick clip previews").
- Deletes are deferred six seconds with **Undo** in the toast; the UI hides
  the clip at once.
- Viewer: playback speed (0.25–2×), frame stepping (`,` `.`), `J`/`L`
  10-second jumps, `Home`/`End`, repeat (`R`), `T` toggles trim, and a `?`
  shortcut sheet.
- Appearance settings: accent colour, card density, hover previews, reduced
  motion. Stored per machine in `localStorage`.
- First-run page is a setup checklist (capture state, hotkey, clips folder)
  with the fix one click away.
- Window size and position persist (`tauri-plugin-window-state`).
- CI (`.github/workflows/ci.yml`): svelte-check, Vite build, `cargo fmt`,
  `clippy -D warnings`, `cargo test` on Ubuntu with GStreamer installed.
- Release workflow (`release.yml`): AppImage, deb and rpm from a `v*` tag
  through `tauri-action`, as a draft release. Windows job is written and
  commented out.
- AUR `PKGBUILD` and a `.desktop` entry under `packaging/`.
- Design pass from `docs/DESIGN.md`: token-based theme (OKLCH surfaces,
  six type roles, radii by nesting, one shadow, no blur), labelled
  collapsible sidebar with the storage row, header carries the view title,
  `1`/`2`/`3` switch views, skeleton loading, designed empty and error
  states, render progress with cancel.
- Performance pass from `docs/DESIGN.md` steps 4–6: no polling (folder
  watcher, thumbnail-ready events, local clock for live counters, refresh on
  focus), coalesced refreshes, cached first paint, WebP thumbnails and
  sprite sheets generated by a two-thread pool off the request path, sprite
  scrubbing on hover, windowed clip grid above 120 clips.
- Favourites, labels and titles, stored in each clip's `.mp4.json` sidecar.
  Star on cards and in the viewer (`S`), rename inline (`N`), add and
  remove labels in the viewer; the Clips page filters by favourite and
  label, and search matches titles and labels.

## 4. Feature roadmap

Phases are ordered by user value per engineering day. Each phase should
ship on its own.

### Phase 1 — Library you can live in (next)

- ~~**Favourites and labels.**~~ Done. Stored in the `<clip>.mp4.json`
  sidecar so the folder stays the source of truth and survives sync tools.
- ~~**Rename / title.**~~ Done.
- **Bulk label / favourite** from selection mode, and label autocompletion
  from labels already in use.
- **Timeline scrubbing on hover.** Generate a 10×1 sprite of frames per clip
  alongside the thumbnail (`clip_core::media::inspect` already decodes one
  frame; take ten). Hover position maps to a sprite frame; cheaper than the
  video preview and works for clips the webview cannot decode.
- ~~**Screenshots.**~~ Done. A second hotkey (`F9`, or `rerunsctl screenshot`)
  decodes the newest GOP from the ring buffer and writes its last frame as a
  PNG next to the game's clips. Nothing is tapped while recording, so the
  capture path pays nothing for it. The hotkey layer now carries one binding
  per action, which is what markers need next.
- **Game page polish.** Session list ("Played 2h 14m on Tuesday, 3 clips"),
  jump to a session's clips.

### Phase 2 — Editor worth opening

- **Multi-clip export / "montage".** Select clips, order them, render one
  file. GStreamer `concat` over the existing `export::render`.
- ~~**Slow motion**~~ Done: a range inside the trim at ½× or ¼× (striped
  on the timeline, its own handles; the preview slows there too). Export
  retimes the decoded streams with a pad probe (frames spread out, the
  segment end moved) and drops the range's audio, filled with silence by
  `audiorate`. Still to do: speed ramps, and slowed audio
  (`scaletempo`) as an option.
- **Crop / zoom** with a draggable rectangle on the preview (`videocrop`
  + `videoscale`).
- ~~**Export size presets.**~~ Done: Full quality, 10 MB (Discord), 50 MB
  (Nitro Basic), 500 MB (Nitro). The bitrate comes from the trimmed length
  with a margin for the container; when the budget is too thin for the
  full picture it steps down (1080p → 720p → 720p30 → 480p30 → 360p30,
  same pixel count for ultrawide) and audio drops to 96/64 kb/s. A file
  that still comes out over is redone at a rate scaled by the miss. Too
  long to fit says how long would. ~~9:16 vertical~~ done: a draggable
  9:16 frame over the preview, `videocrop` before the size plan scales
  it. Still to do: GIF (`gifenc` via libav).
- **Audio**: per-track mute buttons on the timeline, waveform strip under
  the trim track (`level` element at inspect time, cached next to the
  thumbnail).
- ~~**Markers.**~~ Done. `F7` (or `rerunsctl mark`) records the moment on
  the recording's clock; a save keeps the marks inside the clip, relative
  to its start, in the sidecar, and an edit carries the ones inside its
  trim. The viewer shows them as ticks on the line, `[` / `]` jump between
  them. This is the manual half of Steam's timeline and the foundation for
  event detection.

### Phase 3 — Capture quality

- ~~**Zero-copy capture**~~ Implemented with automatic fallback (see
  `docs/DESIGN.md` step 7); needs a run on NVIDIA and on AMD/Intel hardware
  to confirm the GL and VA paths negotiate.
- **`ext-image-copy-capture-v1`** for prompt-free window capture on wlroots
  (Hyprland, Sway, niri) now that compositors ship it.
- **X11** via `ximagesrc` as a fallback backend with `xdotool`-style active
  window tracking.
- ~~**Capture recovery on Linux.**~~ Done: a health tick restarts a
  failed recording with the same back-off as Windows (shared
  `RestartPolicy`), and the encode size is locked at the first frame so
  window resizes are scaled instead of renegotiated.
- ~~**Detection beyond Steam.**~~ Done on Windows: Epic, Riot, GOG,
  Ubisoft Connect, EA and Battle.net installs and Xbox/Game Pass folders
  are recognised even windowed; Microsoft Store apps are traced to their
  own process; "Not a game" (× in the top bar) ignores an app for good,
  undone under Settings → Recording.
- **HDR** (see `docs/HDR.md`): detection is done; next is tone-mapped
  SDR from HDR games through Reruns's own FP16 window capture, then
  optional HDR10 recording.
- **Audio per application** through PipeWire node targeting (record the
  game, not Discord), with Discord as its own optional track.
- **Replay length up to 20 min** with the ring spilling to disk (`splitmuxsink`
  style segments) above a RAM budget.
- **Overlay-free "clip saved" toast** rendered by the compositor is not
  possible on Wayland; keep the notification and sound, add a tray icon
  flash.

### Phase 4 — Share

- ~~**Copy to clipboard**~~ Done: Copy on cards and in the viewer
  (Ctrl+C) puts the clip on the clipboard as a file (`arboard` file list:
  CF_HDROP on Windows, uri-list on Linux), so pasting into Discord
  uploads it. Cards can also be dragged out of the window as files
  (`tauri-plugin-drag`).
- **"Share to…"** sheet using the desktop's share portal when available.
- **Optional uploads** to a user-chosen destination (S3-compatible, Nextcloud
  WebDAV, a self-hosted Reruns server later). No Reruns cloud; users bring
  their own.
- **Game event detection** (Medal-style) is deliberately last: it needs
  per-game log parsing or vision models, and markers + hotkeys cover 80%.

## 5. Platforms

### Windows (the big one)

The frontend, library, settings, storage and trimmer are already
platform-neutral. The work is in `crates/clip-core` and a few spots in
`app/src-tauri`. GStreamer is the right choice on Windows too: it has
first-party Windows Graphics Capture, WASAPI loopback and all three vendor
encoders, so the ring buffer, save and export code is reused unchanged.

**Capture.** `d3d12screencapturesrc capture-api=wgc` (GStreamer ≥ 1.26)
captures a monitor or a specific window (HWND) through the Windows Graphics
Capture API with the frame staying on the GPU. `d3d11screencapturesrc` with
WGC exists from 1.22 but a known bug distorts window capture when paired
with `mfh264enc`; prefer the D3D12 element and `d3d12download`/`d3d12convert`
only when an encoder needs system memory. WGC needs Windows 10 1809+; the
yellow capture border is removable on Windows 11 only.

**Encoders.** Probe in this order and reuse the existing "test-encode once"
logic: `nvd3d12h264enc` / `nvh264enc` (NVIDIA), `amfh264enc` (AMD),
`qsvh264enc` (Intel), `d3d12h264enc` (vendor-neutral, Windows 11), then
`mfh264enc` (Media Foundation, any GPU), then `x264enc`. Same for HEVC and
AV1 variants. `Encoder` gains `Amf`, `Qsv`, `MediaFoundation` variants;
`quality.ts` labels follow.

**Audio.** `wasapi2src loopback=true` for desktop audio and plain
`wasapi2src` for the mic; `loopback-mode=exclude-process-tree` with the
game's PID is available (Windows 10 build 20348+) when per-app audio lands.

**Game detection.** Replace `/proc` scanning with `CreateToolhelp32Snapshot`
(or the `sysinfo` crate) plus the foreground window's PID. Steam games are
identified by the `SteamAppId` environment variable of the process, or by
matching the exe path against `steamapps/*.acf` under the library folders
from `HKCU\Software\Valve\Steam\SteamPath`. Non-Steam games: a fullscreen
or borderless foreground window whose process is not on a denylist, same
heuristic as today's Wine `.exe` path.

**Hotkeys.** `global-hotkey` (what `tauri-plugin-global-shortcut` wraps)
uses `RegisterHotKey`, which fires over borderless-fullscreen games and most
exclusive-fullscreen ones; keep `rerunsctl save` + the IPC socket (named pipe
on Windows) as the fallback.

**Status (November 2026):** steps 1 and most of 2 below are done; see
`docs/WINDOWS.md`. The code compiles for Windows and CI checks it on
`windows-latest`, but it has not run on a Windows machine yet.

**Hard parts, in order.**

1. `clip-core` compiles on Windows: gate `kwin`, `portal`, `hotkey`
   (ashpd), `display` (Wayland) and `game` (`/proc`) behind
   `#[cfg(target_os = "linux")]`, move `wayland-*`, `ashpd`, `zbus` to
   `[target.'cfg(target_os = "linux")'.dependencies]`, and introduce a
   `platform` module with `CaptureBackend`, `GameWatcher`, `HotkeyService`
   traits implemented per OS. The app crate's `system.rs` (D-Bus
   notifications, freedesktop sounds) and `ipc_server.rs` (Unix socket) get
   Windows counterparts (`tauri-plugin-notification`, `windows::Media`
   `SystemSounds`, named pipes).
2. Playback in WebView2: H.264 plays, HEVC needs the HEVC Video Extension
   from the Store, AV1 needs the AV1 extension and is still flaky. The
   viewer already streams from our loopback media server; add a
   "transcode a proxy for preview" fallback (`decodebin ! x264enc` at low
   bitrate into the cache) when the webview reports it cannot play a codec.
3. Packaging: `tauri-action` on `windows-latest` builds NSIS/MSI; bundle
   GStreamer's MSVC runtime (`gstreamer-1.0-msvc-x86_64` ~ 150 MB
   trimmed to the plugins we use) via `bundle.resources` and set
   `GST_PLUGIN_PATH` at startup. Sign with an EV cert eventually; until then
   SmartScreen will warn.

Estimated size: two to three weeks for a first alpha on NVIDIA, with the CI
matrix entry in `ci.yml` and the commented job in `release.yml` switched on.

### macOS (later)

ScreenCaptureKit via GStreamer's `avfvideosrc` (`capture-screen=true`) or a
thin Swift bridge, `vtenc_h264_hw` / `vtenc_h265_hw` encoders,
`osxaudiosrc` with an aggregate device or a loopback driver for desktop
audio (there is no system loopback API before macOS 13's
`CATap`). Fewer gamers, but the Tauri and GStreamer story is the same, and
the editor alone is useful. Do it after Windows is stable.

### Steam Deck

Already Linux/KDE; the KWin path works in desktop mode. Game mode uses
Gamescope, which does not expose the KWin protocol: add a Gamescope-aware
backend using the portal with the restore token (already supported) and a
Decky plugin or Steam Input binding for the hotkey. Low effort, high
visibility.

## 6. Engineering foundations

- **Frontend tests.** Vitest for `format.ts`, `quality.ts` and the sort /
  filter logic in `ClipsView` (extract to `library.ts`); Playwright smoke
  test with the Tauri bridge mocked (the mock in
  `app/tests/mock-tauri.ts` can start from the screenshot harness used to
  review this pass).
- **Rust tests.** `ring.rs`, `save.rs`, `game.rs` have tests; add
  `export::render` round trips with `videotestsrc` in CI (GStreamer is
  installed there now).
- **Crash and log capture.** `tracing` to a rolling file under the cache
  dir; "Copy diagnostics" button in Settings → General.
- **Auto-update.** `tauri-plugin-updater` once releases are signed; AppImage
  and the Windows installer support it, AUR/Flatpak do not need it.
- **Telemetry: none.** The ad slot stays off by default and opt-in.

## 7. Packaging & distribution

| Channel | Status | Notes |
|---|---|---|
| AppImage / deb / rpm | release workflow | Built on Ubuntu 22.04 for glibc reach |
| AUR (`reruns`) | PKGBUILD ready | Publish after the first tagged release |
| Flathub | later | Needs `--socket=pipewire`, `--talk-name=org.kde.KWin`; wait for the portal path to be default |
| Windows NSIS / MSI | after port | Bundle GStreamer runtime; code signing |
| winget / Scoop | after port | Manifests are trivial once the installer exists |
| Homebrew cask | after macOS | |

## Sources

Design and dark-theme guidance:
- Google Material, dark theme (surfaces, elevation by tint, desaturated accents):
  https://m2.material.io/design/color/dark-theme.html
- "Beyond the toggle: rules for dark mode UX in 2026", DEV Community:
  https://dev.to/brandvix/beyond-the-toggle-4-rules-for-perfecting-dark-mode-ux-in-2026-4j55
- "Dark mode UI design best practices", UX Planet:
  https://uxplanet.org/dark-mode-ui-design-best-practices-8d3a00a83924
- WCAG 2.2 contrast minimum: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html

Competitors:
- Medal features: https://medal.tv/features
- Medal changelog 2025: https://blog.medal.tv/posts/medal-changelog-updates-2025
- Medal quick clip previews: https://medium.com/medal-tv/introducing-quick-clip-previews-73d39c7a8228
- Steam Game Recording release coverage (timeline, markers, modes):
  https://pcgamer.com/software/platforms/steam-game-recording-is-now-available-for-everyone-and-its-packed-with-neat-features
  https://www.club386.com/steam-gets-built-in-game-recorder-that-works-on-steam-deck-too

Windows capture, audio and encoding:
- GStreamer `d3d12screencapturesrc` (WGC since 1.26):
  https://gstreamer.freedesktop.org/documentation/d3d12/d3d12screencapturesrc.html
- `mfh264enc` distortion with WGC window capture (issue 2537):
  https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2537
- D3D12 → D3D11 conversion discussion: https://discourse.gstreamer.org/t/d3d12-to-d3d11-conversion/4813
- GStreamer `wasapi2src` (loopback, process loopback):
  https://gstreamer.freedesktop.org/documentation/wasapi2/wasapi2src.html
- `windows-capture` crate (WGC + DXGI from Rust, if we ever bypass GStreamer):
  https://docs.rs/windows-capture
- WebView2 AV1 / HEVC support thread: https://github.com/MicrosoftEdge/WebView2Feedback/issues/3801
- `tauri-plugin-global-shortcut`: https://docs.rs/tauri-plugin-global-shortcut

Packaging:
- Tauri GitHub Actions pipeline: https://v2.tauri.app/distribute/pipelines/github
- `window-vibrancy` (Mica / acrylic, if we want a native Windows 11 look):
  https://crates.io/crates/window-vibrancy
