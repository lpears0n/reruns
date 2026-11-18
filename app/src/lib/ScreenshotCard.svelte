<!-- A screenshot shares the clip's picture, caption and interaction geometry.
     Its quiet picture mark and file size distinguish it from video. -->
<script lang="ts">
  import { Copy, Ellipsis, Image } from '@lucide/svelte'
  import { type Screenshot, thumbUrl } from './api'
  import { bytes, clock, gameName, when } from './format'
  import { dragOut } from './share'

  let {
    shot,
    showGame = true,
    copyBlocked = false,
    onopen,
    oncopy,
    ondelete,
    onmenu,
  }: {
    shot: Screenshot
    showGame?: boolean
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
<div class="screenshot-card media-card group min-w-0" oncontextmenu={menu}>
  <div class="frame media-visual aspect-video">
    <img src={thumbUrl(shot.path)} alt="" loading="lazy" decoding="async" draggable="false" class="size-full object-contain" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
    <span class="media-type absolute right-2 bottom-2 flex h-5 items-center bg-black/75 px-1.5 text-white" aria-hidden="true"><Image size={13} /></span>
    <button
      class="clip-open media-open absolute inset-0 cursor-pointer rounded-frame focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      onclick={onopen}
      draggable="true"
      ondragstart={(e) => {
        // Out of the window as a real file, like a clip.
        e.preventDefault()
        dragOut(shot.path)
      }}
      aria-label="Open {gameName(shot.game)} screenshot from {when(shot.modified_ms)}"
    ></button>
    <div class="media-tools">
      <button class="clip-copy media-tool" onclick={oncopy} disabled={copyBlocked} aria-label="Copy screenshot" title="Copy screenshot"><Copy size={14} /><span class="copy-word">Copy</span></button>
      {#if onmenu}
        <button class="media-tool media-more" onclick={more} aria-label="Screenshot actions" aria-haspopup="menu" title="Screenshot actions"><Ellipsis size={16} /></button>
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
  }

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

  .screenshot-card:is(:hover, :focus-within) .media-tools {
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
</style>
