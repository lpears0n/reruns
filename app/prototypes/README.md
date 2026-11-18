# Design prototypes (November 2026)

Two visual directions for the library and viewer, built as swaps of real
components so they run against the real app. Nothing in `src/` is edited,
and none of this ships: production builds use `vite.config.ts`, which knows
nothing about this folder.

- `a/` — **Direction A, "tinted desk"**: the current layout, refined. Warm
  umber surfaces, a recessed navigation, Steam box art at its own 2:3 shape,
  a compact game header tinted from the game's art, tactile Play and Copy,
  marked moments in their own lane on the timeline.
- `b/` — **Direction B, "sessions"**: the library reads as evenings of play.
  A game page opens on a compact band of the game's own key art and logo,
  its clips grouped by the play sessions the recorder logged; All clips says
  what was played each day. One glyph for a saved moment (a diamond) on the
  cards, the timeline and the transport; a hollow one where F8 was pressed.
  The viewer offers "Set as cover" while paused, and shows the rest of the
  session's clips in the room under the controls.
- `shared/` — what both use: session grouping, the art tint, the cover frame,
  open/return continuity.

`review.html` lays the three builds side by side with the assessment.

## Running

Three servers, side by side (or `preview_start` with `proto-a`, `proto-b`,
`proto-base` from `.claude/launch.json`):

```bash
node app/prototypes/serve.cjs a      # http://localhost:5174
node app/prototypes/serve.cjs b      # http://localhost:5175
node app/prototypes/serve.cjs base   # http://localhost:5176, the app as it is
```

A direction is any file in `a/` or `b/` named like one in `src/lib/` (or
`src/App.svelte`): the plugin in `vite.config.ts` swaps it in, and the
override's own imports resolve as they would from the original. `theme.css`
in a direction is loaded after the app's styles.

The pages need the Tauri bridge, so open them through the screenshot driver,
which mocks it with a library whose cards, art and footage agree:

```bash
cd app
CHROMIUM_PATH="C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" \
RERUNS_SHOT_FRAMES="$PWD/tests/local-assets" \
RERUNS_STEAM_ART="C:/Program Files (x86)/Steam/appcache/librarycache" \
RERUNS_SHOT_CLIP="$PWD/tests/local-assets/clip.webm" \
node prototypes/shots.cjs base a b
```

Pictures land in `prototypes/shots/` as `<build>-<width>-<what>.png`, at
1440 × 900, 1280 × 800, 900 × 600 (the smallest window) and 1920 × 1040 (a
maximized 1080p window), on the ultrawide clip and its 16:9 cut. Edge is used
because it is the engine WebView2 runs on Windows.

Type-check a direction: `node node_modules/svelte-check/bin/svelte-check
--tsconfig ./prototypes/tsconfig.b.json` (from `app/`).

## What is real, what is prototype, what is untested

Real (the app does this today, the prototypes only show it): replay clips
and their F7 marks in sidecars; play sessions logged per game
(`sessions.jsonl`, the newest 50 per game reach the UI); Steam capsule, hero
and logo art from Steam's own cache; Copy clip rendering the trim, audio mix,
9:16 and slow motion to fit the Sharing size, then saying Copied.

Prototype only:

- **Cover frame** is drawn from the playing video into memory and lost on
  reload. The real version keeps the moment in the sidecar (`"cover": 7.25`)
  and has the thumbnailer grab that frame with an accurate seek.
- **Art tint** is read from the picture in the page, which needs the image
  served with CORS. The driver serves it that way; Tauri's asset protocol
  may not, and then the tint falls back to the game's hand-picked colour. A
  real version computes it in Rust when the library is listed.
- **Games and clips reach the library and viewer through a module store**
  (`shared/library.svelte.ts`) written by the navigation, so `App.svelte`
  stays untouched. A real change passes them down.
- **"Saved here" (the hollow diamond)** sits at the clip's end, which is
  where F8 was pressed for an untrimmed replay. It only becomes interesting
  with time kept after the press (a proposal, not built).
- **The session data in the pictures** is fixture data shaped like
  `sessions.jsonl`; the clips, art and footage are real.

Untested: WebKitGTK (Linux) rendering of all of it, `mask-image` on the game
band and `OffscreenCanvas` there in particular; the open/return flight's
smoothness on WebKitGTK; tint extraction inside Tauri; logos whose own
colours sit badly on their tint; libraries over 120 clips (the grid windows
rows, the flight then can't find a card that isn't rendered and only rings
nothing).
