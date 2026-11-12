<script lang="ts">
  import { ArrowLeft } from '@lucide/svelte'
  import { type Clip, type GameEntry, fileUrl } from './api'
  import Art from './Art.svelte'
  import ClipCard from './ClipCard.svelte'
  import { bytes, clock, dayKey, dayLabel, duration, playedOnDay, playtime } from './format'

  let {
    game,
    clips,
    onback,
    onopen,
    ondelete,
    onshare,
    oncopy,
    onfavorite,
  }: {
    game: GameEntry
    clips: Clip[]
    onback: () => void
    onopen: (c: Clip) => void
    ondelete: (c: Clip) => void
    onshare: (c: Clip) => void
    oncopy: (c: Clip) => void
    onfavorite: (c: Clip) => void
  } = $props()

  const own = $derived(clips.filter((c) => c.game === game.name))

  // Clips grouped by local day, newest first, with that day's play time.
  const days = $derived.by(() => {
    const groups: { key: string; label: string; played: number; clips: Clip[] }[] = []
    for (const c of own) {
      const key = dayKey(c.created_ms)
      let g = groups.find((x) => x.key === key)
      if (!g) {
        g = { key, label: dayLabel(c.created_ms), played: playedOnDay(game.sessions, c.created_ms), clips: [] }
        groups.push(g)
      }
      g.clips.push(c)
    }
    return groups
  })
</script>

<!-- Esc goes up one level (DESIGN §2.7). -->
<svelte:window onkeydown={(e) => e.key === 'Escape' && onback()} />

<div class="mx-auto max-w-(--container-library) space-y-8">
  <header class="relative h-(--height-banner) overflow-hidden rounded-surface bg-panel">
    {#snippet frame()}
      {#if game.fallback_image}
        <img src={fileUrl(game.fallback_image)} alt="" draggable="false" class="absolute inset-0 size-full object-cover object-[center_30%] brightness-75" />
      {/if}
    {/snippet}
    {#if game.art.hero}
      <Art path={game.art.hero} class="absolute inset-0 size-full object-cover object-[center_30%]" fallback={frame} />
    {:else}
      {@render frame()}
    {/if}
    <div class="absolute inset-0 bg-gradient-to-t from-black/85 via-black/30 to-transparent"></div>
    <button class="btn-scrim absolute top-4 left-4" onclick={onback} aria-label="Back to games" title="Back">
      <ArrowLeft size={18} />
    </button>
    <div class="absolute inset-x-6 bottom-5 flex items-end justify-between gap-6">
      {#snippet title()}
        <h1 class="display-face text-page leading-none font-bold">{game.name}</h1>
      {/snippet}
      {#if game.art.logo}
        <Art path={game.art.logo} alt={game.name} class="max-h-16 max-w-72 object-contain object-left-bottom" fallback={title} />
      {:else}
        {@render title()}
      {/if}
      <p class="flex-none text-secondary text-fg-2 tabular-nums">
        {own.length} clip{own.length === 1 ? '' : 's'} · {bytes(own.reduce((n, c) => n + c.size_bytes, 0))}{game.total_play_ms
          ? ` · ${playtime(game.total_play_ms)} played`
          : ''}
      </p>
    </div>
  </header>

  {#each days as day (day.key)}
    <section>
      <h2 class="mb-4 flex items-baseline gap-3">
        <span class="section-title">{day.label}</span>
        {#if day.played}<span class="text-secondary text-muted">Played {playtime(day.played)}</span>{/if}
      </h2>
      <div class="clip-grid grid gap-x-4 gap-y-5">
        {#each day.clips as c (c.path)}
          <ClipCard
            clip={c}
            fallback={clock(c.created_ms)}
            subtitle={duration(c.duration)}
            onopen={() => onopen(c)}
            ondelete={() => ondelete(c)}
            onshare={() => onshare(c)}
            oncopy={() => oncopy(c)}
            onfavorite={() => onfavorite(c)}
          />
        {/each}
      </div>
    </section>
  {:else}
    <p class="py-16 text-center text-body text-muted">No clips from {game.name} yet</p>
  {/each}
</div>
