<script lang="ts">
  import { ArrowDownUp, CheckSquare, Clapperboard, Search, Star, Tag, Trash2, X } from '@lucide/svelte'
  import type { Clip } from './api'
  import ClipCard from './ClipCard.svelte'
  import { bytes, date } from './format'
  import SkeletonGrid from './SkeletonGrid.svelte'
  import StateMessage from './StateMessage.svelte'
  import VirtualGrid from './VirtualGrid.svelte'

  let {
    clips,
    loading,
    hotkey,
    onopen,
    ondelete,
    ondeletemany,
    onshare,
    oncopy,
    onfavorite,
  }: {
    clips: Clip[]
    loading: boolean
    hotkey: string
    onopen: (c: Clip) => void
    ondelete: (c: Clip) => void
    ondeletemany: (cs: Clip[]) => void
    onshare: (c: Clip) => void
    oncopy: (c: Clip) => void
    onfavorite: (c: Clip) => void
  } = $props()

  type Sort = 'newest' | 'oldest' | 'longest' | 'largest'
  const SORTS: { value: Sort; label: string }[] = [
    { value: 'newest', label: 'Newest' },
    { value: 'oldest', label: 'Oldest' },
    { value: 'longest', label: 'Longest' },
    { value: 'largest', label: 'Largest file' },
  ]

  let game = $state('')
  let label = $state('')
  let favorites = $state(false)
  let query = $state('')
  let sort = $state<Sort>('newest')
  let search: HTMLInputElement | undefined = $state()

  // Selection mode for bulk actions.
  let selecting = $state(false)
  let selected = $state<Set<string>>(new Set())

  const games = $derived([...new Set(clips.map((c) => c.game))].sort((a, b) => a.localeCompare(b)))
  const labels = $derived(
    [...new Set(clips.flatMap((c) => c.labels))].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' })),
  )
  const anyFavorites = $derived(clips.some((c) => c.favorite))

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    let list = clips.filter(
      (c) => (!game || c.game === game) && (!favorites || c.favorite) && (!label || c.labels.includes(label)),
    )
    if (q) {
      list = list.filter((c) =>
        `${c.title ?? ''} ${c.game} ${c.name} ${c.labels.join(' ')} ${date(c.created_ms)}`.toLowerCase().includes(q),
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
  const total = $derived(shown.reduce((n, c) => n + c.size_bytes, 0))
  const selectedClips = $derived(shown.filter((c) => selected.has(c.path)))
  const selectedBytes = $derived(selectedClips.reduce((n, c) => n + c.size_bytes, 0))

  $effect(() => {
    if (game && !games.includes(game)) game = ''
    if (label && !labels.includes(label)) label = ''
    if (favorites && !anyFavorites) favorites = false
  })

  // Drop selections for clips that disappeared.
  $effect(() => {
    const paths = new Set(clips.map((c) => c.path))
    if ([...selected].some((p) => !paths.has(p))) {
      selected = new Set([...selected].filter((p) => paths.has(p)))
    }
  })

  function toggle(c: Clip) {
    const next = new Set(selected)
    if (next.has(c.path)) next.delete(c.path)
    else next.add(c.path)
    selected = next
  }

  function selectAll() {
    selected = selected.size === shown.length ? new Set() : new Set(shown.map((c) => c.path))
  }

  function stopSelecting() {
    selecting = false
    selected = new Set()
  }

  function deleteSelected() {
    const cs = selectedClips
    stopSelecting()
    ondeletemany(cs)
  }

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement
    if (((e.ctrlKey || e.metaKey) && e.key === 'f') || (e.key === '/' && !typing)) {
      e.preventDefault()
      search?.focus()
      search?.select()
    } else if (e.key === 'Escape' && !typing) {
      if (selecting) stopSelecting()
      else if (query) query = ''
    } else if (e.key === 'Escape' && e.target === search) {
      query = ''
      search?.blur()
    } else if (selecting && !typing && (e.ctrlKey || e.metaKey) && e.key === 'a') {
      e.preventDefault()
      selectAll()
    } else if (selecting && !typing && e.key === 'Delete' && selected.size) {
      deleteSelected()
    }
  }
</script>

<svelte:window {onkeydown} />

{#if clips.length}
  <div class="mx-auto max-w-(--container-library) space-y-5">
    <div class="flex flex-wrap items-center justify-end gap-2">
      <span class="flex-none text-secondary text-muted tabular-nums">{shown.length} clip{shown.length === 1 ? '' : 's'} · {bytes(total)}</span>
      <div class="min-w-4 flex-1"></div>

      <label class="field flex w-56 flex-none items-center gap-2 px-2.5">
        <Search size={15} class="flex-none text-muted" />
        <input
          bind:this={search}
          bind:value={query}
          type="search"
          placeholder="Search (Ctrl+F)"
          class="min-w-0 flex-1 bg-transparent text-body outline-none placeholder:text-muted"
          aria-label="Search clips"
        />
        {#if query}
          <button class="btn-icon size-6 text-muted" onclick={() => (query = '')} aria-label="Clear search">
            <X size={14} />
          </button>
        {/if}
      </label>

      {#if anyFavorites}
        <button
          class="btn flex-none px-2.5 {favorites ? 'border-accent/60 text-accent' : ''}"
          onclick={() => (favorites = !favorites)}
          aria-pressed={favorites}
          title="Only favourites"
        >
          <Star size={16} fill={favorites ? 'currentColor' : 'none'} />
        </button>
      {/if}

      {#if labels.length}
        <label class="field flex flex-none items-center gap-2 pr-0 pl-2.5">
          <Tag size={15} class="flex-none text-muted" />
          <select class="h-full max-w-36 bg-transparent pr-7 text-body outline-none" bind:value={label} aria-label="Filter by label">
            <option value="">Any label</option>
            {#each labels as l (l)}
              <option value={l}>{l}</option>
            {/each}
          </select>
        </label>
      {/if}

      {#if games.length > 1}
        <select class="field w-44 flex-none" bind:value={game} aria-label="Filter by game">
          <option value="">All games</option>
          {#each games as g (g)}
            <option value={g}>{g}</option>
          {/each}
        </select>
      {/if}

      <label class="field flex flex-none items-center gap-2 pr-0 pl-2.5">
        <ArrowDownUp size={15} class="flex-none text-muted" />
        <select class="h-full bg-transparent pr-7 text-body outline-none" bind:value={sort} aria-label="Sort clips">
          {#each SORTS as s (s.value)}
            <option value={s.value}>{s.label}</option>
          {/each}
        </select>
      </label>

      <button
        class="btn flex-none {selecting ? 'border-accent/60 text-accent' : ''}"
        onclick={() => (selecting ? stopSelecting() : (selecting = true))}
        title={selecting ? 'Done selecting (Esc)' : 'Select clips'}
      >
        <CheckSquare size={16} />
        {selecting ? 'Done' : 'Select'}
      </button>
    </div>

    {#if shown.length}
      <VirtualGrid items={shown} key={(c) => c.path}>
        {#snippet children(clip)}
          <ClipCard
            {clip}
            selectable={selecting}
            selected={selected.has(clip.path)}
            onopen={() => onopen(clip)}
            ondelete={() => ondelete(clip)}
            onshare={() => onshare(clip)}
            oncopy={() => oncopy(clip)}
            onselect={() => toggle(clip)}
            onfavorite={() => onfavorite(clip)}
          />
        {/snippet}
      </VirtualGrid>
    {:else}
      <StateMessage title="Nothing matches">
        <button
          class="btn"
          onclick={() => {
            query = ''
            game = ''
            label = ''
            favorites = false
          }}
        >
          Clear filters
        </button>
      </StateMessage>
    {/if}
  </div>

  {#if selecting}
    <div
      class="floating fixed bottom-6 left-1/2 z-40 flex -translate-x-1/2 items-center gap-3 rounded-full py-2 pr-2 pl-5"
      role="toolbar"
      aria-label="Selection actions"
    >
      <span class="text-body font-medium tabular-nums">
        {selected.size} selected
        {#if selected.size}<span class="text-muted"> · {bytes(selectedBytes)}</span>{/if}
      </span>
      <button class="btn h-8 rounded-full px-3 text-secondary" onclick={selectAll}>
        {selected.size === shown.length ? 'Select none' : 'Select all'}
      </button>
      <button
        class="btn-primary h-8 rounded-full bg-danger px-3 text-secondary text-white"
        disabled={!selected.size}
        onclick={deleteSelected}
      >
        <Trash2 size={15} />
        Delete
      </button>
      <button class="btn-icon rounded-full" onclick={stopSelecting} aria-label="Cancel selection" title="Cancel (Esc)">
        <X size={16} />
      </button>
    </div>
  {/if}
{:else if loading}
  <div class="mx-auto max-w-(--container-library)"><SkeletonGrid /></div>
{:else}
  <StateMessage icon={Clapperboard} title="No clips yet">
    {#snippet body()}Press <kbd>{hotkey}</kbd> in a game to save one.{/snippet}
  </StateMessage>
{/if}
