<!--
  The clip grid: sections, each a heading and rows of cards, as many to a
  row as fit the width at `minWidth` each, so the grid always fills the
  page. Above `threshold` items only the rows near the viewport are in the
  DOM. Columns are worked out here rather than by CSS, so the rows are
  known before anything is drawn; every card row is the same height,
  measured from one.
-->
<script lang="ts" generics="T, S extends { key: string; label?: string; items: T[] }">
  import type { Snippet } from 'svelte'

  let {
    sections,
    key,
    minWidth,
    threshold = 120,
    overscan = 2,
    heading,
    children,
  }: {
    /** A section without a label gets no heading. */
    sections: S[]
    key: (item: T) => string
    /** Narrowest a card may get before the row loses a column. */
    minWidth: number
    /** Item count above which windowing switches on. */
    threshold?: number
    /** Extra card rows' worth of height kept above and below the viewport. */
    overscan?: number
    heading: Snippet<[S]>
    children: Snippet<[T]>
  } = $props()

  /** Between cards, across and down; `gap-x-5 pb-5` below. */
  const GAP = 20
  /** A heading row. Over the last row's gap it leaves 22 px under a card's
   *  second line (40 under its title when that line is empty), and 12 px
   *  from the heading to its cards. */
  const HEAD = 36

  let root: HTMLDivElement | undefined = $state()
  let width = $state(1000)
  let rowHeight = $state(200)
  let viewTop = $state(0)
  let viewHeight = $state(800)
  /** Offset of the grid's top from the scroll container's content top. */
  let rootTop = $state(0)

  const columns = $derived(Math.max(1, Math.floor((width + GAP) / (minWidth + GAP))))

  type Row = { id: string; top: number } & ({ head: S } | { items: T[] })
  const layout = $derived.by(() => {
    const rows: Row[] = []
    let top = 0
    for (const s of sections) {
      if (s.label) {
        rows.push({ id: s.key, top, head: s })
        top += HEAD
      }
      for (let i = 0; i < s.items.length; i += columns) {
        rows.push({ id: `${s.key}/${i}`, top, items: s.items.slice(i, i + columns) })
        top += rowHeight
      }
    }
    return { rows, height: top }
  })

  const windowed = $derived(sections.reduce((n, s) => n + s.items.length, 0) > threshold)
  const shown = $derived.by(() => {
    const { rows, height } = layout
    if (!windowed) return { rows, before: 0, after: 0 }
    const from = viewTop - rootTop - overscan * rowHeight
    const to = viewTop - rootTop + viewHeight + overscan * rowHeight
    let a = 0
    while (a < rows.length - 1 && rows[a + 1].top <= from) a++
    let b = a
    while (b < rows.length && rows[b].top < to) b++
    return { rows: rows.slice(a, b), before: rows[a]?.top ?? 0, after: height - (rows[b]?.top ?? height) }
  })

  function scrollParent(el: HTMLElement): HTMLElement {
    let p = el.parentElement
    while (p) {
      const o = getComputedStyle(p).overflowY
      if (o === 'auto' || o === 'scroll') return p
      p = p.parentElement
    }
    return document.documentElement
  }

  function measure(scroller: HTMLElement) {
    if (!root) return
    viewTop = scroller.scrollTop
    viewHeight = scroller.clientHeight
    rootTop = root.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop
    width = root.clientWidth
    // One row's own height, never the grid's, which includes the spacers
    // and would feed back on itself.
    const h = root.querySelector<HTMLElement>('.clip-grid')?.offsetHeight ?? 0
    if (h > 40 && Math.abs(h - rowHeight) > 1) rowHeight = h
  }

  $effect(() => {
    if (!root) return
    const scroller = scrollParent(root)
    let raf = 0
    const onScroll = () => {
      if (raf) return
      raf = requestAnimationFrame(() => {
        raf = 0
        measure(scroller)
      })
    }
    scroller.addEventListener('scroll', onScroll, { passive: true })
    const ro = new ResizeObserver(() => measure(scroller))
    ro.observe(root)
    ro.observe(scroller)
    measure(scroller)
    return () => {
      scroller.removeEventListener('scroll', onScroll)
      ro.disconnect()
      cancelAnimationFrame(raf)
    }
  })
</script>

<div bind:this={root} style:padding-top="{shown.before}px" style:padding-bottom="{shown.after}px">
  {#each shown.rows as row (row.id)}
    {#if 'head' in row}
      <div class="flex items-end gap-3 pb-3" style:height="{HEAD}px">{@render heading(row.head)}</div>
    {:else}
      <!-- The bottom padding is the gap to the next row, so a row's height is its pitch. -->
      <div class="clip-grid grid gap-x-5 pb-5" style:grid-template-columns="repeat({columns}, minmax(0, 1fr))">
        {#each row.items as item (key(item))}
          {@render children(item)}
        {/each}
      </div>
    {/if}
  {/each}
</div>
