// Appearance preferences. These only affect this machine's UI, so they live
// in localStorage rather than the recorder's settings file.

import { ACCENTS, type Accent } from './accents'
export { ACCENTS, type Accent }

export type Density = 'comfortable' | 'compact'

export interface Prefs {
  accent: Accent
  /** Sidebar collapsed to icons. */
  sidebarCollapsed: boolean
  /** Play a muted preview when the pointer rests on a clip card. */
  hoverPreviews: boolean
  /** Shorten or disable transitions and animations. */
  reduceMotion: boolean
  /** Card size in the clip grids. */
  density: Density
}

const KEY = 'reruns/prefs'

const DEFAULTS: Prefs = {
  accent: 'orange',
  sidebarCollapsed: false,
  hoverPreviews: true,
  reduceMotion: false,
  density: 'comfortable',
}

function load(): Prefs {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return { ...DEFAULTS }
    const parsed = JSON.parse(raw) as Partial<Prefs>
    return {
      ...DEFAULTS,
      ...parsed,
      accent: parsed.accent && parsed.accent in ACCENTS ? parsed.accent : DEFAULTS.accent,
    }
  } catch {
    return { ...DEFAULTS }
  }
}

export const prefs = $state<Prefs>(load())

export function setPrefs(patch: Partial<Prefs>) {
  Object.assign(prefs, patch)
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs))
  } catch {
    // Private mode or blocked storage: the choice just lasts this session.
  }
}

/** Minimum card width for the clip grids, per density. */
export const CARD_MIN: Record<Density, number> = { comfortable: 300, compact: 220 }

/** Push the preferences into CSS custom properties / attributes on <html>. */
export function applyPrefs(p: Prefs) {
  const root = document.documentElement
  root.style.setProperty('--color-accent', ACCENTS[p.accent].css)
  root.style.setProperty('--card-min', `${CARD_MIN[p.density]}px`)
  const systemReduce = matchMedia('(prefers-reduced-motion: reduce)').matches
  root.toggleAttribute('data-reduce-motion', p.reduceMotion || systemReduce)
}
