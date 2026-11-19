// What a prototype component needs that the app doesn't pass it: the games
// and clips (the navigation gets both; the library, cards and viewer here
// want their art and sessions too), and a way for the viewer to open another
// clip (the library, which stays mounted behind the viewer, provides it).
// shortcut: a module store instead of props, so App.svelte stays untouched; a
// real change would pass these down from App.
import type { Clip, GameEntry } from '../../src/lib/api'

export const shared = $state<{ games: GameEntry[]; clips: Clip[]; open: ((c: Clip) => void) | null }>({ games: [], clips: [], open: null })

export const gameByName = (name: string) => shared.games.find((g) => g.name === name) ?? null
