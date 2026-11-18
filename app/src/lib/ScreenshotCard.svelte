<!-- A screenshot shares the clip's picture, caption and interaction geometry.
     Its quiet picture mark and file size distinguish it from video. -->
<script lang="ts">
  import { Check, Copy, Ellipsis, Image } from '@lucide/svelte'
  import { type Screenshot, thumbUrl } from './api'
  import { bytes, clock, gameName, when } from './format'
  import { dragOut } from './share'

  let {
    shot,
    showGame = true,
    selecting = false,
    copying = false,
    copied = false,
    copyBlocked = false,
    onopen,
    oncopy,
    ondelete,
    onmenu,
  }: {
    shot: Screenshot
    showGame?: boolean
    /** Clips are being picked: a screenshot is not one of them. */
    selecting?: boolean
    copying?: boolean
    copied?: boolean
    copyBlocked?: boolean
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

  function more(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    onmenu?.(r.right, r.bottom + 4)
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (the context menu is also on the button inside, by keyboard) -->
<div class="screenshot-card media-card group min-w-0" class:set-aside={selecting} oncontextmenu={menu} data-clip-path={shot.path}>
  <div class="frame media-visual aspect-video">
    <img src={thumbUrl(shot.path)} alt="" loading="lazy" decoding="async" draggable="false" class="size-full object-contain" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
    <span class="media-type absolute right-2 bottom-2 flex h-5 items-center bg-black/75 px-1.5 text-white" aria-hidden="true"><Image size={13} /></span>
    <button
      class="clip-open media-open absolute inset-0 cursor-pointer rounded-frame focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      onclick={(e) => {
        // Picking clips (Ctrl/Shift-click, or selection mode) never opens a screenshot.
        if (selecting || e.ctrlKey || e.metaKey || e.shiftKey) return
        onopen()
      }}
      aria-disabled={selecting || undefined}
      draggable="true"
      ondragstart={(e) => {
        // Out of the window as a real file, like a clip.
        e.preventDefault()
        dragOut(shot.path)
      }}
      aria-label="Open {gameName(shot.game)} screenshot from {when(shot.modified_ms)}"
      aria-keyshortcuts="Control+C Delete"
    ></button>
    <div class="media-tools" class:shown={copying || copied}>
      <button class="clip-copy media-tool" tabindex="-1" onclick={oncopy} disabled={copyBlocked && !copying} aria-busy={copying} aria-label="Copy screenshot" title={copied ? 'Copied' : 'Copy screenshot (Ctrl+C)'}>
        {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
        <span class="copy-word"><span>{copied ? 'Copied' : 'Copy'}</span></span>
      </button>
      {#if onmenu}
        <button class="media-tool media-more" tabindex="-1" onclick={more} aria-label="Screenshot actions" aria-haspopup="menu" title="Screenshot actions"><Ellipsis size={16} /></button>
      {/if}
    </div>
  </div>
  <!-- Same 12px picture gap and 40px caption as a clip, regardless of title. -->
  <div class="card-caption">
    <div class="caption-identity">
      <div class="caption-line">
        <div class="card-title truncate" title={showGame ? gameName(shot.game) : 'Screenshot'}>{showGame ? gameName(shot.game) : 'Screenshot'}</div>
        <span class="caption-time text-muted tabular-nums">{clock(shot.modified_ms)}</span>
      </div>
      <div class="card-details text-secondary text-muted tabular-nums" title="Screenshot, {clock(shot.modified_ms)}, {bytes(shot.size)}">
        {#if showGame}<span>Screenshot</span>{/if}<span class="min-w-0 truncate">{bytes(shot.size)}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .screenshot-card {
    container-type: inline-size;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .set-aside { opacity: 0.45; }
  .set-aside .media-tools { display: none; }

  .frame { border-radius: 4px; }
  .media-open { border-radius: inherit; }
  .media-type { border-radius: 2px; }

  .card-caption {
    display: flex;
    align-items: flex-start;
    height: 40px;
    margin-top: 12px;
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

  .caption-identity {
    min-width: 0;
    flex: 1;
  }

  .card-details {
    display: flex;
    min-width: 0;
    gap: 8px;
    margin-top: 2px;
    line-height: 18px;
  }
</style>
