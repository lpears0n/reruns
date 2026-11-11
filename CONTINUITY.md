# Reruns (formerly clip.sh): continuity notes

**Renamed to Reruns for v1.0.0.** Older entries below use the old names.
Mapping: clip.sh → Reruns (display name); `sh.clip.app` → `gg.reruns.app`
(identifier, Windows data/log folder); `clip-sh` → `reruns` (app
executable, desktop file, icon, AUR package); `clipsh` → `rerunsctl`
(CLI; it can't share the app's executable name); `~/.config|.cache|
.local/share/clip.sh` → `…/reruns`; `~/Videos/clip.sh` → `~/Videos/Reruns`;
`CLIPSH_*` env vars → `RERUNS_*`; socket `reruns.sock`, pipe
`\\.\pipe\reruns-<user>`, KGlobalAccel component `reruns`. Internal crate
names (`clip-core`, `clip-ipc`; the CLI crate is `rerunsctl`) are
unchanged. The GitHub repo is now `lpears0n/reruns` (the old
`lpears0n/clip.sh` URL redirects). Nothing migrates old
settings or clips: there were no released users.

Last updated: 2026-11-04. Current plans live in the repo: `docs/ROADMAP.md`
(features, platforms), `docs/DESIGN.md` (design system, performance budgets),
`docs/WINDOWS.md` (Windows build and what is unverified). The notes below
are the running log; the sections at the end are newest.

## Goal
A Linux-first alternative to Medal.TV. An always-on instant-replay buffer: press a hotkey and the last N seconds are saved. Includes a local clip library and a trim editor.
Decisions: **no capture picker** — like Medal, a game window is detected and recorded automatically (KDE: KWin private protocols; portal + restore token only as a non-KDE fallback). **Linux first** (KDE Wayland main target), **v1 = local only** (no cloud/sharing), **Rust + Tauri 2 + Svelte 5**.

## Machine
CachyOS, KDE Plasma 6 (Wayland), RTX 5080 (main) + Radeon 610M iGPU. Rust 1.99, Node 26, pnpm 12, GStreamer 1.28. All required packages are installed.

## Layout
```
Cargo.toml              workspace (resolver 3, edition 2024); shared deps in [workspace.dependencies]
crates/clip-core/       capture/encode library (no UI)
  src/portal.rs         ScreenSource: ashpd ScreenCast portal → PipeWire fd + node id, restore token
  src/encoder.rs        Encoder {Nvenc, Vaapi, X264}: probe() test-encodes a frame; build() sets properties
  src/capture.rs        VideoConfig (60fps, 20Mbps) + add_video_branch() → returns h264parse tail
  src/audio.rs          AudioSource {Desktop, Mic}: pipewiresrc → 48k stereo → fdkaacenc|avenc_aac → aacparse
  src/ring.rs           ReplayBuffer {video, audio[]}: keyframe-aligned eviction, snapshot() → Clip (rebased, shared memory)
  src/save.rs           write_mp4(Clip) via appsrc → parse → mp4mux faststart; clip_path(), default_clips_dir()
  src/recorder.rs       Recorder::start(cfg, CaptureSource{Portal|Kwin})/status/save/stop; appsink callbacks feed the ring
  src/kwin/             KWin client thread: plasma_window_management (window list/pid/state) + zkde_screencast v6
                        (stream_window → PipeWire object serial, read via pipewiresrc target-object). No picker.
                        protocols/*.xml vendored from plasma-wayland-protocols (LGPL-2.1+)
  src/game.rs           detect(pid, title): SteamAppId/SteamGameId in /proc/pid/environ (name from appmanifest), or a wine .exe
  src/auto.rs           AutoRecorder: KWin window events → start recording the first game window, stop on close,
                        save() → <clips>/<game name>/<timestamp>.mp4
  src/encoder.rs        Codec {H264, Hevc, Av1} (parser, caps, make_parser) × Encoder {Nvenc, Vaapi, X264}; probe(codec)
  src/display.rs        max_refresh_hz() via wl_output (this machine: 240 Hz)
  src/media.rs          inspect(): duration, audio track count, JPEG thumbnail
  src/export.rs         render(): trim [start,end] + mix audio tracks at per-track gains → one AAC track, re-encoded
  src/kwin/shortcut.rs  GlobalShortcut via KGlobalAccel D-Bus (no prompt, works in games); parse("Ctrl+Shift+F8")
  examples/auto.rs      Medal-style spike: waits for a game, Enter saves
  examples/kwin_windows.rs  lists windows / streams one by app_id
  examples/record.rs    spike: portal → record N seconds to MP4
  examples/replay.rs    spike: buffer the screen, Enter saves to ~/Videos/clip.sh/Desktop/, q quits
  tests/encoders.rs     checks encoders work on the host
  tests/save.rs         end-to-end without portal: test srcs → x264/AAC → ring → MP4, checked with ffprobe
crates/clip-ipc/        control-socket protocol: $XDG_RUNTIME_DIR/clipsh.sock, one JSON line each way (save/status/show)
crates/clip-cli/        `clipsh save|status|show` (no GStreamer dependency)
app/src-tauri/src/      media_server.rs (loopback HTTP + Range for <video>; token URL; allow-listed roots),
                        system.rs (notifications via D-Bus, save sound via playbin, XDG autostart), storage.rs,
                        lib.rs (commands, tray, close→hide, hotkey task), service.rs (AutoRecorder actor),
                        library.rs (scan clips dir, thumb cache ~/.cache/clip.sh/thumbs), settings.rs (~/.config/clip.sh/settings.json)
app/src/                Svelte 5 UI: Home (drop zone → Trimmer/Timeline), Clips (thumbnail grid), Settings (split pane)
app/                    Svelte+Vite frontend + app/src-tauri (Tauri shell, crate `clip-sh`, lib `clip_sh_lib`)
```
Note: the frontend lives directly in `app/`, not `app/ui/` as the plan said, because the Tauri CLI expects frontend and src-tauri to share a parent folder.

## Status
- [x] **M1 toolchain + skeleton.** `cargo check --workspace` and `pnpm build` (in `app/`) pass.
- [x] **M2 capture spike.** Verified a real 10s capture: 3440×1440 at 60fps, NVENC, 1s keyframes, ~16 Mbps.
  - After that capture, a caps filter now requests H.264 `profile=high` (otherwise encoders default to Baseline). All three encoders were verified on videotestsrc; the real-screen capture wasn't re-run after the change.
- [x] **M3 replay buffer + audio + save.** Unit tests (ring) + e2e save test pass; clippy clean.
  - Real-screen spike verified: NVENC 3440×1440@60, High, desktop + mic AAC tracks, 10s buffer → 10.5s clip, audio starts within ~8ms of video. A/V lip-sync not yet checked by eye in mpv.
- [x] **Auto game capture (added after M3, user requirement: no picker).** Verified on KWin 6.8.1 with a fake Steam game
  (gst-launch window with SteamAppId set): detected, recorded with NVENC, saved under the game's name, stopped on close.
  Not yet tried with a real game (Proton/fullscreen).
- [ ] **M4 daemon wiring.** Partly done: Tauri hosts AutoRecorder, KGlobalAccel hotkey (default F8), tray (Open/Save/Quit), close hides to tray. Still: Unix socket + `clipsh` CLI, notifications. Host `AutoRecorder` (not the portal Recorder). Hotkey: prefer KGlobalAccel over D-Bus on KDE
  (the GlobalShortcuts portal shows a confirm dialog once). Also: Tauri hosts the recorder, Unix socket `$XDG_RUNTIME_DIR/clipsh.sock`, `clipsh` CLI, GlobalShortcuts portal, tray, notifications.
- [~] M5 library + trim/export UI: done without SQLite (filesystem scan + JSON thumb cache). Trimmer + Render verified by type-check/build only — not yet clicked through.
- [ ] M6 settings, persist restore token, disk quota, autostart, PKGBUILD.

Nothing has been committed to git yet (repo initialized, no commits).

## Gotchas learned
- Shell is zsh: `source ~/.cargo/env` doesn't exist (Rust comes from the system package), and unquoted `$var` doesn't word-split (use `${=var}`).
- `pnpm tauri init` wrongly generated path deps to `../../crates/tauri` (it mistook our `crates/` for a Tauri source checkout). Fixed to crates.io `tauri = "2"`.
- ashpd 0.13: `PersistMode` and `Session` are in `ashpd::desktop`. Keep `Session<Screencast>` alive while capturing.
- nvh264enc accepts BGRx system memory directly (it converts on the GPU). `cudaconvertscale` isn't available on this machine.
- pipewiresrc only sends frames on damage, so `keepalive-time` + `videorate` give a constant fps.
- Run the spike with `CLIPSH_RESTORE_TOKEN=<token>` to skip the picker. `CLIPSH_ENCODER=nvenc|vaapi|x264` forces an encoder.
- Ring timestamps are **running time** (`ring::running_time_buffer`), not raw PTS: x264enc offsets PTS by 1000h and compensates in the segment, which silently dropped all audio from clips before the fix.
- Desktop audio = `pipewiresrc` with stream-properties `stream.capture.sink=true` and no target (follows the default sink).
- Caps changes mid-stream (e.g. resolution change) aren't handled: the ring keeps only the latest caps per track.
- Audio tracks are trimmed to the oldest buffered *video* PTS, not to the window: video cuts at keyframes and keeps up to one extra GOP, which otherwise left a ~0.5s audio gap at the clip start.
- KWin 6.8.1 here offered the restricted protocols **without** any .desktop grant. Still ship `X-KDE-Wayland-Interfaces=org_kde_plasma_window_management,zkde_screencast_unstable_v1` in the app's .desktop file, since other KWin builds enforce it.
- Window streams can have odd sizes → `videocrop` in the video branch trims to even (set from a caps probe on pipewiresrc).
- When a recorded window closes, pipewiresrc logs `target not found` just before KWin's unmapped event. Harmless, but it's logged at ERROR level; quiet it in M4.
- No desktop/non-game capture yet. Option without a picker: zkde_screencast `stream_output` (needs a wl_output binding).

## UI design rules (user, 2026-10-28; third pass — supersedes earlier specs)
Tailwind 4; tokens in app/src/app.css (canvas #0D0D0F, panel #17181B, raised #1E1F23, line #26272C, accent orange #FF7A1A,
danger #FF3B5C, radius 6px controls / 8px cards). Fonts bundled: Inter Variable (UI) + Archivo Variable at 112% width
(`display-face` utility: headings, game names, numbers).
Anti-"vibecoded" rules (from research; audit against these):
- No colored accent strips on nav/cards, no glows, no dashed upload boxes, no uppercase eyebrow labels, no status pills.
- One dominant element per screen; vary weight/size instead of uniform cards.
- Recorder status lives in ONE place: right side of the top bar (StatusBar.svelte). Storage ring = storage only.
- Drag-and-drop = full-window overlay only while dragging; "Open video" is an icon in the top bar.
- Motion only when it means something (shutter flash on the Clip button when a clip saves).
- Real, specific copy; design empty/error states.
Layout (games-first, user-approved 2026-10-28): top bar (logo left, StatusBar right) + 72px icon rail
(Games, All clips, spacer, StorageRing with size label, Settings). Views: GamesView (hero = playing/last game with Steam
library_hero + logo.png; "Recently played" grid of library_capsule covers, fallback = darkened clip frame + title),
GameView (banner + clips grouped by local day with "Played 1h 20m"), ClipViewer (replaces trimmer: view mode with
custom controls + video box sized to the clip's aspect via container units; Trim toggles in/out/levels/Render in place),
ClipsView (all clips grid). Focus ring: :focus-visible only, orange.
Data: app/src-tauri/src/games.rs — sessions.jsonl in $XDG_DATA_HOME/clip.sh (logged by service.rs on game start/stop),
list_games joins sessions + clips + steam_art(); clips get `<clip>.mp4.json` {game, steam_app_id} on save (edits copy it);
older clips resolve the app id by name against installed appmanifests.
- Settings: General / Recording / Audio / Hotkeys / Storage; rows in cards with title+description left, control right.
Ad slots: AdSlot component, off unless `"ads": true` in settings.json (728×90 under the main view).
- Screenshot the app without focus: `CLIPSH_SHOT=out.png ./target/debug/examples/kwin_windows clip-sh`.
- uridecodebin `caps` property breaks with nvh264dec (CUDA output never exposed → hang). Let downstream negotiate instead.
- WebKitGTK can't play <video> from Tauri's asset:// scheme (images work). Videos go through media_server.rs instead.
- Settings JSON gained: launch_at_login, start_in_tray, close_to_tray, notify_on_save, sound_on_save, codec, encoder,
  cursor, desktop_audio, mic_device, audio_bitrate_kbps. FPS clamp 24–480, bitrate 2–100 Mbps.
- All three codecs verified end-to-end on NVENC at 100 Mbps (tests/save.rs `saves_every_supported_codec`).
- Follow-up for very high FPS: capture is BGRx in system memory (≈20 MB/frame at 3440×1440). For 240+ FPS, negotiate
  DMA-BUF from pipewiresrc and import into the encoder (zero-copy) instead of copying.
- Main window starts hidden (tauri.conf visible=false) and is shown in setup unless launched with --autostart + start_in_tray.
- Capture judder: clip timestamps are perfect, but under heavy GPU load (game at ~97%) KWin delivers fewer window
  frames than requested, so videorate duplicates (seen: 6–60% dup frames). DMA-BUF→glupload→nvenc did NOT help (worse),
  so the loss is inside KWin. Menus fool mpdecimate (near-identical frames); measure with a moving test pattern
  (examples/record_window.rs + Recorder::stop logs videorate in/out/dup/drop).
- In-app playback stutter (user-reported, trimmer): diagnostics via playbackStats action → `trimmer playback` log line
  (frames, dropped, fps). Not yet measured. WebKit 2.54 knobs: WEBKIT_GST_DISABLE_GL_SINK, WEBKIT_GST_DMABUF_SINK_DISABLED,
  WEBKIT_DISABLE_DMABUF_RENDERER, WEBKIT_GST_VIDEO_DECODING_LIMIT. Check which decoder loaded via /proc/<WebKitWebProcess>/maps.
- Fixed: KWin thread died on EAGAIN from Wayland read (stopped game detection).
- Blank grey app window in screenshots = monitor off (KWin sends no frame callbacks, WebKit doesn't paint). Not a bug.
- Dev: Svelte component CSS is injected from JS (`svelte({ emitCss: false })` in vite.config.ts). With separate
  `?svelte&type=style` modules, a webview reloading against a fresh dev server requested them before compile and
  Tailwind parsed raw .svelte as CSS ("Invalid declaration: FolderOpen") → blank page.
- Shell: `pgrep -f`/`pkill -f` patterns match the invoking shell's own command line; use `pgrep -x clip-sh`.
- In-app playback measured fine: WebKit rendered 289 frames in 4.85 s, 0 dropped. Perceived choppiness = duplicate
  frames inside the recording (capture under GPU load), not the player.
- ClipViewer now overlays controls on the video (bare glyphs, hairline seek, hover volume, fullscreen F/double-click,
  auto-hide while playing); header actions are borderless text. Not yet seen on screen.

## Multi-desktop support (steps 1–3 of the plan, done 2026-10-28)
- Detection: game::running_games() scans /proc (SteamAppId / Wine .exe), no compositor needed.
- auto.rs backends: Kwin (window capture, per-window detection) | Portal (monitor via ScreenCast portal + restore token
  in $XDG_DATA_HOME/clip.sh/portal-restore-token; /proc scan every 2 s). Auto = KWin if available, else portal.
  Settings `capture_backend` (null/kwin/portal) applies on restart. Portal never pops the picker mid-game: without a
  token it reports needs_screen and the UI shows "Choose a screen to record" (choose_screen command).
- Hotkey: clip_core::hotkey tries KGlobalAccel, then the GlobalShortcuts portal (CLIPSH_HOTKEY_PORTAL=1 forces it);
  none → UI tells the user to bind `clipsh save`. Portal hotkey path compiles but is NOT yet tested on a real desktop.
- Control socket (app/src-tauri/src/ipc_server.rs) also gives single-instance: a 2nd launch sends `show` and exits.
- Verified in the real app with capture_backend=portal: /proc detection → portal screen recording (no prompt) →
  `clipsh save` → stop on exit. Not yet tried on GNOME/Hyprland/wlroots themselves.
- Next for coverage: ext-image-copy-capture (prompt-free window capture where offered), X11, optional KMS helper.

## November 2026 pass (branch `windows-port`)
- Design pass per docs/DESIGN.md: OKLCH tokens, labelled sidebar (Ctrl+B
  collapses), header carries the view title, 1/2/3 switch views, skeletons,
  error/empty states, render progress + cancel. The 72 px rail and
  StorageRing from the October layout are gone.
- Library: search/sort/filters, selection + bulk delete, undo-able deletes,
  favourites/labels/titles in the sidecar, hover sprite scrubbing, windowed
  grid above 120 clips, cached first paint.
- No polling: folder watcher (notify), thumbnail-ready events, local clock.
- Thumbnails: WebP 320 px + 10-frame sprite, generated by a 2-thread pool.
- Zero-copy capture (DMA-BUF → GL → NVENC / → vapostproc → VA) with a
  4 s probe and fallback. **Off by default on Linux**: the October spike
  found DMA-BUF→glupload→nvenc worse on this machine (the loss is in
  KWin's frame delivery, not the copy). Settings → Recording toggles it.
- Windows: compiles and is clippy-clean for the Windows target (checked by
  cross-compiling against GStreamer's MinGW dev package; CI runs it on
  windows-latest). WGC via d3d11screencapturesrc, D3D11 zero-copy into
  NVENC/AMF/QSV/MF (on by default there), wasapi2src audio, window-walk
  game detection, RegisterHotKey via tauri-plugin-global-shortcut, named
  pipe control socket, GStreamer runtime bundled by
  scripts/collect-gstreamer.ps1 + tauri.windows.conf.json. **Never run on
  Windows yet.**
- Harness: `scratchpad/shoot.cjs`-style Playwright script with the Tauri
  bridge mocked screenshots every view; it caught two real bugs (undo not
  restoring, virtual grid feedback loop). Worth checking into `app/tests/`.

## Working agreements (owner's rules, keep following them)
- Performance is the top priority on both OS stacks: nothing may cost the
  game frames. Measure before and after any capture change.
- Branches are named for their purpose (`windows-port`, `fix-release-linux-deps`).
  No tool or assistant names anywhere: branches, commits, PR text, docs.
  That includes `Co-Authored-By` and session trailers on commits.
- Commits are authored `lpears0n <lucas.pearson@live.com>` (set in the
  clone's git config). Earlier commits used `notlucaspearson@gmail.com`;
  adding that address to the GitHub account attributes them retroactively.
- PRs: open one when asked; merge with a merge commit once CI is green.
  Tags and branch deletions are done by the owner from their own machine
  (cloud sessions can't delete branches anyway: the git proxy refuses it).
- Keep the PR count low: no long stacks of small PRs. Group related work
  into one branch/PR, and land green PRs before starting new ones.
- Update this file whenever a piece of work is completed.
- Main machine: CachyOS/KDE Plasma 6 Wayland, RTX 5080, 3440×1440 240 Hz.
  Owner is moving to Windows as the daily driver; Linux testing will be in
  a VM (no GPU there: capture tests need real hardware).

## State on 2026-11-02 evening
- `main` = PR #1 (everything above) + PR #2 (release workflow fix).
- Tag `v0.1.0` first pointed at the PR #1 merge; the Windows release job
  built `clip.sh_0.1.0_x64-setup.exe` (41.8 MiB, GStreamer runtime bundled:
  130 libraries, 25 plugins; webp/fdkaac/dxgiscreencap absent by design)
  into a draft release. The Linux job failed on the 22.04 runner's
  libunwind conflict, fixed in PR #2; the tag needs moving onto `main`
  (`git tag -fa v0.1.0 origin/main && git push -f origin v0.1.0`) to get
  the Linux packages.
- CI on `windows-latest` compiles, lints and runs the unit and
  end-to-end save tests against MSVC GStreamer 1.26. It found two real
  bugs already (pkg-config path, no WebP encoder on Windows).
- Still never run on a Windows desktop: WGC capture, D3D11 zero-copy,
  hardware encoders, hotkey. First real run = the next milestone. Logs:
  `%LOCALAPPDATA%\sh.clip.app\logs` (installer) or the terminal (`pnpm tauri dev`).
- Installer is unsigned (SmartScreen warns). Options, cheapest first:
  Azure Trusted Signing (~$10/month, identity validation, works from CI
  via `signCommand` in tauri.conf.json), SignPath (free for OSS), an OV
  Authenticode cert (reputation builds slowly), an EV cert (instant
  SmartScreen trust, hardware token). The `.sig` files are Tauri updater
  signatures and need `TAURI_SIGNING_PRIVATE_KEY` only once the updater
  plugin is added.

## State on 2026-11-04
- PRs #4–#13 were opened as one stack (each on the one before) and are
  all merged into `main`: Windows game capture + first-run fixes, viewer
  scrubber, HDR compatibility toggle, Linux capture restart + fixed encode
  size, launcher game detection, clip markers, export size presets, save
  sounds, updater + signing hooks, experimental HDR window capture.
  No PRs are open.
- #10–#13 were red on Windows CI only because of test helpers, not app
  code: a file path written into a `gst::parse::launch` string loses its
  backslashes (`a\b` → `ab`, even in quotes), so on Windows the test file
  landed elsewhere. Fixed by naming the `filesink` and setting `location`
  as a property. Rule: never put a filesystem path in a launch string;
  set it as a property (URIs from `filename_to_uri` are fine).
- CI flake, fixed: on Windows runners Chocolatey's 370 MB
  `gstreamer-devel` download sometimes arrives corrupted and fails its
  checksum, so the job died before building. Both workflows now retry the
  install up to 3 times (checksum check kept). Linux runners also hung on
  `apt-get` twice on 2026-11-04 (GitHub mirror); a re-run fixed it.
- Old draft release `v0.1.0` (tag at `2626033`, built as clip.sh) is
  stale; the owner deletes the draft and the tag.
- Old branches are deleted; `main` is the only long-lived branch.
- v1.0.0 prep (branch `rename-to-reruns`): rename to Reruns, LICENSE
  (GPL-3.0), bundle publisher/copyright "Lucas Pearson", version 1.0.0,
  release notes in `docs/release-notes/v1.0.0.md`. The Release workflow
  now uses `docs/release-notes/<tag>.md` as the draft's description and
  fails a tag that doesn't match the app version.
- Signing: updater key is free (see `docs/RELEASING.md`). Windows
  Authenticode options: SignPath Foundation (free for OSS, needs the
  LICENSE file), Azure Trusted Signing (individuals: US/Canada only, paid
  subscription; Azure for Students credit doesn't qualify), Microsoft
  Store (free, but only signs MSIX).

- First `v1.0.0` tag (2026-11-04) was pushed on `ed5d932` (pre-rename)
  from a stale local `main`, so it built "clip.sh 0.1.0"; both jobs then
  failed with "Wrong password for that key". A correct password still
  fails that way if the secret has a trailing space or line break. The
  Release workflow now checks key + password right after `pnpm install`
  (seconds, not after the 10-minute build). The tag must be moved to
  `main` (`git tag -f v1.0.0 origin/main`, `git push -f origin v1.0.0`).
- The app icon is still Tauri's default placeholder (app/src-tauri/icons).
- Sidebar wordmark is "reruns" + accent ".gg" ("r." collapsed).

- App polish (branch `app-polish`):
  - Icon: replay arrow round a play button, orange on the dark tile.
    Source `app/src-tauri/icons/icon.svg`; regenerate the set with
    `pnpm tauri icon <1024px png>` from `app/` (delete the android/ios
    folders it adds). The favicon is the same SVG.
  - Windows draws no system frame: the header is the title bar
    (`WindowControls.svelte`, drag regions via `data-tauri-drag-region`,
    `set_decorations(false)` in setup; window-state no longer restores
    DECORATIONS). Linux keeps its native frame. UI test `full` runs
    frameless.
  - Sidebar folds to icons by itself below 1100 px, in Settings and in
    the clip viewer; the toggle opens it there for the moment.
  - Capture failing mid-game shows "Can't record <game> · retrying"
    (status state `retrying`, reason from `RestartPolicy`), not
    "Waiting for a game".
  - Windows: an encoder named in two failures in a row (e.g. NVENC
    "Failed to open session") is set aside for the session and the next
    encoder that passes the probe takes over.
- Real-hardware findings (owner's PC, 2026-11-04): Overwatch,
  3440x1440 HDR display, SteelSeries Sonar. With HDR compatibility on,
  the screen-area capture records the whole monitor for a fullscreen
  game (as designed); "Capture the window in HDR" is being tried. Earlier
  runs failed with NVENC "Failed to open session" (shown as
  not-negotiated upstream). Clips have no audio with Sonar's Stream mix
  selected; PR #17 logs why on the next save.
- Viewer progress line follows the playhead every frame while playing
  (requestAnimationFrame) and moves by transform; it was driven by
  `timeupdate` (~4 Hz) and looked choppy. The UI test asserts ≥12 drawn
  positions in 0.5 s (31 with the fix, 2 before).
- Share and edit (branch `share-and-edit`): live level meters in
  Settings → Audio (`clip_core::meter`, a `level` pipeline per configured
  source, only while the page is open; `audio-level` events); Copy and
  drag-out of clips (`arboard` file list kept alive in AppState,
  `tauri-plugin-drag` 2.1.1 + `@crabnebula/tauri-plugin-drag` 2.1.0);
  vertical 9:16 export (`Edit::vertical`, `videocrop`) and slow motion
  (`Edit::slow`, retime probe; audio in the range is dropped via
  `PadProbeReturn::Handled` — returning Drop there made GStreamer unref a
  buffer gstreamer-rs had taken). Clipboard and drag need a desktop: not
  exercised in CI.
- Copy cleanup (branch `plain-copy`): every visible string cut to the
  rules in `docs/DESIGN.md` §2.8 (no description where the label says it,
  one short sentence for side effects only, no reassurance or "so that"
  tails, toasts say what happened). Settings descriptions went from ~820
  words to ~217. Follow §2.8 for any new text.
- Updates: the first check runs as the app starts (it waited 20 s until
  1.1.3, and the owner kept checking by hand first); a failed one is
  retried after 1, 2, 4… min, up to the 6 h interval. The first answer
  since launch has `prompt: true` and opens `UpdatePrompt.svelte` once per
  launch ("Later" closes it until the next). The backend keeps what it
  found (`updates::Found`) and the page asks for it (`found_update`) once
  it listens, since the check can finish before the page has loaded. Any found update also shows a download icon
  in the header with a breathing halo (`::before`, so the button itself
  stays still; off with reduced motion). 1.0.0 still shows its old text
  button: the prompt only exists from 1.1.0 on.
- Windows game detection is opt-in (branch `strict-game-detection`):
  a window counts only if its exe is from Steam, a launcher index, Xbox
  (`XboxGames`), or Windows' own game list (`GameConfigStore\Children`
  `MatchedExeFullPath`, undocumented and incomplete, so never the only
  source). The old "fills its monitor" fallback is gone: the emoji panel
  (TextInputHost), Snipping Tool and Game Bar overlays were recorded as
  games. Click-through / tool / no-activate windows and anything under
  the Windows folder or a SystemApps package are skipped. A game no
  source knows can be added with Game Bar's "Remember this is a game".
  The Games page has × "Not a game" per card; the list drops ignored
  names.
- NVENC "Failed to open session" on the owner's PC (2026-11-05), every
  path, while the start-up probe passes. The encoder always opens its
  session on its own NVIDIA device, so this is the driver refusing
  (session limit / resources), not a foreign device. The 1.1.0 encoder
  fallback never fired: the bus watch kept the *last* error (a knock-on
  "not-negotiated" that doesn't name the encoder). It now keeps the
  first. Window capture also now makes its device on the encoder's GPU
  (`adapter` from the encoder's `adapter-luid`), avoiding a system-memory
  hop on PCs whose default adapter is an iGPU. Recording worked again once
  the owner turned off "Capture the window in HDR" (and restarted), and
  `nvidia-smi encodersessions` showed no other users: likely GPU memory
  held by HDR window captures from failed attempts (FP16 WGC pools and
  textures stay alive until the appsrc is freed). Every recorder teardown
  now stops that capture explicitly (`hdr_window::stop`). If NVENC fails
  again, get the NVENCSTATUS with `GST_DEBUG=nvencoder:5` and
  `GST_DEBUG_FILE` (quit the tray copy first: a second launch only
  signals the running one).
- 1.1.1 (branch `release-1.1.1`): #25 plus one change: with "Capture the
  window in HDR" on, a failed HDR window capture falls back to plain
  window capture, never the screen area (the owner wants game-only clips;
  HDR compatibility alone still records the screen area by design).
- 1.1.1 tag: first push was on a commit before #26 and the version
  check stopped it; the owner re-tags on `main` (780c83d).
- Taskbar icon showing Tauri's placeholder on Windows: the bundle's
  icons are right; it's Windows' icon cache from the old install
  (`ie4uinit.exe -show`, or unpin/re-pin).
- Broken Steam art after coming back to the window (branch
  `fix-art-images`). Three causes, all handled:
  - The asset scope allowed each art file only after `list_games`
    returned, but the window paints its cached library first: those
    requests were refused, and a broken `<img>` never retries the same
    src. Now Steam's `appcache/librarycache` folders are allowed at
    setup (and again per listing, for a Steam installed later).
  - Steam swaps in a new hashed folder when a game's art changes and
    deletes the old one; `steam_art` took whichever copy `read_dir`
    listed first. Now the newest copy wins (`art_in`, tested).
  - `Art.svelte` wraps every Steam image: a failed load shows the
    fallback (title, clip frame or title cover) and tries again with
    `?retry=n` when the next listing arrives (App calls
    `retryFailedArt()`). The asset protocol reads only the path, so
    the query just forces a new request; loaded art isn't refetched.
- **Windows bundle was missing `level` and `audiorate`** (user report:
  Settings → Audio meters showed "Can't open: level element missing").
  `scripts/collect-gstreamer.ps1` bundles an allow-list of plugins, and
  #21 (meters, slow motion) added two elements without adding their
  plugins: `level` (meters) and `audiorate` (every audio track in an
  export, so exports with audio also failed on Windows). Both are now
  bundled and in the script's required list. Checked against the
  1.26.9 MSVC runtime MSI that the release installs: `gstlevel.dll` is
  in the effects module (with `gstvideocrop.dll`) and `gstaudiorate.dll`
  in the core module; both install by default. That runtime has no
  `webp` or `fdkaac` plugin; thumbnails already fall back to JPEG and
  audio to `avenc_aac`. Rule: a new `ElementFactory::make("x")` needs
  its plugin in `$plugins` and the element in `$required`.
- **Bug sweep (2026-11-05), fixed:**
  - Windows: the viewer's rename/star/labels were disabled for every
    clip (`inLibrary` compared against `clips_dir + '/'`); the "Saved"
    toast showed the whole path. Both now take either separator.
  - Starring, renaming or labelling a clip mid-trim, or its thumbnail
    arriving, reset the trim: the viewer's reset effect re-ran on every
    new clip object. It now resets only when the path changes
    (`tests/ui.cjs` stars mid-trim).
  - Clips and edits are written to `<file>.mp4.part` and renamed when
    complete (`save::partial_path`/`finish_partial`). With faststart
    the file sat at 0 bytes until the end, so the watcher's refresh had
    the thumbnailer fail on it and never retry: edits (long encodes)
    showed no thumbnail or duration until restart. The thumbnailer now
    also retries a failed file once its size or mtime changes.
  - The page only heard status on start/stop: a capture failure
    (Retrying) or recovery wasn't shown until the window was refocused.
    `service.rs` now emits whenever state/game/encoder/error/path change.
  - Windows hotkeys: binding a key Windows refused unregistered the old
    one first, leaving the action with no hotkey until restart.
  - Storage limit: one clip that couldn't be removed stopped the rest
    (and was first in line again after every save).
  - Renders had a flat 10-minute timeout; now max(10 min, 10× output).
  - `update_clip` answered with the canonical path (`\\?\C:\…`), which
    the page didn't match, so its tidied title/labels were dropped.
  - Stale HDR log lines (window capture falls back to the plain window
    since 1.1.1, not the screen area).
- **Follow-ups, also fixed:**
  - Storage limit keeps starred clips (`protect_favorites`, on by
    default, Settings → Storage), read from each clip's sidecar.
  - settings.json is saved through a temp file + rename, and read
    field by field over the defaults: one bad field keeps its default
    instead of resetting everything; text that isn't JSON is copied to
    `settings.json.bad` first.
  - Audio meters start/stop as async commands (device work in
    `spawn_blocking`), serialized by a tokio mutex so old meters close
    before new ones open and a stop during a start still wins.
  - pipewiresrc: `target-object` only where the element has it (else
    `target.object`/`node.target` in `stream-properties`), and
    `keepalive-time` through `encoder::set`. The element builder panics
    on a property the element lacks; never set optional ones through it.
- **Windows CI on PR #28**: `fits_an_export_under_a_size_limit` failed
  moving the finished `.part` into place (os error 32, sharing
  violation). A file just closed is often held open for a moment by
  Defender or the indexer; that blocks a rename but not a write, which is
  why the old write-in-place code never hit it. Renames now go through
  `save::rename_retrying` (retries errors 5/32/33 for up to 5 s, Windows
  only), and a finished file that still can't be moved is kept as
  `.part` instead of deleted.
- **Review fixes on PR #28:** sidecars (`.mp4.json`) and settings are
  written with `save::write_atomic`, and the storage limit treats a
  sidecar that exists but can't be read as starred
  (`library::is_favorite` → `None`), so a clip mid-rename is never taken
  for unstarred. Size-limited exports keep every pass under the `.part`
  name until one fits (a failed later pass used to leave the earlier,
  too-big file at the final path). The thumbnailer stamps a file before
  each attempt; if it changed during a failed attempt it is retried in 2 s
  instead of being recorded as failed in its finished state.
- Left alone: seeking the export pipeline logs a harmless
  GStreamer-CRITICAL (mp4mux's output segment is in bytes; the seek
  still reaches the decoder).
- Deep-dive backlog, not done yet, best first: apply recording settings to the running capture instead of
  "from the next game"; unload the webview while hidden in the tray
  (~100–250 MB during games); make HDR window capture the default once
  it works on real hardware; SignPath signing.

- **v1.0.0 is published** (2026-11-04), tagged on `1a8c275` (the #15
  merge), with the updater: `latest.json` plus `.sig` for every bundle.
  It predates #16–#19, so installed 1.0.0 still shows the clip.sh
  wordmark, Tauri's icon and the system frame. v1.1.0 (never tagged
  until now) ships those plus #19–#22 (share/edit features, meters,
  copy cleanup) and is the first test of self-update. No 1.1.1: patch
  numbers are for fixes on top of a released version.
  Release steps: bump the version in Cargo.toml, app/package.json,
  tauri.conf.json and the two `clip-ipc` version pins; add
  `docs/release-notes/v<version>.md`; merge; tag on `main`; publish the
  draft.
- **v1.1.2 prepared** (2026-11-05): the Windows bundle fix and the bug
  sweep above, notes in `docs/release-notes/v1.1.2.md`. After the PR
  merges: tag `v1.1.2` on `main`, then check the Windows build log says
  "all 39 required elements load from the bundle" before publishing.
  Not yet seen working on Windows: the meters, the hotkey change and the
  HDR log text (CI builds them; nothing here runs Windows).

## Windows bugs from the owner's machine (2026-11-06)
The owner's Windows PC (RTX 5080 + AMD iGPU, 3440×1440 HDR, SteelSeries
Sonar) now has Rust, GStreamer 1.28.8 (per-user, with dev files, under
`%LOCALAPPDATA%\Programs\gstreamer`; set `PKG_CONFIG`/`PKG_CONFIG_PATH`/
`PATH` from it as CI does), ffmpeg and Process Monitor. Its log
(`%LOCALAPPDATA%\gg.reruns.app\logs\reruns.log`) covers real Overwatch and
Fortnite sessions on 1.0.0–1.1.2. Only bugs reproduced there were fixed:
- **Two instances ran at once.** The single-instance check ran before
  GStreamer init, but the control pipe was only created at the end of
  setup, and a failure there just warned. Reproduced on 1.1.2 (two
  launches 0.3 s apart: both kept running, the second without hotkeys).
  Now `ipc_server::claim()` takes the pipe (Linux: socket) first thing in
  `run()`; on Windows a launch that finds the pipe held (access denied)
  asks that instance to show itself and exits. Checked with the fixed
  build: three launches 0.3 s apart plus a late one leave one process.
- **Desktop audio dropped under game load** (662 "Can't record audio fast
  enough" in one Fortnite session, ~22 s of audio missing by AAC frame
  count). `wasapi2src low-latency=true` gives a 20 ms ring (2 × 10 ms
  segments, per its debug log); `false` gives 200 ms. A/B with the bundled
  1.26.9 plugins under 32 busy threads: `true` dropped in 2 of 2 runs,
  `false` in 0 of 2. Now `low-latency=false`.
- **The encoder probe hid its reason**: it logged only "Could not encode
  stream."; it now includes GStreamer's debug text ("Failed to open
  session"). NVENC passes the probe when no game runs; during games it was
  refused (other NVENC users on that PC: SteelSeries GG, Game Bar).
- Not reproduced, left alone: the Windows pipe connect has no timeout (a
  hung instance would hang new launches and `rerunsctl`), and a busy pipe
  reads as "not running".
- Already fixed by 1.1.2 per the log: Steam art blocked by the asset
  scope, non-games recorded (GameOverlay, Snipping Tool, Windows Input
  Experience), capture restart loops on "not-negotiated".
- First test run after installing GStreamer seems to hang: that is the
  plugin registry being built (in `gst-plugin-scanner`), not a test.
- **Encoder fallback no longer sticks** (branch `nvenc-fallback`, on top
  of the fixes above). The log showed the NVENC probe failing once
  (2026-11-05 01:33) and AMF recording every game after it until the app
  restarted, when NVENC worked at once. Windows now probes per game
  (`win::auto::EncoderChoice::new_game`; restarts of the same game keep
  the choice), and while recording the status carries `encoder_fallback`
  {preferred, using, reason}: the header shows "on AMD AMF" with the
  reason in its tooltip. Encoders set aside for failing *while
  recording* still stay aside until restart. Linux still probes once per
  run (`encoder_fallback()` is always `None` there). Not reproducible on
  demand: this RTX 5080 took 48 NVENC sessions at once without refusing,
  so the in-game refusal (cause unknown: VRAM, another app) is covered
  by unit tests with a refusing probe, not a live run.
- UI tests on Windows without pnpm: start `node_modules\.bin\vite.cmd
  --port 5173` yourself (the test reuses a running server) and set
  `CHROMIUM_PATH` to Chrome.
- **In-game popup redesigned and scaled** (`overlay.rs`, `overlay.css`):
  the window is sized from the game's client area, in design units of a
  1080-line screen (`units()`: by height, 16:9 at most, clamped 0.7–3),
  and the page zooms to fill it, so the card is ~7% of screen height at
  any resolution or display scaling. Not yet seen over a running game.
- **Review fixes on PR #29:** Linux takes an exclusive lock on
  `reruns.lock` beside the socket as the instance claim (two launches at
  once could both bind), and while holding it still asks the socket's
  owner to show itself first (an older Reruns without the lock). The
  fallback note says "until it restarts" for an encoder set aside while
  recording.
- `fits_an_export_under_a_size_limit` failed once on Windows (a
  cancelled export's `.part` still visible) and passed 10 of 10 on its
  own; likely a just-deleted file still held open. Watch CI for it.
- **v1.1.3 prepared** (2026-11-06): the changes above merged in PR #29;
  the version bump and `docs/release-notes/v1.1.3.md` are on
  `release-1.1.3`. After that merges: tag `v1.1.3` on `main`, check the
  Windows build log says all required elements load from the bundle,
  then publish the draft. Published 2026-11-07.

## After 1.1.3 (2026-11-07, branch `startup-update-check`, PR #31)

**v1.1.4 prepared** on `release-1.1.4` (#31 merged first, as #29 did
before 1.1.3): version bumped (workspace, `app/package.json`,
`tauri.conf.json`, both `clip-ipc` pins, lock) and
`docs/release-notes/v1.1.4.md` written. After that merges: tag `v1.1.4`
on `main`'s merge commit, check the Windows build log says all required
elements load from the bundle, then publish the draft.

- **NVENC "Failed to open session" root cause (verified):** loading
  Media Foundation's hardware encoder (an `mfh264enc` test frame) leaves
  every *new* NVENC session in the process failing with
  `NV_ENC_ERR_INVALID_VERSION`; running ones keep going. Settings'
  capability list test-encoded every encoder at window load, so NVENC
  worked only when Reruns started after the game (owner's and a friend's
  logs both show it; restart fixed it). AMF and x264 probes don't do it.
  Fix: `Encoder::offered` lists MF from its factory without a test
  frame. Regression test `tests/nvenc_after_listing.rs` (own binary;
  skips without NVIDIA) fails with the old probe. Still true: if the
  per-game probe falls through to MF (no AMF/QSV), NVENC can't come back
  until restart, though the status says "next game".
- **Clips folder fallback:** Controlled folder access (Windows
  ransomware protection) blocks unknown apps from creating anything in
  Videos and reports "file not found"; a friend lost every clip. Now
  `clips_folder.rs` checks writability (a `create_new` check file of its
  own, never an existing one) before each save, and for the default
  folder at start-up too. A blocked folder moves to `~/Reruns` for good
  (saves settings, emits `settings-changed`, notifies naming the old
  folder), the user's own pick included: the library lists one folder,
  so fallback saves for a kept setting stayed invisible (review on #31).
- **Discord Rich Presence** (`discord.rs`), owner's pick of layouts:
  title "<game> with Reruns", "Clipping <game>", "Ready to clip" / "N clips
  saved" (this play session), timer. Picture: Discord's own icon for the
  game (what Medal shows) with Reruns' app icon as the badge
  (`discord_icons.rs`). Source: Discord's public detectable-apps list
  (12.6 MB, ~25k games with `icon_hash`, Steam ids in
  `third_party_skus`, executables), fetched at most weekly and only while
  the status shows, condensed to `%LOCALAPPDATA%\gg.reruns.app\
  discord-games.json` (2.7 MB). Match by Steam id, then name key, then a
  unique exe (`Status.exe`). Badge URL from `/applications/{id}/rpc`, so
  it follows the portal's icon. Unknown game: no images, Discord shows
  the app icon. Tried and dropped: Steam `library_600x900` art (bad
  square crop), repo `icon.png` as badge (transparent margin). Network
  use, otherwise only the updater. No buttons until
  the owner's site exists. Discord shows one "Playing" card per PC, from
  the last app to set one, so this replaces Discord's own "Playing
  <game>" like Medal's does (owner's screenshot); hence the game in the
  title. Updates at most every 4 s (Discord takes ~5 per 20 s). A tokio
  task: every pipe read/write has a 5 s limit (a hung Discord drops the
  connection), and icon lookups run in their own tasks and come back as
  messages, so turning off or a game change never waits on the network.
  List loads take turns (`ListCache.loading`): a lookup during the first
  download waits for it instead of reading no copy and caching "unknown".
  Over the local IPC pipe (`\\.\pipe\discord-ipc-N`, Linux sockets incl.
  Flatpak/Snap). Toggle
  "Show on Discord" in General, on by default. `APP_ID`
  1568480492167233636 is the owner's "Reruns" Discord application (icon:
  a full-bleed 1024 render of `icons/icon.svg`). Checked live: Discord
  accepted SET_ACTIVITY and echoed name, details, timer and button.
- Start-up update check runs at once (was 20 s), retries 1/2/4… min after
  a failure, and the page asks `found_update` once listening.

## Mic missing from clips (2026-11-08)
User report from another Windows PC ("my girlfriend's mic isn't
recording", log on 1.1.3–1.1.4): the mic *was* captured (packets on
every recording, no "left out" warning), but as a second audio track,
and players, Discord and the app's own viewer (WebView `<video>`) play
only the first. Only rendered edits mixed the tracks. Now, with more than
one source, `Pipeline::build` tees each source (`audio::add_audio_source`)
into its own AAC track (`add_aac_branch`) and into a live `audiomixer`
(`add_audio_mix`) whose track comes first; the mixer's inputs and output
sit behind leaky queues, so a stall there drops mix audio, never the
sources'. `ReplayBuffer::with_mix` / `Clip::mixed` → `SavedClip::
mixed_audio` → sidecar `mixed_audio`; the viewer shows levels for the
source tracks only, `render_clip` gives the mix a level of 0, and an
edit's copied sidecar clears the flag. Mixing at save time was measured
first and rejected: 5–9 s per 5-minute clip on 4 cores (avenc_aac's
two-loop coder alone is ~8 s), against one more real-time encoder.
`tests/save.rs` runs two live tones through the real branches: each
track -13.5 dB, the mix -10.5 dB (both), an edit of it -10.5 dB (not
doubled). `a_stalled_mixer_never_holds_up_the_source_tracks` holds the
mixer's output thread for 2 s (4× the queues): game and mic tracks keep
21 ms packet steps, the mix drops ~0.8 s and resumes; with the mixer's
input queues made blocking, the game track gets a ~300 ms hole and the
test fails. Clips saved before this keep the mic on track 2 only; render
an edit to hear it. Not yet run on Windows.

**v1.1.5 prepared** with it on `claude/new-session-kuipn1` (one PR: the
fix, then the version bump and `docs/release-notes/v1.1.5.md`). After it
merges: tag `v1.1.5` on `main`'s merge commit, check the Windows build
log says all required elements load from the bundle, then publish the
draft. Worth a Windows recording with game audio and a mic before
publishing: the live mixer is the one part no Windows machine has run.

## Smooth recording, step 1: measuring judder (2026-11-10, branch `smooth-capture`)

The owner approved a seven-part improvement plan (recording smoothness
first, then startup/memory/updates, sounds, the in-game popup, UI
cleanup, features, mobile groundwork). This is its first piece: a way to
measure judder before any capture change. Nothing about how recording
behaves changed.

- **`clip_core::pacing`**: `Tally` counts **repeats** (the same picture
  again for up to 0.25 s; longer is a still scene) and **holes** (a
  timestamp step over 1.5 frames; over 1 s is a pause, which the recorder
  leaves on purpose). Score = repeated + missing frames as a share of the
  timeline; the budget is under 0.5% (`docs/DESIGN.md` §5).
- **`cargo run --release --example judder -- <clip.mp4>...`** decodes a
  clip at 344×144 grey, compares each picture with the one before (mean
  difference under 0.1 of 255 = same picture) and prints the score and
  the worst moments. `RERUNS_JUDDER_SIZES=1` adds the encoded sizes of
  repeated and new pictures.
- **Recorder counters** (`recorder.rs` `Pacing`): the same tally fed
  from encoded packet sizes in the video appsink callback (a
  non-keyframe under a quarter of the median of the last 128 is a
  repeat), plus the frame queue's `overrun` count (the queue is now named
  `frames`). Logged as `clip pacing` on every save and `frame pacing`
  when a recording stops; `video pacing` gained `source_fps`. The 25%
  cut-off was measured: repeats' 95th percentile was 25–42% of the median
  across five games, new pictures' 5th percentile 42–50%. The first try
  (4%) missed most repeats. The guess follows the decoded truth on clips
  that are all gameplay and is off on clips with menus; the example is
  the truth.
- **Baseline, every clip in `Videos\Reruns` (1.0.0–1.1.5 recordings):**
  none is inside the budget except one Fortnite clip (0.08%). R6S
  2026-11-09: 5.1% (94 repeats in 49 freezes of 1–4 frames, no holes).
  DBD 2026-11-07: 8.9% (157 repeats, 4 holes). Deadlock: 22–27%, mostly
  single repeated frames about 15 times a second. Overwatch 2026-11-04:
  2–13%. Fortnite at 120 fps: 0.1–2.9%; the two 2026-11-06 Fortnite clips
  have a hole every ~5 frames (20%), which is the frame queue dropping.
- **Not known yet:** how much of this is the game drawing fewer than 60
  new frames a second (a repeat then is the truth, not a capture fault).
  The A/B runs need PresentMon alongside to tell.
- Test clips for the analyzer must be 4:2:0: x264 makes a monochrome
  stream from grey input and the D3D12 decoder refuses it. With rate
  control on, x264 refines a repeated picture and it no longer decodes
  identical; the test encodes at a fixed quantizer.
- This PC had Smart App Control on, which blocked Cargo's build scripts
  (os error 4551); the owner turned it off on 2026-11-10.
- **Next:** A/B the three Windows capture paths (HDR compatibility,
  plain window capture, "Capture the window in HDR") on the same scene
  with PresentMon, then try fixes one at a time, keeping only what the
  score says is better: opt out of Windows' background throttling and
  hold a 1 ms timer while recording, MMCSS on the streaming threads, pin
  the pipeline clock, move the leaky queue ahead of `videorate`.

## Next, in order
1. v1.0.0: owner adds the updater key secrets (`docs/RELEASING.md`), then
   tags `v1.0.0` on `main`; the Release workflow makes the draft. Owner
   reviews and publishes it. Then apply to SignPath for Windows signing.
2. Windows run on the owner's machine of what landed: capture restart +
   fixed-size letterbox (#4 noted these as unverified), HDR compatibility.
3. HDR window capture (#13) on a real HDR display; it is off by default
   and marked experimental until then.
4. Roadmap phase 2 (editor) per `docs/ROADMAP.md`.
