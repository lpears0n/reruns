# Reruns design

This direction follows the user's final brief of November 16, 2026. The
previous full-width library, horizontal capture dashboard, orange palette,
rounded workspace, and readiness display were explicitly rejected. Earlier
prototypes and screenshots are references, not approval to restore them.

Reruns is a quick gameplay clipping utility: capture, find, watch, copy.
Trimming is optional. The interface must not grow into a production editor
or a recorder monitoring dashboard. Performance rules remain in
[PERFORMANCE.md](PERFORMANCE.md).

## Identity

The user approved the paired **rr** mark from the product-direction study.
Preserve that mark. Its angular, stepped geometry is also used in the
custom outlined lowercase **reruns** wordmark.

- `app/public/brand-mark.svg`: transparent rr symbol.
- `app/public/brand-wordmark.svg`: outlined lettering, no font dependency.
- `app/public/favicon.svg`: ink-backed icon.
- `app/src-tauri/icons/`: native PNG, ICO and ICNS assets. The ICO contains
  16, 24, 32, 48, 64 and 256px frames. The tray uses the configured app icon.
- In-app overlays use the transparent mark on their existing surface.

Keep pixel styling in the identity; interface text remains readable system
type. Do not introduce another logo concept or arbitrary accent colour.

Settings → Appearance → Theme is the user's choice of accent: Off-white
(the default), Blue, Mint, Violet, Pink (`tokens.css`, `data-theme` on
`<html>`), or Rainbow, which keeps off-white controls. `Brand.svelte` draws
the mark and wordmark inline: a colour theme tints the mark with its
accent; Rainbow gives the eight glyphs one rainbow, turns its hue and
wiggles the letters in a wave.

## Composition

`LibraryNav.svelte` owns the 228px collection sidebar: identity, All clips,
Favorites, game covers/names, capture and Settings. All clips and Favorites
stay fixed; only the games list scrolls, with the app's own scrollbar (never
`scrollbar-width`, which swaps in the system one). A hairline ends the list
above capture and Settings, which line up with the navigation rows. Broken
game covers fall back to an actual clip frame, then an initial.

The window's own strip (Windows) carries on the sidebar's and canvas's
colours, so the content runs to the top edge as in Windows 11's own apps
rather than sitting in a frame; over an open clip it is all canvas.

`ClipsView.svelte` owns one page header: current collection, count, search,
View and occasional library actions. A game selection can narrow Favorites.
The collection being browsed is distinct from the game named in capture.
Choosing another collection starts at its newest clips. Sort and grouping
are remembered between launches. An empty library shows no count or View.

Clips are grouped by recent periods, then months (Today, Yesterday, Past
week, November, October 2025…): a library gains a few clips a day, and day
sections left most rows empty. Four cards fit a row at 1280-1440 (medium).
Screenshots sit among the clips by time (their game's when one is chosen;
never in Favorites). By day, and a game's play sessions, remain View options. Routine browsing does not show playtime totals, capture LEDs,
buffer meters, or readiness counters. Selection actions appear only during
selection.

`CaptureControl.svelte` is one compact command beneath the collections.
While a game records it is a single 44px raised control in two parts:
**Save clip** over the game's name, the shortcut at the right when it is
short, and a joined ⋯ for Capture options. "Save clip", "Saving…" and
"Saved ✓" share one cell, so the control never changes width. When the
encoder fell back, the game line names what records instead (· CPU, · AMF).
Otherwise there is no dead button: one quiet line says "Waiting for a game"
(with the shortcut), "Capture interrupted / Retrying…", or "Capture
unavailable" in the error colour with the first clause of the reason.
Availability follows actual recorder state without displaying its buffer
telemetry; full diagnostics and recording settings remain in the options.

## Media and feedback

Clip and screenshot cards use the same measured frame/caption geometry.
The image opens the item. The caption is one 20px line: a name the user
gave in semibold, an unnamed clip's game in quiet regular type, then (room
permitting) the game after a name, Trimmed, the favorite star, and the time
at the end, as much of it as the section heading leaves unsaid ("4:58 PM",
"Mon 4:58 PM", "Nov 10"). Marked moments are the diamonds under the image.
Everything is in the caption's tooltip. Copy and More are on the image, revealed on
pointer hover or keyboard focus, and always available on touch. Do not move
or scale the card to reveal them. Copy reads Copy, then Copied, in the same
width; while a large clip is prepared its foot line fills with the progress
and the notice carries Cancel. Screenshots get the same Copied.

Each card is one tab stop. Arrow keys move between cards; on a focused card
Ctrl+C copies, Delete deletes (focus moves to the neighbour) and F toggles the
favorite. The card's menu (right-click, Menu key) names these keys. Ctrl+Z
takes back the delete or ignored game that the notice offers.

The preview opens with playback, Copy and optional Trim. Preserve trim edits
when the controls close, source-safe retries, cancellation and fullscreen.
The library and viewers use neutral backgrounds without sampled colour
halos. Footage supplies the colour.

Saved feedback, for clips and screenshots alike, contains the actual frame
when available, concise metadata, Copy and Open. A transient notification has tight corners and no decorative
status dot or placeholder symbol. Progress is shown only for actual work.
Keep Cancel and deletion Undo reachable, including when both operations
overlap. Pause saved-notification expiry during hover or keyboard interaction.
Long error text scrolls without pushing its actions outside the window.

The empty library gives one instruction and the setup action it needs.
Do not display a setup checklist, full folder paths, or reassuring slogans.

## Motion

Motion says one of two things: time was captured, or a clip moved. The 2px
line at the foot of a frame means time: it follows the pointer when
scrubbing, fills while a copy is prepared, and runs once across a clip that
arrived. Everything else is opacity or colour.

- **Arrival:** clips saved while the window was hidden or unfocused come in
  when it is in front again (and at once when saved from the window): the
  frame fades in over 160ms, then the foot line sweeps across in 200ms,
  holds about a second and fades. At most four cards; the pointer stops it.
- **Card and player:** the picture flies to the player in 200ms and back in
  180ms with the frame's 4px corners; the card it lands on rings for 700ms
  and fades over 180ms. Esc during the flight drops it.
- **Feedback:** Save clip crossfades to Saved for 1.2s; notices rise 8px in
  180ms and leave with a 4px drop in 120ms; menus come 4px from their
  trigger in 120ms and close at once; pressed rows and tools step one shade.

Budget: routine motion 200ms or less, transform and opacity only (the
flight's single ghost excepted), no loops, nothing on every card at once.
The one loop is the opt-in Rainbow wordmark: a `hue-rotate` filter and
transforms only (8 ms of main-thread work per 5 s in Edge at 240 Hz;
animating `fill` cost about 15% of a core), and it stops while the window
lacks focus or motion is reduced.
Reduced motion (system or Appearance) keeps every state change but nothing
travels: arrivals ring the card instead.

## Surfaces and type

Tokens live in `app/src/tokens.css`; the app and in-game overlay share them.

| Role | Value |
| --- | --- |
| Sidebar | #121313 |
| Library and viewer | #1b1c1c |
| Controls | #232424 |
| Menus and feedback | #2b2c2c |
| Main text | #f1f0eb |
| Main action / focus | #e4e5df (or the theme's accent) |
| Control corners | 3px |
| Media / menu corners | 4px |
| Workspace corners | 0px |

Use the system typeface at readable sizes: 14px body, 13px supporting text,
12px compact metadata. The collection title is 24px. Do not apply the pixel
wordmark's lettering to navigation or metadata.

The user's constraints are explicit: no orange/purple/blue decorative theme
by default (accent themes change only the accent, and only when chosen),
pill navigation, oversized rounded panels, bubbles, status LEDs, readiness
counters, or unnecessary explanatory wording. Error and destructive-action
colours still communicate actual consequences.

## Interaction and validation

- Maintain keyboard access, visible focus, source-card focus restoration,
  context menus, selection shortcuts, and reduced-motion behavior.
- Menus must survive a scroll event queued before opening, and close when
  the underlying collection actually scrolls afterward.
- Preserve virtualized large libraries and thumbnail/sprite performance.
- Respect configured sharing limits. Small files copy directly; preparation
  uses actual progress and cancellation cannot copy a late result.
- Preserve native Windows drag regions and caption-button behavior.
- Review real-media screenshots at 900×600, 1280×800 and 1440×900, including
  long names, empty libraries, missing artwork, keyboard focus and failures.
- Run Svelte/TypeScript checks, the frontend build and applicable UI tests.
  A passing build does not establish visual approval.

## Research used

The useful observations concern behavior and composition, not a prescribed
palette. Historical screenshots describe their documented versions.

- [NVIDIA app gallery and capture](https://www.nvidia.com/en-us/geforce/news/nvidia-app-download-and-features/):
  organize saved media by game and make capture a direct command.
- [Medal library and Quick Trimmer](https://support.medal.tv/support/solutions/articles/48001170449):
  open the clip for quick review, keep occasional actions contextual.
- [SteelSeries Moments](https://steelseries.com/gg/moments):
  footage, basic metadata and hover replay support gallery browsing.
- [Steam Game Recording](https://store.steampowered.com/gamerecording):
  lightweight clipping and getting a clip out remain the central workflow.

The local `.claude/skills/emil-design-eng/SKILL.md` informed interaction
checks. User instructions and the repository's performance constraints take
precedence over its optional aesthetic or animation suggestions.
