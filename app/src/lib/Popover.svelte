<!--
  A menu or a small panel of controls that leaves the page: opened from its
  trigger (or at a point, as a context menu), placed inside the window,
  closed by Esc, a click elsewhere or choosing a row. Opaque, one shadow.
  Keys pressed inside it stay inside it, so Esc closes this and not the
  view behind, and a page's shortcuts don't fire while it is open.

  Don't put one inside an element with `container-type` or a transform:
  both would become what `position: fixed` is measured from.
-->
<script lang="ts">
  import { tick, type Snippet } from 'svelte'

  let {
    label,
    menu = true,
    side = 'bottom',
    align = 'end',
    class: cls = '',
    trigger,
    children,
  }: {
    label: string
    /** Rows moved through with the arrow keys; false for a panel of controls. */
    menu?: boolean
    /** Where it opens from its trigger; flips when there is no room. */
    side?: 'top' | 'bottom'
    align?: 'start' | 'end'
    class?: string
    /** Spread the argument onto the button that opens it. */
    trigger?: Snippet<[{ onclick: (e: MouseEvent) => void; 'aria-haspopup': 'menu' | 'dialog'; 'aria-expanded': boolean }]>
    /** Its argument closes the popover. */
    children: Snippet<[() => void]>
  } = $props()

  const GAP = 6
  const EDGE = 8

  let open = $state(false)
  let panel: HTMLDivElement | undefined = $state()
  /** Where it was opened from: a control's box, or a point. */
  let from = { left: 0, right: 0, top: 0, bottom: 0 }
  let point = false
  let place = $state<{ left: number; top: number } | null>(null)
  let opener: HTMLElement | null = null

  const rows = () => [...(panel?.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)') ?? [])]

  async function show() {
    place = null
    open = true
    await tick()
    if (!panel) return
    const { width, height } = panel.getBoundingClientRect()
    const below = from.bottom + GAP
    const above = from.top - GAP - height
    const wantsTop = !point && side === 'top'
    let top = wantsTop ? (above >= EDGE ? above : below) : below + height <= innerHeight - EDGE ? below : above
    let left = point || align === 'start' ? from.left : from.right - width
    left = Math.max(EDGE, Math.min(left, innerWidth - width - EDGE))
    top = Math.max(EDGE, Math.min(top, innerHeight - height - EDGE))
    place = { left, top }
    // Hidden until placed, and nothing hidden takes focus: wait for it to show.
    await tick()
    ;(menu ? rows()[0] : (panel.querySelector<HTMLElement>('[autofocus], [tabindex="0"], button, input, select') ?? panel))?.focus()
  }

  /** Open under (or over) an element, as if it were the trigger. */
  export function openFrom(el: HTMLElement) {
    opener = el
    point = false
    from = el.getBoundingClientRect()
    show()
  }

  /** Open at a point: a context menu. Focus returns to what had it. */
  export function openAt(x: number, y: number) {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    point = true
    from = { left: x, right: x, top: y, bottom: y }
    show()
  }

  export function close(refocus = true) {
    if (!open) return
    open = false
    // Once the panel is gone, and only if nothing else took the focus: a row
    // may have opened a field or another panel that wants it.
    if (refocus) setTimeout(() => document.activeElement === document.body && opener?.isConnected && opener.focus())
  }

  function toggle(e: MouseEvent) {
    if (open) close()
    else openFrom(e.currentTarget as HTMLElement)
  }

  function onkeydown(e: KeyboardEvent) {
    // Nothing typed in here is a shortcut of the page behind.
    e.stopPropagation()
    if (e.key === 'Escape') {
      e.preventDefault()
      close()
    } else if (e.key === 'Tab' && menu) {
      close()
    } else if (menu && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      e.preventDefault()
      const all = rows()
      const i = all.indexOf(document.activeElement as HTMLElement)
      const next = e.key === 'Home' ? 0 : e.key === 'End' ? all.length - 1 : (i + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length
      all[next]?.focus()
    }
  }

  /** Choosing a row is the end of a menu. */
  function onclick(e: MouseEvent) {
    if (menu && (e.target as HTMLElement).closest('[role^="menuitem"]')) close()
  }

  function outside(e: Event) {
    if (!open || !(e.target instanceof Node)) return
    if (panel?.contains(e.target) || (!point && opener?.contains(e.target))) return
    // The click goes on to whatever was clicked; focus goes with it.
    close(false)
  }
</script>

<svelte:window onpointerdowncapture={outside} onscrollcapture={outside} onresize={() => close(false)} onblur={() => close(false)} />

{@render trigger?.({ onclick: toggle, 'aria-haspopup': menu ? 'menu' : 'dialog', 'aria-expanded': open })}

{#if open}
  <div
    bind:this={panel}
    class={['pop pop-surface', menu && 'flex min-w-52 flex-col p-1.5', cls]}
    style:left="{place?.left ?? 0}px"
    style:top="{place?.top ?? 0}px"
    style:visibility={place ? 'visible' : 'hidden'}
    role={menu ? 'menu' : 'dialog'}
    aria-label={label}
    tabindex="-1"
    {onkeydown}
    {onclick}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {@render children(close)}
  </div>
{/if}

<style>
  .pop {
    position: fixed;
    z-index: 70;
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    outline: none;
    transition: opacity var(--duration-fast) ease-out;
    @starting-style {
      opacity: 0;
    }
  }
</style>
