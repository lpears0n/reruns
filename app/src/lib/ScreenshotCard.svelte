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
    timeOf = clock,
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
    /** Its time, as much as the section heading above leaves unsaid. */
    timeOf?: (ms: number) => string
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
  <!-- Same 12px picture gap and one 20px caption line as a clip. -->
  <div class="card-caption" title="{showGame ? `${gameName(shot.game)}, ` : ''}Screenshot, {when(shot.modified_ms)}, {bytes(shot.size)}">
    <div class="card-title truncate">{showGame ? gameName(shot.game) : 'Screenshot'}</div>
    {#if showGame}<span class="caption-flag">Screenshot</span>{/if}
    <span class="caption-time text-muted tabular-nums">{timeOf(shot.modified_ms)}</span>
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
    min-width: 0;
    align-items: center;
    gap: 6px;
    height: 20px;
    margin-top: 12px;
    line-height: 20px;
  }

  .card-title { flex: 0 1 auto; min-width: 0; color: var(--color-fg-2); }
  .caption-flag { flex: none; color: var(--color-muted); font-size: var(--text-caption); }
  .caption-time { margin-left: auto; flex: none; padding-left: 8px; font-size: 12px; }
</style>
