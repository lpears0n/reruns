<script lang="ts">
  import { Check, Clapperboard, Copy, FolderOpen, Scissors, Star, Trash2 } from '@lucide/svelte'
  import { type Clip, fileUrl, videoUrl } from './api'
  import { duration, when } from './format'
  import { prefs } from './prefs.svelte'
  import { dragOut } from './share'

  let {
    clip,
    fallback = clip.game,
    subtitle = when(clip.created_ms),
    selectable = false,
    selected = false,
    onopen,
    ondelete,
    onshare,
    oncopy,
    onselect,
    onfavorite,
  }: {
    clip: Clip
    /** Shown when the clip has no title: the game name, or the time on game pages. */
    fallback?: string
    subtitle?: string
    /** Selection mode: clicking toggles selection instead of opening. */
    selectable?: boolean
    selected?: boolean
    onopen: () => void
    ondelete: () => void
    onshare: () => void
    /** Put the clip on the clipboard as a file. */
    oncopy: () => void
    onselect?: () => void
    /** Absent for clips outside the library, which can't be starred. */
    onfavorite?: () => void
  } = $props()

  const title = $derived(clip.title ?? fallback)

  // Hovering scrubs the sprite sheet (ten frames, no decoding). With hover
  // previews on, a muted video takes over after the pointer rests a moment.
  let hovering = $state(false)
  let frame = $state(0)
  let previewing = $state(false)
  let previewTimer: ReturnType<typeof setTimeout>

  function enter() {
    hovering = true
    if (!prefs.hoverPreviews || prefs.reduceMotion) return
    clearTimeout(previewTimer)
    previewTimer = setTimeout(() => (previewing = true), 450)
  }

  function scrub(e: PointerEvent) {
    if (!clip.sprite) return
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width))
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

  function activate() {
    if (selectable) onselect?.()
    else onopen()
  }

  $effect(() => () => clearTimeout(previewTimer))
</script>

<div class="group cv-auto min-w-0">
  <!-- svelte-ignore a11y_no_static_element_interactions (hover scrubbing only; the button below is the control) -->
  <div
    class="relative aspect-video overflow-hidden rounded-card bg-panel ring-1 transition-shadow duration-fast ring-inset
      {selected ? 'ring-2 ring-accent' : 'ring-line hover:ring-line-strong'}"
    onpointerenter={enter}
    onpointermove={scrub}
    onpointerleave={leave}
  >
    {#if clip.thumbnail}
      <img
        src={fileUrl(clip.thumbnail)}
        alt=""
        loading="lazy"
        draggable="false"
        class="size-full object-cover"
      />
    {:else}
      <div class="thumb-empty flex size-full items-center justify-center text-muted/50">
        <Clapperboard size={20} strokeWidth={1.6} />
      </div>
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

    <span class="absolute right-2 bottom-2 rounded-chip bg-black/70 px-1.5 py-0.5 text-caption font-medium text-white tabular-nums">
      {duration(clip.duration)}
    </span>
    {#if clip.is_edit}
      <span class="absolute bottom-2 left-2 rounded-chip bg-black/70 p-1 text-fg-2" title="Edited clip">
        <Scissors size={12} />
      </span>
    {/if}

    <!-- The whole card opens (or selects) the clip; the action buttons sit
         beside it, on top, not inside it. -->
    <button
      class="absolute inset-0 cursor-pointer rounded-card focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none focus-visible:ring-inset"
      onclick={activate}
      draggable={!selectable}
      ondragstart={(e) => {
        // Out of the window as a real file: drop it into Discord or a folder.
        e.preventDefault()
        dragOut(clip.path)
      }}
      aria-pressed={selectable ? selected : undefined}
      aria-label="{selectable ? 'Select' : 'Open'} {clip.game} clip from {when(clip.created_ms)}"
    ></button>

    {#if selectable}
      <span
        class="pointer-events-none absolute top-2 left-2 flex size-6 items-center justify-center rounded-full border-2 transition-colors duration-fast
          {selected ? 'border-accent bg-accent text-on-accent' : 'border-white/70 bg-black/50 text-transparent'}"
        aria-hidden="true"
      >
        <Check size={14} strokeWidth={3} />
      </span>
    {:else}
      {#if onfavorite}
        <button
          class="btn-scrim absolute top-2 left-2 transition-opacity duration-fast
            {clip.favorite ? 'text-accent' : 'opacity-0 group-focus-within:opacity-100 group-hover:opacity-100'}"
          onclick={onfavorite}
          title={clip.favorite ? 'Remove from favourites' : 'Add to favourites'}
          aria-label={clip.favorite ? 'Remove from favourites' : 'Add to favourites'}
          aria-pressed={clip.favorite}
        >
          <Star size={16} fill={clip.favorite ? 'currentColor' : 'none'} />
        </button>
      {/if}
      <div class="absolute top-2 right-2 flex gap-1 opacity-0 transition-opacity duration-fast group-focus-within:opacity-100 group-hover:opacity-100">
        <button
          class="btn-scrim"
          onclick={oncopy}
          title="Copy"
          aria-label="Copy clip"
        >
          <Copy size={16} />
        </button>
        <button
          class="btn-scrim"
          onclick={onshare}
          title="Show in folder"
          aria-label="Show in folder"
        >
          <FolderOpen size={16} />
        </button>
        <button class="btn-scrim" onclick={ondelete} title="Delete" aria-label="Delete">
          <Trash2 size={16} />
        </button>
      </div>
    {/if}
  </div>
  <div class="mt-2 flex items-baseline justify-between gap-3 px-0.5">
    <span class="display-face truncate text-body font-semibold" title={clip.title ? fallback : undefined}>{title}</span>
    <span class="flex-none text-caption text-muted tabular-nums">{subtitle}</span>
  </div>
  {#if clip.labels.length}
    <div class="mt-1.5 flex flex-wrap gap-1 px-0.5">
      {#each clip.labels.slice(0, 3) as l (l)}
        <span class="chip">{l}</span>
      {/each}
      {#if clip.labels.length > 3}
        <span class="chip" title={clip.labels.slice(3).join(', ')}>+{clip.labels.length - 3}</span>
      {/if}
    </div>
  {/if}
</div>
