<script lang="ts">
  import { hotkeyFromEvent } from './format'

  let {
    value,
    onbind,
  }: { value: string; onbind: (keys: string) => Promise<void> } = $props()

  let listening = $state(false)
  let error = $state<string | null>(null)

  async function onkeydown(e: KeyboardEvent) {
    if (!listening) return
    e.preventDefault()
    e.stopPropagation()
    if (e.code === 'Escape') {
      listening = false
      return
    }
    const keys = hotkeyFromEvent(e)
    if (!keys) return
    listening = false
    try {
      await onbind(keys)
      error = null
    } catch (err) {
      error = String(err)
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="flex flex-col items-end gap-2">
  <button
    class="field flex min-w-44 items-center justify-center gap-1.5 tabular-nums {listening ? 'border-accent text-accent' : ''}"
    title="Click, then press the keys"
    onclick={() => {
      listening = !listening
      error = null
    }}
    onblur={() => (listening = false)}
  >
    {#if listening}
      Press keys…
    {:else}
      {value ? value.split('+').join(' + ') : 'Not set'}
    {/if}
  </button>
  {#if error}
    <span class="text-secondary text-danger">{error}</span>
  {/if}
</div>
