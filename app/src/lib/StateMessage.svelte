<!-- Empty and error states: what is so, in a line or two, and one thing to do about it. -->
<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    title,
    body,
    error = false,
    children,
  }: {
    title: string
    body?: string
    error?: boolean
    children?: Snippet
  } = $props()
</script>

<div class="flex h-full flex-col items-center justify-center gap-2 py-16 text-center">
  <p class="text-title font-semibold {error ? 'text-danger' : ''}">{title}</p>
  {#if body}
    <p class="max-w-md text-fg-2 [overflow-wrap:anywhere]">{body}</p>
  {/if}
  {#if children}
    <div class="mt-4 flex items-center gap-2">{@render children()}</div>
  {/if}
</div>
