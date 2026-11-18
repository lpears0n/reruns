<!-- The capture command at the foot of the collection sidebar. While a game
     records: one control, Save clip with the game under it and its options
     joined on. Otherwise one quiet line that says why there is nothing to
     save. Recorder details live in the options menu; the library stays
     about the clips. -->
<script lang="ts">
  import { Check, Ellipsis } from '@lucide/svelte'
  import type { Status } from './api'
  import { gameName } from './format'
  import Popover from './Popover.svelte'
  import { ENCODER_LABEL } from './quality'

  /** What records instead, short enough to sit beside the game. */
  const SHORT = { nvenc: 'NVENC', vaapi: 'VA-API', amf: 'AMF', qsv: 'Quick Sync', mf: 'Media Foundation', x264: 'CPU' } as const

  let {
    status,
    now,
    flash = false,
    saving = false,
    hotkey,
    onsave,
    onchoosescreen,
    onignore,
    onrecording,
  }: {
    status: Status | null
    now: number
    /** A clip was just saved (by the hotkey or this control): say so. */
    flash?: boolean
    saving?: boolean
    hotkey: string
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
  const fallback = $derived(status?.encoder_fallback)
  const mode = $derived(
    status?.needs_screen ? 'screen' :
    recording ? 'recording' :
    status?.state === 'retrying' ? 'retrying' :
    status?.state === 'unavailable' ? 'unavailable' :
    'waiting'
  )
  const line = $derived(
    mode === 'screen' ? 'Choose a screen to record' :
    mode === 'retrying' ? 'Capture interrupted' :
    mode === 'unavailable' ? 'Capture unavailable' :
    status ? 'Waiting for a game' : 'Starting capture'
  )
  /** A failure's reason on one line under it: its first clause. */
  const reason = $derived(mode === 'retrying' ? 'Retrying…' : mode === 'unavailable' && status?.error ? status.error.split(/[.:;] /)[0] : '')
  /** What changed, for a screen reader. */
  const announce = $derived(recording ? `Clipping ${game ?? 'your screen'}${fallback ? `, with ${ENCODER_LABEL[fallback.using]}` : ''}` : line)
  // A long chord (Ctrl+Alt+Shift+…) is named in the tooltip only.
  const shortKey = $derived(hotkey.length <= 8)
  const label = $derived(saving ? 'saving' : flash ? 'saved' : 'save')
  const explain = $derived(fallback ? `${ENCODER_LABEL[fallback.preferred]} could not record, so Reruns is using ${ENCODER_LABEL[fallback.using]}.` : '')
</script>

{#snippet options(cls: string)}
  <Popover label="Capture options" side="top" align="end">
    {#snippet trigger(props)}
      <button {...props} class={cls} aria-label="Capture options" title="Capture options"><Ellipsis size={16} /></button>
    {/snippet}
    {#if status?.error}
      <p class="capture-explanation">{status.error}</p>
      <div class="menu-rule"></div>
    {/if}
    {#if fallback}
      <p class="capture-explanation">
        {explain}
        {fallback.set_aside ? 'The preferred encoder will be tried after Reruns restarts.' : 'The preferred encoder will be tried with the next game.'}
      </p>
      {#if fallback.reason}<p class="capture-explanation fallback-reason">{fallback.reason}</p>{/if}
      <div class="menu-rule"></div>
    {/if}
    <button class="menu-item" role="menuitem" onclick={onrecording}>Recording settings</button>
    {#if status?.needs_screen}<button class="menu-item" role="menuitem" onclick={onchoosescreen}>Choose a screen to record</button>{/if}
    {#if game}<button class="menu-item" role="menuitem" onclick={onignore}>{game} is not a game</button>{/if}
  </Popover>
{/snippet}

<section class="capture" aria-label="Capture">
  <p class="sr-only" role="status" aria-live="polite" aria-atomic="true">{announce}</p>

  {#if mode === 'recording'}
    <div class="split" class:saved={label === 'saved'}>
      <button
        class="save"
        onclick={onsave}
        disabled={buffered <= 0 || saving}
        aria-label={saving ? 'Saving clip' : 'Save clip'}
        aria-keyshortcuts={hotkey}
        title={`Save clip (${hotkey})${explain ? `\n${explain}` : ''}`}
      >
        <!-- All three labels share one cell, so the control never changes width. -->
        <span class="labels" aria-hidden="true">
          <span class:on={label === 'save'}>Save clip</span>
          <span class:on={label === 'saving'}>Saving…</span>
          <span class:on={label === 'saved'}><Check size={13} strokeWidth={2.5} />Saved</span>
        </span>
        {#if shortKey}<span class="key" aria-hidden="true">{hotkey}</span>{/if}
        <span class="game" aria-hidden="true"><span class="game-name">{game ?? 'Your screen'}</span>{#if fallback}<span class="fallback">· {SHORT[fallback.using]}</span>{/if}</span>
      </button>
      {@render options('options')}
    </div>
  {:else if mode === 'screen'}
    <div class="row">
      <button class="btn-sm choose" onclick={onchoosescreen}>Choose screen</button>
      {@render options('btn-icon-sm row-options')}
    </div>
  {:else}
    <div class="row">
      <div class="row-text" title={status?.error ?? line}>
        <p class="line" class:danger={mode === 'unavailable'}>{line}</p>
        {#if reason}<p class="reason">{reason}</p>{/if}
      </div>
      {#if mode === 'waiting' && shortKey}<span class="key" aria-hidden="true">{hotkey}</span>{/if}
      {@render options('btn-icon-sm row-options')}
    </div>
  {/if}
</section>

<style>
  .capture {
    min-width: 0;
    width: 100%;
  }

  /* One raised control in two parts: the command, and its options. */
  .split {
    display: flex;
    min-width: 0;
    height: 44px;
    border: 1px solid var(--color-line-strong);
    border-radius: var(--radius-ctl);
    background: var(--color-raised);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 5%);
    @starting-style { opacity: 0; translate: 0 4px; }
    transition: opacity var(--duration-normal) var(--ease-out), translate var(--duration-normal) var(--ease-out), border-color var(--duration-fast) var(--ease-out);
  }

  .split:hover { border-color: var(--color-faint); }
  .split.saved { border-color: var(--color-fg-2); }

  .save {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: 'label key' 'game game';
    align-content: center;
    column-gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 0 10px 0 11px;
    border-radius: calc(var(--radius-ctl) - 1px) 0 0 calc(var(--radius-ctl) - 1px);
    text-align: left;
    transition: background-color var(--duration-fast) var(--ease-out);
  }

  .save:hover:not(:disabled) { background: var(--color-hover); }
  .save:active:not(:disabled) { background: var(--color-chrome); }
  .save:focus-visible { outline-offset: -2px; }
  .save:disabled { cursor: default; }
  .save:disabled .labels { color: var(--color-muted); }

  .labels {
    display: grid;
    grid-area: label;
    font-size: var(--text-secondary);
    line-height: 18px;
    font-weight: 600;
    color: var(--color-fg);
  }

  .labels > span {
    display: inline-flex;
    grid-area: 1 / 1;
    align-items: center;
    gap: 5px;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .labels > span.on { opacity: 1; }

  .key {
    grid-area: key;
    align-self: center;
    color: var(--color-muted);
    font-size: var(--text-caption);
    line-height: 16px;
    white-space: nowrap;
  }

  .game {
    display: flex;
    grid-area: game;
    gap: 4px;
    min-width: 0;
    color: var(--color-muted);
    font-size: var(--text-caption);
    line-height: 16px;
    white-space: nowrap;
  }

  .game-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .fallback { flex: none; color: var(--color-fg-2); }

  .split :global(.options) {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 32px;
    border-left: 1px solid var(--color-line-strong);
    border-radius: 0 calc(var(--radius-ctl) - 1px) calc(var(--radius-ctl) - 1px) 0;
    color: var(--color-fg-2);
    transition: background-color var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }

  .split :global(.options:hover),
  .split :global(.options[aria-expanded='true']) { background: var(--color-hover); color: var(--color-fg); }
  .split :global(.options:active) { background: var(--color-chrome); }
  .split :global(.options:focus-visible) { outline-offset: -2px; }

  /* Not recording: one line, no dead button. */
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    min-height: 36px;
    padding-left: 12px;
    @starting-style { opacity: 0; }
    transition: opacity var(--duration-normal) var(--ease-out);
  }

  .row-text { flex: 1; min-width: 0; }

  .line,
  .reason {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .line {
    color: var(--color-fg-2);
    font-size: var(--text-secondary);
    line-height: 18px;
  }

  .line.danger { color: var(--color-danger); }

  .reason {
    color: var(--color-muted);
    font-size: var(--text-caption);
    line-height: 16px;
  }

  .row :global(.row-options) { margin-right: 2px; color: var(--color-muted); }
  .choose { flex: 1; min-width: 0; margin-left: -12px; }

  .capture-explanation {
    max-width: 300px;
    padding: 8px 10px;
    font-size: var(--text-secondary);
    line-height: 19px;
    overflow-wrap: anywhere;
  }

  .fallback-reason { padding-top: 0; color: var(--color-muted); }
</style>
