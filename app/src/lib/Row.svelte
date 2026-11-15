<!-- A settings row: what it is on the left in regular weight, its control
     on the right. Rows sit together by spacing; no rule under each. A
     disabled row greys its words; the control shows its own disabled state. -->
<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    title,
    description,
    disabled = false,
    children,
  }: { title: string; description?: string; disabled?: boolean; children?: Snippet } = $props()
</script>

<div class="flex min-h-12 items-center gap-6 py-2">
  <div class="min-w-0 flex-1 {disabled ? 'text-muted' : ''}">
    <div class={disabled ? '' : 'text-fg'}>{title}</div>
    {#if description}
      <div class="text-secondary [overflow-wrap:anywhere] {disabled ? '' : 'text-fg-2'}">{description}</div>
    {/if}
  </div>
  {#if children}
    <div class="flex flex-none items-center gap-2">
      {@render children()}
    </div>
  {/if}
</div>
