<!-- Stands in for the clip grid while the library loads; same layout as the cards. -->
<script lang="ts">
  let { minWidth, count = 9 }: { minWidth: number; count?: number } = $props()
</script>

<div class="grid gap-x-5 gap-y-5" style:grid-template-columns="repeat(auto-fill, minmax(min(100%, {minWidth}px), 1fr))" aria-busy="true" aria-label="Loading clips">
  {#each { length: count } as _, i (i)}
    <div>
      <div class="frame aspect-video"></div>
      <div class="skeleton mt-3 h-3.5 w-1/2"></div>
      <div class="skeleton mt-2 h-3 w-1/4"></div>
    </div>
  {/each}
</div>
