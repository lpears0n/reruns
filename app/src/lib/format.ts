/** `m:ss` */
export function duration(seconds: number | null): string {
  if (seconds == null) return '–:––'
  const s = Math.round(seconds)
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

/** `m:ss.cc`, for the trimmer. */
export function timecode(seconds: number): string {
  const cs = Math.max(0, Math.round(seconds * 100))
  const m = Math.floor(cs / 6000)
  const s = Math.floor((cs % 6000) / 100)
  return `${m}:${String(s).padStart(2, '0')}.${String(cs % 100).padStart(2, '0')}`
}

/** Seconds from what a person types: `7`, `7.5`, `1:07`, `1:07.50`. Null when it isn't a time. */
export function parseTime(text: string): number | null {
  const m = /^\s*(?:(\d+):)?(\d+(?:[.,]\d+)?)\s*$/.exec(text)
  if (!m) return null
  const t = Number(m[1] ?? 0) * 60 + Number(m[2].replace(',', '.'))
  return Number.isFinite(t) ? t : null
}

/** How much is kept, said plainly: "7.0 seconds", "1:12". */
export function span(seconds: number): string {
  return seconds < 60 ? `${seconds.toFixed(1)} second${seconds.toFixed(1) === '1.0' ? '' : 's'}` : duration(seconds)
}

// A game is shown by the name people call it. The name on disk and in the
// sidecars (Steam's, or the executable's) is never changed: this is display
// only. Nothing is re-cased by rule, which would break acronyms and names
// that are capitals on purpose; a name that needs it gets an entry here.
const ALIASES: Record<string, string> = {
  "tom clancy's rainbow six siege": 'Rainbow Six Siege',
  "tom clancy's rainbow six siege x": 'Rainbow Six Siege X',
  "playerunknown's battlegrounds": 'PUBG: Battlegrounds',
  'elden ring': 'Elden Ring',
  astroneer: 'Astroneer',
}

/** The name to show for a game; trademark signs dropped. */
export function gameName(name: string): string {
  const plain = name.replace(/[®™©]/g, '').trim()
  return ALIASES[plain.toLowerCase().replace(/’/g, "'")] ?? plain
}

/** `2026-10-28 15:04` */
export function date(ms: number): string {
  const d = new Date(ms)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

/** Label for an audio track of one of our clips (desktop first, then mic). */
export function trackName(i: number): string {
  return ['Game', 'Mic'][i] ?? `Track ${i + 1}`
}

/** Settings hotkey string (`Ctrl+Shift+F8`) from a key event, or null for
 *  modifier-only presses and unsupported keys. */
export function hotkeyFromEvent(e: KeyboardEvent): string | null {
  const c = e.code
  let key: string | null = null
  if (/^Key[A-Z]$/.test(c)) key = c.slice(3)
  else if (/^Digit[0-9]$/.test(c)) key = c.slice(5)
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(c)) key = c
  else {
    const named: Record<string, string> = {
      Space: 'Space',
      Insert: 'Insert',
      Delete: 'Delete',
      Home: 'Home',
      End: 'End',
      PageUp: 'PageUp',
      PageDown: 'PageDown',
      Pause: 'Pause',
      PrintScreen: 'Print',
      ScrollLock: 'ScrollLock',
      ArrowUp: 'Up',
      ArrowDown: 'Down',
      ArrowLeft: 'Left',
      ArrowRight: 'Right',
    }
    key = named[c] ?? null
  }
  if (!key) return null
  const mods = [
    e.ctrlKey && 'Ctrl',
    e.altKey && 'Alt',
    e.shiftKey && 'Shift',
    e.metaKey && 'Meta',
  ].filter(Boolean)
  return [...mods, key].join('+')
}

/** Decimal units, like file managers and drive labels. */
export function bytes(n: number): string {
  if (n >= 1e12) return `${(n / 1e12).toFixed(2)} TB`
  if (n >= 1e9) return `${(n / 1e9).toFixed(n >= 1e11 ? 0 : 1)} GB`
  if (n >= 1e6) return `${(n / 1e6).toFixed(0)} MB`
  return `${(n / 1e3).toFixed(0)} KB`
}

/** Fraction of the storage budget in use: of the limit, or of what the
 *  library could grow to on this drive when unlimited. */
export function storageFraction(u: { clips_bytes: number; disk_free: number; limit_bytes: number | null }): number {
  const cap = u.limit_bytes ?? u.clips_bytes + u.disk_free
  return cap > 0 ? Math.min(1, u.clips_bytes / cap) : 0
}

/** File size; same units as `bytes`. */
export const size = (n: number) => bytes(n)

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

/** "2 minutes ago", "yesterday". */
export function ago(ms: number, now = Date.now()): string {
  const s = (ms - now) / 1000
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ['year', 31536000],
    ['month', 2592000],
    ['week', 604800],
    ['day', 86400],
    ['hour', 3600],
    ['minute', 60],
  ]
  for (const [unit, secs] of units) {
    if (Math.abs(s) >= secs) return rtf.format(Math.round(s / secs), unit)
  }
  return 'just now'
}

/** "1h 20m", "2h", "45m", "<1m". */
export function playtime(ms: number): string {
  const m = Math.floor(ms / 60000)
  if (m < 1) return '<1m'
  const h = Math.floor(m / 60)
  if (h && m % 60 === 0) return `${h}h`
  return h ? `${h}h ${String(m % 60).padStart(2, '0')}m` : `${m}m`
}

/** Local-day key and label: "Today", "Yesterday", "Tuesday", "November 6". */
export function dayKey(ms: number): string {
  const d = new Date(ms)
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
}

export function dayLabel(ms: number, now = new Date()): string {
  const d = new Date(ms)
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const days = Math.round((start(now) - start(d)) / 86400000)
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return d.toLocaleDateString(undefined, { weekday: 'long' })
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' })
}

const daysAgo = (ms: number, now: Date) => {
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  return Math.round((start(now) - start(new Date(ms))) / 86400000)
}

/** A coarse period, for a library that gains a few clips a day: Today,
 *  Yesterday, Past week, then each month ("November", "October 2025"). */
export function period(ms: number, now = new Date()): { key: string; label: string } {
  const days = daysAgo(ms, now)
  if (days <= 0) return { key: 'today', label: 'Today' }
  if (days === 1) return { key: 'yesterday', label: 'Yesterday' }
  if (days < 7) return { key: 'week', label: 'Past week' }
  const d = new Date(ms)
  return { key: `${d.getFullYear()}-${d.getMonth()}`, label: d.toLocaleDateString(undefined, { month: 'long', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' }) }
}

/** When, inside its period: "15:58" today and yesterday, "Mon 15:58" in
 *  the past week, "Nov 3" further back. */
export function stamp(ms: number, now = new Date()): string {
  const days = daysAgo(ms, now)
  if (days <= 1) return clock(ms)
  const d = new Date(ms)
  if (days < 7) return `${d.toLocaleDateString(undefined, { weekday: 'short' })} ${clock(ms)}`
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

/** When something happened, as people say it: "Today, 15:58",
 *  "Yesterday, 09:12", "Tuesday, 21:40", "November 3, 18:05". */
export function when(ms: number, now = new Date()): string {
  return `${dayLabel(ms, now)}, ${clock(ms)}`
}

/** Time of day as the locale writes it: "15:58", "4:29 PM". */
export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
}

/** Play time within the local day of `dayMs`, from sessions overlapping it. */
export function playedOnDay(sessions: { start_ms: number; end_ms: number }[], dayMs: number): number {
  const d = new Date(dayMs)
  const from = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const to = from + 86400000
  return sessions.reduce((n, s) => n + Math.max(0, Math.min(s.end_ms, to) - Math.max(s.start_ms, from)), 0)
}
