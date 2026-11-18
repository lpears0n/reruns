<script lang="ts">
  import { ArrowLeft, Download, Grid2X2, Settings, Star } from '@lucide/svelte'
  import type { Snippet } from 'svelte'
  import type { Clip, GameEntry, UpdateInfo } from './api'
  import Art from './Art.svelte'
  import { gameName } from './format'
  import type { Scope } from './types'
  import { CATEGORIES, type Category } from './SettingsView.svelte'

  let { games, clips, scope, game, settings, category, update, onscope, onsettings, oncategory, onback, onupdate, capture }: {
    games: GameEntry[]
    clips: Clip[]
    scope: Scope
    game: string | null
    settings: boolean
    category: Category
    update: UpdateInfo | null
    onscope: (scope: Scope, name?: string | null) => void
    onsettings: () => void
    oncategory: (category: Category) => void
    onback: () => void
    onupdate: () => void
    capture: Snippet
  } = $props()

  const frames = $derived.by(() => {
    const found = new Map<string, Clip>()
    for (const clip of clips) if (clip.thumbnail && clip.created_ms > (found.get(clip.game)?.created_ms ?? 0)) found.set(clip.game, clip)
    return found
  })
</script>

<aside class="library-sidebar" aria-label="Reruns">
  <button class="brand" onclick={onback} aria-label="Reruns library">
    <img class="brand-mark" src="/brand-mark.svg" alt="" draggable="false" />
    <img class="brand-wordmark" src="/brand-wordmark.svg" alt="Reruns" draggable="false" />
  </button>

  {#if settings}
    <nav class="settings-navigation sidebar-scroll" aria-label="Settings pages">
      <button class="destination back" onclick={onback}><ArrowLeft size={16} />Back to clips</button>
      <p class="section-label">Settings</p>
      {#each CATEGORIES as c}
        <button class="destination category" aria-current={category === c ? 'page' : undefined} onclick={() => oncategory(c)}>{c}</button>
      {/each}
    </nav>
  {:else}
    <!-- All clips and Favorites stay put; only the games scroll. -->
    <nav class="library-navigation" aria-label="Library">
      <div class="destinations">
        <button class="destination" aria-current={scope !== 'starred' && !game ? 'page' : undefined} onclick={() => onscope('all')}><Grid2X2 size={16} />All clips</button>
        <button class="destination" aria-current={scope === 'starred' ? 'page' : undefined} onclick={() => onscope('starred', game)}><Star size={16} />Favorites</button>
      </div>
      {#if games.length}
        <div class="games-scroll">
        <div class="collection-heading">
          <p class="section-label">Games</p>
          {#if game}<button class="clear-game" onclick={() => onscope(scope === 'starred' ? 'starred' : 'all')} aria-label="Show all games" title="Show all games">All</button>{/if}
        </div>
        <div class="collections">
          {#each games as entry (entry.name)}
            {@const art = entry.art.capsule ?? entry.fallback_image ?? frames.get(entry.name)?.thumbnail}
            <button class="collection" aria-current={game === entry.name ? 'page' : undefined} onclick={() => onscope(scope === 'starred' ? 'starred' : 'game', entry.name)} title={gameName(entry.name)}>
              <span class="cover" aria-hidden="true">
                {#snippet fallback()}
                  {@const frame = frames.get(entry.name)?.thumbnail}
                  {#if frame && frame !== art}
                    <Art path={frame} class="size-full object-cover">
                      {#snippet fallback()}<span>{Array.from(gameName(entry.name))[0]}</span>{/snippet}
                    </Art>
                  {:else}<span>{Array.from(gameName(entry.name))[0]}</span>{/if}
                {/snippet}
                {#if art}<Art path={art} class="size-full object-cover" {fallback} />{:else}{@render fallback()}{/if}
              </span>
              <span class="collection-name">{gameName(entry.name)}</span>
            </button>
          {/each}
        </div>
        </div>
      {/if}
    </nav>
  {/if}

  <div class="sidebar-bottom">
    {@render capture()}
    <div class="sidebar-utilities">
      <button class="settings-link" aria-label="Settings" aria-current={settings ? 'page' : undefined} onclick={onsettings}><Settings size={16} />Settings</button>
      {#if update}<button class="btn-icon-sm" onclick={onupdate} aria-label="Update to {update.version}" title="Update to {update.version}"><Download size={16} /></button>{/if}
    </div>
  </div>
</aside>

<style>
  .library-sidebar { display: flex; flex: none; flex-direction: column; width: 228px; min-height: 0; background: var(--color-nav); }
  .brand { display: flex; flex: none; align-items: center; gap: 12px; height: 88px; padding: 0 24px; text-align: left; }
  .brand-mark { width: 28px; height: 28px; object-fit: contain; }
  .brand-wordmark { width: 124px; height: 27px; object-fit: contain; object-position: left center; }
  /* The app's own scrollbar (app.css); `scrollbar-width` would swap in the system's. */
  .sidebar-scroll { flex: 1; min-height: 0; overflow: auto; padding: 0 12px 12px; }
  .library-navigation { display: flex; flex: 1; flex-direction: column; min-height: 0; padding: 0 12px; }
  .destinations { flex: none; }
  .games-scroll { flex: 1; min-height: 0; overflow-y: auto; margin: 0 -12px; padding: 0 12px 12px; }
  .destination { display: flex; width: 100%; align-items: center; gap: 12px; min-height: 38px; padding: 0 12px; color: var(--color-fg-2); text-align: left; border-radius: 2px; }
  .destination :global(svg) { opacity: .75; }
  .destination, .collection { transition: background-color var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .destination:hover, .collection:hover { background: rgb(255 255 255 / 4%); color: var(--color-fg); }
  .destination[aria-current], .collection[aria-current] { background: rgb(255 255 255 / 7%); color: var(--color-fg); font-weight: 600; }
  /* Pressed: one step brighter, at once. */
  .destination:active, .collection:active, .settings-link:active { background: rgb(255 255 255 / 10%); transition-duration: 0s; }
  .destination:focus-visible, .collection:focus-visible { outline-offset: -2px; }
  .section-label { padding: 0 12px; color: var(--color-muted); font-size: 12px; line-height: 20px; font-weight: 500; }
  .collection-heading { display: flex; justify-content: space-between; align-items: center; margin: 27px 0 9px; }
  .clear-game { margin-right: 12px; color: var(--color-fg-2); font-size: 12px; text-decoration: underline; text-underline-offset: 3px; }
  .collections { display: flex; flex-direction: column; gap: 2px; }
  .collection { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 48px; padding: 4px 12px; text-align: left; color: var(--color-fg-2); border-radius: 2px; }
  .cover { display: grid; place-items: center; flex: none; width: 26px; height: 36px; overflow: hidden; background: var(--color-chrome); font-size: 12px; }
  .collection-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; line-height: 20px; }
  /* Capture and Settings, under a hairline that ends the games list. */
  .sidebar-bottom { flex: none; padding: 8px 12px 12px; border-top: 1px solid var(--color-line); }
  .sidebar-utilities { display: flex; align-items: center; gap: 4px; padding-top: 4px; }
  .settings-link { display: flex; flex: 1; align-items: center; gap: 12px; min-height: 36px; padding: 0 12px; border-radius: 2px; font-size: 13px; color: var(--color-muted); transition: background-color var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out); }
  .settings-link :global(svg) { opacity: .75; }
  .settings-link:hover { background: rgb(255 255 255 / 4%); color: var(--color-fg); }
  .settings-link[aria-current] { color: var(--color-fg); }
  .settings-link:focus-visible { outline-offset: -2px; }
  .settings-navigation .section-label { margin: 26px 0 10px; }
  .category { padding-left: 12px; }
  @media (max-height: 650px) { .brand { height: 76px; } .collection-heading { margin-top: 20px; } .collection { min-height: 44px; } }
</style>
