# Reruns

An instant replay recorder for games on Windows and Linux, kind of like Medal. It keeps the last 30 seconds (or however long you set) in memory while you play, and when you hit F8 it saves that to an MP4 named after the game. It also has a basic trimmer for cutting clips and mixing the game and mic audio.

## Download

Installers for Windows and Linux (AppImage, `.deb`, `.rpm`) are on the
[releases page](https://github.com/lpears0n/reruns/releases). On Windows,
download `Reruns-Setup-<version>.exe`; it bundles everything it needs. The
other `.exe` there is the update package the app fetches by itself.

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

Building on Windows is in [docs/WINDOWS.md](docs/WINDOWS.md#building).

To build a release binary instead, use `pnpm tauri build`. `pnpm test:ui`
drives every view in a headless Chromium with the Tauri bridge mocked and
leaves screenshots under `app/tests/shots/`; it runs in CI too. There's also a small CLI, `rerunsctl save` (and `rerunsctl screenshot`, `rerunsctl mark`), that talks to the running app. Bind it to a key in your desktop settings if the in-app hotkeys don't work for you.

Releases, updates and Windows code signing: [docs/RELEASING.md](docs/RELEASING.md).

## How it works / Tech stack

- **Tauri 2 + Svelte 5** for the UI. Tauri uses the system webview, so the app stays small compared to Electron.
- **GStreamer** does all the capture, encoding and muxing. It's a pipeline: screen, then encoder, then a ring buffer in memory. Saving copies references to the buffered frames into a new MP4 pipeline, so it doesn't re-encode anything.
- **NVENC / VA-API / x264** for encoding on Linux, and **NVENC / AMD AMF / Intel Quick Sync / Media Foundation** on Windows. It probes which one actually works on your machine and uses the best one.
- **Windows Graphics Capture** (`d3d11screencapturesrc`) records the game's window on Windows and keeps the frames on the GPU. HDR compatibility switches to Desktop Duplication (`d3d12screencapturesrc`), which records the screen area instead. Audio comes from WASAPI.
- **KWin's private screencast protocol** on KDE, so it can record a game window without a permission popup. Everywhere else on Linux it falls back to the standard screen-share portal, which asks once and remembers your choice.
- **Process scanning** (`/proc`) to find running games on Linux, looking for Steam's app id or a Wine `.exe`. Works on any desktop. On Windows it walks the top-level windows every two seconds.
- **ashpd** for the portal calls, and **zbus** for the KDE and notification D-Bus stuff.
- **Tailwind 4** for styling, plus Lucide icons.

## Using it

- The sidebar has **All clips**, **Favorites** and a page for each game.
  A game's page can group its clips by play session.
- Search with `Ctrl+F` or `/`. **View** sorts (newest, oldest, longest,
  largest), sets the thumbnail size, groups by recent periods or by day,
  and filters by tag. The **⋯** menu selects clips for deleting many at
  once (`Ctrl+click` a card does too) and opens any video file.
- Moving the pointer across a card scrubs through ten frames; resting on
  it plays a muted preview. With a card focused, `F` stars it, `Ctrl+C`
  copies it and `Delete` deletes it. A delete can be undone for six
  seconds from the toast or with `Ctrl+Z`.
- Open a clip to watch it. Press `?` for every shortcut: `←` `→` open the
  previous or next clip, `,` `.` step a frame, `<` `>` change speed, `I`
  `O` set the in and out points, `R` repeats, and `T` makes the current
  frame the clip's cover.
- **Trim** shows a filmstrip with handles. **Copy clip** puts the result on
  the clipboard to paste into Discord, and **Save trimmed copy** writes a
  new file; the original is kept. The viewer's ⋯ menu adds **Audio mix…**
  (a level for each track), **Vertical 9:16** for TikTok/Shorts (drag the
  frame to choose what's kept) and **Slow motion** over a range of the
  timeline. You can also drag a clip card out of the window.
- **Settings → Sharing** caps the size of copied clips, such as 10 MB for
  Discord, and saved ones too if you like. The bitrate comes from the
  clip's length, the picture drops to 720p or 480p (and 30 fps) when it
  has to, and if a clip is too long to fit you are told how long would.
- Detected something that isn't a game? The ⋯ next to **Save clip**, or
  on the game's page, has "*Game* is not a game", which stops recording
  it for good (undo under Settings → Recording → Not games).
- **Settings → Audio** picks which output's sound is recorded as the game
  track (say, only your headset) and which microphone goes on its own
  track, with a live level for each so you can see what a recording would
  hear before you play. An unplugged device falls back to the default.
  With the mic on, clips start with game and mic mixed into one track,
  the one every player (and Discord) plays; the separate tracks follow it
  for the editor's levels.
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
- Star a clip (`S`) or rename it (`N`) in the viewer. Tags are off until
  you turn them on under Settings → Appearance; then you add them in the
  viewer and filter by them from View. Favourites, titles and tags live in
  the small `.json` file next to each clip, so they survive moving the
  folder.
- While a game records, your Discord profile shows "Clipping *game*".
  Turn it off under Settings → General.
- **Settings → Appearance** picks the accent (off-white, blue, mint,
  violet, pink or rainbow) and turns off hover previews or motion.

The whole thing is a Cargo workspace: `crates/clip-core` has the capture and encoding code with no UI, `crates/clip-ipc` and `crates/clip-cli` are the `rerunsctl` command, and `app/` is the Tauri app.

## Current state / What's next

It works, but I've mostly tested it on my own machines: Windows and KDE Plasma 6 on Wayland, both with an NVIDIA card.

The design system is in [docs/DESIGN.md](docs/DESIGN.md), the performance rules and budgets in [docs/PERFORMANCE.md](docs/PERFORMANCE.md), and a roadmap with the research behind it in [docs/ROADMAP.md](docs/ROADMAP.md),
starting with what has to be true before 2.0.0 ships. CI builds and lints
every push; tagging `v*` builds Reruns Setup and the Windows update
package, plus AppImage, deb and rpm, as a draft release.

Known problems:
- **Clips can stutter on the HDR compatibility path.** Saved clips repeat or skip frames more often than the 0.5% budget in the v2.0.0 checklist: Deadlock clips scored 0.9–13% on 2026-11-18. `cargo run --release -p clip-core --example judder -- <clip.mp4>` scores a clip.
- **The redesigned UI hasn't been run on Linux** (WebKitGTK) yet.
- **GPU direct capture on Linux is unverified.** The zero-copy path (DMA-BUF straight into the encoder's GPU memory) is tried first and falls back by itself to copying frames through system memory; Settings → Recording shows which one is active. It hasn't been verified on real hardware yet.
- **Only KDE gets prompt-free window capture.** On GNOME and Hyprland you get a one-time screen-share prompt instead. That part is only partly tested.
- **The portal hotkey path isn't tested** on a real desktop yet.
- **X11 isn't supported.** There's no capture backend for it.
- **Windows is tested on NVIDIA only.** Game detection, window capture and NVENC zero-copy recording work there; AMD AMF and Intel Quick Sync, and HDR window capture, haven't been run on real hardware. See [docs/WINDOWS.md](docs/WINDOWS.md).
- **The Windows installer isn't code-signed yet**, so SmartScreen warns on first run.
- **Storage limits are a soft cap.** Old clips get deleted after a save if you go over, and the Trash option is on by default so you can recover them. Favourite and edited clips are kept unless you turn that off in Settings → Storage.

Next is the [v2.0.0 checklist](docs/ROADMAP.md#v200-checklist). After 2.0: watching clips from a phone over Wi-Fi, the `ext-image-copy-capture` protocol for window capture on wlroots desktops, and X11 support.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
