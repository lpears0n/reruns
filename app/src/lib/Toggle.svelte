<script lang="ts">
  let {
    checked,
    onchange,
    label,
    disabled = false,
  }: { checked: boolean; onchange: (v: boolean) => void; label: string; disabled?: boolean } = $props()
</script>

<!-- A slide switch. Off: an outlined track, the key grey on the left. On:
     the accent fills it, the key dark on the right. Disabled keeps whichever
     it is, faded, so "off" and "on but unavailable" still read apart. -->
<button
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative h-5 w-9 flex-none rounded-ctl border transition-colors duration-fast disabled:cursor-default disabled:opacity-40
    {checked ? 'border-accent bg-accent hover:not-disabled:border-accent-hover hover:not-disabled:bg-accent-hover' : 'border-faint hover:not-disabled:border-fg-2'}"
  onclick={() => onchange(!checked)}
>
  <span
    class="absolute top-[3px] left-[3px] size-3 rounded-[3px] transition-[translate,background-color] duration-fast ease-out
      {checked ? 'translate-x-4 bg-on-accent' : 'bg-fg-2'}"
  ></span>
</button>
