<!--
  The viewer's one timeline: the clip's frames as a filmstrip, its audio as
  a thin lane under them. Click or drag on it to seek. The two handles at
  its ends set what is kept; they can be dragged at once, with no editing
  mode: what is cut is dimmed, what is kept has a thin accent edge (fainter
  while it is the whole clip). The playhead is the brightest thing on it;
  a handle comes forward under the pointer, the keyboard or a drag, and
  then says its time. Handles take the arrow keys. Moments marked while recording stand as flags; clicking one jumps
  there.
-->
<script lang="ts">
  import { timecode } from './format'

  let {
    duration,
    time,
    start = $bindable(),
    end = $bindable(),
    sprite = null,
    frames = 0,
    peaks = null,
    markers = [],
    slow = false,
    slowFrom = $bindable(0),
    slowTo = $bindable(0),
    onseek,
  }: {
    duration: number
    time: number
    start: number
    end: number
    /** URL of the clip's frames in one row (the hover sprite), when it has one. */
    sprite?: string | null
    frames?: number
    /** The audio's loudness along the clip, 0 to 1. Undefined while it is
     *  being read (an empty lane); null when there is none to draw (no lane). */
    peaks?: Float32Array | null
    /** Marked moments, in seconds. */
    markers?: number[]
    /** Show and edit a slow-motion range inside the trim. */
    slow?: boolean
    slowFrom?: number
    slowTo?: number
    onseek: (t: number) => void
  } = $props()

  /** The shortest selection: the handles never meet or cross. */
  const MIN = 0.1
  /** The whole strip, and the audio lane's share of it; `.strip`, `.wave` below. */
  const HEIGHT = 60
  const WAVE = 16
  type Part = 'start' | 'end' | 'slowFrom' | 'slowTo' | 'seek'
  let strip: HTMLDivElement
  let width = $state(0)
  let drag = $state<Part | null>(null)

  const frac = (t: number) => (duration > 0 ? Math.min(1, Math.max(0, t / duration)) : 0)
  const pct = (t: number) => `${frac(t) * 100}%`
  /** Nothing trimmed yet: the kept range's edge stays faint. */
  const whole = $derived(start <= 0.001 && end >= duration - 0.001)

  // As many 16:9 cells as fit, each showing the frame nearest its moment.
  const cells = $derived.by(() => {
    if (!sprite || frames < 1) return []
    const film = HEIGHT - (peaks === null ? 0 : WAVE)
    const n = Math.max(1, Math.round(width / ((film * 16) / 9)))
    return Array.from({ length: n }, (_, i) => Math.round(((i + 0.5) / n) * (frames - 1)))
  })

  // The waveform as one closed shape, mirrored about the lane's middle, in
  // a box one unit per peak wide and two tall (stretched to fit by the SVG:
  // a resize rescales it, nothing is read or built again).
  const shape = $derived.by(() => {
    if (!peaks?.length) return ''
    // Quiet passages still show: loudness is drawn on a curve, not linearly.
    const h = Array.from(peaks, (p) => Math.max(0.02, Math.min(1, p) ** 0.6))
    const top = h.map((v, i) => `${i},${(1 - v).toFixed(3)} ${i + 1},${(1 - v).toFixed(3)}`)
    const bottom = h.map((v, i) => `${i + 1},${(1 + v).toFixed(3)} ${i},${(1 + v).toFixed(3)}`).reverse()
    return `M${top.join(' ')} ${bottom.join(' ')}Z`
  })

  /** Move one edge to `t`, kept inside the clip and clear of the other. */
  function set(what: Part, t: number) {
    if (what === 'start') onseek((start = Math.max(0, Math.min(t, end - MIN))))
    else if (what === 'end') onseek((end = Math.min(duration, Math.max(t, start + MIN))))
    else if (what === 'slowFrom') onseek((slowFrom = Math.min(Math.max(t, start), slowTo - MIN)))
    else if (what === 'slowTo') onseek((slowTo = Math.max(Math.min(t, end), slowFrom + MIN)))
    else onseek(Math.min(duration, Math.max(0, t)))
  }

  function at(e: PointerEvent) {
    const r = strip.getBoundingClientRect()
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duration
  }

  function down(e: PointerEvent, what: Part) {
    if (e.button !== 0) return
    e.stopPropagation()
    drag = what
    strip.setPointerCapture(e.pointerId)
    set(what, at(e))
  }

  function move(e: PointerEvent) {
    if (drag) set(drag, at(e))
  }

  function up() {
    drag = null
  }

  /** Arrow keys: a tenth of a second on a handle, a second on the strip;
   *  ten times that with Shift. Home and End go to the clip's ends. */
  function key(e: KeyboardEvent, what: Part, now: number) {
    const unit = (what === 'seek' ? 1 : 0.1) * (e.shiftKey ? 10 : 1)
    const to: Record<string, number> = { ArrowLeft: now - unit, ArrowDown: now - unit, ArrowRight: now + unit, ArrowUp: now + unit, Home: 0, End: duration }
    if (!(e.key in to) || e.ctrlKey || e.metaKey || e.altKey) return
    // The viewer's own arrows (the clip before and after) stay out of it.
    e.preventDefault()
    e.stopPropagation()
    set(what, Math.round(to[e.key] * 100) / 100)
  }
</script>

<!-- The handles stand just outside the strip's ends, over the workspace's margin. -->
<div class="scrubber">
  <div class={['strip', { resizing: drag && drag !== 'seek' }]} bind:this={strip} bind:clientWidth={width} onpointermove={move} onpointerup={up} onpointercancel={up} role="group" aria-label="Timeline">
    <div
      class="track"
      onpointerdown={(e) => down(e, 'seek')}
      onkeydown={(e) => key(e, 'seek', time)}
      role="slider"
      aria-label="Seek"
      aria-valuemin={0}
      aria-valuemax={duration}
      aria-valuenow={time}
      aria-valuetext={timecode(time)}
      tabindex="0"
    >
      <div class="film" aria-hidden="true">
        {#each cells as frame, i (i)}
          <div
            style:background-image="url({sprite})"
            style:background-size="{frames * 100}% 100%"
            style:background-position="{frames > 1 ? (frame / (frames - 1)) * 100 : 0}% 0"
          ></div>
        {/each}
      </div>
      {#if peaks !== null}
        <div class="wave" aria-hidden="true">
          {#if peaks}
            <svg viewBox="0 0 {peaks.length} 2" preserveAspectRatio="none"><path d={shape} /></svg>
          {/if}
        </div>
      {/if}
      <div class="cut" style:left="0" style:width={pct(start)}></div>
      <div class="cut" style:left={pct(end)} style:right="0"></div>
      <div class={['kept', { whole }]} style:left={pct(start)} style:width="{(frac(end) - frac(start)) * 100}%"></div>
    </div>

    {#if slow}
      <div class="slow-band" style:left={pct(slowFrom)} style:width="{(frac(slowTo) - frac(slowFrom)) * 100}%" title="Slow motion"></div>
      {#each [['slowFrom', slowFrom, 'Slow motion start'], ['slowTo', slowTo, 'Slow motion end']] as const as [what, t, label] (what)}
        <div
          class="slow-marker"
          style:left={pct(t)}
          onpointerdown={(e) => down(e, what)}
          onkeydown={(e) => key(e, what, t)}
          role="slider"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={duration}
          aria-valuenow={t}
          aria-valuetext={timecode(t)}
          tabindex="0"
        ></div>
      {/each}
    {/if}
    {#each markers as m, i (i)}
      <button
        class="tick"
        style:left={pct(m)}
        onpointerdown={(e) => {
          e.stopPropagation()
          onseek(m)
        }}
        aria-label="Marked moment at {m.toFixed(1)} s"
        title="Marked moment"
        tabindex="-1"
      ></button>
    {/each}
    {#each [['start', start, 'Trim start'], ['end', end, 'Trim end']] as const as [what, t, label] (what)}
      <div
        class={['handle', what, { held: drag === what }]}
        style:left={pct(t)}
        onpointerdown={(e) => down(e, what)}
        onkeydown={(e) => key(e, what, t)}
        role="slider"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={duration}
        aria-valuenow={t}
        aria-valuetext={timecode(t)}
        tabindex="0"
      >
        <span class="stamp" aria-hidden="true">{timecode(t)}</span>
      </div>
    {/each}
    <!-- Moves every frame while playing, so by transform: a full-width
         layer shifted by the fraction (compositor only, no layout). -->
    <div class="head" style:transform="translateX({pct(time)})"><div class="line"></div></div>
  </div>
</div>

<style>
  /* The strip spans the workspace edge to edge; the handles stand just
     outside its ends. */
  .scrubber {
    padding: 0;
  }

  /* A 12 px lane over the frames holds the marked moments, so no mark is
     ever drawn on top of a frame or mistaken for the playhead. */
  .strip {
    position: relative;
    height: 72px;
    touch-action: none;
  }

  /* A handle is held: the pointer is captured by the strip, so the resize
     cursor has to be said here to stay while dragging. */
  .strip.resizing,
  .strip.resizing .track {
    cursor: ew-resize;
  }

  .track {
    position: absolute;
    inset: 12px 0 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: var(--radius-ctl);
    background: var(--color-chrome);
    cursor: pointer;
  }

  .track:focus-visible {
    outline-offset: 3px;
  }

  .film {
    display: flex;
    flex: 1;
    min-height: 0;
    background: var(--color-raised);
  }

  .film > div {
    flex: 1;
    background-repeat: no-repeat;
    /* A seam between frames, as on film: just enough to count them. */
    box-shadow: inset -1px 0 0 rgb(0 0 0 / 28%);
  }

  /* The audio lane: under the frames in every sense, still readable. */
  .wave {
    position: relative;
    flex: none;
    height: 16px;
    background: var(--color-chrome);
  }

  .wave svg {
    position: absolute;
    inset: 2px 0;
    width: 100%;
    height: calc(100% - 4px);
    fill: var(--color-muted);
    fill-opacity: 0.7;
  }

  /* Outside the trim: held back, not blacked out. */
  .cut {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgb(21 23 25 / 66%);
    pointer-events: none;
  }

  /* The kept range: one hairline of the accent along its top and bottom,
     joining the handles; faint while nothing is cut. */
  .kept {
    position: absolute;
    top: 0;
    bottom: 0;
    box-shadow:
      inset 0 1px 0 var(--edge),
      inset 0 -1px 0 var(--edge);
    pointer-events: none;
    --edge: var(--color-accent);
  }

  .kept.whole {
    --edge: rgb(255 152 94 / 40%);
  }

  /* In and out: a slim bar of the accent outside each end of the kept
     range, a little quieter than the accent itself. The bar is 4 px; what
     can be grabbed is 24. Touched, it widens from its inner edge (by
     transform: nothing moves) and takes the full accent. */
  .handle {
    position: absolute;
    top: 12px;
    bottom: 0;
    width: 4px;
    cursor: ew-resize;
  }

  .handle::before {
    content: '';
    position: absolute;
    inset: 0 -10px;
  }

  /* The bar itself, apart from the handle so its time isn't scaled with it. */
  .handle::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: color-mix(in oklab, var(--color-accent) 82%, var(--color-canvas));
    transition:
      background-color var(--duration-fast) var(--ease-out),
      scale var(--duration-fast) var(--ease-out);
  }

  .handle:hover::after,
  .handle.held::after,
  .handle:focus-visible::after {
    background: var(--color-accent-hover);
    scale: 1.5 1;
  }

  .handle:focus-visible {
    outline: 2px solid var(--color-fg);
    outline-offset: 1px;
  }

  /* Each widens outward, away from the footage it keeps. */
  .handle.start {
    translate: -100% 0;
    border-radius: 3px 0 0 3px;
  }

  .handle.start::after {
    transform-origin: right;
  }

  .handle.end {
    border-radius: 0 3px 3px 0;
  }

  .handle.end::after {
    transform-origin: left;
  }

  /* Where the handle is, while it is pointed at, held or has the keyboard:
     inside the kept range, beside the handle. Nothing else moves. */
  .stamp {
    position: absolute;
    top: 8px;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--color-raised);
    box-shadow: 0 0 0 1px var(--color-line-strong);
    color: var(--color-fg);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
    line-height: 20px;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .handle.start .stamp {
    left: calc(100% + 8px);
  }

  .handle.end .stamp {
    right: calc(100% + 8px);
  }

  .handle:hover .stamp,
  .handle.held .stamp,
  .handle:focus-visible .stamp {
    opacity: 1;
  }

  .head {
    position: absolute;
    inset: 0;
    pointer-events: none;
    will-change: transform;
  }

  /* The playhead: a white line a little taller than the strip, with a cap. */
  .line {
    position: absolute;
    left: 0;
    top: 7px;
    bottom: -3px;
    width: 2px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 45%);
    translate: -50% 0;
  }

  .line::before {
    content: '';
    position: absolute;
    top: 0;
    left: -3px;
    width: 8px;
    height: 5px;
    border-radius: 1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 45%);
  }

  /* Slow-motion range: stripes over the frames, and white edges of its
     own so it reads apart from the trim handles. */
  .slow-band {
    position: absolute;
    top: 14px;
    bottom: 2px;
    background: repeating-linear-gradient(135deg, rgb(255 255 255 / 30%) 0 4px, rgb(255 255 255 / 6%) 4px 8px);
    pointer-events: none;
  }

  .slow-marker {
    position: absolute;
    top: 14px;
    bottom: 2px;
    width: 14px;
    translate: -50% 0;
    cursor: ew-resize;
  }

  .slow-marker::after {
    content: '';
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 3px;
    border-radius: 1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 35%);
    translate: -50% 0;
  }

  /* Marked moments: a diamond in the lane over the frames, a faint thread
     down through them. A different shape, a different place and a quieter
     tone than the playhead (a white line through everything) and the trim
     handles (orange bars at the ends). 16 px to aim at. */
  .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 16px;
    padding: 0;
    border: 0;
    background: linear-gradient(rgb(255 255 255 / 30%), rgb(255 255 255 / 30%)) center 12px / 1px calc(100% - 12px) no-repeat;
    translate: -50% 0;
    cursor: pointer;
  }

  .tick::after {
    content: '';
    position: absolute;
    left: 50%;
    top: 2px;
    width: 8px;
    height: 8px;
    border-radius: 1px;
    background: var(--color-fg-2);
    box-shadow: 0 0 0 2px var(--color-canvas);
    translate: -50% 0;
    rotate: 45deg;
    transition: background-color var(--duration-fast) var(--ease-out);
  }

  .tick:hover::after {
    background: var(--color-fg);
  }
</style>
