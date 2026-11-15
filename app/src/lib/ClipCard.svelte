<!--
  A clip in the grid: its own frame, 16:9, with only its length in a corner.
  Under it two lines with fixed places, nothing joined by dots: what it is
  called (the user's title, else the game, else when it was saved) with the
  time it was saved on the right; then the game when the title is the
  user's. On hover or focus the actions cover the right-hand side of those
  lines (nothing moves); a favorite keeps its star there. Moving the pointer
  across the picture scrubs it.
-->
<script lang="ts">
  import { Check, Copy, Star, Trash2 } from '@lucide/svelte'
  import { type Clip, fileUrl, videoUrl } from './api'
  import { clock, duration, gameName, when } from './format'
  import { prefs } from './prefs.svelte'
  import { dragOut } from './share'

  let {
    clip,
    showGame = true,
    dated = false,
    selectable = false,
    selected = false,
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
    /** The grid isn't in dated sections: say the day with the time. */
    dated?: boolean
    /** Selection mode: clicking toggles selection instead of opening. */
    selectable?: boolean
    selected?: boolean
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

  function activate(e: MouseEvent) {
    if (selectable || e.ctrlKey || e.metaKey || e.shiftKey) onselect?.(e.shiftKey)
    else onopen()
  }

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

  /** There while the card is pointed at or focused; never moves anything. */
  const quiet = 'pointer-events-none opacity-0 transition-opacity duration-fast ease-out group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100'
</script>

<!-- Every card is the same height: the grid counts on it (VirtualGrid). -->
<!-- svelte-ignore a11y_no_static_element_interactions (the context menu is also on the button inside, by keyboard) -->
<div class="group min-w-0" oncontextmenu={menu}>
  <!-- svelte-ignore a11y_no_static_element_interactions (hover scrubbing only; the button below is the control) -->
  <div class="frame aspect-video {selected ? 'outline-2 outline-offset-2 outline-accent' : ''}" onpointerenter={enter} onpointermove={scrub} onpointerleave={leave}>
    {#if clip.thumbnail && clip.thumbnail !== missing}
      <img
        src={fileUrl(clip.thumbnail)}
        alt=""
        loading="lazy"
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
      <span class="absolute right-2 bottom-2 rounded-chip bg-black/75 px-1.5 text-caption leading-5 font-medium text-white tabular-nums" aria-hidden="true">{duration(clip.duration)}</span>
    {/if}

    {#if hovering && !selectable}
      <!-- Follows the pointer across the picture, by transform. -->
      <span class="absolute inset-x-0 bottom-0 h-0.5 origin-left bg-accent" style:transform="scaleX({across})" aria-hidden="true"></span>
    {/if}

    <!-- The whole picture opens (or selects) the clip; the action buttons
         are under the picture, not on it. Pointed at, the card answers with
         a hairline round the picture: nothing grows, nothing moves. -->
    <button
      class="clip-open absolute inset-0 cursor-pointer rounded-frame shadow-[inset_0_0_0_1px_rgb(255_255_255/0)] transition-shadow duration-fast ease-out group-hover:shadow-[inset_0_0_0_1px_rgb(255_255_255/0.18)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      onclick={activate}
      draggable={!selectable}
      ondragstart={(e) => {
        // Out of the window as a real file: drop it into a chat or a folder.
        e.preventDefault()
        dragOut(clip.path)
      }}
      aria-pressed={selectable ? selected : undefined}
      aria-label="{selectable ? 'Select' : 'Open'} {clip.title ? `${clip.title}, ` : ''}{game} clip from {when(clip.created_ms)}{clip.duration != null ? `, ${duration(clip.duration)}` : ''}"
    ></button>
  </div>

  <div class="relative mt-2.5 grid grid-cols-[minmax(0,1fr)_auto] gap-x-3">
    <div class="truncate font-semibold" title={title}>{title}</div>
    <div class="text-secondary leading-5 text-fg-2 tabular-nums">{title === time ? '' : time}</div>
    <div class="flex h-4.5 min-w-0 items-center gap-3 text-secondary text-fg-2">
      {#if subtitle}<span class="truncate">{subtitle}</span>{/if}
      {#if clip.is_edit}<span class="flex-none text-muted">Trimmed</span>{/if}
    </div>
    <div class="flex h-4.5 items-center justify-end">
      {#if selectable}
        <span
          class="flex size-5 flex-none items-center justify-center rounded-chip border transition-colors duration-fast
            {selected ? 'border-accent bg-accent text-on-accent' : 'border-line-strong text-transparent'}"
          aria-hidden="true"
        >
          <Check size={14} strokeWidth={3} />
        </span>
      {:else if clip.favorite && onfavorite}
        <Star size={15} fill="currentColor" class="mr-1.5 text-fg-2 transition-opacity duration-fast group-focus-within:opacity-0 group-hover:opacity-0" aria-hidden="true" />
      {/if}
    </div>
    {#if !selectable}
      <!-- Over the right of both lines, on the page's own colour: what it
           covers is the time, still in the button's label and the menu. -->
      <div class="actions absolute top-0 right-0 bottom-0 -mr-1.5 flex items-center bg-canvas pl-2 {quiet}">
        <button class="btn-icon-sm" onclick={oncopy} title="Copy file" aria-label="Copy file">
          <Copy size={16} />
        </button>
        <button class="btn-icon-sm" onclick={ondelete} title="Delete" aria-label="Delete">
          <Trash2 size={16} />
        </button>
        {#if onfavorite}
          <button
            class="btn-icon-sm {clip.favorite ? 'text-fg' : ''}"
            onclick={onfavorite}
            title={clip.favorite ? 'Remove from favorites' : 'Add to favorites'}
            aria-label={clip.favorite ? 'Remove from favorites' : 'Add to favorites'}
            aria-pressed={clip.favorite}
          >
            <Star size={16} fill={clip.favorite ? 'currentColor' : 'none'} />
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
