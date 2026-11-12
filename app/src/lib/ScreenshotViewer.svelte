<!-- A screenshot at full size over the app, as a modal dialog: keys stay in
     it, and Esc closes it. -->
<script lang="ts">
  import { Copy, FolderOpen, Trash2, X } from '@lucide/svelte'
  import { type Screenshot, fileUrl } from './api'
  import { when } from './format'

  let {
    shot,
    onclose,
    oncopy,
    onshare,
    ondelete,
  }: {
    shot: Screenshot
    onclose: () => void
    oncopy: () => void
    onshare: () => void
    ondelete: () => void
  } = $props()

  let dialog: HTMLDialogElement
  $effect(() => dialog.showModal())
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  class="m-0 size-full max-h-none max-w-none bg-canvas p-6 text-fg"
  aria-label="{shot.game} screenshot"
  onkeydown={(e) => e.stopPropagation()}
  oncancel={(e) => {
    e.preventDefault()
    onclose()
  }}
>
  <div class="flex h-full flex-col gap-4">
    <div class="flex items-center gap-2">
      <span class="display-face truncate text-title font-semibold">{shot.game}</span>
      <span class="flex-none text-secondary text-muted tabular-nums">{when(shot.modified_ms)}</span>
      <div class="flex-1"></div>
      <button class="btn" onclick={oncopy}><Copy size={16} /> Copy</button>
      <button class="btn" onclick={onshare}><FolderOpen size={16} /> Show in folder</button>
      <button class="btn" onclick={ondelete}><Trash2 size={16} /> Delete</button>
      <button class="btn-icon ml-2" onclick={onclose} aria-label="Close" title="Close (Esc)"><X size={18} /></button>
    </div>
    <img src={fileUrl(shot.path)} alt="" class="min-h-0 flex-1 object-contain" />
  </div>
</dialog>
