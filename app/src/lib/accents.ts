// A colour per game: it tints the game's initial in the rail when there is
// no picture of it, and supplies ambient light until artwork can be sampled.

/** Chosen by hand; a game whose name starts with one of these gets it. */
const KNOWN: [string, string][] = [
  ['elden ring', '#D4A853'],
  ['counter-strike 2', '#E08E45'],
  ['hades ii', '#38B28B'],
  ['overwatch', '#F99E1A'],
]

/** Any other game: a hue from its name, as light and as strong as those. */
export function gameAccent(name: string): string {
  const key = name.trim().toLowerCase()
  const known = KNOWN.find(([start]) => key.startsWith(start))
  if (known) return known[1]
  let hash = 0
  for (let i = 0; i < key.length; i++) hash = (hash * 31 + key.charCodeAt(i)) >>> 0
  return `oklch(76% 0.11 ${hash % 360})`
}
