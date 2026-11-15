<!--
  The window's own strip, for a window without the system frame (Windows):
  32 px across the top that drags the window (double-click maximizes) and
  ends in minimize / maximize / close. It holds nothing of the app's, so
  window chrome and content never share a bar. Renders nothing when the
  window keeps its own decorations (Linux), so the frame is never doubled.
  Close goes through the normal close request: hide to tray or quit, as set.
-->
<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'

  const win = getCurrentWindow()
  // What it was last time, so the strip is there from the first paint.
  const KEY = 'reruns/framed'
  let framed = $state(localStorage.getItem(KEY) !== 'false')
  let maximized = $state(false)

  $effect(() => {
    let unlisten: (() => void) | undefined
    let alive = true
    ;(async () => {
      try {
        framed = Boolean(await win.isDecorated())
        localStorage.setItem(KEY, String(framed))
        if (framed || !alive) return
        maximized = Boolean(await win.isMaximized())
        unlisten = await win.onResized(async () => {
          maximized = Boolean(await win.isMaximized())
        })
      } catch {
        framed = true
      }
    })()
    return () => {
      alive = false
      unlisten?.()
    }
  })
</script>

{#if !framed}
  <div class="flex h-chrome flex-none bg-chrome" data-tauri-drag-region>
  <div class="flex-1" data-tauri-drag-region></div>
  <div class="flex flex-none" role="group" aria-label="Window">
    <button class="caption" onclick={() => win.minimize()} aria-label="Minimize" title="Minimize">
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" /></svg>
    </button>
    <button
      class="caption"
      onclick={() => win.toggleMaximize()}
      aria-label={maximized ? 'Restore' : 'Maximize'}
      title={maximized ? 'Restore' : 'Maximize'}
    >
      {#if maximized}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
          <rect x="0.5" y="2.5" width="7" height="7" rx="1" /><path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
        </svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor"><rect x="0.5" y="0.5" width="9" height="9" rx="1" /></svg>
      {/if}
    </button>
    <button class="caption caption-close" onclick={() => win.close()} aria-label="Close" title="Close">
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" stroke="currentColor"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" /></svg>
    </button>
  </div>
  </div>
{/if}

<style>
  /* Sized and coloured like Windows 11's own caption buttons. */
  .caption {
    display: flex;
    width: 46px;
    align-items: center;
    justify-content: center;
    color: var(--color-fg-2);
    transition: background-color var(--duration-fast), color var(--duration-fast);
  }
  .caption:hover {
    background: var(--color-hover);
    color: var(--color-fg);
  }
  .caption:active {
    background: var(--color-raised);
  }
  .caption-close:hover {
    background: #c42b1c;
    color: #fff;
  }
  .caption-close:active {
    background: #b22a1b;
  }
  .caption:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }
</style>
