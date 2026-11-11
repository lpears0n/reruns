# Reruns on Windows

The Windows build captures the **game's window** with **Windows Graphics
Capture** and keeps every frame on the GPU: `d3d11screencapturesrc` (WGC
window capture) hands D3D11
textures to the encoder (NVENC, AMD AMF, Intel Quick Sync or Media
Foundation) through `d3d11convert`, and only the encoded packets reach the
ring buffer. Nothing per frame runs on the CPU. Game detection walks the
top-level windows every two seconds (a few hundred microseconds). The
hotkey is a native `RegisterHotKey`, which fires over fullscreen games.

This is the design that costs a game the least: WGC is the same path the
Xbox Game Bar uses, it never hooks the game's swap chain, and the
compositor's copy is the only extra work the GPU does.

## Status

- Compiles for Windows (the core crates are cross-checked in CI on
  `windows-latest` and locally against the MinGW target).
- **Not yet run on a Windows machine.** The first run is the verification
  step. Things most likely to need a tweak: element property names on
  `d3d11screencapturesrc` across GStreamer versions, and the AMF/QSV
  property names (unknown ones are skipped, so the worst case is default
  encoder settings).
- The plugin list in `scripts/collect-gstreamer.ps1` is the first thing
  to check if an element is missing. The script now refuses to finish
  unless every element the app makes by name loads from the bundle alone;
  the first installer shipped without `videotestsrc`, which the encoder
  probe uses, so no encoder ever passed and nothing recorded.
- Zero-copy is the first path tried; if D3D11 negotiation fails, the
  recorder rebuilds on the system-memory path on its own and Settings →
  Recording says which is active.
- **Only the game's window is captured** (WGC window capture, client
  area), like Medal: notifications, Discord and other monitors never end
  up in clips. There is deliberately no fallback to monitor capture. If
  the game replaces its window (splash screen, fullscreen switch) the
  recorder follows the new one; with several windows it takes the
  largest. Whether WGC can capture a given game in exclusive fullscreen,
  and how it does for frame pacing next to monitor capture (which made
  Windows compose the game and caused constant hitching in Rainbow Six
  Siege with G-Sync on), is still to be confirmed on real machines.
- **In-game popup** ("Ready to record", "Clip saved"): a transparent,
  click-through, never-focused window kept on top of the game for a few
  seconds, not a hook into the game (which anti-cheat flags). It is shown
  with `SW_SHOWNOACTIVATE` so a fullscreen game keeps focus, placed at the
  game window's top-right, and skipped when Reruns itself is in front.
  While visible it makes Windows compose the game, so it stays brief.
  Untested so far: whether it appears over exclusive-fullscreen games.
- **Fixed encode size.** A window can change size mid-game (resolution
  or display-mode switches); passing that on made the pipeline fail to
  renegotiate ("not-negotiated") on the owner's machine, and with
  restart-on-failure that became a restart every couple of seconds. The
  encoder now runs at the size the window had when recording started
  (its monitor's, if the window was tiny then); every frame is scaled
  to it on the GPU, letterboxed if the shape changed. Repeated failures
  back off (at once, 5 s, 15 s, 30 s, then every minute) and a restart
  keeps the play session (no second "Ready to record").
- The capture source now runs at the recording frame rate. Left to
  itself it settled on 30 fps and every clip had each frame twice.
- The app logs to `%LOCALAPPDATA%\gg.reruns.app\logs\Reruns.log` (there is
  no terminal to print to). A crash shows a message box with the reason and
  that path. `RUST_LOG=debug` and `GST_DEBUG=2` in the environment make the
  log more talkative; `GST_DEBUG` names any element that fails to load.

## Building

Install:

1. Rust (MSVC toolchain) and Node 22 with pnpm.
2. GStreamer **MSVC** 1.26 runtime and development installers from
   https://gstreamer.freedesktop.org/download/ (complete install). They set
   `GSTREAMER_1_0_ROOT_MSVC_X86_64`; add `%GSTREAMER_1_0_ROOT_MSVC_X86_64%\bin`
   to `PATH` and set `PKG_CONFIG_PATH` to
   `%GSTREAMER_1_0_ROOT_MSVC_X86_64%\lib\pkgconfig`.
3. `pkg-config` (`choco install pkgconfiglite`).

Then, from `app/`:

```
pnpm install
pnpm tauri dev
```

`pnpm tauri build` produces an NSIS installer under
`target\release\bundle\nsis\`. Run `pwsh scripts/collect-gstreamer.ps1`
first: it copies GStreamer's libraries and the plugins Reruns uses into
`app/src-tauri/gst/`, and `tauri.windows.conf.json` bundles them next to
the executable (libraries) and under `gst-plugins/` (plugins). At startup
the app points GStreamer at that folder, so users install nothing else.
The release workflow does this automatically.

## What is platform-specific

| Concern | Linux | Windows |
|---|---|---|
| Capture | PipeWire (KWin direct or portal), DMA-BUF zero-copy | WGC via `d3d11screencapturesrc`, D3D11 zero-copy |
| Encoders | NVENC (CUDA), VA-API, x264 | NVENC (D3D11), AMF, QSV, Media Foundation, x264 |
| Audio | `pipewiresrc` (sink monitor, mic node) | `wasapi2src` (loopback, device id) |
| Game detection | `/proc` environ (`SteamAppId`) and Wine exes | window walk, exe under a Steam library, or a fullscreen window |
| Hotkey | KGlobalAccel / GlobalShortcuts portal | `tauri-plugin-global-shortcut` (`RegisterHotKey`) |
| Notifications, sound | D-Bus, freedesktop sounds | `tauri-plugin-notification`, `PlaySound` from memory (GStreamer for non-WAV files) |
| Autostart | XDG autostart entry | `HKCU\...\Run` value |
| Control socket | Unix socket in `$XDG_RUNTIME_DIR` | named pipe `\\.\pipe\reruns-<user>` |
| Disk space | `statvfs` | `GetDiskFreeSpaceExW` |

Everything else (ring buffer, save, export, library, thumbnails, watcher,
settings, the whole UI) is shared.
