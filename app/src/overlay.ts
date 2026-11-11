// The in-game popup window. The backend positions and shows it (without
// taking focus from the game) and sends what to say; this page only draws
// the card and fades it out before the window is hidden again.

import { emit, listen } from '@tauri-apps/api/event'
import './overlay.css'
import { ACCENTS, type Accent } from './lib/prefs.svelte'

interface Popup {
  kind: 'ready' | 'saved' | 'screenshot' | 'marked'
  title: string
  detail: string
  /** Hotkey to show as a key cap after the detail, if any. */
  key: string | null
  /** How long the window stays up. */
  ms: number
}

const popup = document.getElementById('popup')!
const title = document.getElementById('title')!
const detail = document.getElementById('detail')!
let fade: ReturnType<typeof setTimeout> | undefined

/** Width of the page in design units (a unit is a pixel on a 1080-line
 *  screen). Matches `WIDTH` in src-tauri/src/overlay.rs. */
const DESIGN_WIDTH = 480
/** How long the card takes to fade out before the window is hidden. */
const EXIT_MS = 220

/** The backend sizes the window for the game's resolution; zoom the page
 *  so the design fills it, sharp at any size. */
function fit() {
  document.documentElement.style.zoom = String(window.innerWidth / DESIGN_WIDTH)
}
fit()
window.addEventListener('resize', fit)

/** The user's accent and motion preference, from the main window's storage. */
function applyPrefs() {
  let accent: Accent = 'orange'
  let reduceMotion = false
  try {
    const p = JSON.parse(localStorage.getItem('reruns/prefs') ?? '{}')
    if (p.accent in ACCENTS) accent = p.accent
    reduceMotion = !!p.reduceMotion
  } catch {
    // Storage unavailable: defaults.
  }
  document.documentElement.style.setProperty('--accent', ACCENTS[accent].css)
  document.documentElement.classList.toggle('reduce-motion', reduceMotion)
}

export function show(p: Popup) {
  applyPrefs()
  popup.dataset.kind = p.kind
  title.textContent = p.title
  detail.textContent = ''
  // The key hint comes first: a long game name is what gets cut short.
  if (p.key) {
    const kbd = document.createElement('kbd')
    kbd.textContent = p.key
    detail.append('Press ', kbd, ' to clip')
    if (p.detail) detail.append(' · ')
  }
  detail.append(p.detail)
  const visible = Math.max(0, p.ms - EXIT_MS)
  popup.style.setProperty('--life', `${visible}ms`)
  // Restart the entrance even if a popup is already showing.
  popup.classList.remove('in')
  void popup.offsetWidth
  popup.classList.add('in')
  clearTimeout(fade)
  fade = setTimeout(() => popup.classList.remove('in'), visible)
}

// The window is made for the first popup, which waits until this page
// says it is listening.
listen<Popup>('overlay-show', (e) => show(e.payload)).then(() => emit('overlay-ready'))
