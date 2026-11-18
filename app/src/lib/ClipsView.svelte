<!--
  The library: a compact identity and search, then the clips the navigation
  chose. View options stay in a menu; clips are in dated sections,
  newest first, as many to a row as fit. Renders the header and the scroller
  into the page; it stays mounted behind an open clip, so coming back finds
  the same filters and the same scroll position.
-->
<script lang="ts">
  import { tick } from 'svelte'
  import { Check, Ellipsis, Search, SlidersHorizontal, Trash2, X } from '@lucide/svelte'
  import { type Clip, type GameEntry, type Screenshot, type Settings, type Status } from './api'
  import { bytes } from './format'
  import { bySession } from './sessions'
  import ClipCard from './ClipCard.svelte'
  import FirstRun from './FirstRun.svelte'
  import { clock, date, dayKey, dayLabel, gameName, period, stamp, when } from './format'
  import Popover from './Popover.svelte'
  import { prefs, setPrefs, type Sort, type ThumbSize } from './prefs.svelte'
  import type { Scope } from './types'
  import ScreenshotCard from './ScreenshotCard.svelte'
  import type { Category } from './SettingsView.svelte'
  import SkeletonGrid from './SkeletonGrid.svelte'
  import StateMessage from './StateMessage.svelte'
  import type { Selection } from './types'
  import VirtualGrid from './VirtualGrid.svelte'

  let {
    clips,
    screenshots,
    scope,
    game,
    games,
    behind,
    loading,
    hotkey,
    status,
    settings,
    copyingPath = null,
    copiedPath = null,
    copyProgress = null,
    copyBlocked = false,
    onopen,
    onshot,
    ondelete,
    ondeletemany,
    oncopy,
    onreveal,
    onfavorite,
    onnotgame,
    onpick,
    onchoosescreen,
    onsettings,
  }: {
    clips: Clip[]
    screenshots: Screenshot[]
    scope: Scope
    /** Optional game filter, shared by All clips and Favorites. */
    game: GameEntry | null
    /** Every game available in the filter. */
    games: GameEntry[]
    /** A clip or screenshot is open in the library's place: the keys are its. */
    behind: boolean
    loading: boolean
    hotkey: string
    status: Status | null
    settings: Settings | null
    copyingPath?: string | null
    copiedPath?: string | null
    /** How far a large clip's copy has been prepared, 0 to 1; null when not preparing. */
    copyProgress?: number | null
    copyBlocked?: boolean
    /** `queue`: the clips as listed, for stepping to the next one. */
    onopen: (c: Clip, queue: string[]) => void
    onshot: (s: Screenshot) => void
    ondelete: (c: Clip | Screenshot) => void
    ondeletemany: (cs: Clip[]) => void
    oncopy: (c: Clip | Screenshot) => void
    /** Show the file in the system's file manager. */
    onreveal: (c: Clip | Screenshot) => void
    onfavorite: (c: Clip) => void
    /** Remove the game from the list and never detect it again. */
    onnotgame: (g: GameEntry) => void
    /** Open a video from anywhere on disk. */
    onpick: () => void
    onchoosescreen: () => void
    onsettings: (c: Category) => void
  } = $props()

  const SORTS: { value: Sort; label: string }[] = [
    { value: 'newest', label: 'Newest first' },
    { value: 'oldest', label: 'Oldest first' },
    { value: 'longest', label: 'Longest first' },
    { value: 'largest', label: 'Largest first' },
  ]
  /** Narrowest a card gets before the grid drops a column: four to a row
   *  at 1280 to 1440 with the navigation open, at the middle size. */
  const SIZES: { value: ThumbSize; label: string; min: number; max: number }[] = [
    { value: 'small', label: 'Small', min: 170, max: 240 },
    { value: 'medium', label: 'Medium', min: 220, max: 340 },
    { value: 'large', label: 'Large', min: 360, max: 520 },
  ]
  const minWidth = $derived(SIZES.find((s) => s.value === prefs.thumbSize)?.min ?? 300)
  const maxWidth = $derived(SIZES.find((s) => s.value === prefs.thumbSize)?.max ?? 380)

  let query = $state('')
  let search: HTMLInputElement | undefined = $state()
  let label = $state('')
  /** Order and grouping are kept between launches (prefs). */
  const sort = $derived(prefs.sort)

  // Selection mode for bulk actions.
  let selecting = $state(false)
  let selected = $state<Set<string>>(new Set())
  /** The clip last picked: where a Shift-click's range starts. */
  let anchor: string | null = null

  const scoped = $derived(
    clips.filter((c) => (!game || c.game === game.name) && (scope !== 'starred' || c.favorite)),
  )
  const labels = $derived(
    prefs.tagging ? [...new Set(scoped.flatMap((c) => c.labels))].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' })) : [],
  )
  /** The tag being filtered by, while a clip in the list still has it. */
  const activeLabel = $derived(labels.includes(label) ? label : '')

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    let list = activeLabel ? scoped.filter((c) => c.labels.includes(activeLabel)) : scoped
    if (q) {
      list = list.filter((c) =>
        `${c.title ?? ''} ${c.game} ${gameName(c.game)} ${c.name} ${c.labels.join(' ')} ${date(c.created_ms)}`.toLowerCase().includes(q),
      )
    }
    const by: Record<Sort, (a: Clip, b: Clip) => number> = {
      newest: (a, b) => b.created_ms - a.created_ms,
      oldest: (a, b) => a.created_ms - b.created_ms,
      longest: (a, b) => (b.duration ?? 0) - (a.duration ?? 0),
      largest: (a, b) => b.size_bytes - a.size_bytes,
    }
    return [...list].sort(by[sort])
  })
  // Only what is still listed counts as picked: a clip deleted or filtered
  // out drops from the count without anything having to tidy the set.
  const selectedClips = $derived(shown.filter((c) => selected.has(c.path)))

  // Screenshots sit among the clips by time (their game's, when one is
  // chosen; never in Favorites), unless a filter or a
  // sort by something else has narrowed the list to clips.
  const byDay = $derived(sort === 'newest' || sort === 'oldest')
  const shots = $derived(scope !== 'starred' ? screenshots.filter((s) => !game || s.game === game.name) : [])
  /** Recent periods and months by default: a few clips a day would leave
   *  day sections mostly empty rows. By day, or a game's play sessions, on request. */
  const groupBy = $derived(prefs.groupBy === 'session' && !game ? 'recent' : prefs.groupBy)
  /** A card's time, as much as its section heading leaves unsaid. */
  const timeOf = $derived(!byDay ? when : groupBy === 'recent' ? stamp : clock)
  type Item = { at: number } & ({ clip: Clip } | { shot: Screenshot })
  type Section = { key: string; label: string; range?: string; played: number; live?: boolean; items: Item[] }
  const path = (it: Item) => ('clip' in it ? it.clip.path : it.shot.path)
  /** A period (or day, or play session) with what was saved in it; one unnamed section when not sorted by time. */
  const sections = $derived.by(() => {
    const items: Item[] = shown.map((clip) => ({ at: clip.created_ms, clip }))
    if (!byDay) return items.length ? [{ key: 'all', label: '', played: 0, items }] : []
    if (!activeLabel && !query.trim()) items.push(...shots.map((shot) => ({ at: shot.modified_ms, shot })))
    items.sort((a, b) => (sort === 'oldest' ? a.at - b.at : b.at - a.at))
    if (groupBy === 'session' && game) {
      const groups = bySession(items, (it) => it.at, game)
      if (sort === 'oldest') groups.reverse()
      return groups as Section[]
    }
    const days: Section[] = []
    for (const it of items) {
      const { key, label } = groupBy === 'day' ? { key: dayKey(it.at), label: dayLabel(it.at) } : period(it.at)
      if (days.at(-1)?.key !== key) {
        days.push({ key, label, played: 0, items: [] })
      }
      days.at(-1)!.items.push(it)
    }
    return days
  })

  /** What the header says is listed, including screenshots when visible. */
  const listed = $derived(game ? gameName(game.name) : scope === 'starred' ? 'Favorites' : 'All clips')
  const visibleShots = $derived(byDay && !activeLabel && !query.trim() ? shots.length : 0)
  const count = $derived(`${shown.length} clip${shown.length === 1 ? '' : 's'}`)

  /** Pick or unpick a clip; `range`: everything from the last pick to it. */
  function toggle(c: Clip, range = false) {
    const next = new Set(selected)
    const from = range && anchor ? shown.findIndex((x) => x.path === anchor) : -1
    const to = shown.indexOf(c)
    if (from >= 0 && to >= 0) {
      for (const x of shown.slice(Math.min(from, to), Math.max(from, to) + 1)) next.add(x.path)
    } else if (next.has(c.path)) next.delete(c.path)
    else next.add(c.path)
    anchor = c.path
    selected = next
    selecting = true
  }

  function selectAll() {
    selected = selectedClips.length === shown.length ? new Set() : new Set(shown.map((c) => c.path))
  }

  /** Also the app's, when the navigation lists something else. */
  export function stopSelecting() {
    selecting = false
    selected = new Set()
    anchor = null
  }

  function deleteSelected() {
    const cs = selectedClips
    stopSelecting()
    ondeletemany(cs)
  }

  /** The current selection, also available to the containing workspace. */
  export function selection(): Selection | null {
    if (!selecting) return null
    return {
      count: selectedClips.length,
      bytes: selectedClips.reduce((n, c) => n + c.size_bytes, 0),
      all: shown.length > 0 && selectedClips.length === shown.length,
      onall: selectAll,
      ondelete: deleteSelected,
      oncancel: stopSelecting,
    }
  }

  // One context menu for whichever card was right-clicked (or asked for by
  // the Menu key): a popover per card would be hundreds of them.
  let context: ReturnType<typeof Popover> | undefined = $state()
  let target = $state<Item | null>(null)
  function menuFor(it: Item, x: number, y: number) {
    target = it
    context?.openAt(x, y)
  }

  /** Arrow keys move between cards, by where they are on screen. */
  function moveFocus(from: HTMLElement, key: string) {
    const cards = [...document.querySelectorAll<HTMLElement>('main .clip-open')]
    const i = cards.indexOf(from)
    if (key === 'ArrowLeft' || key === 'ArrowRight') return cards[i + (key === 'ArrowRight' ? 1 : -1)]?.focus()
    const here = from.getBoundingClientRect()
    const down = key === 'ArrowDown'
    let best: HTMLElement | undefined
    let bestScore = Infinity
    for (const c of cards) {
      const r = c.getBoundingClientRect()
      const dy = down ? r.top - here.top : here.top - r.top
      if (dy < 1) continue
      // The nearest row first, then the card most nearly in line.
      const score = dy * 4 + Math.abs(r.left - here.left)
      if (score < bestScore) [best, bestScore] = [c, score]
    }
    best?.focus()
  }

  /** The listed item a card's open button belongs to. */
  function itemOf(el: HTMLElement): Item | undefined {
    const at = el.closest<HTMLElement>('[data-clip-path]')?.dataset.clipPath
    return at ? sections.flatMap((s) => s.items).find((it) => path(it) === at) : undefined
  }

  const itemPath = (it: Item) => ('clip' in it ? it.clip : it.shot)

  /** Delete one card; the keyboard moves on to its neighbour rather than
   *  falling back to the top of the window. */
  async function removeItem(it: Item) {
    const order = sections.flatMap((s) => s.items)
    // By path: the context menu's target is a reactive copy of the item.
    const i = order.findIndex((x) => path(x) === path(it))
    const next = order[i + 1] ?? order[i - 1]
    const keyboard = !!document.activeElement?.closest('[data-clip-path], [role="menu"]')
    ondelete(itemPath(it))
    if (!next || !keyboard) return
    await tick()
    document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(path(next))}"] .clip-open`)?.focus()
  }

  // A different collection starts at its newest clips, not at the offset
  // the last one was scrolled to. Returning from a clip or Settings keeps it.
  let scroller: HTMLDivElement | undefined = $state()
  const collection = $derived(`${scope}/${game?.name ?? ''}`)
  $effect(() => {
    void collection
    if (scroller) scroller.scrollTop = 0
  })

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement
    if (behind) return
    if (search && (((e.ctrlKey || e.metaKey) && e.key === 'f') || (e.key === '/' && !typing))) {
      e.preventDefault()
      search.focus()
      search.select()
      return
    }
    if (typing) return
    // A focused card's own keys: copy, delete, favorite (as in its menu).
    const card = !selecting && e.target instanceof HTMLElement && e.target.classList.contains('clip-open') ? itemOf(e.target) : undefined
    if (card && !e.altKey && !e.shiftKey) {
      const ctrl = e.ctrlKey || e.metaKey
      if (ctrl && e.code === 'KeyC') {
        e.preventDefault()
        return oncopy(itemPath(card))
      }
      if (!ctrl && e.key === 'Delete') {
        e.preventDefault()
        return void removeItem(card)
      }
      if (!ctrl && (e.key === 'f' || e.key === 'F') && 'clip' in card) {
        e.preventDefault()
        return onfavorite(card.clip)
      }
    }
    if (e.key.startsWith('Arrow') && e.target instanceof HTMLElement && e.target.classList.contains('clip-open')) {
      e.preventDefault()
      moveFocus(e.target, e.key)
    } else if (e.key === 'Escape') {
      if (selecting) stopSelecting()
      else query = ''
    } else if (selecting && (e.ctrlKey || e.metaKey) && e.key === 'a') {
      e.preventDefault()
      selectAll()
    } else if (selecting && e.key === 'Delete' && selectedClips.length) {
      deleteSelected()
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet radio(checked: boolean, text: string, onclick: () => void)}
  <button class="menu-item" role="menuitemradio" aria-checked={checked} {onclick}>
    <span class="flex w-4 flex-none justify-center text-accent">{#if checked}<Check size={15} strokeWidth={2.5} />{/if}</span>
    {text}
  </button>
{/snippet}

<header class="library-toolbar">
  <div class="library-heading">
    <h1 title={listed}>{listed}</h1>
    {#if scoped.length || shots.length}<p class="library-count">{scope === 'starred' && game ? 'Favorites · ' : ''}{count}{visibleShots ? `, ${visibleShots} screenshot${visibleShots === 1 ? '' : 's'}` : ''}</p>{/if}
  </div>

  {#if scoped.length || shots.length}
    <label class="search-field field flex items-center gap-2" title="Search (Ctrl+F)">
      <Search size={16} class="flex-none text-muted" />
      <input
        bind:this={search}
        bind:value={query}
        type="text"
        placeholder="Search clips"
        class="min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted"
        aria-label="Search clips"
        onkeydown={(e) => {
          if (e.key !== 'Escape') return
          query = ''
          search?.blur()
        }}
      />
      {#if query}
        <button class="-mr-1 flex size-6 flex-none items-center justify-center rounded-chip text-muted hover:text-fg" onclick={() => (query = '')} aria-label="Clear search">
          <X size={15} />
        </button>
      {/if}
    </label>
    {#if activeLabel}
      <button class="active-tag btn-text gap-1 text-fg-2" onclick={() => (label = '')} aria-label="Stop filtering by tag {activeLabel}" title="Stop filtering by tag {activeLabel}">
        <span class="truncate">#{activeLabel}</span>
        <X size={13} class="flex-none" />
      </button>
    {/if}
  {/if}

  {#if scoped.length || shots.length}
  <Popover label="View options">
    {#snippet trigger(props)}
      <button {...props} class="view-button press" aria-label="View options" title="View options">
        <SlidersHorizontal size={16} /><span>View</span>
      </button>
    {/snippet}
    <div role="group" aria-label="Sort clips">
      <div class="menu-label">Sort</div>
      {#each SORTS as s (s.value)}
        {@render radio(sort === s.value, s.label, () => setPrefs({ sort: s.value }))}
      {/each}
    </div>
    <div class="menu-rule"></div>
    <div role="group" aria-label="Thumbnail size">
      <div class="menu-label">Thumbnail size</div>
      {#each SIZES as s (s.value)}
        {@render radio(prefs.thumbSize === s.value, `${s.label} thumbnails`, () => setPrefs({ thumbSize: s.value }))}
      {/each}
    </div>
    <div class="menu-rule"></div>
    <div role="group" aria-label="Group clips">
      <div class="menu-label">Group</div>
      {@render radio(groupBy === 'recent', 'Recent, then by month', () => setPrefs({ groupBy: 'recent' }))}
      {@render radio(groupBy === 'day', 'By day', () => setPrefs({ groupBy: 'day' }))}
      {#if game}{@render radio(groupBy === 'session', 'By play session', () => setPrefs({ groupBy: 'session' }))}{/if}
    </div>
    {#if labels.length}
      <div class="menu-rule"></div>
      <div role="group" aria-label="Filter by tag">
        <div class="menu-label">Tag</div>
        {@render radio(!activeLabel, 'Any tag', () => (label = ''))}
        {#each labels as l (l)}
          {@render radio(activeLabel === l, `#${l}`, () => (label = l))}
        {/each}
      </div>
    {/if}
  </Popover>
  {/if}

  <Popover label="Library actions">
    {#snippet trigger(props)}
      <button {...props} class="btn-icon" aria-label="Library actions" title="Library actions"><Ellipsis size={20} /></button>
    {/snippet}
    <button class="menu-item" role="menuitem" disabled={!shown.length} onclick={() => (selecting ? stopSelecting() : (selecting = true))}>
      {selecting ? 'Stop selecting' : 'Select clips'}<span class="menu-key">Ctrl+click</span>
    </button>
    <button class="menu-item" role="menuitem" onclick={onpick}>Open a video file…</button>
    {#if game}
      <div class="menu-rule"></div>
      <button class="menu-item" role="menuitem" onclick={() => onnotgame(game)}>{gameName(game.name)} is not a game</button>
    {/if}
  </Popover>
</header>

{#if selecting}
  <div class="selection-toolbar" role="toolbar" aria-label="Selection actions">
    <div class="selection-summary">
      <span class="font-medium tabular-nums">{selectedClips.length} selected</span>
      {#if selectedClips.length}<span class="text-muted tabular-nums">{bytes(selectedClips.reduce((sum, clip) => sum + clip.size_bytes, 0))}</span>{/if}
    </div>
    <button class="btn-sm" onclick={selectAll} disabled={!shown.length}>{shown.length > 0 && selectedClips.length === shown.length ? 'Select none' : 'Select all'}</button>
    <button class="btn-danger" onclick={deleteSelected} disabled={!selectedClips.length}><Trash2 size={15} />Delete</button>
    <button class="btn-sm" onclick={stopSelecting} title="Done selecting (Esc)">Done</button>
  </div>
{/if}

<div bind:this={scroller} class="library-scroll min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-8 pt-1 pb-4">
  {#if scoped.length || shots.length}
    {#if sections.length}
      <VirtualGrid {sections} key={path} {minWidth} {maxWidth}>
        {#snippet heading(day)}
          {#if day.range !== undefined}
            <!-- A play session, from the recorder's log: when, and for how long. -->
            <div class="session section-heading flex min-w-0 flex-1 items-baseline gap-3">
              <h2 class="flex-none text-title font-semibold">{day.label}</h2>
              {#if day.range}<span class="min-w-0 truncate text-secondary text-muted tabular-nums" title={day.range}>{day.range}</span>{/if}
            </div>
          {:else}
            <div class="section-heading flex min-w-0 flex-1 items-baseline gap-3">
              <h2 class="flex-none text-title font-semibold">{day.label}</h2>
            </div>
          {/if}
        {/snippet}
        {#snippet children(it)}
          {#if 'clip' in it}
            {@const clip = it.clip}
            <ClipCard
              {clip}
              showGame={!game}
              {timeOf}
              selectable={selecting}
              selected={selected.has(clip.path)}
              copying={copyingPath === clip.path}
              copied={copiedPath === clip.path}
              copyProgress={copyingPath === clip.path ? copyProgress : null}
              {copyBlocked}
              onopen={() => onopen(clip, shown.map((c) => c.path))}
              ondelete={() => ondelete(clip)}
              oncopy={() => oncopy(clip)}
              onselect={(range) => toggle(clip, range)}
              onmenu={(x, y) => menuFor(it, x, y)}
              onfavorite={() => onfavorite(clip)}
            />
          {:else}
            {@const shot = it.shot}
            <ScreenshotCard {shot} {timeOf} selecting={selecting} {copyBlocked} copying={copyingPath === shot.path} copied={copiedPath === shot.path} showGame={!game} onopen={() => onshot(shot)} ondelete={() => ondelete(shot)} oncopy={() => oncopy(shot)} onmenu={(x, y) => menuFor(it, x, y)} />
          {/if}
        {/snippet}
      </VirtualGrid>
    {:else}
      <StateMessage title="No clips match" body={query.trim() ? `Nothing here is called “${query.trim()}”.` : 'No clip has that tag.'}>
        <button
          class="btn"
          onclick={() => {
            query = ''
            label = ''
          }}
        >
          Clear search
        </button>
      </StateMessage>
    {/if}
  {:else if loading}
    <div class="pt-3"><SkeletonGrid {minWidth} /></div>
  {:else if scope === 'starred'}
    <StateMessage title="No favorites{game ? ` from ${gameName(game.name)}` : ''} yet" body="Star a clip to keep it here." />
  {:else if game}
    <StateMessage title="No clips from {gameName(game.name)} yet" body="Press {hotkey} while playing to save the last {settings?.replay_seconds ?? 30} seconds." />
  {:else}
    <FirstRun {hotkey} {status} {onchoosescreen} {onsettings} />
  {/if}
</div>

<!-- The context menu of the card last asked. -->
<Popover bind:this={context} label="Clip actions">
  {#if target && 'clip' in target}
    {@const clip = target.clip}
    <button class="menu-item" role="menuitem" onclick={() => onopen(clip, shown.map((c) => c.path))}>Open</button>
    <button class="menu-item" role="menuitem" disabled={copyBlocked} onclick={() => oncopy(clip)} aria-keyshortcuts="Control+C">Copy clip<span class="menu-key" aria-hidden="true">Ctrl+C</span></button>
    <button class="menu-item" role="menuitem" onclick={() => onfavorite(clip)} aria-keyshortcuts="F">{clip.favorite ? 'Remove from favorites' : 'Add to favorites'}<span class="menu-key" aria-hidden="true">F</span></button>
    <button class="menu-item" role="menuitem" onclick={() => onreveal(clip)}>Show in folder</button>
    <button class="menu-item" role="menuitem" onclick={() => toggle(clip)}>{selected.has(clip.path) ? 'Deselect' : 'Select'}</button>
    <div class="menu-rule"></div>
    <button class="menu-item text-danger" role="menuitem" onclick={() => target && removeItem(target)} aria-keyshortcuts="Delete">Delete<span class="menu-key" aria-hidden="true">Del</span></button>
  {:else if target}
    {@const shot = target.shot}
    <button class="menu-item" role="menuitem" onclick={() => onshot(shot)}>Open</button>
    <button class="menu-item" role="menuitem" disabled={copyBlocked} onclick={() => oncopy(shot)} aria-keyshortcuts="Control+C">Copy screenshot<span class="menu-key" aria-hidden="true">Ctrl+C</span></button>
    <button class="menu-item" role="menuitem" onclick={() => onreveal(shot)}>Show in folder</button>
    <div class="menu-rule"></div>
    <button class="menu-item text-danger" role="menuitem" onclick={() => target && removeItem(target)} aria-keyshortcuts="Delete">Delete<span class="menu-key" aria-hidden="true">Del</span></button>
  {/if}
</Popover>

<style>
  .library-toolbar { position: relative; display: flex; flex: none; align-items: center; gap: 12px; min-height: 88px; padding: 0 32px; }
  .library-heading { min-width: 0; flex: 1; padding-right: 16px; }
  .library-heading h1 { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 24px; line-height: 32px; font-weight: 600; letter-spacing: -.5px; }
  .library-count { margin-top: 2px; color: var(--color-muted); font-size: 12px; line-height: 18px; font-variant-numeric: tabular-nums; }
  .search-field { width: 204px; min-width: 136px; flex: 0 1 204px; background: transparent; border-color: var(--color-line); }
  .view-button { display: inline-flex; flex: none; align-items: center; justify-content: center; gap: 7px; height: 36px; padding: 0 8px; border-radius: 2px; color: var(--color-fg-2); font-size: 13px; white-space: nowrap; }
  .view-button:hover, .view-button[aria-expanded='true'] { background: var(--color-chrome); color: var(--color-fg); }
  .view-button:active { background: var(--color-hover); transition-duration: 0s; }
  .menu-label { padding: 6px 10px 4px 36px; color: var(--color-muted); font-size: 12px; line-height: 18px; }
  .active-tag { min-width: 0; max-width: 100px; }
  .library-scroll { position: relative; scrollbar-gutter: stable; }
  .section-heading { max-width: 100%; }
  .section-heading :global(h2) { font-size: 13px; line-height: 20px; font-weight: 600; color: var(--color-fg); }
  .selection-toolbar { display: flex; flex: none; align-items: center; gap: 12px; min-height: 44px; padding: 6px 32px; background: var(--color-chrome); font-size: 13px; }
  .selection-summary { display: flex; min-width: 0; flex: 1; align-items: center; gap: 12px; }
  @media (max-width: 1100px) {
    .library-toolbar, .library-scroll, .selection-toolbar { padding-inline: 24px; }
    .library-toolbar { gap: 8px; }
    .search-field { width: 164px; flex-basis: 164px; }
    .active-tag { max-width: 68px; }
  }
  @media (max-height: 650px) { .library-toolbar { min-height: 76px; } }
</style>
