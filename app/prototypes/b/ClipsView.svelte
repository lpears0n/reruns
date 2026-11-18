<!--
  The library: a header (what is listed and how many, search, one options
  menu) and under it the clips the navigation chose, in dated sections,
  newest first, as many to a row as fit. Renders the header and the scroller
  into the page; it stays mounted behind an open clip, so coming back finds
  the same filters and the same scroll position.
-->
<script lang="ts">
  import { Check, Search, SlidersHorizontal, X } from '@lucide/svelte'
  import { type Clip, type GameEntry, type Screenshot, type Settings, type Status, fileUrl } from './api'
  import Art from './Art.svelte'
  import { ago } from './format'
  import { tint } from '../shared/tint.svelte'
  import { bySession, playedThatDay } from '../shared/sessions'
  import { shared } from '../shared/library.svelte'
  import ClipCard from './ClipCard.svelte'
  import FirstRun from './FirstRun.svelte'
  import { date, dayKey, dayLabel, gameName, playedOnDay, playtime } from './format'
  import Popover from './Popover.svelte'
  import { prefs, setPrefs, type ThumbSize } from './prefs.svelte'
  import type { Scope } from './Rail.svelte'
  import ScreenshotCard from './ScreenshotCard.svelte'
  import type { Category } from './SettingsView.svelte'
  import SkeletonGrid from './SkeletonGrid.svelte'
  import StateMessage from './StateMessage.svelte'
  import type { Selection } from './StatusBar.svelte'
  import VirtualGrid from './VirtualGrid.svelte'

  let {
    clips,
    screenshots,
    scope,
    game,
    behind,
    loading,
    hotkey,
    status,
    settings,
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
    /** The game listed when `scope` is 'game'. */
    game: GameEntry | null
    /** A clip or screenshot is open in the library's place: the keys are its. */
    behind: boolean
    loading: boolean
    hotkey: string
    status: Status | null
    settings: Settings | null
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

  type Sort = 'newest' | 'oldest' | 'longest' | 'largest'
  const SORTS: { value: Sort; label: string }[] = [
    { value: 'newest', label: 'Newest first' },
    { value: 'oldest', label: 'Oldest first' },
    { value: 'longest', label: 'Longest first' },
    { value: 'largest', label: 'Largest first' },
  ]
  /** Narrowest a card gets before the grid drops a column: three to a row
   *  at 1280 to 1440 with the navigation open, at the middle size. */
  const SIZES: { value: ThumbSize; label: string; min: number }[] = [
    { value: 'large', label: 'Large', min: 440 },
    { value: 'medium', label: 'Medium', min: 320 },
    { value: 'small', label: 'Small', min: 230 },
  ]
  const minWidth = $derived(SIZES.find((s) => s.value === prefs.thumbSize)?.min ?? 320)

  let query = $state('')
  let search: HTMLInputElement | undefined = $state()
  let label = $state('')
  let sort = $state<Sort>('newest')

  // Selection mode for bulk actions.
  let selecting = $state(false)
  let selected = $state<Set<string>>(new Set())
  /** The clip last picked: where a Shift-click's range starts. */
  let anchor: string | null = null

  const scoped = $derived(
    scope === 'game' ? clips.filter((c) => c.game === game?.name) : scope === 'starred' ? clips.filter((c) => c.favorite) : clips,
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

  // A game's screenshots sit among its clips by time, unless a filter or a
  // sort by something else has narrowed the list to clips.
  const byDay = $derived(sort === 'newest' || sort === 'oldest')
  const shots = $derived(scope === 'game' ? screenshots.filter((s) => s.game === game?.name) : [])
  /** A game's clips: by the play session they were saved in (the default),
   *  or by day as everywhere else. */
  let grouping = $state<'session' | 'day'>('session')
  type Item = { at: number } & ({ clip: Clip } | { shot: Screenshot })
  type Section = { key: string; label: string; range?: string; played: number; live?: boolean; games?: { game: GameEntry; ms: number }[]; items: Item[] }
  const path = (it: Item) => ('clip' in it ? it.clip.path : it.shot.path)
  /** A local day with what was saved on it; one unnamed section when not sorted by time. */
  const sections = $derived.by(() => {
    const items: Item[] = shown.map((clip) => ({ at: clip.created_ms, clip }))
    if (!byDay) return items.length ? [{ key: 'all', label: '', played: 0, items }] : []
    if (!activeLabel && !query.trim()) items.push(...shots.map((shot) => ({ at: shot.modified_ms, shot })))
    items.sort((a, b) => (sort === 'oldest' ? a.at - b.at : b.at - a.at))
    if (game && grouping === 'session') {
      const groups = bySession(items, (it) => it.at, game)
      if (sort === 'oldest') groups.reverse()
      return groups as Section[]
    }
    const days: Section[] = []
    for (const it of items) {
      const key = dayKey(it.at)
      if (days.at(-1)?.key !== key) {
        days.push({ key, label: dayLabel(it.at), played: game ? playedOnDay(game.sessions, it.at) : 0, games: game ? [] : playedThatDay(shared.games, it.at), items: [] })
      }
      days.at(-1)!.items.push(it)
    }
    return days
  })

  /** What the header says is listed, and under what terms. */
  // The viewer opens the session's other clips through the library.
  $effect(() => {
    const queue = shown.map((c) => c.path)
    shared.open = (c) => onopen(c, queue.includes(c.path) ? queue : [c.path])
  })

  /** The listed game's pictures, and the tint read from its art. */
  const newest = $derived(scoped.find((c) => c.thumbnail)?.thumbnail ?? null)
  const poster = $derived(game ? (game.art.capsule ?? game.fallback_image ?? newest) : null)
  const hue = $derived(game ? tint(game.name, poster ? fileUrl(poster) : null) : null)
  const capturing = $derived(!!game && status?.state === 'recording' && status.game === game.name)
  let logoReady = $state(false)
  $effect(() => {
    game?.name
    logoReady = false
  })

  const listed = $derived(game ? gameName(game.name) : scope === 'starred' ? 'Favorites' : 'All clips')
  const stats = $derived(
    [
      `${shown.length} clip${shown.length === 1 ? '' : 's'}`,
      sort !== 'newest' && SORTS.find((s) => s.value === sort)?.label.toLowerCase(),
      game?.total_play_ms ? `${playtime(game.total_play_ms)} played` : null,
    ].filter((s): s is string => !!s),
  )

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

  /** What the capture strip shows in place of the recorder while clips are
   *  being picked; null otherwise. Read by the app, which owns the strip. */
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

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement
    if (behind) return
    if (search && (((e.ctrlKey || e.metaKey) && e.key === 'f') || (e.key === '/' && !typing))) {
      e.preventDefault()
      search.focus()
      search.select()
      return
    }
    if (typing) return
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

{#snippet spine(g: GameEntry)}
  {#if g.art.capsule}
    <span class="spine" aria-hidden="true"><Art path={g.art.capsule} class="size-full object-cover" /></span>
  {/if}
{/snippet}

<header class={['page-header', { band: game, plain: game && !game.art.hero }]} style:--tint={hue}>
  {#if game}
    <!-- The game's own key art, in its band and nowhere else: bleeds off the
         band's right edge and fades into the tint read from it. -->
    {#if game.art.hero}
      <Art path={game.art.hero} class="hero" />
    {/if}
    <div class="band-info min-w-0 flex-1">
      {#if game.art.logo}
        <!-- The logo once it has loaded; its name in type until then (and if it never does). -->
        {#key game.name}
          <h1 class="sr-only">{listed}</h1>
          <div class={['logo-slot', { ready: logoReady }]} onloadcapture={() => (logoReady = true)}>
            <Art path={game.art.logo} class="logo" alt="" />
            <div class="band-title truncate" aria-hidden="true">{listed}</div>
          </div>
        {/key}
      {:else}
        <h1 class="band-title truncate" title={listed}>{listed}</h1>
      {/if}
      <p class="mt-2 flex min-w-0 gap-5 overflow-hidden text-secondary whitespace-nowrap text-fg-2 tabular-nums">
        {#if capturing}
          <span class="flex items-center gap-1.5 font-medium text-fg"><span class="size-1.5 rounded-full bg-ok"></span>Capturing now</span>
        {:else if game.playing}
          <span class="font-medium text-fg">Playing now</span>
        {:else if game.last_played_ms}
          <span>Played {ago(game.last_played_ms)}</span>
        {/if}
        <span>{scoped.length} clip{scoped.length === 1 ? '' : 's'}</span>
        {#if game.total_play_ms}<span>{playtime(game.total_play_ms)} played</span>{/if}
      </p>
    </div>
  {/if}
  <div class={['flex min-w-0 items-baseline gap-3', game ? 'flex-none self-start pt-5' : 'flex-1']}>
    {#if !game}<h1 class="truncate font-display text-page font-semibold" title={listed}>{listed}</h1>{/if}
    {#if scoped.length || shots.length}
      {#if !game}<p class="flex-none text-secondary whitespace-nowrap text-fg-2 tabular-nums">{stats.join(' · ')}</p>{/if}
      {#if activeLabel}
        <button class="btn-text -ml-1 flex-none gap-1 self-center font-medium text-fg-2" onclick={() => (label = '')} aria-label="Stop filtering by tag {activeLabel}">
          #{activeLabel}
          <X size={13} />
        </button>
      {/if}
    {/if}
  </div>

  {#if scoped.length || shots.length}
    <label class={['field flex w-64 flex-none items-center gap-2 max-lg:w-48', { 'mt-5 self-start': game }]} title="Search (Ctrl+F)">
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
  {/if}

  <Popover label="Library options">
    {#snippet trigger(props)}
      <button {...props} class={['btn-icon -mr-2', { 'tool mt-5 self-start': game }]} aria-label="Library options" title="Sort, thumbnail size and selection">
        <SlidersHorizontal size={18} />
      </button>
    {/snippet}
    <div class="pt-1.5 pr-2.5 pb-1 pl-9 text-caption text-muted">Sort</div>
    {#each SORTS as s (s.value)}
      {@render radio(sort === s.value, s.label, () => (sort = s.value))}
    {/each}
    <div class="menu-rule"></div>
    <div class="pt-1.5 pr-2.5 pb-1 pl-9 text-caption text-muted">Thumbnails</div>
    {#each SIZES as s (s.value)}
      {@render radio(prefs.thumbSize === s.value, s.label, () => setPrefs({ thumbSize: s.value }))}
    {/each}
    {#if labels.length}
      <div class="menu-rule"></div>
      <div class="pt-1.5 pr-2.5 pb-1 pl-9 text-caption text-muted">Tag</div>
      {@render radio(!activeLabel, 'Any tag', () => (label = ''))}
      {#each labels as l (l)}
        {@render radio(activeLabel === l, `#${l}`, () => (label = l))}
      {/each}
    {/if}
    <div class="menu-rule"></div>
    <button class="menu-item" role="menuitem" disabled={!shown.length} onclick={() => (selecting ? stopSelecting() : (selecting = true))}>
      <span class="w-4 flex-none"></span>{selecting ? 'Stop selecting' : 'Select clips'}
      <span class="menu-key">Ctrl+click</span>
    </button>
    {#if game}
      <div class="menu-rule"></div>
      <div class="pt-1.5 pr-2.5 pb-1 pl-9 text-caption text-muted">Group</div>
      {@render radio(grouping === 'session', 'By play session', () => (grouping = 'session'))}
      {@render radio(grouping === 'day', 'By day', () => (grouping = 'day'))}
    {/if}
    <div class="menu-rule"></div>
    <button class="menu-item" role="menuitem" onclick={onpick}><span class="w-4 flex-none"></span>Open a video file…</button>
    {#if game}
      <div class="menu-rule"></div>
      <button class="menu-item" role="menuitem" onclick={() => onnotgame(game)}><span class="w-4 flex-none"></span>{gameName(game.name)} is not a game</button>
    {/if}
  </Popover>
</header>

<div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-8 pt-1 pb-4">
  {#if scoped.length || shots.length}
    {#if sections.length}
      <VirtualGrid {sections} key={path} {minWidth} head={48}>
        {#snippet heading(day)}
          {#if day.range !== undefined}
            <!-- A play session, from the recorder's log: when, and for how long. -->
            <div class="session flex min-w-0 flex-1 items-baseline gap-3" style:--tint={hue}>
              <h2 class="flex-none text-title font-semibold">{day.label}</h2>
              {#if day.range}<span class="flex-none text-secondary text-fg-2 tabular-nums">{day.range}</span>{/if}
              {#if day.live && capturing}<span class="flex flex-none items-center gap-1.5 self-center text-secondary text-fg"><span class="size-1.5 rounded-full bg-ok"></span>Capturing</span>{/if}
              <span class="ml-auto flex-none text-secondary text-muted tabular-nums">
                {#if day.played}{playtime(day.played)} played{/if}
              </span>
            </div>
          {:else}
            <div class="flex min-w-0 flex-1 items-center gap-3">
            <h2 class="flex-none text-title font-semibold">{day.label}</h2>
            {#if day.played}<span class="text-secondary text-fg-2">Played {playtime(day.played)}</span>{/if}
            {#if day.games?.length}
              <!-- What was played that day, from the log: the day reads as an evening. -->
              <ul class="ml-3 flex min-w-0 items-center gap-5 overflow-hidden" aria-label="Played that day">
                {#each day.games as { game: g, ms } (g.name)}
                  <li class="flex flex-none items-center gap-2 text-secondary whitespace-nowrap text-fg-2">
                    {@render spine(g)}
                    <span>{gameName(g.name)}</span>
                    <span class="text-muted tabular-nums">{playtime(ms)}</span>
                  </li>
                {/each}
              </ul>
            {/if}
            </div>
          {/if}
        {/snippet}
        {#snippet children(it)}
          {#if 'clip' in it}
            {@const clip = it.clip}
            <ClipCard
              {clip}
              showGame={!game}
              dated={!byDay}
              selectable={selecting}
              selected={selected.has(clip.path)}
              onopen={() => onopen(clip, shown.map((c) => c.path))}
              ondelete={() => ondelete(clip)}
              oncopy={() => oncopy(clip)}
              onselect={(range) => toggle(clip, range)}
              onmenu={(x, y) => menuFor(it, x, y)}
              onfavorite={() => onfavorite(clip)}
            />
          {:else}
            {@const shot = it.shot}
            <ScreenshotCard {shot} showGame={!game} onopen={() => onshot(shot)} ondelete={() => ondelete(shot)} oncopy={() => oncopy(shot)} onmenu={(x, y) => menuFor(it, x, y)} />
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
  {:else if game}
    <StateMessage title="No clips from {gameName(game.name)} yet" body="Press {hotkey} while playing to save the last {settings?.replay_seconds ?? 30} seconds." />
  {:else if scope === 'starred'}
    <StateMessage title="No favorites yet" body="Star a clip to keep it here." />
  {:else}
    <FirstRun {hotkey} {status} {settings} {onchoosescreen} {onsettings} />
  {/if}
</div>

<!-- The context menu of the card last asked. -->
<Popover bind:this={context} label="Clip actions">
  {#if target && 'clip' in target}
    {@const clip = target.clip}
    <button class="menu-item" role="menuitem" onclick={() => onopen(clip, shown.map((c) => c.path))}>Open</button>
    <button class="menu-item" role="menuitem" onclick={() => oncopy(clip)}>Copy file</button>
    <button class="menu-item" role="menuitem" onclick={() => onfavorite(clip)}>{clip.favorite ? 'Remove from favorites' : 'Add to favorites'}</button>
    <button class="menu-item" role="menuitem" onclick={() => onreveal(clip)}>Show in folder</button>
    <button class="menu-item" role="menuitem" onclick={() => toggle(clip)}>{selected.has(clip.path) ? 'Deselect' : 'Select'}</button>
    <div class="menu-rule"></div>
    <button class="menu-item text-danger" role="menuitem" onclick={() => ondelete(clip)}>Delete</button>
  {:else if target}
    {@const shot = target.shot}
    <button class="menu-item" role="menuitem" onclick={() => onshot(shot)}>Open</button>
    <button class="menu-item" role="menuitem" onclick={() => oncopy(shot)}>Copy file</button>
    <button class="menu-item" role="menuitem" onclick={() => onreveal(shot)}>Show in folder</button>
    <div class="menu-rule"></div>
    <button class="menu-item text-danger" role="menuitem" onclick={() => ondelete(shot)}>Delete</button>
  {/if}
</Popover>

<style>
  /* A game's page opens on its band: the game's key art bleeding off the
     right edge, fading into a surface tinted from that same art, its logo
     (or its name) and what the log knows about playing it. Compact: the
     first row of clips stays in view at the smallest window. Only the band
     takes the game's colour. */
  .band {
    position: relative;
    height: 156px;
    align-items: flex-start;
    gap: 12px;
    overflow: hidden;
    background: color-mix(in oklab, var(--tint) 62%, var(--color-canvas));
    isolation: isolate;
  }

  @media (max-height: 760px) {
    .band {
      height: 120px;
    }
  }

  /* No key art from Steam: a shorter band, its name and its facts. */
  .band.plain {
    height: 108px;
  }

  .band :global(.hero) {
    position: absolute;
    inset: 0 0 0 auto;
    z-index: -1;
    width: min(64%, 900px);
    height: 100%;
    object-fit: cover;
    object-position: center 30%;
    mask-image: linear-gradient(to right, transparent, #000 38%);
  }

  .band-info {
    align-self: flex-end;
    padding-bottom: 22px;
  }

  @media (max-height: 760px) {
    .band-info {
      padding-bottom: 16px;
    }
  }

  .band :global(.logo) {
    display: block;
    max-width: min(300px, 40vw);
    max-height: 64px;
    object-fit: contain;
    object-position: left bottom;
    filter: drop-shadow(0 2px 6px rgb(0 0 0 / 45%));
  }

  @media (max-height: 760px) {
    .band :global(.logo) {
      max-height: 44px;
    }
  }

  .logo-slot {
    position: relative;
  }

  .logo-slot:not(.ready) :global(.logo) {
    position: absolute;
    opacity: 0;
  }

  .logo-slot.ready .band-title {
    display: none;
  }

  .band-title {
    font-family: var(--font-display);
    font-size: 32px;
    line-height: 38px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  /* Tools over the art keep their own opaque surface. */
  .band :global(.tool) {
    background: var(--color-chrome);
  }

  .band + div {
    padding-top: 12px;
  }

  /* A session's heading sits on a hairline of the game's colour. */
  .session {
    padding-bottom: 9px;
    box-shadow: inset 0 -1px 0 color-mix(in oklab, var(--tint) 70%, var(--color-fg) 12%);
  }

  .spine {
    position: relative;
    display: block;
    flex: none;
    width: 14px;
    height: 20px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--color-raised);
  }

  .spine::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 14%);
  }
</style>
