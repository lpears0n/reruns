<!--
  A saved moment: the picture and its caption form one item. Actions appear
  over the picture when it is in use, leaving its identity room to breathe.
  The same actions remain reachable by keyboard and visible on touch.
-->
<script lang="ts">
  import { Check, Copy, Ellipsis, Star } from '@lucide/svelte'
  import { type Clip, fileUrl, videoUrl } from './api'
  import { clock, duration, gameName, when } from './format'
  import { prefs } from './prefs.svelte'
  import { dragOut } from './share'
  import { noteOpen } from './continuity'

  let {
    clip,
    showGame = true,
    dated = false,
    selectable = false,
    selected = false,
    copying = false,
    copied = false,
    copyBlocked = false,
    onopen,
    ondelete,
    oncopy,
    onselect,
    onmenu,
    onfavorite,
  }: {
    clip: Clip
    /** Name the game; off where the list is one game's. */
    showGame?: boolean
    /** The game's box art, beside its name where the card names it. */
    box?: string | null
    /** The grid isn't in dated sections: say the day with the time. */
    dated?: boolean
    /** Selection mode: clicking toggles selection instead of opening. */
    selectable?: boolean
    selected?: boolean
    copying?: boolean
    copied?: boolean
    copyBlocked?: boolean
    onopen: () => void
    ondelete: () => void
    /** Put the clip's file on the clipboard. */
    oncopy: () => void
    /** Pick it (Ctrl-click starts picking; `range`: Shift-click, up to here). */
    onselect?: (range: boolean) => void
    /** Its context menu, at this point of the window. */
    onmenu?: (x: number, y: number) => void
    /** Absent for clips outside the library, which can't be favorites. */
    onfavorite?: () => void
  } = $props()

  const game = $derived(gameName(clip.game))
  const time = $derived(dated ? when(clip.created_ms) : clock(clip.created_ms))
  // Its title; in a list of every game, the game; in one game's list, where
  // every card would say the same, when it was saved.
  const title = $derived(clip.title ?? (showGame ? game : time))
  /** The second line's left: the game, when the title doesn't already say it. */
  const subtitle = $derived(clip.title && showGame ? game : '')

  // Hovering scrubs the sprite sheet (ten frames, no decoding). With hover
  // previews on, a muted video takes over after the pointer rests a moment:
  // one decoder, for the one card under the pointer.
  let hovering = $state(false)
  let frame = $state(0)
  /** Where the pointer is across the picture, 0 to 1: the line along its
   *  bottom edge. */
  let across = $state(0)
  let previewing = $state(false)
  let previewTimer: ReturnType<typeof setTimeout>

  function enter() {
    hovering = true
    if (!prefs.hoverPreviews || prefs.reduceMotion) return
    clearTimeout(previewTimer)
    previewTimer = setTimeout(() => (previewing = true), 450)
  }

  function scrub(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width))
    across = x
    if (!clip.sprite) return
    frame = Math.min(clip.sprite_frames - 1, Math.floor(x * clip.sprite_frames))
    // Moving the pointer means the user is scrubbing, not waiting for video.
    clearTimeout(previewTimer)
    if (!previewing && prefs.hoverPreviews && !prefs.reduceMotion) {
      previewTimer = setTimeout(() => (previewing = true), 450)
    }
  }

  function leave() {
    clearTimeout(previewTimer)
    hovering = false
    previewing = false
  }

  let frameEl: HTMLDivElement | undefined = $state()
  function activate(e: MouseEvent) {
    if (selectable || e.ctrlKey || e.metaKey || e.shiftKey) onselect?.(e.shiftKey)
    else {
      if (frameEl) noteOpen(clip.path, frameEl)
      onopen()
    }
  }

  /** Marked moments along the picture's foot, where they are in the clip. */
  const moments = $derived(clip.duration ? clip.markers.filter((m) => m >= 0 && m <= clip.duration!).map((m) => m / clip.duration!) : [])

  function menu(e: MouseEvent) {
    if (!onmenu) return
    e.preventDefault()
    // From the keyboard there is no pointer: open it on the card.
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const keyboard = e.clientX === 0 && e.clientY === 0
    onmenu(keyboard ? r.left + 24 : e.clientX, keyboard ? r.top + 24 : e.clientY)
  }

  $effect(() => () => clearTimeout(previewTimer))

  /** The thumbnail that failed to load (its file is gone): the frame stays plain. */
  let missing = $state<string | null>(null)

  const details = $derived([subtitle, title !== time && time, clip.is_edit && 'Trimmed', moments.length && `${moments.length} marked`].filter(Boolean).join('\u2003'))
  const primaryTime = $derived(!clip.title && showGame && !dated)
  const secondaryTime = $derived(title !== time && !primaryTime)
  const detailLabel = $derived(subtitle || (clip.is_edit ? 'Trimmed' : moments.length ? `${moments.length} marked` : ''))
  const hasDetails = $derived(!!detailLabel || secondaryTime)

  function more(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    onmenu?.(r.right, r.bottom + 4)
  }
</script>

<!-- Every card is the same height: the grid counts on it (VirtualGrid). -->
<!-- svelte-ignore a11y_no_static_element_interactions (the context menu is also on the button inside, by keyboard) -->
<div class="card media-card group min-w-0" oncontextmenu={menu} data-clip-path={clip.path}>
  <!-- svelte-ignore a11y_no_static_element_interactions (hover scrubbing only; the button below is the control) -->
  <div bind:this={frameEl} class="frame media-visual aspect-video {selected ? 'is-selected outline-2 outline-offset-2 outline-accent' : ''}" onpointerenter={enter} onpointermove={scrub} onpointerleave={leave}>
    {#if clip.thumbnail && clip.thumbnail !== missing}
      <img
        src={fileUrl(clip.thumbnail)}
        alt=""
        loading="lazy"
        decoding="async"
        draggable="false"
        class="size-full object-cover"
        onerror={() => (missing = clip.thumbnail)}
      />
    {/if}

    {#if hovering && clip.sprite && !previewing}
      <div
        class="absolute inset-0 bg-cover"
        style:background-image="url({fileUrl(clip.sprite)})"
        style:background-size="{clip.sprite_frames * 100}% 100%"
        style:background-position="{(frame / (clip.sprite_frames - 1)) * 100}% 0"
        aria-hidden="true"
      ></div>
    {/if}

    {#if previewing}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        src={videoUrl(clip.path)}
        class="absolute inset-0 size-full object-cover"
        muted
        autoplay
        loop
        playsinline
        preload="metadata"
        draggable="false"
      ></video>
    {/if}

    {#if clip.duration != null}
      <!-- How long it is; nothing is shown for a clip not measured yet. -->
      <span class="media-duration absolute right-2 bottom-2 bg-black/75 px-1.5 text-caption leading-5 font-medium text-white tabular-nums" aria-hidden="true">{duration(clip.duration)}</span>
    {/if}

    <span class="ring" aria-hidden="true"></span>
    {#if selectable}
      <span class={['pick', { on: selected }]} aria-hidden="true"><Check size={14} strokeWidth={3} /></span>
    {/if}

    {#if hovering && !selectable}
      <!-- Follows the pointer across the picture, by transform. -->
      <span class="absolute inset-x-0 bottom-0 h-0.5 origin-left bg-accent" style:transform="scaleX({across})" aria-hidden="true"></span>
    {/if}

    <!-- Independent sibling buttons keep opening, copying and selecting
         distinct for pointer, keyboard and assistive technology. -->
    <button
      class="clip-open media-open absolute inset-0 cursor-pointer rounded-frame focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      onclick={activate}
      draggable={!selectable}
      ondragstart={(e) => {
        // Out of the window as a real file: drop it into a chat or a folder.
        e.preventDefault()
        dragOut(clip.path)
      }}
      aria-pressed={selectable ? selected : undefined}
      aria-label="{selectable ? 'Select' : 'Open'} {clip.title ? `${clip.title}, ` : ''}{game} clip from {when(clip.created_ms)}{clip.duration != null ? `, ${duration(clip.duration)}` : ''}{moments.length ? `, ${moments.length} marked moment${moments.length === 1 ? '' : 's'}` : ''}"
    ></button>
    {#if !selectable}
      <div class="media-tools" class:shown={copying || copied}>
        <button class="clip-copy media-tool" onclick={oncopy} disabled={copyBlocked} aria-label="Copy clip" title={copied ? 'Copied' : copying ? 'Copying…' : 'Copy clip'}>
          {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
          <span class="copy-word">{copied ? 'Copied' : copying ? 'Copying…' : 'Copy'}</span>
        </button>
        {#if onmenu}
          <button class="media-tool media-more" onclick={more} aria-label="Clip actions" aria-haspopup="menu" title="Clip actions"><Ellipsis size={16} /></button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Moments marked while playing (F7), where they fall in the clip: the
       same diamonds as on the viewer's timeline, under the picture's foot,
       where the scrub line runs. Every card keeps the room, marks or not. -->
  <div class="moments media-visual" aria-hidden="true">
    {#each moments as m, i (i)}<i style:left="{m * 100}%"></i>{/each}
  </div>
  <div class="card-caption">
    <div class="caption-identity">
      <div class="caption-line">
        <div class="card-title truncate" title={title}>{title}</div>
        {#if clip.favorite && !selectable}
          <Star size={11} fill="currentColor" class="favorite-mark flex-none text-fg-2" aria-hidden="true" />
        {/if}
        {#if primaryTime}<span class="caption-time text-muted tabular-nums">{time}</span>{/if}
      </div>
      {#if hasDetails}
        <div class="card-details text-secondary text-muted" title={details}>
          {#if detailLabel}<span class="detail-label truncate">{detailLabel}</span>{/if}
          {#if secondaryTime}<span class="detail-time truncate tabular-nums">{time}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .card {
    container-type: inline-size;
  }

  .frame { border-radius: 4px; }
  .media-open { border-radius: inherit; }
  .media-duration { border-radius: 2px; }

  .card-caption {
    display: flex;
    align-items: flex-start;
    height: 40px;
  }

  .caption-identity {
    min-width: 0;
    flex: 1;
  }

  .caption-line {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 6px;
    line-height: 20px;
  }

  .card-title { min-width: 0; font-weight: 500; }
  .caption-time { margin-left: auto; flex: none; padding-left: 8px; font-size: 12px; }

  .card-details {
    display: flex;
    min-width: 0;
    gap: 12px;
    margin-top: 2px;
    line-height: 18px;
  }

  .detail-label { min-width: 0; }
  .detail-time { min-width: 0; margin-left: auto; }
  .detail-time:only-child { margin-left: 0; }

  .media-tools {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    align-items: center;
    padding: 2px;
    border: 1px solid rgb(255 255 255 / 14%);
    border-radius: 4px;
    background: rgb(15 16 18 / 94%);
    color: #f4f4f2;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .card:is(:hover, :focus-within) .media-tools,
  .media-tools.shown {
    opacity: 1;
    pointer-events: auto;
  }

  .media-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 30px;
    padding: 0 8px;
    border-radius: 2px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .media-tool:hover { background: rgb(255 255 255 / 12%); }
  .media-tool:focus-visible { outline: 1px solid white; outline-offset: -2px; }
  .media-tool:disabled { opacity: 0.5; cursor: default; }
  .media-more { width: 30px; padding: 0; }
  .clip-copy + .media-more { border-left: 1px solid rgb(255 255 255 / 16%); border-radius: 0 2px 2px 0; }

  @media (hover: none), (pointer: coarse) {
    .media-tools { opacity: 1; pointer-events: auto; }
  }

  @media (pointer: coarse) {
    .media-tool {
      min-width: 44px;
      min-height: 44px;
    }
  }

  /* Marked moments: small white diamonds standing on the picture's foot. */
  .moments {
    position: relative;
    height: 12px;
    margin: 0 4px;
    pointer-events: none;
  }

  .moments:has(i)::before {
    content: '';
    position: absolute;
    inset: 4px 0 auto;
    height: 1px;
    background: var(--color-line-strong);
  }

  .moments i {
    position: absolute;
    top: 1px;
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--color-fg-2);
    outline: 2px solid var(--color-canvas);
    translate: -50% 0;
    rotate: 45deg;
  }

  .card:hover .moments i {
    background: var(--color-fg);
  }

  .ring {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 2px var(--color-accent);
    opacity: 0;
    pointer-events: none;
    transition: opacity 700ms var(--ease-out);
  }

  .card:global(.returned) .ring {
    opacity: 1;
    transition: none;
  }

  /* Opacity, not visibility: the card's button inside keeps its focus. */
  .card:global(.landing) .frame {
    opacity: 0;
  }

  .pick {
    position: absolute;
    top: 8px;
    left: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    background: rgb(0 0 0 / 55%);
    box-shadow: inset 0 0 0 1.5px rgb(255 255 255 / 70%);
    color: transparent;
    pointer-events: none;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .pick.on {
    background: var(--color-accent);
    box-shadow: none;
    color: var(--color-on-accent);
  }
</style>
