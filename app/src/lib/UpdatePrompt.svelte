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

  // A modal <dialog>: focus moves in and stays in, and the page behind
  // can't be clicked or tabbed to.
  let dialog: HTMLDialogElement
  $effect(() => dialog.showModal())
  const close = () => progress === undefined && onclose()
</script>

<!-- Keys stop here, or the viewer behind would act on them too. A click on
     the dialog itself is a click on its backdrop. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  class="dialog-surface m-auto w-md text-fg backdrop:bg-black/60"
  aria-labelledby="update-title"
  onkeydown={(e) => e.stopPropagation()}
  oncancel={(e) => {
    e.preventDefault()
    close()
  }}
  onclick={(e) => e.target === dialog && close()}
>
  <div class="p-6">
    <div class="flex items-baseline gap-3">
      <div id="update-title" class="text-title font-semibold">Reruns {info.version}</div>
      <div class="meta">You have {info.current_version}</div>
    </div>

    {#if progress !== undefined}
      <div class="mt-6 h-1.5 overflow-hidden rounded-[3px] bg-hover" role="progressbar" aria-label="Downloading" aria-valuenow={progress == null ? undefined : Math.round(progress * 100)} aria-valuemin={0} aria-valuemax={100}>
        <div class="h-full bg-accent transition-[width] duration-normal" style:width="{(progress ?? 0.1) * 100}%"></div>
      </div>
      <div class="mt-3 flex items-center justify-between text-muted">
        <span>Downloading…</span>
        <button class="btn bg-hover" onclick={() => api.cancelUpdate()}>Cancel</button>
      </div>
    {:else if waitingFor}
      <div class="mt-4 text-fg-2">Downloaded. Installs when {waitingFor} closes.</div>
      <div class="mt-6 flex justify-end gap-2">
        <button class="btn-primary" onclick={onclose}>OK</button>
      </div>
    {:else}
      {#if notes.length}
        <div class="mt-4 max-h-48 space-y-1 overflow-y-auto pr-1 text-fg-2" aria-label="What's new">
          {#each notes as line}
            <div class={line.kind === 'heading' ? 'pt-1 font-semibold text-fg' : line.kind === 'bullet' ? 'pl-4' : ''}>
              {#if line.kind === 'bullet'}<span class="-ml-4 inline-block w-3 text-muted">•</span>{/if}{#each line.parts as part}{#if part.bold}<span class="font-semibold text-fg">{part.text}</span>{:else}{part.text}{/if}{/each}
            </div>
          {/each}
        </div>
      {/if}
      {#if error}
        <div class="mt-4 text-danger [overflow-wrap:anywhere]">{error}</div>
      {/if}
      <div class="mt-6 flex justify-end gap-2">
        <button class="btn-text mr-auto h-10 text-body font-medium text-fg-2" onclick={skip}>Skip this version</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn bg-hover" onclick={onclose} autofocus>Later</button>
        <button class="btn-primary whitespace-nowrap" onclick={install}>Update and restart</button>
      </div>
    {/if}
  </div>
</dialog>
