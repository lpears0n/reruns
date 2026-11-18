<!--
  The navigation down the window's left edge. In the library: everything,
  the favorites, each game, and Settings at the bottom. In Settings: Back to
  clips and Settings' own pages, so Settings has one column of navigation.
  Open it is 216 px with names; collapsed, 72 px of icons that name
  themselves under the pointer or focus. A narrow window collapses it
  without changing what the user chose.

  Rows are a list, not tiles: no surface until touched. Under the pointer a
  faint one, inset 8 px from the edges with modest corners; the current row
  a firmer one, its name in medium and a short accent tab on its inner
  left edge. Icons and names sit in the same place in every state.
-->
<script lang="ts" module>
  /** Which clips the library lists: all, the favorites, or one game's. */
  export type Scope = 'all' | 'starred' | 'game'
</script>

<script lang="ts">
  import {
    ArrowLeft,
    Download,
    HardDrive,
    Keyboard,
    LayoutGrid,
    Palette,
    PanelLeftClose,
    PanelLeftOpen,
    Settings as Cog,
    Settings2,
    Share2,
    Star,
    Video,
    Volume2,
  } from '@lucide/svelte'
  import type { Component, Snippet } from 'svelte'
  import { gameAccent } from './accents'
  import { type Clip, type GameEntry, type UpdateInfo, fileUrl } from './api'
  import Art from './Art.svelte'
  import { gameName } from './format'
  import { CATEGORIES, type Category } from './SettingsView.svelte'
  import { tint } from './tint.svelte'

  let {
    open,
    narrow,
    settings,
    category,
    scope,
    game,
    clips,
    games,
    update,
    ontoggle,
    onscope,
    onsettings,
    oncategory,
    onback,
    onupdate,
  }: {
    /** Names show beside the icons. */
    open: boolean
    /** The window is too narrow to open it: no collapse control. */
    narrow: boolean
    /** Settings is the page showing: its pages take the games' place. */
    settings: boolean
    category: Category
    scope: Scope
    /** The game listed, by name, when `scope` is 'game'. */
    game: string | null
    clips: Clip[]
    games: GameEntry[]
    /** A newer release, once one has been found. */
    update: UpdateInfo | null
    ontoggle: () => void
    onscope: (scope: Scope, game?: string) => void
    onsettings: () => void
    oncategory: (c: Category) => void
    /** From Settings, back to the library as it was left. */
    onback: () => void
    onupdate: () => void
  } = $props()

  const PAGE_ICON: Record<Category, Component> = {
    General: Settings2,
    Recording: Video,
    Audio: Volume2,
    Hotkeys: Keyboard,
    Storage: HardDrive,
    Sharing: Share2,
    Appearance: Palette,
  }

  /** A game without Steam art is shown by a frame of its newest clip. */
  const newestFrame = $derived.by(() => {
    const m = new Map<string, { at: number; src: string }>()
    for (const c of clips) {
      if (c.thumbnail && c.created_ms > (m.get(c.game)?.at ?? 0)) m.set(c.game, { at: c.created_ms, src: c.thumbnail })
    }
    return m
  })

  // One tooltip, beside whichever row the pointer or focus is on (the list
  // scrolls, so it can't be drawn inside it). Collapsed, every row has one;
  // open, only a row whose name is cut short.
  let tip = $state<{ label: string; detail: string; top: number; left: number } | null>(null)
  function show(e: Event, label: string, detail: string) {
    const row = e.currentTarget as HTMLElement
    const name = row.querySelector<HTMLElement>('.name')
    if (open && !(name && name.scrollWidth > name.clientWidth)) return
    const r = row.getBoundingClientRect()
    // Rows are inset 8 px: clear the navigation's edge by another 8.
    tip = { label, detail, top: r.top + r.height / 2, left: r.right + 16 }
  }
  const hide = () => (tip = null)
</script>

{#snippet item(label: string, current: boolean, onclick: () => void, glyph: Snippet, detail = '', hue = '')}
  <button
    class={['item', { current, tinted: current && hue }]}
    style:--tint={hue || undefined}
    aria-current={current ? 'page' : undefined}
    {onclick}
    onpointerenter={(e) => show(e, label, detail)}
    onpointerleave={hide}
    onfocus={(e) => show(e, label, detail)}
    onblur={hide}
  >
    <span class="glyph">{@render glyph()}</span>
    <span class="name min-w-0 flex-1 truncate text-left">{label}</span>
  </button>
{/snippet}

<aside class={['nav', { open }]}>
  <div class="flex h-16 flex-none items-center gap-2.5 px-5">
    {#if open || narrow}
      <img src="/favicon.svg" alt="" class="size-7 flex-none {open ? '-ml-0.5' : 'mx-auto'}" draggable="false" />
    {/if}
    {#if open}
      <span class="min-w-0 flex-1 truncate font-display text-title font-semibold">Reruns</span>
    {/if}
    {#if !narrow}
      <button class="btn-icon {open ? '-mr-2' : 'mx-auto'}" onclick={ontoggle} aria-expanded={open} aria-label={open ? 'Collapse navigation' : 'Expand navigation'} title={open ? 'Collapse navigation' : 'Expand navigation'}>
        {#if open}<PanelLeftClose size={18} />{:else}<PanelLeftOpen size={18} />{/if}
      </button>
    {/if}
  </div>

  {#if settings}
    <nav class="list flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto pb-3" aria-label="Settings pages">
      {#snippet back()}<ArrowLeft size={18} />{/snippet}
      {@render item('Back to clips', false, onback, back)}
      {#if open}
        <h2 class="section-title flex-none px-5 pt-5 pb-2">Settings</h2>
      {:else}
        <div class="mx-5 my-3 h-px flex-none bg-line-strong"></div>
      {/if}
      {#each CATEGORIES as c (c)}
        {@const Icon = PAGE_ICON[c]}
        {#snippet glyph()}<Icon size={18} />{/snippet}
        {@render item(c, category === c, () => oncategory(c), glyph)}
      {/each}
    </nav>
  {:else}
    <nav class="list flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto pb-3" aria-label="Library">
      {#snippet all()}<LayoutGrid size={18} />{/snippet}
      {#snippet star()}<Star size={18} />{/snippet}
      {@render item('All clips', scope === 'all', () => onscope('all'), all)}
      {@render item('Favorites', scope === 'starred', () => onscope('starred'), star)}

      {#if games.length}
        {#if open}
          <h2 class="section-title flex-none px-5 pt-5 pb-2">Games</h2>
        {:else}
          <div class="mx-5 my-3 h-px flex-none bg-line-strong"></div>
        {/if}
        {#each games as g (g.name)}
          {#snippet art()}
            <!-- The game's picture: Steam's, else a frame of its newest clip,
                 over its initial in its own colour.
                 shortcut: a crop of Steam's portrait capsule, use Steam's square icon once the backend lists it -->
            <span class="art" style:--c={gameAccent(g.name)} aria-hidden="true">
              {[...gameName(g.name)][0]}
              {#snippet frame()}
                {@const src = g.fallback_image ?? newestFrame.get(g.name)?.src}
                {#if src}
                  <img src={fileUrl(src)} alt="" draggable="false" class="absolute inset-0 size-full object-cover" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
                {/if}
              {/snippet}
              {#if g.art.capsule}
                <Art path={g.art.capsule} class="absolute inset-0 size-full object-cover" fallback={frame} />
              {:else}
                {@render frame()}
              {/if}
            </span>
          {/snippet}
          {@const pic = g.art.capsule ?? g.fallback_image ?? newestFrame.get(g.name)?.src ?? null}
          {@render item(gameName(g.name), scope === 'game' && game === g.name, () => onscope('game', g.name), art, g.playing ? 'Playing now' : '', tint(g.name, pic ? fileUrl(pic) : null))}
        {/each}
      {/if}
    </nav>
  {/if}

  {#if update || !settings}
  <div class="nav-footer flex flex-none flex-col py-2">
    {#if update}
      {#snippet download()}<Download size={18} class="text-accent" />{/snippet}
      {@render item(`Update to ${update.version}`, false, onupdate, download)}
    {/if}
    {#if !settings}
      {#snippet cog()}<Cog size={18} />{/snippet}
      {@render item('Settings', false, onsettings, cog)}
    {/if}
  </div>
  {/if}
</aside>

{#if tip}
  <div class="tip pop-surface" style:top="{tip.top}px" style:left="{tip.left}px" role="tooltip">
    <span>{tip.label}</span>
    {#if tip.detail}<span class="meta">{tip.detail}</span>{/if}
  </div>
{/if}

<style>
  .nav {
    display: flex;
    flex: none;
    flex-direction: column;
    width: var(--spacing-nav-collapsed);
    overflow: hidden;
    background: transparent;
    transition: width var(--duration-normal) var(--ease-out);
  }

  .nav.open {
    width: var(--spacing-nav);
  }

  .list {
    scrollbar-width: none;
  }

  .nav-footer {
    position: relative;
  }

  .nav-footer::before {
    content: '';
    position: absolute;
    top: 0;
    left: 20px;
    right: 20px;
    height: 1px;
    background: color-mix(in srgb, var(--color-line) 65%, transparent);
  }

  /* 40 px rows, inset 8 px from both edges (56 px wide folded). The glyph
     sits 36 px from the window's edge open or folded, so folding moves
     nothing but the names. */
  .item {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
    height: 40px;
    margin: 0 8px;
    padding: 0 12px;
    border-radius: var(--radius-ctl);
    color: var(--color-fg-2);
    white-space: nowrap;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .item:hover {
    background: rgb(255 255 255 / 4%);
    color: var(--color-fg);
  }

  .item:active {
    background: rgb(255 255 255 / 8%);
  }

  /* Where you are: a firmer surface, the name in medium, and the one
     indicator: a short tab of the accent on the selection's inner edge. */
  .item.current {
    background: linear-gradient(90deg, var(--color-raised), color-mix(in srgb, var(--color-raised) 28%, transparent));
    color: var(--color-fg);
    font-weight: 500;
  }

  .item.current::before {
    content: '';
    position: absolute;
    left: 0;
    top: 12px;
    bottom: 12px;
    width: 3px;
    border-radius: 0 2px 2px 0;
    background: var(--color-accent);
  }

  /* Its own ring, inside the row, so it reads apart from the current one. */
  .item:focus-visible {
    outline-offset: -2px;
  }

  .glyph {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
  }

  /* No picture: the game's initial in its colour, on a dark tile of it. */
  .art {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 32px;
    overflow: hidden;
    border-radius: 3px;
    background: color-mix(in oklab, var(--c) 22%, var(--color-raised));
    color: var(--c);
    font-size: 12px;
    font-weight: 600;
  }

  .art::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 12%);
  }

  /* The game being listed: its row takes the game's own colour. */
  .item.tinted {
    background: linear-gradient(90deg, color-mix(in oklab, var(--tint) 75%, var(--color-chrome)), color-mix(in oklab, var(--tint) 32%, transparent));
  }

  .item.tinted .art::after {
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 30%);
  }

  .tip {
    position: fixed;
    z-index: 60;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 10px;
    border-radius: var(--radius-ctl);
    white-space: nowrap;
    translate: 0 -50%;
    pointer-events: none;
  }
</style>
