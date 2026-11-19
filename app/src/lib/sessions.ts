// Library items grouped by the play session they were saved in. Sessions
// come from the recorder's own log (`sessions.jsonl`, `GameEntry.sessions`:
// the 50 newest per game), so nothing here is guessed: an item outside every
// logged session falls back to its day, and a session is only ever named by
// its day and clock times.
import type { GameEntry } from './api'
import { clock, dayKey, dayLabel, playedOnDay } from './format'

/** A clip saved this long after a session's logged end still belongs to it
 *  (the save is written after the press; the log is written at the end). */
const SLACK = 90_000

export interface SessionGroup<T> {
  key: string
  /** "Today", "Tuesday", "November 6". */
  label: string
  /** "6:05 – 7:20 PM", "Since 6:05 PM", or "" for a day outside any session. */
  range: string
  /** Logged play time: the session's, or for a day outside every session, that day's. */
  played: number
  /** The session is still going (the game is being played now). */
  live: boolean
  start: number
  items: T[]
}

/** Newest first; `items` keep the order they came in. */
export function bySession<T>(items: T[], at: (t: T) => number, game: GameEntry | null, now = Date.now()): SessionGroup<T>[] {
  const sessions = [...(game?.sessions ?? [])].sort((a, b) => b.start_ms - a.start_ms)
  const groups = new Map<string, SessionGroup<T>>()
  for (const it of items) {
    const t = at(it)
    const s = sessions.find((s) => t >= s.start_ms - SLACK && t <= s.end_ms + SLACK)
    const key = s ? `s${s.start_ms}` : `d${dayKey(t)}`
    let g = groups.get(key)
    if (!g) {
      const live = !!s && !!game?.playing && now - s.end_ms < 120_000
      g = s
        ? { key, label: dayLabel(s.start_ms), range: live ? `Since ${clock(s.start_ms)}` : `${clock(s.start_ms)} – ${clock(s.end_ms)}`, played: s.end_ms - s.start_ms, live, start: s.start_ms, items: [] }
        : { key, label: dayLabel(t), range: '', played: game ? playedOnDay(game.sessions, t) : 0, live: false, start: t, items: [] }
      groups.set(key, g)
    }
    g.items.push(it)
  }
  return [...groups.values()].sort((a, b) => b.start - a.start)
}

/** The games played on the local day of `dayMs`, longest first, with their
 *  logged time that day. Only what the log holds. */
export function playedThatDay(games: GameEntry[], dayMs: number): { game: GameEntry; ms: number }[] {
  const d = new Date(dayMs)
  const from = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const to = from + 86_400_000
  return games
    .map((game) => ({
      game,
      ms: game.sessions.reduce((n, s) => n + Math.max(0, Math.min(s.end_ms, to) - Math.max(s.start_ms, from)), 0),
    }))
    .filter((x) => x.ms >= 60_000)
    .sort((a, b) => b.ms - a.ms)
}
