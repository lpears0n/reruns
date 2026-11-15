<!-- A row of mutually exclusive options. -->
<script lang="ts" generics="T extends string">
  let {
    options,
    value,
    onchange,
    label,
  }: {
    options: { value: T; label: string; disabled?: boolean; title?: string }[]
    value: T | null
    onchange: (v: T) => void
    label: string
  } = $props()
</script>

<!-- The key slides to the chosen option (app.css `seg`). -->
<div
  class="seg inline-grid {value === null ? 'before:opacity-0' : ''}"
  style:--n={options.length}
  style:--i={Math.max(0, options.findIndex((o) => o.value === value))}
  role="radiogroup"
  aria-label={label}
>
  {#each options as o (o.value)}
    <button
      role="radio"
      aria-checked={value === o.value}
      disabled={o.disabled}
      title={o.title}
      class="h-8 rounded-[4px] px-3.5 text-secondary whitespace-nowrap transition-colors duration-fast disabled:cursor-default disabled:opacity-40
        {value === o.value ? 'text-fg' : 'text-fg-2 hover:text-fg'}"
      onclick={() => onchange(o.value)}
    >
      {o.label}
    </button>
  {/each}
</div>
