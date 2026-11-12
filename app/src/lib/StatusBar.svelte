<!-- Recorder state, in one place: the right side of the top bar. -->
<script lang="ts">
  import { FolderOpen, X } from '@lucide/svelte'
  import type { Status } from './api'
  import { ENCODER_LABEL } from './quality'

  let {
    status,
    now,
    hotkey,
    saving,
    flash,
    onsave,
    onopen,
    onchoosescreen,
    onignore,
  }: {
    status: Status | null
    /** Wall clock from the app's local tick; the buffer fill is derived from it. */
    now: number
    hotkey: string
    saving: boolean
    /** Briefly true after a clip is saved; plays the shutter flash. */
    flash: boolean
    onsave: () => void
    onopen: () => void
    onchoosescreen: () => void
    /** "Not a game": stop recording it and never detect it again. */
    onignore: () => void
  } = $props()

  const recording = $derived(status?.state === 'recording')
  // The buffer fills from the session start up to the replay length; no
  // need to ask the backend every second.
  const buffered = $derived.by(() => {
    if (!status) return 0
    const since = status.session_start_ms ? (now - status.session_start_ms) / 1000 : status.buffered_seconds
    return Math.floor(Math.min(status.replay_seconds, Math.max(status.buffered_seconds, since)))
  })
</script>

<div class="flex min-w-0 items-center gap-3">
  <button class="btn-icon" onclick={onopen} title="Open a video to trim" aria-label="Open a video to trim">
    <FolderOpen size={18} />
  </button>
  <span class="h-5 w-px bg-line-strong"></span>

  {#if recording}
    <div class="flex min-w-0 items-center gap-2.5">
      <span class="size-2 flex-none rounded-full bg-danger"></span>
      <span class="display-face max-w-64 truncate text-body font-semibold">{status?.game}</span>
      <span class="text-secondary text-muted tabular-nums">{buffered}s</span>
      {#if status?.encoder_fallback}
        <!-- The usual encoder refused (a driver out of sessions, say): say
             which one records instead, and when the usual one is tried
             again: the next game, or after a restart when it kept failing
             while recording. -->
        {@const f = status.encoder_fallback}
        {@const preferred = ENCODER_LABEL[f.preferred]}
        {@const using = ENCODER_LABEL[f.using]}
        <span
          class="whitespace-nowrap text-secondary text-fg-2"
          title={f.set_aside
            ? `${preferred} kept failing while recording (${f.reason}), so Reruns records with ${using} until it restarts.`
            : `${preferred} refused to start (${f.reason}), so this game records with ${using}. Reruns tries ${preferred} again at the next game.`}
        >
          on {ENCODER_LABEL[f.using]}
        </span>
      {/if}
      <button
        class="btn-icon size-7 text-muted"
        onclick={onignore}
        title="Not a game"
        aria-label="Not a game"
      >
        <X size={14} />
      </button>
    </div>
    <button class="btn-primary relative h-8 overflow-hidden px-3" onclick={onsave} disabled={saving}>
      Clip <kbd class="h-5 border-black/20 bg-black/15 text-on-accent">{hotkey}</kbd>
      {#if flash}
        <span class="shutter absolute inset-0 bg-white"></span>
      {/if}
    </button>
  {:else if status?.needs_screen}
    <button class="btn h-8 border-accent/60 text-accent" onclick={onchoosescreen}>Choose a screen to record</button>
  {:else if status?.state === 'retrying'}
    <!-- Capture broke mid-game; the recorder retries by itself, backing off.
         Say so, rather than "waiting for a game" while one is running. -->
    <div class="flex min-w-0 items-center gap-2.5" title={status.error ?? ''}>
      <span class="size-2 flex-none rounded-full bg-danger/70"></span>
      <span class="max-w-md truncate text-secondary text-fg-2">
        Can't record <span class="display-face font-semibold text-fg">{status.game}</span> · retrying
      </span>
    </div>
  {:else if status?.state === 'unavailable'}
    <span class="max-w-md truncate text-secondary text-danger" title={status.error ?? ''}>
      Can't capture: {status.error}
    </span>
  {:else}
    <span class="truncate text-secondary text-muted">Waiting for a game</span>
  {/if}
</div>

<style>
  .shutter {
    animation: shutter 400ms ease-out forwards;
  }

  @keyframes shutter {
    from {
      opacity: 0.85;
    }
    to {
      opacity: 0;
    }
  }
</style>
