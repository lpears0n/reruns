// The accent colours the Appearance settings offer. Plain data in a file
// of its own: the in-game popup page reads it too, and importing it from
// prefs.svelte.ts made that page load the whole Svelte runtime.

// OKLCH so every accent has the same lightness and a capped chroma (≤ 0.2):
// none of them glares on the dark surfaces, and dark text reads on all.
export const ACCENTS = {
  orange: { label: 'Orange', css: 'oklch(72% 0.19 50)' },
  blue: { label: 'Blue', css: 'oklch(70% 0.16 250)' },
  green: { label: 'Green', css: 'oklch(74% 0.17 155)' },
  pink: { label: 'Pink', css: 'oklch(68% 0.2 5)' },
  purple: { label: 'Purple', css: 'oklch(70% 0.15 295)' },
  yellow: { label: 'Yellow', css: 'oklch(82% 0.16 90)' },
} as const
export type Accent = keyof typeof ACCENTS
