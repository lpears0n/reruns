<!--
  A fader: a track and a thumb to hold. Press anywhere
  on it and drag; arrow keys step; a double-click goes back to `detent`
  (marked by a tick), which the thumb also catches on the way past.
-->
<script lang="ts">
  let {
    value = $bindable(),
    min = 0,
    max = 100,
    step = 1,
    detent,
    label,
    text,
    disabled = false,
    oninput,
    onchange,
    class: cls = '',
  }: {
    value: number
    min?: number
    max?: number
    step?: number
    /** The value a double-click returns to. */
    detent?: number
    label: string
    /** How a screen reader says the value ("120%"). */
    text?: string
    disabled?: boolean
    /** Every change while dragging. */
    oninput?: (v: number) => void
    /** Once, when the thumb is let go (or a key steps it). */
    onchange?: (v: number) => void
    class?: string
  } = $props()

  let dragging = $state(false)
  const frac = (v: number) => Math.min(1, Math.max(0, (v - min) / (max - min)))

  function set(v: number) {
    v = Math.min(max, Math.max(min, Math.round(v / step) * step))
    // Float steps (0.05) leave 0.30000000000000004 behind.
    v = Number(v.toFixed(4))
    if (v === value) return
    value = v
    oninput?.(v)
  }

  /** Half the thumb: its centre stops this far inside each end. */
  const INSET = 7
  function at(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const v = min + ((e.clientX - r.left - INSET) / (r.width - INSET * 2)) * (max - min)
    return detent !== undefined && Math.abs(v - detent) < (max - min) * 0.03 ? detent : v
  }

  function down(e: PointerEvent) {
    if (disabled || e.button !== 0) return
    dragging = true
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    set(at(e))
  }

  function up() {
    if (!dragging) return
    dragging = false
    onchange?.(value)
  }

  function key(e: KeyboardEvent) {
    if (disabled) return
    const by: Record<string, number> = { ArrowLeft: -step, ArrowDown: -step, ArrowRight: step, ArrowUp: step, PageDown: -step * 5, PageUp: step * 5 }
    const before = value
    if (e.key in by) set(value + by[e.key])
    else if (e.key === 'Home') set(min)
    else if (e.key === 'End') set(max)
    else return
    // The page's own shortcuts (arrows step clips in the studio) stay out of it.
    e.preventDefault()
    e.stopPropagation()
    if (value !== before) onchange?.(value)
  }

  function reset() {
    if (disabled || detent === undefined || value === detent) return
    set(detent)
    onchange?.(value)
  }
</script>

<div
  class={['fader', cls, { dragging, disabled }]}
  role="slider"
  tabindex={disabled ? -1 : 0}
  aria-label={label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={value}
  aria-valuetext={text}
  aria-disabled={disabled}
  onpointerdown={down}
  onpointermove={(e) => dragging && set(at(e))}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
  ondblclick={reset}
>
  <div class="track"><div class="fill" style:transform="scaleX({frac(value)})"></div></div>
  {#if detent !== undefined}
    <span class="detent" style:left="calc({INSET}px + (100% - {INSET * 2}px) * {frac(detent)})"></span>
  {/if}
  <!-- Moved by transform: a layer as wide as the thumb's travel. -->
  <div class="travel" style:transform="translateX({frac(value) * 100}%)"><span class="thumb"></span></div>
</div>

<style>
  .fader {
    position: relative;
    flex: none;
    width: 96px;
    height: 20px;
    cursor: pointer;
    touch-action: none;
  }

  .fader.disabled {
    opacity: 0.4;
    cursor: default;
  }

  .track {
    position: absolute;
    inset: 8px 0;
    overflow: hidden;
    border-radius: 2px;
    background: var(--color-line-strong);
  }

  .fill {
    height: 100%;
    background: var(--fader-fill, var(--color-fg-2));
    transform-origin: left;
  }

  .detent {
    position: absolute;
    top: 2px;
    width: 1px;
    height: 4px;
    background: var(--color-faint);
  }

  .travel {
    position: absolute;
    inset: 0 7px;
    pointer-events: none;
  }

  .thumb {
    position: absolute;
    top: 3px;
    left: -7px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-fg);
    box-shadow: 0 1px 3px rgb(0 0 0 / 45%);
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .fader:focus-visible {
    outline: none;
  }

  .fader:focus-visible .thumb {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .dragging .thumb {
    transform: scale(1.12);
  }
</style>
