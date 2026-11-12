<script lang="ts">
  import { ArrowLeft } from '@lucide/svelte'
  import { type Clip, type GameEntry, type Screenshot, fileUrl } from './api'
  import Art from './Art.svelte'
  import ClipCard from './ClipCard.svelte'
  import { bytes, clock, dayKey, dayLabel, duration, playedOnDay, playtime } from './format'
  import ScreenshotCard from './ScreenshotCard.svelte'

  let {
    game,
    clips,
    screenshots,
    onback,
    onopen,
    onshot,
    ondelete,
    onshare,
    oncopy,
    onfavorite,
  }: {
    game: GameEntry
    clips: Clip[]
    screenshots: Screenshot[]
    onback: () => void
    onopen: (c: Clip) => void
    onshot: (s: Screenshot) => void
    ondelete: (c: Clip | Screenshot) => void
    onshare: (c: Clip | Screenshot) => void
    oncopy: (c: Clip | Screenshot) => void
    onfavorite: (c: Clip) => void
  } = $props()

  const own = $derived(clips.filter((c) => c.game === game.name))

  // Clips and screenshots grouped by local day, newest first, with that
  // day's play time.
  type Item = { at: number } & ({ clip: Clip } | { shot: Screenshot })
  const days = $derived.by(() => {
    const items: Item[] = [
      ...own.map((clip) => ({ at: clip.created_ms, clip })),
      ...screenshots.filter((s) => s.game === game.name).map((shot) => ({ at: shot.modified_ms, shot })),
    ].sort((a, b) => b.at - a.at)
    const groups: { key: string; label: string; played: number; items: Item[] }[] = []
    for (const it of items) {
      const key = dayKey(it.at)
      let g = groups.find((x) => x.key === key)
      if (!g) {
        g = { key, label: dayLabel(it.at), played: playedOnDay(game.sessions, it.at), items: [] }
        groups.push(g)
      }
      g.items.push(it)
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
        {#each day.items as it ('clip' in it ? it.clip.path : it.shot.path)}
          {#if 'clip' in it}
            {@const c = it.clip}
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
          {:else}
            {@const s = it.shot}
            <ScreenshotCard
              shot={s}
              onopen={() => onshot(s)}
              ondelete={() => ondelete(s)}
              onshare={() => onshare(s)}
              oncopy={() => oncopy(s)}
            />
          {/if}
        {/each}
      </div>
    </section>
  {:else}
    <p class="py-16 text-center text-body text-muted">No clips from {game.name} yet</p>
  {/each}
</div>
