<!-- A quiet capture command at the foot of the collection sidebar.
     Recorder details live in its menu; the library stays about the clips. -->
<script lang="ts">
  import { ChevronDown } from '@lucide/svelte'
  import type { GameEntry, Status } from './api'
  import { gameName } from './format'
  import Popover from './Popover.svelte'
  import { ENCODER_LABEL } from './quality'

  let {
    status,
    now,
    saving = false,
    hotkey,
    onsave,
    onchoosescreen,
    onignore,
    onrecording,
  }: {
    status: Status | null
    now: number
    flash?: boolean
    saving?: boolean
    hotkey: string
    games?: GameEntry[]
    onsave: () => void
    onchoosescreen: () => void
    onignore: () => void
    onrecording: () => void
  } = $props()

  const recording = $derived(status?.state === 'recording' && !status.needs_screen)
  const game = $derived(status?.game ? gameName(status.game) : null)
  // Keep availability tied to the recorder. The local clock advances an
  // active buffer between status events without adding a visible counter.
  const buffered = $derived.by(() => {
    if (!recording || !status) return 0
    const elapsed = status.session_start_ms == null ? status.buffered_seconds : (now - status.session_start_ms) / 1000
    return Math.floor(Math.max(0, Math.min(status.replay_seconds, Math.max(status.buffered_seconds, elapsed))))
  })
  const failed = $derived(!status?.needs_screen && (status?.state === 'retrying' || status?.state === 'unavailable'))
  const title = $derived(
    status?.needs_screen ? 'Choose a screen' :
    recording ? (game ?? 'Your screen') :
    status?.state === 'retrying' ? 'Capture interrupted' :
    status?.state === 'unavailable' ? 'Capture unavailable' :
    status ? 'Waiting for a game' : 'Starting capture'
  )
  const fallback = $derived(status?.encoder_fallback)
</script>

<section class="capture-control" aria-label="Capture">
  <div class="capture-heading">
    <div class="capture-identity" role="status" aria-live="polite" aria-atomic="true">
      {#if recording}<p class="capture-caption">Clipping</p>{/if}
      <p class="capture-title" class:active={recording} title={title}>{title}</p>
      {#if status?.state === 'retrying' && !status.needs_screen}<p class="capture-caption">Retrying…</p>{/if}
    </div>

    <Popover label="Capture options" side="top" align="end">
      {#snippet trigger(props)}
        <button {...props} class="btn-icon-sm capture-options" aria-label="Capture options" title="Capture options"><ChevronDown size={15} /></button>
      {/snippet}
      {#if status?.error}
        <p class="capture-explanation">{status.error}</p>
        <div class="menu-rule"></div>
      {/if}
      {#if fallback}
        <p class="capture-explanation">
          {ENCODER_LABEL[fallback.preferred]} could not record, so Reruns is using {ENCODER_LABEL[fallback.using]}.
          {fallback.set_aside ? 'The preferred encoder will be tried after Reruns restarts.' : 'The preferred encoder will be tried with the next game.'}
        </p>
        {#if fallback.reason}<p class="capture-explanation fallback-reason">{fallback.reason}</p>{/if}
        <div class="menu-rule"></div>
      {/if}
      <button class="menu-item" role="menuitem" onclick={onrecording}>Recording settings</button>
      {#if status?.needs_screen}<button class="menu-item" role="menuitem" onclick={onchoosescreen}>Choose a screen to record</button>{/if}
      {#if game}<button class="menu-item" role="menuitem" onclick={onignore}>{game} is not a game</button>{/if}
    </Popover>
  </div>

  {#if status?.needs_screen}
    <button class="btn-primary capture-action" onclick={onchoosescreen}>Choose screen</button>
  {:else if failed}
    <button class="btn-sm capture-action" onclick={onrecording}>Recording settings</button>
  {:else}
    <button
      class="btn-primary capture-action capture-save"
      onclick={onsave}
      disabled={!recording || buffered <= 0 || saving}
      aria-label={saving ? 'Saving clip' : 'Save clip'}
      title={recording ? `Save clip (${hotkey})` : `Open a game to start clipping (${hotkey})`}
    >
      <span>{saving ? 'Saving…' : 'Save clip'}</span>
      <kbd aria-hidden="true" title={hotkey}>{hotkey}</kbd>
    </button>
  {/if}
</section>

<style>
  .capture-control {
    display: grid;
    gap: 12px;
    min-width: 0;
    width: 100%;
  }

  .capture-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .capture-identity {
    flex: 1;
    min-width: 0;
  }

  .capture-caption {
    color: var(--color-muted);
    font-size: var(--text-caption);
    line-height: 18px;
  }

  .capture-title {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
    color: var(--color-fg-2);
    font-size: var(--text-secondary);
    line-height: 18px;
  }

  .capture-title.active {
    color: var(--color-fg);
    font-weight: 600;
  }

  .capture-options {
    flex: none;
    width: 28px;
    height: 28px;
    color: var(--color-muted);
  }

  .capture-action {
    width: 100%;
    min-width: 0;
    height: 36px;
    padding-inline: 12px;
    font-size: var(--text-secondary);
    font-weight: 600;
  }

  .capture-save {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    text-align: left;
  }

  .capture-save kbd {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
    font-size: var(--text-caption);
    font-weight: 400;
    color: inherit;
    opacity: 0.65;
  }

  .capture-explanation {
    max-width: 300px;
    padding: 8px 10px;
    font-size: var(--text-secondary);
    line-height: 19px;
    overflow-wrap: anywhere;
  }

  .fallback-reason { padding-top: 0; color: var(--color-muted); }
</style>
