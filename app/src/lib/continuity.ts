// Opening a clip and coming back: the card's picture travels to the player's
// place, and on the way back from the player to the card the clip is on now
// (which may be another one, after ← →), so the eye never loses its place.
//
// Neither direction waits on it: the viewer is there and takes keys at once,
// and Esc returns at once; the moving picture is a copy drawn over the page
// in its own fixed layer and removed when it lands. One element, ~200 ms,
// nothing repeated. Reduced motion skips it.
const reduced = () => document.documentElement.hasAttribute('data-reduce-motion')
const EASE = 'cubic-bezier(0.23, 1, 0.32, 1)'

/** A card being opened: its copy, already in place over the card, so the
 *  library can vanish without a blank frame before the flight starts. */
let opened: { path: string; rect: DOMRect; el: HTMLElement; timer: ReturnType<typeof setTimeout> } | null = null
/** The copy on its way into the player: Esc before it lands removes it. */
let flying: HTMLElement | null = null
/** Corners of the copy: the card's frame (tokens.css --t-radius-frame). */
const RADIUS = 4

function ghost(src: string | null, r: { left: number; top: number; width: number; height: number }, radius: number) {
  const el = document.createElement('div')
  Object.assign(el.style, {
    position: 'fixed',
    zIndex: '70',
    left: `${r.left}px`,
    top: `${r.top}px`,
    width: `${r.width}px`,
    height: `${r.height}px`,
    borderRadius: `${radius}px`,
    overflow: 'hidden',
    background: 'var(--color-chrome)',
    pointerEvents: 'none',
  })
  if (src) {
    const img = document.createElement('img')
    img.src = src
    Object.assign(img.style, { width: '100%', height: '100%', objectFit: 'cover', display: 'block' })
    el.append(img)
  }
  document.body.append(el)
  return el
}

const box = (r: DOMRect) => ({ left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` })

/** A card is being opened: put its copy over it now. */
export function noteOpen(path: string, frame: HTMLElement) {
  drop()
  if (reduced()) return
  const rect = frame.getBoundingClientRect()
  const el = ghost(frame.querySelector('img')?.currentSrc || null, rect, RADIUS)
  // The open can fail (the file is gone): don't leave the copy behind.
  opened = { path, rect, el, timer: setTimeout(drop, 1500) }
}

function drop() {
  if (!opened) return
  clearTimeout(opened.timer)
  opened.el.remove()
  opened = null
}

/** The viewer's picture is laid out: fly the card's copy into it, then hand
 *  over with a cut once the video has a frame to show (a cross-fade between
 *  a 16:9 thumbnail and a wider picture reads as a double exposure).
 *  Resolves when the real picture should show. */
export function flyIn(path: string, player: HTMLElement, video: HTMLVideoElement): Promise<void> {
  const from = opened
  if (!from || from.path !== path) {
    drop()
    return Promise.resolve()
  }
  opened = null
  clearTimeout(from.timer)
  const to = player.getBoundingClientRect()
  if (!to.width) {
    from.el.remove()
    return Promise.resolve()
  }
  const ready = new Promise<void>((done) => {
    if (video.readyState >= 2) return done()
    video.addEventListener('loadeddata', () => done(), { once: true })
    setTimeout(done, 700)
  })
  flying = from.el
  const land = () => {
    from.el.remove()
    if (flying === from.el) flying = null
  }
  const move = from.el.animate([box(from.rect), box(to)], { duration: 200, easing: EASE, fill: 'forwards' })
  return move.finished.then(() => ready).then(land, land)
}

/** Ring a card once, holding a moment before it fades (Undo brought it
 *  back). The ring is opacity only; reduced motion keeps it, just without
 *  the fade. */
export function ring(path: string) {
  const card = document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(path)}"]`)
  if (!card) return
  card.classList.add('returned')
  setTimeout(() => card.classList.remove('returned'), 700)
}

/** Back to the library from `path`, whose picture was at `from`: once the
 *  library is showing, fly to that clip's card and ring it once. */
export function flyBack(path: string, from: DOMRect | null, src: string | null) {
  drop()
  flying?.remove()
  flying = null
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      const card = document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(path)}"]`)
      if (!card) return
      const frame = card.querySelector<HTMLElement>('.frame')!
      // The app gives focus back to the card first opened; the clip on screen
      // at the end is the one to come back to.
      card.querySelector<HTMLElement>('.clip-open')?.focus({ preventScroll: true })
      frame.scrollIntoView({ block: 'nearest' })
      const to = frame.getBoundingClientRect()
      const ring = () => card.classList.add('returned')
      setTimeout(() => card.classList.remove('returned'), 700)
      if (!from || reduced()) return ring()
      const el = ghost(src ?? frame.querySelector('img')?.currentSrc ?? null, from, RADIUS)
      card.classList.add('landing')
      el.animate([box(from), box(to)], { duration: 180, easing: EASE, fill: 'forwards' }).onfinish = () => {
        card.classList.remove('landing')
        el.remove()
        ring()
      }
    }),
  )
}

/** Clips saved while the window was away (or just now, from it): their
 *  cards come in, and a line runs along each one's foot, the line that
 *  means time when scrubbing. At most four; the pointer stops it; reduced
 *  motion rings the card instead. Cards outside the grid's window are
 *  skipped. */
export function arrive(paths: string[]) {
  for (const path of paths.slice(0, 4)) {
    const card = document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(path)}"]`)
    if (!card) continue
    if (reduced()) {
      ring(path)
      continue
    }
    card.classList.remove('arrived')
    void card.offsetWidth // restart, if it was still arriving
    card.classList.add('arrived')
    const done = () => card.classList.remove('arrived')
    setTimeout(done, 1400)
    card.addEventListener('pointerenter', done, { once: true })
  }
}
