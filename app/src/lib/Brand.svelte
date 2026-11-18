<!-- The rr mark and the reruns wordmark, inline so a theme can colour them.
     The paths are those of public/brand-mark.svg and brand-wordmark.svg,
     which the in-game popup still loads; change both together. A colour
     theme tints the mark with its accent. Rainbow runs a hue across all
     eight glyphs and wiggles them in a wave, only while the window has
     focus (the app sits beside a game) and never with reduced motion. -->
<script lang="ts">
  const R = 'M0 0H13.2L20 7V13.44H12.6V8.96H7.4V28H0Z'
  const skew = (x: number, y = 3.2278456) => `matrix(1 -.12278456 0 1 ${x} ${y})`
  const small = (x: number) => `matrix(.75 -.09208842 0 .75 ${x} 3.2278456)`
  const MARK = [skew(0, 5.2278456), skew(16, 2.2278456)]
  const WORD: [transform: string, d: string][] = [
    [small(0), R],
    [skew(19), 'M5 0H15L20 5V12H5V16H20V21H5L0 16V5ZM5 5V8H15V5Z'],
    [small(43), R],
    [skew(62), 'M0 0H5V16H15V0H20V21H5L0 16Z'],
    [skew(86), 'M0 0H15L20 5V21H15V5H5V21H0Z'],
    [skew(110), 'M5 0H20V5H5V8H15L20 13V16L15 21H0V16H15V13H5L0 8V5Z'],
  ]

  let focused = $state(document.hasFocus())
</script>

<svelte:window onfocus={() => (focused = true)} onblur={() => (focused = false)} />

<span class="brand-lockup" class:still={!focused}>
  <svg class="mark" viewBox="0 0 36 34" aria-hidden="true">
    {#each MARK as t, i}<g style:--i={i}><path transform={t} d={R} /></g>{/each}
  </svg>
  <svg class="wordmark" viewBox="0 0 130 26" preserveAspectRatio="xMinYMid meet" role="img" aria-label="Reruns">
    {#each WORD as [t, d], i}<g style:--i={i + 2}><path transform={t} {d} fill-rule="evenodd" /></g>{/each}
  </svg>
</span>

<style>
  .brand-lockup { display: flex; align-items: center; gap: 12px; }
  svg { flex: none; fill: var(--color-fg); }
  .mark { width: 28px; height: 28px; }
  .wordmark { width: 124px; height: 27px; }
  :global([data-theme]) .mark { fill: var(--color-accent); }

  /* Each glyph sits 45° further round the wheel, so the eight make one
     rainbow; turning every hue up together runs it left to right. Only
     the filter and transforms animate, which left the main thread idle in
     Edge; animating `fill` instead cost about 15% of a core at 240 Hz. */
  :global([data-theme='rainbow']) svg { animation: hue 4s linear infinite; }
  :global([data-theme='rainbow']) g {
    fill: hsl(calc(var(--i) * -45) 88% 72%);
    transform-box: fill-box;
    transform-origin: 50% 100%;
    animation: wiggle 2.8s ease-in-out calc(var(--i) * 70ms) infinite;
  }
  /* It holds still without focus or with reduced motion. */
  .still svg, .still g, :global([data-reduce-motion]) svg, :global([data-reduce-motion]) g { animation: none; }
  @keyframes hue {
    to { filter: hue-rotate(360deg); }
  }
  /* A short wave across the letters, then a rest. */
  @keyframes wiggle {
    0%, 24%, 100% { transform: none; }
    6% { transform: translateY(-2px) rotate(-8deg); }
    12% { transform: translateY(-1px) rotate(6deg); }
    18% { transform: rotate(-3deg); }
  }
</style>
