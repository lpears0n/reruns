<!-- A screenshot among a game's clips, laid out like ClipCard (and as tall):
     the picture opens it, the buttons beside its text copy and delete. Where
     a clip shows its length, a screenshot shows a small picture mark. -->
<script lang="ts">
  import { Copy, Image, Trash2 } from '@lucide/svelte'
  import { type Screenshot, thumbUrl } from './api'
  import { bytes, clock, gameName, when } from './format'
  import { dragOut } from './share'

  let {
    shot,
    showGame = true,
    onopen,
    oncopy,
    ondelete,
    onmenu,
  }: {
    shot: Screenshot
    showGame?: boolean
    onopen: () => void
    oncopy: () => void
    ondelete: () => void
    /** Its context menu, at this point of the window. */
    onmenu?: (x: number, y: number) => void
  } = $props()

  function menu(e: MouseEvent) {
    if (!onmenu) return
    e.preventDefault()
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const keyboard = e.clientX === 0 && e.clientY === 0
    onmenu(keyboard ? r.left + 24 : e.clientX, keyboard ? r.top + 24 : e.clientY)
  }

  const quiet = 'pointer-events-none opacity-0 transition-opacity duration-fast ease-out group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100'
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (the context menu is also on the button inside, by keyboard) -->
<div class="group min-w-0" oncontextmenu={menu}>
  <div class="frame aspect-video">
    <img src={thumbUrl(shot.path)} alt="" loading="lazy" decoding="async" draggable="false" class="size-full object-cover" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
    <span class="absolute right-2 bottom-2 flex h-5 items-center rounded-chip bg-black/75 px-1.5 text-white" aria-hidden="true"><Image size={13} /></span>
    <button
      class="clip-open absolute inset-0 cursor-pointer rounded-frame shadow-[inset_0_0_0_1px_rgb(255_255_255/0)] transition-shadow duration-fast ease-out group-hover:shadow-[inset_0_0_0_1px_rgb(255_255_255/0.18)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      onclick={onopen}
      draggable="true"
      ondragstart={(e) => {
        // Out of the window as a real file, like a clip.
        e.preventDefault()
        dragOut(shot.path)
      }}
      aria-label="Open {gameName(shot.game)} screenshot from {when(shot.modified_ms)}"
    ></button>
  </div>
  <!-- The same two lines as a clip card, in the same places. -->
  <div class="relative mt-2.5 grid grid-cols-[minmax(0,1fr)_auto] gap-x-3">
    <div class="truncate font-semibold">{showGame ? gameName(shot.game) : 'Screenshot'}</div>
    <div class="text-secondary leading-5 text-fg-2 tabular-nums">{clock(shot.modified_ms)}</div>
    <div class="flex h-4.5 min-w-0 items-center gap-3 text-secondary text-fg-2">
      {#if showGame}<span class="flex-none">Screenshot</span>{/if}
      <span class="flex-none text-muted tabular-nums">{bytes(shot.size)}</span>
    </div>
    <div class="h-4.5"></div>
    <div class="absolute top-0 right-0 bottom-0 -mr-1.5 flex items-center bg-canvas pl-2 {quiet}">
      <button class="btn-icon-sm" onclick={oncopy} title="Copy file" aria-label="Copy screenshot">
        <Copy size={16} />
      </button>
      <button class="btn-icon-sm" onclick={ondelete} title="Delete" aria-label="Delete screenshot">
        <Trash2 size={16} />
      </button>
    </div>
  </div>
</div>
