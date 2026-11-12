<!-- A screenshot on a game page, laid out like ClipCard: the card opens the
     image, the buttons on it copy, show and delete. -->
<script lang="ts">
  import { Copy, FolderOpen, Trash2 } from '@lucide/svelte'
  import { type Screenshot, thumbUrl } from './api'
  import { clock, when } from './format'
  import { dragOut } from './share'

  let {
    shot,
    onopen,
    oncopy,
    onshare,
    ondelete,
  }: {
    shot: Screenshot
    onopen: () => void
    oncopy: () => void
    onshare: () => void
    ondelete: () => void
  } = $props()
</script>

<div class="group cv-auto min-w-0">
  <div class="relative aspect-video overflow-hidden rounded-card bg-panel ring-1 ring-line ring-inset hover:ring-line-strong">
    <img src={thumbUrl(shot.path)} alt="" loading="lazy" decoding="async" draggable="false" class="size-full object-cover" />
    <button
      class="absolute inset-0 cursor-pointer rounded-card focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none focus-visible:ring-inset"
      onclick={onopen}
      draggable="true"
      ondragstart={(e) => {
        // Out of the window as a real file, like a clip.
        e.preventDefault()
        dragOut(shot.path)
      }}
      aria-label="Open {shot.game} screenshot from {when(shot.modified_ms)}"
    ></button>
    <div class="absolute top-2 right-2 flex gap-1 opacity-0 transition-opacity duration-fast group-focus-within:opacity-100 group-hover:opacity-100">
      <button class="btn-scrim" onclick={oncopy} title="Copy" aria-label="Copy screenshot">
        <Copy size={16} />
      </button>
      <button class="btn-scrim" onclick={onshare} title="Show in folder" aria-label="Show in folder">
        <FolderOpen size={16} />
      </button>
      <button class="btn-scrim" onclick={ondelete} title="Delete" aria-label="Delete screenshot">
        <Trash2 size={16} />
      </button>
    </div>
  </div>
  <div class="mt-2 flex items-baseline justify-between gap-3 px-0.5">
    <span class="display-face truncate text-body font-semibold">{clock(shot.modified_ms)}</span>
    <span class="flex-none text-caption text-muted">Screenshot</span>
  </div>
</div>
