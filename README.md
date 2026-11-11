# Reruns

An instant replay recorder for games on Windows and Linux, kind of like Medal. It keeps the last 30 seconds (or however long you set) in memory while you play, and when you hit F8 it saves that to an MP4 named after the game. It also has a basic trimmer for cutting clips and mixing the game and mic audio.

## Download

Installers for Windows (`.exe`) and Linux (AppImage, `.deb`, `.rpm`) are on the
[releases page](https://github.com/lpears0n/reruns/releases). The Windows
installer bundles everything it needs.

## How to run it locally

You'll need Rust, Node with pnpm, and GStreamer with the PipeWire and VA-API plugins. On Arch that's roughly:

```
sudo pacman -S rustup nodejs pnpm gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugin-pipewire gst-plugin-va gst-libav webkit2gtk-4.1 libayatana-appindicator
rustup default stable
```

Then, from the repo root:

```
cd app
pnpm install
pnpm tauri dev
```

To build a release binary instead, use `pnpm tauri build`. `pnpm test:ui`
drives every view in a headless Chromium with the Tauri bridge mocked and
leaves screenshots under `app/tests/shots/`; it runs in CI too. There's also a small CLI, `rerunsctl save` (and `rerunsctl screenshot`, `rerunsctl mark`), that talks to the running app. Bind it to a key in your desktop settings if the in-app hotkeys don't work for you.

Releases, updates and Windows code signing: [docs/RELEASING.md](docs/RELEASING.md).

## How it works / Tech stack

- **Tauri 2 + Svelte 5** for the UI. Tauri uses the system webview, so the app stays small compared to Electron.
- **GStreamer** does all the capture, encoding and muxing. It's a pipeline: screen, then encoder, then a ring buffer in memory. Saving copies references to the buffered frames into a new MP4 pipeline, so it doesn't re-encode anything.
- **NVENC / VA-API / x264** for encoding. It probes which one actually works on your machine and uses the best one.
- **KWin's private screencast protocol** on KDE, so it can record a game window without a permission popup. Everywhere else it falls back to the standard screen-share portal, which asks once and remembers your choice.
- **Process scanning** (`/proc`) to find running games, looking for Steam's app id or a Wine `.exe`. Works on any desktop.
- **ashpd** for the portal calls, and **zbus** for the KDE and notification D-Bus stuff.
- **Tailwind 4** for styling, plus Lucide icons.

## Using it

- **Games** shows what you've played, with a hero for the most recent game.
- **All clips** has search (`Ctrl+F`), sorting, a game filter and a selection
  mode for deleting many clips at once. Deletes can be undone for a few
  seconds from the toast.
- Hover a clip for a silent preview; open it to watch or trim. Press `?` in
  the viewer for every keyboard shortcut (speed, frame stepping, in/out
  points, repeat).
- When trimming, pick a size to render under, such as 10 MB for Discord.
  The bitrate comes from the clip's length, the picture drops to 720p or
  480p (and 30 fps) when it has to, and if a clip is too long to fit you
  are told how long would.
- Detected something that isn't a game? The × next to its name in the
  top bar stops recording it for good (undo under Settings → Recording).
- **Settings → Audio** picks which output's sound is recorded as the game
  track (say, only your headset) and which microphone goes on its own
  track. An unplugged device falls back to the default. With the mic on,
  clips start with game and mic mixed into one track, the one every
  player (and Discord) plays; the separate tracks follow it for the
  editor's levels.
- The save sound can be any short file (WAV, MP3, OGG, Opus, FLAC; ten
  seconds at most), with its own volume and, if you like, a different one
  for screenshots: Settings → General. The file is copied in, so moving
  the original doesn't break it.
- On Windows a small popup over the game says **Recording** and the game's name when a
  game starts recording and **Clip saved** when you clip. It never ends up
  in clips; turn it off under Settings → General.
- Press `F9` in a game for a screenshot: the newest frame from the replay
  buffer, saved as a PNG next to the game's clips.
- Press `F7` to mark a moment. A clip saved later shows the marks inside it
  as ticks on the viewer's line; `[` and `]` jump between them. All three
  keys are set under Settings → Hotkeys.
- Star a clip (`S`), rename it (`N`) or add labels in the viewer. Favourites
  and labels are filters on the clips page, and they live in the small
  `.json` file next to each clip, so they survive moving the folder.
- **Share a clip**: Copy (or Ctrl+C in the viewer) and paste it into
  Discord, or drag a clip card out of the window. When trimming, Format
  makes a vertical 9:16 version for TikTok/Shorts (drag the frame to
  choose what's kept), and Slow-mo slows a range you set on the timeline.
- **Settings → Audio** shows a live level for each device, so you can see
  what a recording would hear before you play.
- **Settings → Appearance** picks the accent colour and card size, and turns
  off hover previews or motion.

The whole thing is a Cargo workspace: `crates/clip-core` has the capture and encoding code with no UI, `crates/clip-ipc` and `crates/clip-cli` are the `rerunsctl` command, and `app/` is the Tauri app.

## Current state / What's next

It works, but I've mostly tested it on my own machines: Windows and KDE Plasma 6 on Wayland, both with an NVIDIA card.

There's a design and performance guide in [docs/DESIGN.md](docs/DESIGN.md) and a roadmap with the research behind it in [docs/ROADMAP.md](docs/ROADMAP.md):
what's planned for the library and editor, how a Windows port would work
(GStreamer's Windows Graphics Capture and WASAPI elements, so the ring
buffer and trimmer carry over), and packaging. CI builds and lints every
push; tagging `v*` builds AppImage, deb and rpm as a draft release.

Known problems:
- **Recordings could stutter under heavy GPU load** because every frame was copied through system memory. There is now a zero-copy path (DMA-BUF straight into the encoder's GPU memory) that is tried first and falls back by itself; Settings → Recording shows which one is active. It hasn't been verified on real hardware yet.
- **Only KDE gets prompt-free window capture.** On GNOME and Hyprland you get a one-time screen-share prompt instead. That part is only partly tested.
- **The portal hotkey path isn't tested** on a real desktop yet.
- **X11 isn't supported.** There's no capture backend for it.
- **Windows is tested on NVIDIA only.** Game detection, window capture and NVENC zero-copy recording work there; AMD AMF and Intel Quick Sync, and HDR window capture, haven't been run on real hardware. See [docs/WINDOWS.md](docs/WINDOWS.md).
- **The Windows installer isn't code-signed yet**, so SmartScreen warns on first run.
- **Storage limits are a soft cap.** Old clips get deleted after a save if you go over, and the Trash option is on by default so you can recover them. Starred and edited clips are kept unless you turn that off in Settings → Storage.

What I'd do next, if I had more time: zero-copy capture for high frame rates (the current path copies every frame through system memory), the `ext-image-copy-capture` protocol for window capture on wlroots desktops, X11 support, and a proper package.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
