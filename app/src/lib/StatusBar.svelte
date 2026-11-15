<!--
  The capture strip under the library and Settings: one dot, one sentence
  that says what the recorder is actually doing, and Save replay while there
  is a buffer to save. It is also where things are said, so nothing floats
  over a page: a notice (with its Undo) shows inside it, and while clips are
  being picked it becomes the selection's own bar. An open clip has no
  strip; the viewer says its own notices.
-->
<script lang="ts" module>
  /** Something that happened; optionally with one action. */
  export interface Notice {
    text: string
    error?: boolean
    action?: { label: string; run: () => void }
  }

  /** The clips being picked in the library, and what can be done with them. */
  export interface Selection {
    count: number
    bytes: number
    /** Every clip listed is picked. */
    all: boolean
    onall: () => void
    ondelete: () => void
    oncancel: () => void
  }
</script>

<script lang="ts">
  import { Ellipsis, Trash2, X } from '@lucide/svelte'
  import type { Status } from './api'
  import { bytes, gameName } from './format'
  import Popover from './Popover.svelte'
  import { ENCODER_LABEL } from './quality'

  let {
    status,
    now,
    hotkey,
    saving,
    flash,
    notice,
    selection,
    onsave,
    onchoosescreen,
    onignore,
    onrecording,
    ondismiss,
  }: {
    status: Status | null
    /** Wall clock from the app's local tick; the buffer fill is derived from it. */
    now: number
    hotkey: string
    saving: boolean
    /** Briefly true after a clip is saved; plays the shutter flash. */
    flash: boolean
    notice: Notice | null
    /** Set while clips are being picked: the strip shows this instead. */
    selection: Selection | null
    /** Save the replay buffer: what the hotkey does in game. */
    onsave: () => void
    onchoosescreen: () => void
    /** "Not a game": stop recording it and never detect it again. */
    onignore: () => void
    /** Open Settings → Recording. */
    onrecording: () => void
    /** Close the notice. */
    ondismiss: () => void
  } = $props()

  const recording = $derived(status?.state === 'recording')
  const game = $derived(status?.game ? gameName(status.game) : 'a game')
  // The buffer fills from the session start up to the replay length; no
  // need to ask the backend every second.
  const buffered = $derived.by(() => {
    if (!status) return 0
    const since = status.session_start_ms ? (now - status.session_start_ms) / 1000 : status.buffered_seconds
    return Math.floor(Math.min(status.replay_seconds, Math.max(status.buffered_seconds, since)))
  })

  /** Still, never animated: green is capturing, red can't, grey is idle. */
  const dot = 'size-2 flex-none rounded-full'
</script>

<footer class="flex h-strip flex-none items-center gap-3 border-t border-line bg-canvas pr-4 pl-8 text-secondary" aria-label={selection ? 'Selection actions' : 'Capture'} role={selection ? 'toolbar' : undefined}>
  {#if selection}
    <div class="swap flex min-w-0 flex-1 items-center gap-2">
      <span class="flex-none font-medium tabular-nums">{selection.count} selected</span>
      {#if selection.count}<span class="flex-none text-muted tabular-nums">{bytes(selection.bytes)}</span>{/if}
    </div>
  {:else}
    <div class="swap flex min-w-0 flex-1 items-center gap-2.5 overflow-hidden">
      {#if recording}
        <span class="{dot} bg-signal-ok"></span>
        <span class="truncate" title="{buffered} of {status?.replay_seconds} seconds ready to save">Capturing {game}</span>
        {#if status?.encoder_fallback}
          <!-- The usual encoder refused (a driver out of sessions, say): say
               which one records instead, and when the usual one is tried
               again: the next game, or after a restart when it kept failing
               while recording. -->
          {@const f = status.encoder_fallback}
          {@const preferred = ENCODER_LABEL[f.preferred]}
          {@const using = ENCODER_LABEL[f.using]}
          <span
            class="flex-none whitespace-nowrap text-muted"
            title={f.set_aside
              ? `${preferred} kept failing while recording (${f.reason}), so Reruns records with ${using} until it restarts.`
              : `${preferred} refused to start (${f.reason}), so this game records with ${using}. Reruns tries ${preferred} again at the next game.`}
          >
            on {ENCODER_LABEL[f.using]}
          </span>
        {/if}
        <Popover label="Capture options" side="top" align="start">
          {#snippet trigger(props)}
            <button {...props} class="btn-icon-sm -ml-1" aria-label="Capture options" title="Capture options">
              <Ellipsis size={16} />
            </button>
          {/snippet}
          <button class="menu-item" role="menuitem" onclick={onignore}>{game} is not a game</button>
          <button class="menu-item" role="menuitem" onclick={onrecording}>Recording settings</button>
        </Popover>
      {:else if status?.needs_screen}
        <span class="{dot} bg-warn"></span>
        <span class="truncate text-fg-2">Capture needs a screen</span>
        <button class="btn-sm" onclick={onchoosescreen}>Choose a screen to record</button>
      {:else if status?.state === 'retrying'}
        <!-- Capture broke mid-game; the recorder retries by itself, backing off.
             Say so, rather than "waiting for a game" while one is running. -->
        <span class="{dot} bg-signal-bad"></span>
        <span class="truncate" title={status.error ?? ''}>Can't capture {game}, retrying</span>
      {:else if status?.state === 'unavailable'}
        <span class="{dot} bg-signal-bad"></span>
        <span class="truncate text-fg-2" title={status.error ?? ''}>Can't capture: {status.error}</span>
      {:else}
        <span class="{dot} bg-faint"></span>
        <span class="truncate text-fg-2">Waiting for a game</span>
      {/if}
    </div>
  {/if}

  {#if notice}
    <!-- Keyed by its words: a new notice comes in, it doesn't morph. -->
    {#key notice.text}
      <div class="notice flex max-w-[55%] min-w-0 flex-initial items-center gap-1 {notice.error ? 'text-danger' : ''}" role="status" aria-live="polite">
        <span class="truncate" title={notice.text}>{notice.text}</span>
        {#if notice.action}
          <button class="btn-text" onclick={notice.action.run}>{notice.action.label}</button>
        {/if}
        <button class="btn-icon-sm text-muted" onclick={ondismiss} aria-label="Dismiss">
          <X size={15} />
        </button>
      </div>
    {/key}
  {/if}

  {#if selection}
    <button class="btn-sm" onclick={selection.onall}>{selection.all ? 'Select none' : 'Select all'}</button>
    <button class="btn-danger" disabled={!selection.count} onclick={selection.ondelete}>
      <Trash2 size={15} />
      Delete
    </button>
    <button class="btn-sm" onclick={selection.oncancel} title="Done selecting (Esc)">Done</button>
  {:else if recording}
    <!-- Only while there is a buffer to save: the hotkey's twin. -->
    <button class="btn-sm relative overflow-hidden" onclick={onsave} disabled={saving} title="Save the last {buffered} seconds ({hotkey})">
      Save replay
      {#if flash}
        <span class="shutter absolute inset-0 bg-fg"></span>
      {/if}
    </button>
  {/if}
</footer>

<style>
  /* The strip's left side changes hands (recorder, selection): a short
     fade in, nothing on the way out. */
  .swap {
    transition: opacity var(--duration-fast) ease-out;
    @starting-style {
      opacity: 0;
    }
  }

  .notice {
    transition: opacity var(--duration-normal) var(--ease-out);
    @starting-style {
      opacity: 0;
    }
  }

  /* One flash as a replay is saved; the only keyframe in the app. */
  .shutter {
    animation: shutter 400ms ease-out forwards;
  }

  @keyframes shutter {
    from {
      opacity: 0.5;
    }
    to {
      opacity: 0;
    }
  }
</style>
