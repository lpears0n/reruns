// Two uses of the same artwork colour: a dark tint for solid surfaces and
// a brighter colour for light cast onto the canvas. Read a 24 × 24 copy
// once per image URL; clips from one game must keep their own colours.
// Cards request their colour on first hover/focus, never while scrubbing.
// Unreadable images retain the game's fallback colour.
import { gameAccent } from './accents'

type Palette = { surface: string; light: string }
const palettes = $state<Record<string, Palette>>({})
const asked = new Set<string>()

/** The game's tint as `oklch(L C H)`, at a lightness meant for a surface. */
export function tint(name: string, src: string | null): string {
  return palette(src)?.surface ?? fallback(name)
}

/** Artwork light, bright enough to remain coloured at a low opacity. */
export function ambient(name: string, src: string | null): string {
  return palette(src)?.light ?? gameAccent(name)
}

function palette(src: string | null): Palette | undefined {
  if (src && !asked.has(src)) {
    asked.add(src)
    read(src).then((value) => {
      if (value) palettes[src] = value
    })
  }
  return src ? palettes[src] : undefined
}

/** Hand-picked or hashed hue, as the rail's initials use. */
function fallback(name: string) {
  const c = gameAccent(name)
  return `color-mix(in oklab, ${c} 34%, #101214)`
}

async function read(src: string): Promise<Palette | null> {
  const img = new Image()
  img.crossOrigin = 'anonymous'
  img.decoding = 'async'
  img.src = src
  try {
    await img.decode()
    const n = 24
    const canvas = new OffscreenCanvas(n, n)
    const ctx = canvas.getContext('2d', { willReadFrequently: true })!
    ctx.drawImage(img, 0, 0, n, n)
    const px = ctx.getImageData(0, 0, n, n).data
    let a = 0
    let b = 0
    let w = 0
    for (let i = 0; i < px.length; i += 4) {
      const [L, A, B] = oklab(px[i], px[i + 1], px[i + 2])
      const chroma = Math.hypot(A, B)
      // Colourful, not near-black or blown out, counts most.
      const weight = chroma * chroma * (L > 0.12 && L < 0.92 ? 1 : 0.1)
      a += A * weight
      b += B * weight
      w += weight
    }
    if (w < 1e-4) return null
    const hue = ((Math.atan2(b / w, a / w) * 180) / Math.PI + 360) % 360
    const chroma = Math.hypot(a / w, b / w)
    return {
      surface: `oklch(0.3 ${Math.min(0.07, chroma * 0.45).toFixed(3)} ${hue.toFixed(0)})`,
      light: `oklch(0.72 ${Math.min(0.16, Math.max(0.08, chroma * 0.9)).toFixed(3)} ${hue.toFixed(0)})`,
    }
  } catch {
    // Not decodable, or a cross-origin picture the canvas can't read.
    return null
  }
}

function oklab(r8: number, g8: number, b8: number): [number, number, number] {
  const lin = (c: number) => ((c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)
  const r = lin(r8)
  const g = lin(g8)
  const b = lin(b8)
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ]
}
