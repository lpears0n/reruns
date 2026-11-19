/** Which clips the library lists: all, the favorites, or one game's. */
export type Scope = 'all' | 'starred' | 'game'

/** Something that happened; optionally with one action. */
export interface Notice {
  text: string
  error?: boolean
  /** Ongoing work keeps its progress and Cancel action visible. */
  persistent?: boolean
  /** Saved capture metadata; path lets late thumbnails fill the same notice.
   *  `src`: a picture URL to show as is (a screenshot's card-sized image). */
  media?: { path?: string; thumbnail: string | null; src?: string; game: string; duration: number | null }
  detail?: string
  /** Measured preparation progress, from zero to one. */
  progress?: number
  action?: { label: string; run: () => void; key?: string }
  secondaryAction?: { label: string; run: () => void; key?: string }
}

/** The clips being picked in the library, and what can be done with them. */
export interface Selection {
  count: number
  bytes: number
  /** Every clip listed is picked. */
  all: boolean
  onall: () => void
  ondelete: () => void
  oncancel: () => void
}
