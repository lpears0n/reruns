# Reruns design system

A desktop media utility: open Reruns, find the latest moment, scrub, trim,
copy or save. Warm charcoal surfaces that stay out of the gameplay's way,
one warm accent for the thing to do, the system's own typeface. Its
character is in proportions and restraint, not decoration: modest
rectangles, regular-weight text with few semibold anchors, controls that
show a surface only when touched. Written from the code as built
(2026-11-14). Values live in `app/src/tokens.css`;
`app/src/app.css` maps them into Tailwind and defines the shared pieces.
Performance rules are in `docs/PERFORMANCE.md` and bind this too.

## Never

- `backdrop-filter`, blur, translucent panels, gradient washes or glows.
- Pill-shaped metadata badges, coloured chips, all-caps section labels,
  decorative monospace, keycap badges (keys are plain type).
- A toast or bar floating over a page. Notices are said in the capture
  strip, or in the viewer's action row.
- A box over the grid for a clip: the viewer takes the library's place.
- Looping or pulsing indicators. Nothing animates forever.
- A native `<input type="range">`. Use `Fader.svelte`.

## Colour

| Token | Value | For |
|---|---|---|
| `canvas` | `#151719` | The page, where pictures sit |
| `chrome` | `#1C1F22` | Navigation, the window's strip, fields, empty frames |
| `raised` | `#25292D` | Buttons, menus and dialogs, the current navigation row |
| `hover` | `#2F3439` | A raised control or menu row under the pointer |
| `line` / `line-strong` | `#2A2E32` / `#3D4349` | Hairlines; field borders and menu rules |
| `fg` / `fg-2` / `muted` / `faint` | `#F2F1ED` / `#A9AFB6` / `#8D939B` / `#5B6169` | Text; faint only for marks and disabled things |
| `accent` | `#FF985E` | The one main action (near-black `on-accent` text), the kept trim range and handles, the current navigation row's tab, a switched-on toggle, focus |
| `ok` | `#4CC77A` | The recorder really capturing. Nothing else is green |
| `danger` | `#F26D5F` | Errors, Delete |

## Shape and controls

- Radii: **3 px** small marks, the duration label, game art; **6 px**
  controls and fields; **8 px** thumbnails and the player; **10 px**
  dialogs. Navigation rows are a list with no surface until touched;
  then it is inset 8 px from the edges with 6 px corners.
- Controls are not all the same box. `btn-primary`: filled accent, one
  per view. `btn` / `btn-sm`: no fill, a `line-strong` hairline, a faint
  white surface under the pointer. `btn-icon`: bare until hover or focus.
  Fields: `canvas` well with a hairline. 36 px tall (`btn-sm` 32).
- Toggles (`Toggle.svelte`): off is an outlined track, on is accent-filled;
  disabled keeps its state at 40% so "off" and "on but unavailable" differ.
- Only what leaves the page has a shadow (`shadow-pop`: menus, popovers,
  dialogs, the navigation tooltip). Nothing is nested in a card.

## Type

System font (Segoe UI Variable on Windows). Body 14/20; secondary 13/18
for metadata and descriptions; caption 12/16; title 16/22 (day headings,
settings groups); heading 18/24 (a clip's title in the viewer); page 24/32
(a page's name). Numbers are tabular.

Weights: **regular** for navigation, settings labels, descriptions and
metadata; **medium** for buttons and the current navigation row;
**semibold** only for page titles, clip titles and section headings. Sentence case
everywhere. Metadata is placed by alignment and spacing, never joined with
middle dots.

## Layout

```
┌──────────────────────────────────────────────────────┐
│ window strip (32, Windows only)            – □ ×     │
├──────────┬───────────────────────────────────────────┤
│ nav 216  │ page header: title  count      search ⚙  │
│ (72      │ the page's scroller                       │
│ folded)  ├───────────────────────────────────────────┤
│          │ capture strip (44)            Save replay │
└──────────┴───────────────────────────────────────────┘
```

- **Navigation** (`Rail.svelte`): 216 px with names (no counts: the
  page header has them), 40 px rows inset 8 px (glyphs 36 px from the
  window's edge, open or folded), game art at 28 px; folds
  to 72 px of icons (the choice is kept in prefs). A window under 1080 px
  folds it without touching the choice, and hides the control. Hover: a
  4% white surface. Current: 8%, medium, and a 3 × 16 px accent tab on the
  selection's inner left edge. Keyboard focus: its own accent ring inside
  the row. Folded, or with a name cut short, a row names itself in a
  tooltip at once. While Settings is open its contents are Back to clips
  and Settings' pages.
- **Page header** (`page-header`, 80 px, 32 px side padding): the page's
  title (24 px), what it holds, then search and one options menu. Sort, thumbnail size, the tag
  filter, selecting and "not a game" are in that menu.
- **Grid**: newest first in day sections; 3 columns at 1280–1440 with the
  navigation open (Medium; Small and Large in the menu). A card is the
  16:9 picture with its length in the corner, then two lines with fixed
  places: the title (the clip's, else its game as people write it, else
  the time in one game's list) with the time saved on the right; under
  it the game when the title is the user's, "Trimmed", and a favorite's
  star on the right. Copy, delete, favorite cover the right of both lines
  on hover or focus, without moving anything; pointed at, a hairline
  rings the picture. Moving across the picture scrubs it. Spacing: 10 px
  from picture to title, 20 px between cards, a day's heading 40 px under
  the last row's title and 12 px over its own cards. Right-click or the Menu key opens the card's menu;
  arrow keys move between cards; Ctrl-click / Shift-click pick.
- **Capture strip** (`StatusBar.svelte`, 44 px, library only): a still dot and one true
  sentence ("Capturing Rainbow Six Siege", "Waiting for a game"), its menu
  ("… is not a game", Recording settings), and Save replay while there is
  a buffer. Notices with their action sit on its right. While clips are
  picked it becomes the selection bar. There is no strip in the viewer.
- **Viewer** (`ClipStudio.svelte`): replaces the library, opaque. One
  workspace column, 1080 px at most, that follows the picture's width (it
  is as wide as the column allows and as tall as leaves room for the rest;
  never narrower than 560 px). Spare height goes one third above, two
  thirds below. Stacked: header 56 (Back, title, game and when, one menu
  holding favorite, rename, tags, speed, loop, fullscreen, trim points,
  audio mix, 9:16, slow motion, previous/next, shortcuts, delete, file
  details), 20, the picture at its own shape, 14, the 60 px timeline
  (frames over a 16 px waveform, playhead, two handles that never cross,
  the cut parts dimmed). Its order of attention: the white playhead and a
  handle being touched, then the kept range (a 1 px accent hairline, faint
  while it is the whole clip) and its 4 px handles a step under the
  accent, then the frames, then the waveform. A handle under the pointer,
  the keyboard or a drag widens outward and says its time; 10, the 40 px row: Play, `0:05 / 0:31`, volume,
  fullscreen, "7.0 seconds selected" and Reset once something changed;
  then status, Copy clip (primary), Save clip. Nothing is drawn on the
  picture outside fullscreen.
- **Settings**: opens over the library (kept mounted, so Back to clips
  returns to the same search and scroll). The page's name as the title,
  one column of rows (680 px at most), groups apart by space under a
  heading, no rule under each row. No capture strip; notices are said in
  its header.

## Copy clip and Save clip

- Copy clip puts a file on the clipboard. The clip itself when nothing
  changed and it is under the Settings → Sharing limit (10 MB by default,
  on); otherwise the selection rendered to fit, which also stays in the
  library as an edit.
- Save clip renders the selection as a new `_edit` file beside the
  original; the original is never touched. Only limited when Sharing says
  so.
- Both show progress with Cancel, refuse a second press while running,
  and on failure keep the selection and offer Try again. Only when done:
  Copy clip's own button says "✓ Copied" for 1.6 s at the same width
  (with "Clip copied, under 10 MB" beside it when it was fitted); Save
  clip says "Clip saved".
- A trim is kept per clip for the session: going back and reopening finds
  it.

## Motion

Colour and opacity only (and a trim handle widening by transform),
120–180 ms, strong ease-out; a press darkens, nothing bounces. Dragging
and scrubbing follow the pointer with no easing at all. The viewer fades in
and leaves at once; keys never wait on an animation. The only keyframe is
the one flash on Save replay. Reduced motion (system or Appearance) zeroes
all of it.

## Copy

The fewest words that work; verbs on buttons; notices say what happened.
A button whose action finishes in place may say so itself.

## Checking a change

`pnpm test:ui` drives every view with the Tauri bridge mocked
(`app/tests/ui.cjs`); `worst` stresses long names, missing pictures and a
900×600 window. `pnpm test:ui shots` takes pictures at 1440×900, 1280×800
and the 900×600 minimum from a local library whose cards, art and footage
agree, with the viewer on an ultrawide clip and on a 16:9 cut of it
(`clip-16x9.webm` beside the clip). It needs the `RERUNS_SHOT_*` variables
and a `frames.json`; see the top of the test.
Look at `app/tests/shots/` before calling a UI change done.
