// A muted colour from a game's own artwork, for the few bounded places a
// direction lets a game colour the app (its header, its session headings, its
// selected row). Read once per picture from a 24 × 24 copy of it: the average
// of its colourful pixels (so a dark poster with one red moon reads red, not
// grey), then pushed dark and quiet enough to put text on.
//
// Untested in the app itself: reading pixels needs the picture to be served
// with CORS. The prototype's server sends it; Tauri's asset protocol may not,
// in which case this falls back to the game's hand-picked colour, and a real
// version would compute the tint in Rust when the library is listed.
import { gameAccent } from '../../src/lib/accents'

const tints = $state<Record<string, string>>({})
const asked = new Set<string>()

/** The game's tint as `oklch(L C H)`, at a lightness meant for a surface. */
export function tint(name: string, src: string | null): string {
  if (src && !asked.has(src)) {
    asked.add(src)
    read(src).then((t) => t && (tints[name] = t))
  }
  return tints[name] ?? fallback(name)
}

/** Hand-picked or hashed hue, as the rail's initials use. */
function fallback(name: string) {
  const c = gameAccent(name)
  return `color-mix(in oklab, ${c} 34%, #101214)`
}

async function read(src: string): Promise<string | null> {
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
    const chroma = Math.min(0.07, Math.hypot(a / w, b / w) * 0.45)
    return `oklch(0.3 ${chroma.toFixed(3)} ${hue.toFixed(0)})`
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
