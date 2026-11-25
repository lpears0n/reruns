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
(features, platforms), `docs/DESIGN.md` (design system), `docs/PERFORMANCE.md` (performance rules and budgets),
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

## UI notes
The design system is `docs/DESIGN.md` (rewritten from scratch 2026-11-13; the earlier design specs were deleted).
Data: app/src-tauri/src/games.rs — sessions.jsonl in $XDG_DATA_HOME/clip.sh (logged by service.rs on game start/stop),
list_games joins sessions + clips + steam_art(); clips get `<clip>.mp4.json` {game, steam_app_id} on save (edits copy it);
older clips resolve the app id by name against installed appmanifests.
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

**v1.1.5 was never tagged.** Its mic-mix change went to `main` in PR #33
with the version bump, and it is folded into **v1.2.0** (2026-11-11), so
`docs/release-notes/v1.1.5.md` was removed. Before publishing, check the
Windows build log says all required elements load from the bundle, and
make a Windows recording with game audio and a mic: the live mixer is the
one part no Windows machine has run.

**v1.2.0 prepared** on branch `release-1.2.0` from `main` (version 1.2.0 in
the workspace, `app/package.json`, `tauri.conf.json` and both `clip-ipc`
path dependencies; notes in `docs/release-notes/v1.2.0.md`). It carries
W1–W3 and the mic-mix change. Not yet: a game clip with the timer fix,
heard sounds over a game, and a run of the update flow. After the PR
merges: tag `v1.2.0` on `main`'s merge commit, then publish the draft.

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
- **A/B harness: `scripts/judder-ab.ps1`.** One run per capture path
  (`-Path compat | window | hdr-window`): stops any running Reruns,
  backs up `settings.json` once and sets `hdr_compatibility`,
  `hdr_window_capture` and `replay_seconds` (60 by default), starts
  `target\release\reruns.exe` in the tray, waits for the clip saved with
  the hotkey, scores it, and writes a report, the run's log and a row in
  `target\judder-ab\summary.csv`. `-Restore` puts the settings back.
  `-Runtime bundled` (default) runs on the installed Reruns' GStreamer
  (1.26.9, what users get); `-Runtime dev` on the development install
  (1.28.8). Checked without a game: the scorer and the app both run on
  the bundled runtime, and the app began recording Deadlock. Not yet
  done: a full run ending in a saved clip.
  - A local `tauri build --no-bundle` leaves an **empty `gst-plugins`
    folder beside `reruns.exe`**; the app then loads plugins only from
    there and reports "no working H264 encoder found". The script
    removes it when it holds no DLL.
  - pnpm isn't on this PC: build with `node_modules\.bin\vite build`
    then `node_modules\.bin\tauri build --no-bundle --config
    '{"build":{"beforeBuildCommand":""}}'` from `app\`.
  - `video pacing` and `frame pacing` are now logged on every save too:
    a recording that ends without a clean stop left no totals.
  - PresentMon 2.6.0 (Intel's console build, signature checked) is at
    `target	ools\presentmon\` (not in git). From an administrator
    terminal the script records the game's presents during a run and
    reports its fps and the share of the clip's frame slots in which the
    game presented nothing: the floor no capture can beat (a steady
    45 fps game gives 25% at 60 fps). Its CSV summary is tested on a
    made-up capture (`-PresentsCsv <file>`); a live trace hasn't been run
    (this needs elevation), so the `--v1_metrics` column names and the
    `--terminate_existing_session` stop are still to be seen working.
- **First runs and what the clips say (2026-11-10 night):** only two
  `compat` runs in Deadlock, no PresentMon (terminal not elevated), so
  no comparison between paths yet. Findings from them and the older
  clips:
  - **The analyzer over-counted slow scenes.** In a clip with little
    motion (Deadlock: median picture difference 0.25–0.66 of 255)
    ordinary frames dip under the "same picture" threshold. A short
    freeze now counts only with clear motion (difference ≥ 0.5) on both
    sides; the rest are reported as "not counted" with the share of the
    clip that could be judged. Rescored: the old Deadlock clips fall
    from 22–27% to 0.4–3.9% (only 4–41% of them judgeable), tonight's
    45 s clip from 7.6% to 2.0%, R6S 5.1% → 4.7%, DBD 8.9% → 7.5%. The
    earlier baseline numbers in this file are from before this rule.
  - **Freezes come on a steady rhythm of about five a second** in the
    clips with real motion: R6S 190 ms, DBD 197 ms (the period noted on
    2026-11-07), Deadlock tonight 227 ms; strength 0.45–0.58 against
    0.15–0.17 by chance. Two games, same rhythm: it is the capture side,
    not the games.
  - **The capture source runs fast:** 61–69 frames a second for the 60
    asked for in every HDR-compatibility session, and `videorate` drops
    the extra. Tonight's clip: 64.65 fps, which beats against 60 every
    215 ms; its freezes came every 227 ms. Likely cause, not proven: the
    source wakes early, grabs twice in one frame slot, gets nothing new
    the second time and re-sends the old picture. To prove: make the
    source keep to 60 and see whether the rhythm goes.
  - **Live counters over a 39-minute Deadlock session** (the test build,
    after the runs): 6.6% by the size guess, 9,319 repeats in 4,832
    freezes, 35 missing frames in 32 holes, 37 frame-queue overruns. So
    the queue does drop, rarely; repeats are the bulk.
- **Cause found and fixed (2026-11-11): Windows' timer.** GStreamer's
  screen capture sources (`d3d12screencapturesrc`, `d3d11screencapturesrc`)
  sleep on the pipeline clock until the next frame slot, compute the
  slot from the time they wake, and capture at once when the slot isn't
  the last one they did. GStreamer never raises Windows' timer
  resolution (`gstsystemclock.c` waits on a GLib condition variable), so
  sleeps end on the 15.6 ms system tick, 64 a second; and Windows 11
  ignores a process's own request for finer ticks while it has no
  visible window (Reruns in the tray). On a 64 Hz tick the 60 fps source
  wakes late, lands in the next slot, and grabs again straight away;
  Desktop Duplication has nothing new for the second grab and the old
  picture goes out again. Hence 61–69 fps from the source and a freeze
  rhythm near the 64-against-60 beat.
  `clip_core::timer::PreciseTimer` (held by every capture pipeline)
  calls `timeBeginPeriod(1)` and `SetProcessInformation(
  ProcessPowerThrottling)` with the execution-speed and
  ignore-timer-resolution bits controlled and off, and undoes both when
  the last pipeline goes. Measured here by `tests/capture_rate.rs`
  (desktop duplication into a fakesink, 5 s): default timer 67.44 fps
  with 42 second grabs in a slot (the very rate the owner's sessions
  logged); with the timer held 60.01 fps and none. Not yet seen: a clip
  from a game with the fix (the next sessions' `video pacing` line
  should show `source_fps` at 60 and `dropped` near 0).
- **Next:** A/B the three Windows capture paths (HDR compatibility,
  plain window capture, "Capture the window in HDR") on the same scene
  with PresentMon, then try fixes one at a time, keeping only what the
  score says is better: opt out of Windows' background throttling and
  hold a 1 ms timer while recording, MMCSS on the streaming threads, pin
  the pipeline clock, move the leaky queue ahead of `videorate`.

## Startup, memory, updates (W2), begun 2026-11-11 (branch `startup-memory-updates`, on top of `smooth-capture`)

- **Media server token (Windows):** `random_token` read `/dev/urandom`,
  which Windows lacks, so the token was the clock's nanoseconds. Now
  `getrandom::fill`, with a test.
- **No encoder probes at launch:** `App.svelte` called `get_capabilities`
  on mount, test-encoding every encoder on each launch, tray launches at
  login included. Settings already asks when it opens; the mount call is
  gone.
- **In-game popup made on demand:** `overlay::init` only listens; the
  window (a webview of its own) is built for the first popup, which
  waits in `PENDING` until the page emits `overlay-ready` after its
  `listen`. With the popup off, or no game, it is never built. Checked:
  a tray launch now has one webview renderer; UI test `overlay` passes.
  Not yet seen: the first "Ready to record" over a real game arriving
  through the wait.
- **Updates no longer kill a recording:** `install_update` downloads
  (`Update::download`, 15-minute timeout, `cancel_update` stops it; the
  prompt has a Cancel button), then installs only when no game is being
  recorded. Otherwise the bytes wait in `updates::Pending`, the command
  answers with the game's name ("Downloaded. Installs when <game>
  closes."), and the `status-changed` listener installs once the state
  leaves recording/retrying. A render in progress refuses the update
  (`AppState::rendering`). Checks time out after 30 s.
- **Window back after an update:** the plugin relaunches with this
  launch's arguments, `--autostart` included, and its builder's
  `current_exe_args` is private. So `install` writes the new version to
  `%APPDATA%\reruns\updated-to`; `updates::take_marker` at start-up
  shows the window when it names the running version, and the page
  toasts "Updated to x.y.z" (`updated_to`). None of the update flow can
  be run from a local build (no updater key): it is compiled, linted
  and its UI mocked, and the first release after this one is the test.
- **GStreamer registry of its own (Windows bundle):**
  `%LOCALAPPDATA%\gg.reruns.app\gst-registry-<version>.bin`, older ones
  removed, unless `GST_REGISTRY` is set. Only takes effect in an
  installed build (the bundled `gst-plugins` folder must exist).
- **Hidden window does less:** a status change no longer re-lists the
  library, and the 1 s clock tick is skipped, while
  `document.visibilityState` isn't `visible`; coming back refreshes.
- **Window memory freed in the tray:** the main window has
  `"create": false`; `show_main` builds it (`create_main`, from the
  config entry, decorations off on Windows). Closed to the tray it is
  hidden, then destroyed after 60 s (`free_main_later`) or as soon as a
  game starts recording (`free_main`), unless an edit is rendering.
  With no window Tauri asks to exit; `RunEvent::ExitRequested` without
  a code is refused. On `Destroyed` the audio meters are closed (the
  page's clean-up never ran), and `found_update` hands out the start-up
  prompt once per launch. Measured on the owner's PC with the release
  build: tray launch 38 MB and no webview (was ~420 MB); window open
  422 MB; closed to the tray 416 MB, and 51 MB after the minute;
  reopening builds a new window (second launch → "showed its window").
  Lost with the window: an unfinished trim, and which page was open.
- **`list_library`** replaces `list_clips` + `list_games` (each scanned
  the whole folder); the page makes one call.
- **Update prompt:** shows the release's notes (`app/src/lib/notes.ts`
  turns the Markdown's headings, bullets and bold into elements, links
  into their text; never HTML) and "Skip this version"
  (`localStorage` `reruns/skip-update`: no prompt for that version at
  later starts, the header icon stays). UI test covers both; the
  dialog is `w-md` so three buttons fit.
- **Smaller Windows bundle:** `scripts/collect-gstreamer.ps1` now copies
  only the libraries something bundled loads: it reads each plugin's PE
  import tables (delay-loaded too), follows them through `bin/`, and
  adds the libraries the app links plus `gstd3d11`. Against the local
  1.28.8 install: 45 of 146 libraries, 41 MB of 174 MB before
  compression. The script's element check now runs `gst-inspect` from a
  folder holding only the bundled libraries, with GStreamer's `bin/` off
  the PATH. Also run against that bundle alone here: the encoder tests
  (NVENC, AMF, MF, x264), `tests/save.rs`, decoding a clip, and the app
  starting. `-All` copies everything as before. Not yet seen: the
  installer's new size from the release workflow (1.26.9), and AMD or
  Intel-only PCs.
- **GStreamer pinned to 1.26.9** in `ci.yml` and `release.yml` (it was
  whatever Chocolatey had; 1.26.9 is its newest today).
- **Left from W2:** the screenshot path converts every frame of the GOP
  (keep the last decoded frame, convert once); encoder probe results
  cached on disk by driver version; start-up phase timings in the log.

## Sounds (W3), 2026-11-11 (branch `sounds-and-popup`, on top of `startup-memory-updates`)

- **`scripts/make-sounds.mjs`** (Node, no dependencies, same output
  every run) writes the three built-in sounds to
  `app/src-tauri/sounds/`, 48 kHz mono 16-bit WAV: `clip-saved.wav`
  (two soft notes a fifth apart, D5 then A5, 270 ms, peak -14 dBFS,
  mean -24.9 dB against the old beep's -25.7), `screenshot.wav` (two
  band-passed noise clicks 60 ms apart over a faint 150 Hz thump,
  150 ms, peak -8 dBFS since clicks are so short, mean -25.8 dB) and
  `save-failed.wav` (the chime downwards an octave lower, 300 ms).
  Tune by editing the numbers and running it again; the owner hasn't
  heard them yet.
- **Echo added at the owner's request (same day):** `echo()` in the
  script adds a few duller, quieter copies after each sound, on top of
  the sound at its old level (the peak is set before the echo). Lengths
  are now 413 / 233 / 443 ms. On the chimes the delays are multiples of
  6.81 ms, a whole number of periods of every note used, so each
  reflection adds to its note: the first try at 47 ms arrived out of
  step and made the chime quieter (a comb filter). Chime over its first
  270 ms: -22.8 dB mean, from -24.9.
- **Clicks removed, sounds slowed (owner: "clippy, not cleanly made"):**
  each sound was written into a buffer that ended while it still rang
  (the chime at 270 ms, at 7% of its level), and the echo copied that
  cut two or three times: kinks 50–440 times the usual size at 265,
  315 and 365 ms. Sounds are now written into 1.2 s of room, fade out
  by themselves, get their echo, and are trimmed where they fall under
  -54 dB of the peak with a 40 ms cosine fade; the largest kink left is
  the first sample of the attack. Notes rise over 12 ms (was 6), come
  110 ms apart (was 85) and ring longer; the shutter's clicks are 80 ms
  apart (was 60). Files are 784 / 285 / 872 ms, most of it a quiet tail.
- **Screenshots have their own default** (`system::Cue`): with
  `screenshot_sound` unset they play the shutter, unless the clip sound
  is a file of the user's, which they still follow. Settings shows
  "Built-in chime" / "Built-in shutter" / "Same as clips"; the
  screenshot row's reset is "Use default". `preview_sound` takes the
  slot and resolves an unset sound from the settings.
- **Sounds play at the key press.** They used to play when the file was
  written (a clip: after the whole MP4). `service.rs` emits
  `clip-saving` / `screenshot-saving` as the command arrives while a
  game is recorded, and `save-failed` if that save then fails;
  `lib.rs::play_cue` plays the chime, shutter or failed sound. An
  edit's sound still plays when it is rendered. The popup and
  notification still come when the file exists.
- **Instant playback on Windows:** `system::play_sound` scales a plain
  16-bit PCM WAV in memory (`scaled_pcm16`: the built-in sounds, and
  such files of the user's) and hands it to `PlaySoundW(SND_MEMORY |
  SND_ASYNC)`; the image is kept in a static until the next sound
  replaces it. Anything else (MP3, OGG…) and all of Linux still go
  through GStreamer's `playbin`. Checked here: the ignored test
  `windows_plays_a_built_in_sound_without_waiting` (volume 0) starts
  all three in under 100 ms each. Not yet heard over a game; custom
  non-WAV sounds keep playbin's start-up delay (converting them to WAV
  on import would need `wavenc` in the bundle).
- Running `scripts/collect-gstreamer.ps1` locally fills
  `app/src-tauri/gst/`, and the next debug build copies those DLLs
  beside the test binaries, which then load GStreamer without its
  plugins ("no element uridecodebin"). Delete the DLLs from
  `app/src-tauri/gst/bin`, `gst/plugins`, `target/debug` and
  `target/debug/gst-plugins` afterwards.
- **Redesign (branch `redesign`, on top of `library-features`, 2026-11-13):**
  light "tactile minimalism" to the owner's written spec; see
  docs/DESIGN.md §2.0. Canvas #F5F5F7, white static panels with one
  hairline, ink #1D1D1F, accent #0071E3. Relief only on controls, from
  `tokens.css` (`--t-shadow-raise`, `-raise-accent`, `-press`; `-edge` is
  the hairline round a picture). Inter only (Archivo removed, by hand
  from `pnpm-lock.yaml` since pnpm isn't on PATH here: run `pnpm install`
  once). Layout spacing is on an 8 px grid, 4 px only for tight pairs;
  controls are 32 px high. Accents were re-tuned dark enough for white
  text ("Yellow" became "Gold").
  The sidebar, its collapse (Ctrl+B) and `prefs.sidebarCollapsed` are
  gone: pages are a segmented control in the title bar (`App.svelte`),
  and `StatusBar.svelte` is the dock at the bottom with storage and the
  Clip button. `GamesView` is a row per game with its newest clips (no
  hero, no cover cards). Earlier notes in this file about the sidebar and
  the dark theme describe the old shell. Two earlier passes the same
  days (guide blue with condensed Archivo, then dark graphite
  neumorphism) were replaced by this one.
  Text and buttons on video, art and thumbnails stay white on a dark
  scrim; anything new placed over a picture must not use `text-fg`.
  There is no dark theme. The in-game popup is the same white card.
  UI tests pass against the installed Edge (`CHROMIUM_PATH`); Playwright's
  own Chromium isn't installed on the owner's PC. Not yet looked at in
  the real app window with real game art, nor on Linux (WebKitGTK).
  Known: a toast (bottom right) covers "Render clip" in the trimmer for
  its 3 s; it did before the redesign too.
- **Sidebar and dated grid (same branch, 2026-11-13, later):** the row
  per game is gone, to the owner's second written spec; see
  docs/DESIGN.md §2.7. `App.svelte` is header (wordmark, `Library |
  Settings`, filter box, thumbnail-size slider), `Sidebar.svelte` (All
  Clips, Starred, games with counts; Settings' pages while Settings is
  open; storage at the foot), the page, and `StatusBar.svelte` as a flat
  dock (dot, game, buffer, Save Clip + key). `ClipsView.svelte` is the
  whole library: the sidebar's choice (`Scope`), in sections by day, with
  a game's screenshots among its clips. `GamesView`, `GameView` and
  `GameCover` are deleted; the first-run list moved to `FirstRun.svelte`.
  `VirtualGrid` now takes sections and windows rows (headings and card
  rows); it works out the columns itself and needs every card the same
  height, so cards no longer show label chips (labels still filter, and
  show in the viewer). The card-size setting became the slider
  (`prefs.cardMin`, px). Cards lost "Show in folder" (the viewer has it).
  "Favourite" is "Star" everywhere. Keys: `1` Library, `2` Settings.
  Left out on purpose: a Sessions list, auto-named clips ("Boss Fight":
  nothing detects events), a blur behind the duration pill (§3.1), and
  square game icons (the sidebar crops Steam's portrait capsule).
  The pass before this one is kept as a stash entry, "redesign: before
  the sidebar layout". UI tests pass (all six scenarios, Edge); not yet
  seen in the real window, with real thumbnails and art, or on WebKitGTK.
- **Obsidian (same branch, 2026-11-14):** the app is dark, to the owner's
  third written spec; see docs/DESIGN.md §2.0. It is all in `tokens.css`
  (the in-game popup follows). Primary buttons and switched-on toggles
  are platinum. The accent picker in Settings → Appearance is gone
  (`prefs.accent` is ignored): colour now comes from the game
  (`lib/accents.ts` `gameAccent`: three by hand, the rest a hue from the
  name), and only as light on an edge. The sidebar row and Save Clip take
  it through `--rim` (the `inset-shadow-rim` utility); a lightbox sets
  `--color-accent` for everything inside it.
  A clip opens in a lightbox (`ClipViewer.svelte`, the `lightbox`
  utility) over the sidebar and page; the library stays mounted and
  `inert` behind it, and `ClipsView` gets `behind` so it leaves the keys
  alone. Trim mode is gone: the strip under the picture (`Timeline.svelte`,
  the hover sprite's frames as a filmstrip) always has its handles, `I`
  and `O` always work, `T` does nothing. "Render clip" is "Save Trim";
  size, format and slow motion are behind Options. New: "Copy for
  Discord (10MB)" (`App.svelte` `copyForDiscord`) copies the clip itself
  when it is untouched and under 10 MB, else renders the edit at 10 MB
  and copies that; the render stays in the library as an edit.
  `ScreenshotViewer` is the same lightbox (no longer a `<dialog>`).
  Toasts are a pill centred above the dock; the lightboxes keep that band
  clear.
  Not done, on purpose: a waveform (nothing measures a clip's audio; it
  needs a pass in the thumbnailer), and `#52525B` as text (2.5:1).
  The light sidebar layout is kept as a stash entry, "light sidebar
  layout, before the dark theme". UI tests pass (six scenarios, Edge);
  their thumbnails are drawn stand-ins. Not yet seen in the real window
  with real frames, sprites and art, nor on WebKitGTK (two blurs, §3.1).
- **Smoked glass (same branch, 2026-11-13, fourth pass):** to the owner's
  fourth written spec; see docs/DESIGN.md §2.0. No lines between title
  bar, sidebar and page; an ambient light in the game's colour behind
  everything (`App.svelte` `.ambient`, `glowGame`); glass (`liquid-glass`,
  `seg`, `thumb` utilities in `app.css`, values in `tokens.css`). The dock
  is a floating capsule (`StatusBar.svelte`, still a `<footer>` carrying
  `--rim`) that the page scrolls under; it sits inside the `inert`
  wrapper, so a lightbox, which now covers it and the whole window under
  the title bar, takes it out of reach. Toasts drop to the window's
  bottom edge over a lightbox (`Toast` `low`). A game's page has a banner
  header (`ClipsView.svelte`). Sidebar icons fall back to a frame of the
  game's newest clip. Cards lost `cv-auto` (its paint containment clipped
  the hover lift and glow; the grid still windows above 120). Range
  inputs are restyled globally as faders. Hades II has a colour by hand.
  The lightbox header is one line; the picture gained about 56 px.
  Not done, on purpose: a waveform (still nothing measures a clip's
  audio; the strip is the clip's frames), and Steam's square icon (the
  backend doesn't list it). The "mountains, dots and red/green bars" in
  earlier screenshots were the UI test's stand-in frames, not the app's;
  the stand-ins are now soft landscapes. Screenshots came from
  `tests/ui.cjs` (Playwright, Edge), not the Playwright MCP: that browser
  has no Tauri bridge. UI tests pass (six scenarios). Not yet seen in the
  real window with real frames and art, nor on WebKitGTK, where the page
  now scrolls under one blurred capsule (§3.1 advised against that).
- **Sharp native (same branch, 2026-11-13, fifth pass):** the owner
  rejected the smoked-glass pass above as bubbly; see docs/DESIGN.md §2.0.
  Gone: the ambient light, `liquid-glass`, the floating capsule, pills,
  the hover lift and glow, the banner header, `Toast`'s `low`. Now: page
  `#0E0E10`, bars `#141417`, radii 6/4/8, a 38 px status bar edge to edge
  (in flow again, after the `inert` wrapper; a lightbox ends above it as
  before the glass pass). Select mode draws its actions over the status
  bar (`ClipsView` `bind:selecting` → `StatusBar` `covered`, which makes
  the bar `inert`). Game art slots in the sidebar and the game header fall
  back to a clip frame, then the game's colour. `thumb` is a dark frame
  with a film glyph. The UI test no longer draws stand-in frames: clip
  thumbnails 404 there, so its screenshots show empty frames. Kept from
  the glass pass: the sliding `seg` key, restyled faders, the one-line
  lightbox header, sidebar clip-frame fallback, Hades II's colour.
  "Clip copied" beside "Copied!" in earlier screenshots was the test's
  Ctrl+C notice still up; Copy for Discord raises none, and the test now
  asserts it. The Settings scroller already began under the title bar;
  the test asserts that too. UI tests pass (six scenarios, Edge). Not
  yet seen in the real window with real frames and art, nor on WebKitGTK.
- **Glass on sharp corners (same branch, 2026-11-13, sixth pass):** the
  owner asked for the glass and atmosphere again, on the fifth pass's
  geometry; see docs/DESIGN.md §2.0. Page `#09090B`, bars `#0F1014`, radii
  6/4/10/14. Back, by the owner's choice of "confined and cooler": an
  ambient light in the page's top right corner only (`App.svelte`
  `.ambient`, a `<span>` so `main > div` stays the scroller; `glowGame`;
  `Sidebar` `onhover`). `liquid-glass` for the tabs, notices and dialogs;
  the lightbox is an inset 14 px glass pane that dims the window round it
  with a spread shadow. Coloured badges became tracked type (`meta`). A
  game's page has Steam's hero as a banner fading into the page. Cards
  lift on hover again (`thumb`). The status bar is 40 px with a blur,
  which makes it a stacking context: the page's scroller is `z-[1]` so
  the selection bar inside it paints over the status bar (it was hidden
  under it for one run; the test now asks what is on top).
  **Real pictures in UI screenshots:** `tests/ui.cjs` takes
  `RERUNS_SHOT_FRAMES` (a folder of `thumb-N.jpg` / `sprite-N.jpg`) and
  `RERUNS_STEAM_ART` (Steam's `appcache/librarycache`). On the owner's PC
  the frames are in `app/tests/local-assets/` (git-ignored), pulled with
  ffmpeg from his own clips in `Videos\Reruns`, so they are real gameplay
  but not of the games the fixtures name. Without the variables the grid
  shows empty frames, as in CI. All three fixture games now list art; only
  the sidebar's portrait fails its first request (the retry test).
  Not done, by the owner's choice: a waveform (the strip stays the clip's
  frames). Screenshots are from `tests/ui.cjs`, not the Playwright MCP
  (no Tauri bridge there). UI tests pass (six scenarios, Edge), with and
  without the pictures. Not yet seen in the real window, nor on WebKitGTK.

- **Studio redesign (same branch, 2026-11-13, seventh pass):** the owner threw
  out the glass passes and the old design doc; see the new `docs/DESIGN.md`
  (the old one's performance sections are now `docs/PERFORMANCE.md`, same
  section numbers). Solid surfaces (`#0A0B0D` / `#111317` / `#181B21`,
  hairline `#242832`), radii 4/6/8, no blur anywhere, accent `#FF6A2B` for
  trim, selection and focus only; per-game colours are gone from the chrome
  (`accents.ts` now only tints a game's initial in the rail). Layout: a
  52 px icon rail (`Rail.svelte`, opens to 208 px; replaces `Sidebar`), each
  page renders its own 40 px title bar + scroller into `<main>`, a 36 px
  status strip that also carries notices (`Toast` is gone) and becomes the
  selection bar (`ClipsView` exports `selection()`, read by `App`).
  `ClipStudio.svelte` replaces `ClipViewer`: full canvas, Left/Right step
  clips (J/L now seek 5 s), deck with custom `Fader.svelte` (no native
  range inputs left). Density is `prefs.columns` (default 3), not a size
  slider. Cards never show the game as title and nothing is drawn on a
  thumbnail. **Waveform:** `waveform.ts` fetches the clip from the media
  server and decodes its first audio track in the webview (clips over
  300 MB get none); `media_server.rs` now sends
  `Access-Control-Allow-Origin: *` for that. Seen working in Edge with a
  VP9/Opus cut of a real clip; **not yet seen in the real window** (AAC in
  WebView2, WebKitGTK), where a failed decode just hides the audio lane.
  The Rust change was not compiled here. `tests/ui.cjs`: new `worst`
  scenario (worst-case data at 900×600, fails on overlap or overflow in
  any bar), `RERUNS_SHOT_CLIP` for a clip with sound, and it exports
  `fixtures`/`mockBridge`. Screenshots were also taken through the
  Playwright MCP this time: its page takes the bridge with
  `addInitScript({ path })` (a generated `tests/local-assets/mcp-mock.js`)
  and routes asset requests to Vite, which serves `tests/local-assets/`.
  UI tests pass (seven scenarios, Edge), with and without real pictures.
- **Debug builds on Windows had no encoder.** The build copies
  `gst/plugins/.keep` to `target/debug/gst-plugins`, and
  `point_gstreamer_at_bundled_plugins` took any folder there for the
  bundle, leaving GStreamer with no plugins ("no working H264 encoder
  found" on every dev run). It now needs a DLL in the folder. Seen and
  fixed on the owner's PC: the next run recorded with NVENC. This PC has
  no pnpm on PATH (Node 26, no corepack); `npx tauri dev --config` with
  `{"build":{"beforeDevCommand":"npm run dev"}}` runs it without, with
  the GStreamer paths set as CI does.

- **Media utility redesign (same branch, 2026-11-13, eighth pass):** to the
  owner's long written brief (open → latest moment → scrub → trim → copy
  or save). Replaces the studio pass above; see the rewritten
  `docs/DESIGN.md`. Warm charcoal (`#151719` / `#1C1F22` / `#25292D`),
  accent `#FF985E` with near-black text on the one primary button, system
  font (Segoe UI Variable), radii 4/8/10/12. Navigation is 216 px with
  names (72 px folded, kept in `prefs.navCollapsed`; under 1080 px it folds
  by itself). Page headers carry title, count, search and one options menu
  (sort, thumbnail size `prefs.thumbSize`, tag filter, select, not a game).
  Cards: length drawn in the picture's corner, title = clip title else the
  game (`format.ts` `gameName`: drops ®™, aliases like "Rainbow Six
  Siege"). Viewer: header with one menu, one timeline, row with Play /
  time / volume / summary + Reset / Copy clip / Save clip; trims kept per
  clip for the session. "Copy for Discord (10MB)" became Copy clip with a
  size limit in Settings → Sharing (`prefs.shareLimit`, on, 10 MB);
  Save clip renders the selection as an `_edit`. Tags are off unless
  Settings → Appearance → Tags (`prefs.tagging`); the data is untouched.
  Notices: capture strip, or the viewer's row. `Popover.svelte` is the one
  menu/popover (fixed, clamped; keys stay inside; it now waits until shown
  before focusing, and refocuses its opener only if nothing else took
  focus: both found by the tests). Earlier memory/notes banning a
  duration on the thumbnail and the game as a card title are superseded by
  this brief. `tests/ui.cjs` rewritten for it (seven scenarios pass,
  Edge), plus `shots`: pictures at 1440/1280/1024 from
  `tests/local-assets/frames.json` (git-ignored; maps real frames to their
  games). Not yet seen in the real Tauri window, nor on WebKitGTK; the
  waveform's AAC decode in WebView2 is still unverified. `app/package.json`
  still lists `@fontsource-variable/inter` (unused; pnpm isn't on PATH
  here to update the lock).

- **Proportions pass (same branch, 2026-11-14, ninth pass):** to the
  owner's brief that the eighth pass still read as a generic dark
  dashboard and the viewer was too large. See docs/DESIGN.md. Viewer: one
  workspace column (1080 px at most, following the picture's width, 560
  minimum) sized in JS from the viewer's measured box (`bind:clientWidth/
  Height`, so only a resize recomputes it), a little above the middle;
  timeline 60 px, row 40 px right under it, fullscreen as a button in the
  row (the hover button over the picture is gone). The stage no longer has
  `container-type`, which had trapped the fullscreen picture inside the
  stage. Weights: regular for nav, labels, metadata; medium for buttons;
  semibold for titles, headings, the current nav row. Radii 3/6/8/10.
  Secondary buttons are outlined, icon buttons bare, toggles accent when
  on. Sidebar: full-width 40 px rows, no counts, 2 px accent edge on the
  current row; in Settings it holds Back to clips and Settings' pages
  (Settings' own page list is gone). Settings opens over the library,
  which stays mounted (`invisible inert`), so search and scroll survive;
  no capture strip there, notices in Settings' header. Cards: title and
  time on one line, game/Trimmed/star under it, hover actions cover the
  right of both lines. `tests/ui.cjs` updated (seven scenarios pass,
  Edge) and asserts Back to clips keeps the search and scroll; `shots`
  covers 1440×900, 1280×800, 900×600 and a 16:9 clip
  (`tests/local-assets/clip-16x9.webm`, a crop of the ultrawide one).
  Then run in the real Tauri window on Windows (dev build, the owner's
  library), driven over WebView2's debugging port
  (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…`,
  Playwright `connectOverCDP`). Found: fullscreen made the window
  fullscreen but the picture kept its place in the column, because the
  player carried both `relative` and `fixed` and Tailwind's order let
  `relative` win; `relative` is now only outside fullscreen, and the
  test asserts fullscreen fills the window. Seen working natively: the
  viewer at 1280×902 (1080×452 for a 3440×1440 clip) and the 884×591
  minimum, fullscreen at 3440×1440 and back, the waveform decoded in
  WebView2, playback with no dropped frames, Settings and Back to clips.
  A benign "ResizeObserver loop" warning fires in the library on window
  resize (VirtualGrid; delivered next frame, not logged by the app).
  Seen in passing: the recorder's HDR lookup gave 240 nits SDR white /
  456 peak on one start and the 80 / 0 fallback on the next two, same
  display. Not tried: Copy clip / Save clip (they write files), the
  in-game popup, WebKitGTK.
- **Finishing pass (same branch, 2026-11-14, tenth):** the owner kept the
  ninth pass's structure and asked for softer, more responsive details.
  Nav rows inset 8 px with 6 px corners, current row 8% + medium + a
  3 × 16 accent tab inside it (glyphs unmoved). Timeline: 1 px accent
  hairline on the kept range (40% while it is the whole clip), 4 px
  handles a step under the accent that widen outward (transform) and show
  their time on hover/focus/drag, lighter seams, waveform at 70%, cut
  parts at 66%; `ew-resize` stays while a handle is held. Grid: second
  card line 18 px and day heading row 36 px (title to next day 66 → 40
  px), a hairline ring on card hover. Copy clip's button says ✓ Copied
  for 1.6 s at a fixed width (App owns `copied`; a `copying` guard stops
  a second as-is copy); the notice beside it only for "under N MB".
  Hover/press now use the strong ease-out (plain `ease` started slowly).
  `tests/ui.cjs`: copy failure → Try again → Copied (width checked), nav
  weight 500, handle colours; `shots` adds nav hover/focus, folded hover,
  handle hover/drag. Checked in Edge only: drag has 0 px lag, reduced
  motion zeroes every touched transition, maximized 3440 at 100%/125%
  keeps the 1080 workspace. Not run in the real Tauri window.
  HDR: the 80 / 0 above was the display not answering (game minimized,
  03:46 local); an unread SDR white level is now 0 (was a silent 80),
  re-read each scan; see docs/HDR.md.

## v2.0.0-beta.1 prepared (2026-11-24, branch `release/prepare-v2-0-0-beta-1`)

Version 2.0.0-beta.1 in the workspace, `app/package.json`,
`tauri.conf.json` and both `clip-ipc` path dependencies (a `1.2.0`
requirement doesn't match a 2.0 pre-release). Notes are in
`docs/release-notes/v2.0.0-beta.1.md`. The dash in the tag makes the Release
workflow publish a pre-release, which installed 1.2.0 copies never see,
because the updater reads `releases/latest`. The beta ticks none of the
v2.0.0 must-haves; it gets the redesign and Setup into testers' hands.
Checked locally: Setup's tests and the app's build scripts accept the
version. A Release workflow run on the branch built NSIS, Setup, AppImage
and deb, but Tauri CLI 2.12.4's rpm bundler refuses a version with a dash
(fixed upstream in tauri-apps/tauri#16336), so pre-releases now skip the
`.rpm`. The deb's control file gets `Version: 2.0.0-beta.1` as is (Tauri's
`debian.rs`), which dpkg sorts after 2.0.0; the notes tell deb testers.
After the PR merges: tag `v2.0.0-beta.1` on `main`'s merge commit, then
publish the draft as a pre-release.

## Next, in order
1. v1.0.0: owner adds the updater key secrets (`docs/RELEASING.md`), then
   tags `v1.0.0` on `main`; the Release workflow makes the draft. Owner
   reviews and publishes it. Then apply to SignPath for Windows signing.
2. Windows run on the owner's machine of what landed: capture restart +
   fixed-size letterbox (#4 noted these as unverified), HDR compatibility.
3. HDR window capture (#13) on a real HDR display; it is off by default
   and marked experimental until then.
4. Roadmap phase 2 (editor) per `docs/ROADMAP.md`.
