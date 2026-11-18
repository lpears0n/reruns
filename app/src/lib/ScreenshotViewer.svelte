<!-- A screenshot opened: it takes the library's place like a clip in the
     viewer, with the same header. Esc goes back. -->
<script lang="ts">
  import { ArrowLeft, Trash2, X } from '@lucide/svelte'
  import { type Screenshot, fileUrl } from './api'
  import { gameName, size, when } from './format'
  import type { Notice } from './types'

  let {
    shot,
    notice,
    copyBlocked = false,
    ondismiss,
    onclose,
    oncopy,
    onreveal,
    ondelete,
  }: {
    shot: Screenshot
    /** What just happened, said beside the actions. */
    notice: Notice | null
    copyBlocked?: boolean
    ondismiss: () => void
    onclose: () => void
    oncopy: () => void
    /** Show the file in the system's file manager. */
    onreveal: () => void
    ondelete: () => void
  } = $props()

  let root: HTMLDivElement
  let boxH = $state(0)
  $effect(() => root.focus())
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<!-- Laid out like the clip viewer: one column, a little above the middle,
     the picture at its own shape under the header. -->
<div
  bind:this={root}
  bind:clientHeight={boxH}
  class="viewer absolute inset-0 z-30 flex flex-col items-center overflow-hidden bg-canvas px-6 outline-none"
  role="dialog"
  aria-modal="true"
  aria-label="{gameName(shot.game)} screenshot"
  tabindex="-1"
>
  <div class="min-h-6 flex-1"></div>
  <div class="flex w-full max-w-(--container-viewer) flex-none flex-col">
    <header class="flex h-14 flex-none items-center gap-2">
      <button class="btn-icon -ml-2" onclick={onclose} aria-label="Back to the library" title="Back to the library (Esc)">
        <ArrowLeft size={20} />
      </button>
      <div class="min-w-0 flex-1">
        <h1 class="truncate text-heading font-semibold">{gameName(shot.game)}</h1>
        <p class="flex gap-3 text-secondary whitespace-nowrap text-fg-2 tabular-nums">
          <span>Screenshot</span><span>{when(shot.modified_ms)}</span><span class="text-muted">{size(shot.size)}</span>
        </p>
      </div>
      {#if notice}
        <div class="flex min-w-0 flex-initial items-center gap-1 text-secondary {notice.error ? 'text-danger' : ''}" role="status" aria-live="polite">
          <span class="truncate" title={notice.text}>{notice.text}</span>
          {#if notice.action}<button class="btn-text" onclick={notice.action.run}>{notice.action.label}</button>{/if}
          {#if notice.secondaryAction}<button class="btn-text" onclick={notice.secondaryAction.run}>{notice.secondaryAction.label}</button>{/if}
          {#if !notice.persistent}<button class="btn-icon-sm text-muted" onclick={ondismiss} aria-label="Dismiss"><X size={15} /></button>{/if}
        </div>
      {/if}
      <button class="btn-icon" onclick={ondelete} title="Delete" aria-label="Delete screenshot">
        <Trash2 size={18} />
      </button>
      <button class="btn" onclick={onreveal}>Show in folder</button>
      <button class="btn-primary" onclick={oncopy} disabled={copyBlocked} title="Put this screenshot on the clipboard as a file">Copy screenshot</button>
    </header>
    <div class="picture-stage mt-5 flex justify-center">
      <!-- As tall as leaves the header and the space round it in view. -->
      <img src={fileUrl(shot.path)} alt="" class="max-w-full rounded-frame object-contain" style:max-height="{Math.max(120, boxH - 56 - 20 - 48)}px" />
    </div>
  </div>
  <div class="min-h-6 flex-[2]"></div>
</div>

<style>
  .viewer {
    isolation: isolate;
    transition: opacity var(--duration-normal) var(--ease-out);
    @starting-style {
      opacity: 0;
    }
  }

  .picture-stage {
    position: relative;
  }

</style>
