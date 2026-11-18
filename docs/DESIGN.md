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

## Composition

`LibraryNav.svelte` owns the 228px collection sidebar: identity, All clips,
Favorites, game covers/names, capture and Settings. Games scroll independently
when the window is short; capture remains reachable. Broken game covers fall
back to an actual clip frame, then an initial.

`ClipsView.svelte` owns one page header: current collection, count, search,
View and occasional library actions. A game selection can narrow Favorites.
The collection being browsed is distinct from the game named in capture.

Clips are grouped by day by default. Session grouping remains an explicit
View option. Routine browsing does not show playtime totals, capture LEDs,
buffer meters, or readiness counters. Selection actions appear only during
selection.

`CaptureControl.svelte` is one compact command beneath the collections.
It names the game being clipped and offers **Save clip** with the configured
shortcut. Availability follows actual recorder state without displaying its
buffer telemetry. Waiting and failures are described when relevant; full
diagnostics and recording settings remain in Capture options.

## Media and feedback

Clip and screenshot cards use the same measured frame/caption geometry.
The image opens the item. Title, game and timestamp retain stable positions.
Copy and More are on the image, revealed on pointer hover or keyboard focus,
and always available on touch. Do not move or scale the card to reveal them.

The preview opens with playback, Copy and optional Trim. Preserve trim edits
when the controls close, source-safe retries, cancellation and fullscreen.
The library and viewers use neutral backgrounds without sampled colour
halos. Footage supplies the colour.

Saved feedback contains the actual frame when available, concise metadata,
Copy and Open. A transient notification has tight corners and no decorative
status dot or placeholder symbol. Progress is shown only for actual work.
Keep Cancel and deletion Undo reachable, including when both operations
overlap. Pause saved-notification expiry during hover or keyboard interaction.
Long error text scrolls without pushing its actions outside the window.

The empty library gives one instruction and the setup action it needs.
Do not display a setup checklist, full folder paths, or reassuring slogans.

## Surfaces and type

Tokens live in `app/src/tokens.css`; the app and in-game overlay share them.

| Role | Value |
| --- | --- |
| Sidebar | #121313 |
| Library and viewer | #1b1c1c |
| Controls | #232424 |
| Menus and feedback | #2b2c2c |
| Main text | #f1f0eb |
| Main action / focus | #e4e5df |
| Control corners | 3px |
| Media / menu corners | 4px |
| Workspace corners | 0px |

Use the system typeface at readable sizes: 14px body, 13px supporting text,
12px compact metadata. The collection title is 24px. Do not apply the pixel
wordmark's lettering to navigation or metadata.

The user's constraints are explicit: no orange/purple/blue decorative theme,
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
