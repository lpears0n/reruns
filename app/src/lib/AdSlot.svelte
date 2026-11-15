<!--
  Reserved space for an ad unit. Renders nothing unless enabled, so the
  layout doesn't change for users without ads. Swap the placeholder for the
  provider's markup when monetising.
-->
<script lang="ts">
  let {
    enabled,
    width,
    height,
  }: { enabled: boolean; width: number; height: number } = $props()
</script>

{#if enabled}
  <aside
    class="flex max-w-full flex-none items-center justify-center rounded-ctl border border-line bg-chrome text-caption text-muted"
    style:width="{width}px"
    style:height="{height}px"
    aria-label="Advertisement"
  >
    Advertisement · {width}×{height}
  </aside>
{/if}
