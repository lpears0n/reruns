# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

PC gamers on Windows and Linux, weighted equally: neither platform leads
decisions. They use the core feature mid-game, often fullscreen, with one
hotkey and no window switching. They come back to the desktop window
later to watch, trim and share, mostly into Discord. The owner uses it
daily on Windows and on KDE Plasma 6 (Wayland), both with NVIDIA.

## Product Purpose

Reruns is an instant replay recorder. It keeps the last N seconds of the
game in memory; F8 saves them as an MP4 named after the game, F9 saves a
screenshot from the same buffer, F7 marks a moment. A local library and
editor turn those saves into clips worth sending.

Success: a moment is saved without the player noticing the recorder,
recordings play back without stutter, and a clip gets from the library
into Discord in seconds.

## Positioning

Against Medal, ShadowPlay / NVIDIA App and Steam Game Recording, Reruns
claims all four of these together:

- **Local, no account.** No sign-up and no Reruns cloud. Clips stay on
  disk until the user shares them.
- **Light and smooth.** A small Tauri app, GPU encoding probed per machine
  at native refresh rate, no stutter in game or in the recording.
- **Works on Linux.** Prompt-free window capture on KDE Wayland, and any
  game (Steam app id or Wine `.exe`), not only Steam titles.
- **The best editor and library of the free options.** Trim, multi-track
  mix, render under a size (10 MB for Discord), 9:16, slow-mo, marks,
  favorites and labels.

Steam Game Recording is free and already on Linux, so these claims are
where Reruns has to win (docs/ROADMAP.md §1).

## Operating Context

- In game: global hotkeys (F8 clip, F9 screenshot, F7 mark; rebindable,
  KGlobalAccel on KDE), a save sound, and on Windows a small popup over
  the game ("Recording <game>", "Clip saved") that never shows up in clips.
- Recording starts by itself when a game is detected; there is no capture
  picker. The app lives in the tray and closing the window hides it.
- Desktop window: Games, All clips, the viewer/trimmer and Settings.
  Keyboard-first (`?` lists viewer keys, `Ctrl+F`, `1 2 3` switch views).
  Destructive actions are undone from a toast, not confirmed.
- Sharing: copy a clip as a file (`Ctrl+C`) or drag a card out of the
  window, then paste or drop into Discord.
- Stars, names and labels live in a `.json` sidecar next to each clip, so
  they survive moving the folder.
- `rerunsctl save|screenshot|mark` drives the running app from a shell.
- Distribution: GitHub releases (Windows installer, AppImage, deb, rpm)
  with an in-app update prompt and release notes (docs/release-notes/).

## Capabilities and Constraints

- Existing stack: Rust workspace (`crates/clip-core` capture and encoding,
  `clip-ipc`, `rerunsctl`), Tauri 2 + Svelte 5 + Tailwind 4 in `app/`,
  GStreamer for capture, encoding and muxing.
- The UI runs in WebView2 on Windows and WebKitGTK on Linux. The
  WebKitGTK rules and budgets in docs/DESIGN.md §3 and §5 are binding: the
  app runs next to a game and must never cost it frames.
- Dark only. No light theme until the app has a reason for one
  (docs/ROADMAP.md §2).
- The accent color, card size, hover previews and motion are user
  settings (Settings → Appearance).
- **Ads are planned**: a free, ad-supported app. `settings.ads` (off by
  default) shows a 728×90 slot under the main view
  (`app/src/lib/AdSlot.svelte`). Future layouts leave room for it. Open:
  the provider, placement rules, and whether an ad-free option exists.
- **Mobile is planned**: watch clips away from the PC, like Medal. First a
  companion over Wi-Fi (pairing, browsing, streaming, remote save), then a
  sync server run by the Reruns owner. Desktop work keeps both paths open
  (stable clip ids, a library index with delta listing, one
  transport-neutral library API). Open: how that server fits "no account".
- Unverified on real hardware: AMD AMF, Intel Quick Sync, HDR window
  capture on Windows, the GNOME/Hyprland portal path, Linux zero-copy
  capture. X11 is not supported. The Windows installer is not yet signed.
- Terms: clip, screenshot, mark, replay buffer, library, Games, All
  clips, viewer, trim, render, Format (9:16), Slow-mo, favorite, label.

## Brand Commitments

- Name: Reruns (formerly clip.sh). Identifier `gg.reruns.app`, CLI
  `rerunsctl`. Icons in `app/src-tauri/icons/`, `app/public/favicon.svg`.
- Voice: the copy rules in docs/DESIGN.md §2.8. Sentence case, verbs on
  buttons, the fewest words that work, no selling or reassurance, toasts
  say what happened. UI copy mostly uses US spelling (Favorites, Color).

## Evidence on Hand

- Real: the GitHub releases and their notes, the README, screenshots of
  every view from `pnpm test:ui` (`app/tests/shots/`), the owner's own
  clips.
- Tested only on the owner's machines (Windows and KDE Plasma 6, NVIDIA).
- None yet: user counts, testimonials, reviews, press, or measured
  comparisons with Medal, ShadowPlay or Steam. Numbers in docs/DESIGN.md
  are budgets or third-party measurements, not Reruns results. Do not
  invent any of these.

## Product Principles

1. **The game comes first.** Nothing Reruns does may cost frames, focus or
   attention in game. The in-game surface is a hotkey, a sound and a brief
   popup.
2. **Local by default.** Anything that leaves the disk is the user's own
   action.
3. **Measured, not claimed.** Smoothness and speed claims ship when they
   are measured; untested hardware is labelled untested.
4. **Windows and Linux are equals.** When a feature works on one and not
   the other, the app says so.
5. **Keyboard first, undo over confirm.**

## Accessibility & Inclusion

- WCAG AA contrast at minimum: 4.5:1 for text, 3:1 for icons.
- `prefers-reduced-motion` is honoured, with an in-app override.
- Every viewer and library action has a key; focus rings show on
  `:focus-visible`.
- Visible copy is terse, so `aria-label`s stay fully descriptive. The
  in-game popup is a `role="status"` live region.
