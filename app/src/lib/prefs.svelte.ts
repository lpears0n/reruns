// Appearance and sharing preferences. These only affect this machine's UI,
// so they live in localStorage rather than the recorder's settings file.

export type ThumbSize = 'small' | 'medium' | 'large'
export type Sort = 'newest' | 'oldest' | 'longest' | 'largest'
/** The accent colour (tokens.css); rainbow keeps offwhite controls and
 *  animates the rr mark and wordmark (Brand.svelte). */
export type Theme = 'offwhite' | 'blue' | 'mint' | 'violet' | 'pink' | 'rainbow'

export interface Prefs {
  theme: Theme
  /** Play a muted preview when the pointer rests on a clip card. */
  hoverPreviews: boolean
  /** Shorten or disable transitions and animations. */
  reduceMotion: boolean
  /** How wide a clip card is before the grid adds a column. */
  thumbSize: ThumbSize
  /** The navigation shows icons only. Wide windows only: a narrow one
   *  collapses it without touching this. */
  navCollapsed: boolean
  /** Show clips' tags, the tag editor and the tag filter. Off hides them;
   *  the tags themselves stay in each clip's sidecar. */
  tagging: boolean
  /** Copy clip fits the file under `shareMaxMb`. */
  shareLimit: boolean
  /** Save clip does too. */
  shareLimitSave: boolean
  /** The size to fit under, in megabytes. */
  shareMaxMb: number
  /** The library's order, and how it is grouped: by recent periods and
   *  months, by day, or (a game's clips) by play session. */
  sort: Sort
  groupBy: 'recent' | 'day' | 'session'
}

const KEY = 'reruns/prefs'

const DEFAULTS: Prefs = {
  theme: 'offwhite',
  hoverPreviews: true,
  reduceMotion: false,
  thumbSize: 'medium',
  navCollapsed: false,
  tagging: false,
  // What "Copy for Discord" always did; now a setting (Settings → Sharing).
  shareLimit: true,
  shareLimitSave: false,
  shareMaxMb: 10,
  sort: 'newest',
  groupBy: 'recent',
}

function load(): Prefs {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return { ...DEFAULTS }
    return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<Prefs>) }
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

/** Push the preferences into attributes on <html>. */
export function applyPrefs(p: Prefs) {
  const root = document.documentElement
  const systemReduce = matchMedia('(prefers-reduced-motion: reduce)').matches
  root.toggleAttribute('data-reduce-motion', p.reduceMotion || systemReduce)
  // No attribute for the default, so its look comes from :root alone.
  if (p.theme === 'offwhite') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', p.theme)
}
