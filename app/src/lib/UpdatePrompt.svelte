<!--
  Asks whether to install a new version: once at start-up when one is out,
  and whenever the header's update icon is clicked. "Later" closes it until
  the next launch. Installing downloads, then restarts into the new version.
-->
<script lang="ts" module>
  const SKIP_KEY = 'reruns/skip-update'

  /** The version the user asked not to be prompted about again. */
  export function skippedUpdate(): string | null {
    try {
      return localStorage.getItem(SKIP_KEY)
    } catch {
      return null
    }
  }
</script>

<script lang="ts">
  import { Download } from '@lucide/svelte'
  import { noteLines } from './notes'
  import { api, onUpdateProgress, type UpdateInfo } from './api'

  let { info, onclose }: { info: UpdateInfo; onclose: () => void } = $props()

  /** undefined: not installing; null: size unknown; else 0..1. */
  let progress = $state<number | null | undefined>(undefined)
  let error = $state<string | null>(null)
  /** The game whose recording the downloaded update is waiting on. */
  let waitingFor = $state<string | null>(null)

  async function install() {
    error = null
    progress = null
    const stop = await onUpdateProgress((f) => (progress = f))
    try {
      // Restarts into the new version, or waits for the game to close.
      waitingFor = await api.installUpdate()
    } catch (e) {
      error = String(e)
    } finally {
      progress = undefined
      stop()
    }
  }

  const notes = $derived(info.notes ? noteLines(info.notes) : [])

  /** No prompt for this version again; the header icon still offers it. */
  function skip() {
    try {
      localStorage.setItem(SKIP_KEY, info.version)
    } catch {
      // Storage unavailable: it is only skipped for now.
    }
    onclose()
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && progress === undefined) onclose()
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-60 flex items-center justify-center bg-black/60" onclick={() => progress === undefined && onclose()}>
  <div
    class="floating w-md p-5"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="update-title"
    tabindex="-1"
  >
    <div class="flex items-center gap-3">
      <span class="flex size-9 flex-none items-center justify-center rounded-full bg-accent/15 text-accent">
        <Download size={18} />
      </span>
      <div class="min-w-0">
        <div id="update-title" class="display-face text-title font-semibold">Reruns {info.version}</div>
        <div class="text-secondary text-muted">You have {info.current_version}</div>
      </div>
    </div>

    {#if progress !== undefined}
      <div class="mt-5 h-1.5 overflow-hidden rounded-full bg-line-strong" role="progressbar" aria-label="Downloading" aria-valuenow={progress == null ? undefined : Math.round(progress * 100)} aria-valuemin={0} aria-valuemax={100}>
        <div class="h-full rounded-full bg-accent transition-[width] duration-normal" style:width="{(progress ?? 0.1) * 100}%"></div>
      </div>
      <div class="mt-2 flex items-center justify-between text-secondary text-muted">
        <span>Downloading…</span>
        <button class="btn" onclick={() => api.cancelUpdate()}>Cancel</button>
      </div>
    {:else if waitingFor}
      <div class="mt-4 text-secondary text-fg-2">Downloaded. Installs when {waitingFor} closes.</div>
      <div class="mt-5 flex justify-end gap-2">
        <button class="btn-primary" onclick={onclose}>OK</button>
      </div>
    {:else}
      {#if notes.length}
        <div class="mt-4 max-h-48 space-y-1 overflow-y-auto pr-1 text-secondary text-fg-2" aria-label="What's new">
          {#each notes as line}
            <div class={line.kind === 'heading' ? 'pt-1 font-semibold text-fg' : line.kind === 'bullet' ? 'pl-3' : ''}>
              {#if line.kind === 'bullet'}<span class="-ml-3 inline-block w-3 text-muted">•</span>{/if}{#each line.parts as part}{#if part.bold}<span class="font-semibold text-fg">{part.text}</span>{:else}{part.text}{/if}{/each}
            </div>
          {/each}
        </div>
      {/if}
      {#if error}
        <div class="mt-4 text-secondary text-danger">{error}</div>
      {/if}
      <div class="mt-5 flex justify-end gap-2">
        <button class="btn mr-auto whitespace-nowrap" onclick={skip}>Skip this version</button>
        <button class="btn" onclick={onclose}>Later</button>
        <button class="btn-primary whitespace-nowrap" onclick={install}>Update and restart</button>
      </div>
    {/if}
  </div>
</div>
