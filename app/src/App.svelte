<script lang="ts">
  import { getCurrentWebview } from '@tauri-apps/api/webview'
  import { open as openDialog } from '@tauri-apps/plugin-dialog'
  import { onMount, tick } from 'svelte'
  import AdSlot from './lib/AdSlot.svelte'
  import {
    api,
    type Capabilities,
    type Clip,
    type ClipPatch,
    type GameEntry,
    initMedia,
    thumbUrl,
    onClipSaved,
    onScreenshotSaved,
    onMarked,
    onUpdateAvailable,
    onLibraryChanged,
    onSettingsChanged,
    onRenderProgress,
    onStatusChanged,
    onThumbnailReady,
    type Screenshot,
    type Settings,
    type Status,
    type UpdateInfo,
    type Usage,
  } from './lib/api'
  import { retryFailedArt } from './lib/Art.svelte'
  import { readLibraryCache, writeLibraryCache } from './lib/cache'
  import ClipsView from './lib/ClipsView.svelte'
  import CaptureControl from './lib/CaptureControl.svelte'
  import CaptureNotice from './lib/CaptureNotice.svelte'
  import LibraryNav from './lib/LibraryNav.svelte'
  import { coalesce } from './lib/coalesce'
  import { arrive, ring } from './lib/continuity'
  import { gameName as gameLabel } from './lib/format'
  import ClipStudio, { type Edit } from './lib/ClipStudio.svelte'
  import { applyPrefs, prefs } from './lib/prefs.svelte'
  import SettingsView, { type Category } from './lib/SettingsView.svelte'
  import type { Scope } from './lib/types'
  import StateMessage from './lib/StateMessage.svelte'
  import type { Notice } from './lib/types'
  import ScreenshotViewer from './lib/ScreenshotViewer.svelte'
  import UpdatePrompt, { skippedUpdate } from './lib/UpdatePrompt.svelte'
  import WindowControls from './lib/WindowControls.svelte'

  type View = 'library' | 'settings'
  /** The number keys switch between the pages. */
  const KEYS: Record<string, View> = { '1': 'library', '2': 'settings' }
  const VIDEO = /\.(mp4|mkv|mov|webm)$/i
  /** How long a deleted clip can be brought back from the toast. */
  const UNDO_MS = 6000

  let view = $state<View>('library')
  /** The active library view. */
  let scope = $state<Scope>('all')
  /** An optional game filter, also applied within Favorites. */
  let gameName = $state<string | null>(null)
  /** Clip open in the studio, in place of the workspace. */
  let viewing = $state<Clip | null>(null)
  /** The clips as the library listed them when one was opened: Left and
   *  Right in the studio step through these. */
  let queue = $state.raw<string[]>([])
  /** The mounted library owns its selection and scroll position. */
  let library = $state<ReturnType<typeof ClipsView>>()
  let category = $state<Category>('General')

  let status = $state<Status | null>(null)
  let settings = $state<Settings | null>(null)
  /** A newer release, found by the background check or Settings. */
  let newRelease = $state<UpdateInfo | null>(null)
  /** The update dialog: opened by the start-up check, or the header icon. */
  let updatePrompt = $state(false)
  function announceUpdate(u: UpdateInfo) {
    newRelease = u
    // A version the user chose to skip still shows the header icon.
    if (u.prompt && skippedUpdate() !== u.version) updatePrompt = true
  }
  let usage = $state<Usage | null>(null)
  let caps = $state<Capabilities | null>(null)
  // Raw: replaced wholesale on every refresh; a deep proxy over thousands of
  // clips is slow and unnecessary.
  let allClips = $state.raw<Clip[]>([])
  let games = $state.raw<GameEntry[]>([])
  let screenshots = $state.raw<Screenshot[]>([])
  /** Screenshot open in the viewer, over everything. */
  let shot = $state<Screenshot | null>(null)
  let loading = $state(true)
  /** Wall clock for live counters (buffer fill, "playing for"); ticks once
   *  a second while recording, otherwise once a minute. No IPC involved. */
  let now = $state(Date.now())
  /** Why the library couldn't be read, shown in place of the views. */
  let libraryError = $state<string | null>(null)
  /** What the viewer is rendering for: Copy clip, or Save clip. */
  let busy = $state<'copy' | 'save' | null>(null)
  let renderPath = $state<string | null>(null)
  let renderProgress = $state(0)
  /** One clipboard job, pinned to its source even if the user changes views. */
  let copyJob: { path: string; cancelled: boolean } | null = null
  let copyingPath = $state<string | null>(null)
  let copyPhase = $state<'preparing' | 'copying' | 'cancelling'>('copying')
  let copiedPath = $state<string | null>(null)
  const copyBlocked = $derived(!!copyingPath || !!busy)
  let copiedTimer: ReturnType<typeof setTimeout>
  let dragging = $state(false)
  let saving = $state(false)
  /** Clips to bring in once the window is in front again: saved since it
   *  was hidden (`arriveAfter`), or saved while it wasn't focused. */
  const pendingArrivals = new Set<string>()
  let hiddenAt = 0
  let arriveAfter = 0

  function landArrivals() {
    if (document.visibilityState !== 'visible' || !document.hasFocus() || covered || view !== 'library') return
    const paths = new Set(pendingArrivals)
    if (arriveAfter) for (const c of allClips) if (c.created_ms > arriveAfter && !c.is_edit) paths.add(c.path)
    pendingArrivals.clear()
    arriveAfter = 0
    if (paths.size) tick().then(() => arrive([...paths]))
  }

  /** A clip from the game was just saved: Save clip says Saved for a moment. */
  let flash = $state(false)
  let flashTimer: ReturnType<typeof setTimeout>
  let toast = $state<Notice | null>(null)
  let toastTimer: ReturnType<typeof setTimeout>
  let toastDeadline = 0
  let toastRemaining = 0
  let toastPaused = false

  // Deletes are deferred so they can be undone; the clips vanish from the
  // UI at once and are only removed from disk when the toast expires.
  // Raw state: entries are compared by identity when undone or committed,
  // which a deep proxy would break. The array is always reassigned.
  let pending = $state.raw<{ clips: (Clip | Screenshot)[]; timer: ReturnType<typeof setTimeout> }[]>([])
  const copyingGame = $derived(allClips.find((c) => c.path === copyingPath)?.game)
  const workNotice = $derived<Notice | null>(copyingPath ? {
    text: copyPhase === 'cancelling' ? 'Cancelling…' : copyPhase === 'preparing' ? `Preparing clip… ${Math.round(renderProgress * 100)}%` : 'Copying…',
    persistent: true,
    progress: copyPhase === 'preparing' ? renderProgress : undefined,
    detail: copyingGame ? gameLabel(copyingGame) : undefined,
    action: copyPhase === 'preparing' ? { label: 'Cancel', run: cancelWork } : undefined,
    // An export must not hide a deletion's time-limited way back.
    secondaryAction: toast?.action?.label === 'Undo' ? { label: pending.length ? 'Undo delete' : 'Undo', run: toast.action.run, key: 'Ctrl+Z' } : undefined,
  } : toast)
  const displayNotice = $derived.by(() => {
    const saved = workNotice?.media?.path && allClips.find((c) => c.path === workNotice.media?.path)
    return saved && workNotice ? { ...workNotice, media: { path: saved.path, thumbnail: saved.thumbnail, game: saved.game, duration: saved.duration } } : workNotice
  })
  const hidden = $derived(new Set(pending.flatMap((p) => p.clips.map((c) => c.path))))
  const clips = $derived(hidden.size ? allClips.filter((c) => !hidden.has(c.path)) : allClips)
  const shots = $derived(hidden.size ? screenshots.filter((s) => !hidden.has(s.path)) : screenshots)

  const game = $derived(gameName ? (games.find((g) => g.name === gameName) ?? null) : null)
  const hotkey = $derived(settings?.hotkey ?? 'F8')
  /** A clip or screenshot is open: the workspace behind is out of sight and reach. */
  const covered = $derived(!!viewing || !!shot)
  /** The studio's clips either side: what is still there of the list it was opened from. */
  const order = $derived(queue.filter((p) => !hidden.has(p)))
  const position = $derived.by(() => {
    const index = viewing ? order.indexOf(viewing.path) : -1
    return index < 0 ? null : { index, total: order.length }
  })

  function step(dir: 1 | -1) {
    const next = position && order[position.index + dir]
    // A render in progress belongs to the clip on screen.
    if (next && !busy) open(next)
  }

  /** A backend's reason, as a sentence. */
  const sentence = (e: unknown) => String(e).replace(/^./, (c) => c.toUpperCase())

  /** Library and preview use the same Sharing settings. Screenshots stay original. */
  async function copyClip(c: Clip | Screenshot) {
    if ('title' in c) return copyReady(c)
    if (copyBlocked) return
    copyingPath = c.path
    copyPhase = 'copying'
    try {
      await api.copyClip(c.path)
      markCopied(c.path)
      notify('Screenshot copied')
    } catch (e) {
      notify(sentence(e), true)
    } finally {
      copyingPath = null
    }
  }

  /** The card (or viewer) that was copied says Copied for a moment. */
  function markCopied(path: string) {
    copiedPath = path
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => (copiedPath = null), 1600)
  }

  function notify(text: string, error = false) {
    showToast({ text, error })
  }

  /** A save event can precede the refreshed library listing. */
  async function copySaved(path: string) {
    if (copyBlocked) return
    try {
      const saved = allClips.find((c) => c.path === path) ?? await api.openForEdit(path)
      await copyClip(saved)
    } catch (e) {
      failed('Couldn’t copy clip', e, () => copySaved(path))
    }
  }

  /** A notice that arrived while an Undo was up: shown once that closes. */
  let queued: [Notice, number] | null = null

  function showToast(t: Notice, ms = 3000) {
    // Never take a pending Undo off the screen. Another delete's Undo may
    // take a delete's place (one Undo covers them all); any other waits,
    // since the deletes' timers keep running without a way back on screen.
    const deleting = toast?.action?.run === undo && t.action?.run !== undo
    if (toast?.action?.label === 'Undo' && (t.action?.label !== 'Undo' || deleting)) {
      queued = [t, ms]
      return
    }
    toast = t
    clearTimeout(toastTimer)
    toastPaused = false
    toastRemaining = ms
    toastDeadline = Date.now() + ms
    if (!t.persistent) toastTimer = setTimeout(closeToast, ms)
  }

  // Saved results stay available while someone is using their actions.
  // Undo retains its actual deletion deadline.
  function pauseNotice() {
    if (!toast?.media || toastPaused) return
    toastRemaining = Math.max(0, toastDeadline - Date.now())
    toastPaused = true
    clearTimeout(toastTimer)
  }

  function resumeNotice() {
    if (!toastPaused) return
    toastPaused = false
    toastDeadline = Date.now() + toastRemaining
    toastTimer = setTimeout(closeToast, toastRemaining)
  }

  function closeToast() {
    clearTimeout(toastTimer)
    toastPaused = false
    toast = null
    const next = queued
    queued = null
    if (next) showToast(...next)
  }

  function go(v: View) {
    view = v
    viewing = null
  }

  /** What had focus in the library when a clip or screenshot was opened. */
  let cameFrom: HTMLElement | null = null

  /** Back to the library as it was left: same list, same scroll, same card. */
  async function closeViewer() {
    const returningPath = viewing?.path ?? shot?.path
    viewing = null
    shot = null
    await tick()
    if (cameFrom?.isConnected && cameFrom !== document.body) cameFrom.focus()
    else {
      // A notification's Open button disappears as its result opens. Return
      // to that capture if it is listed, otherwise to a stable library tab.
      const card = returningPath ? document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(returningPath)}"] .media-open`) : null
      const fallback = document.querySelector<HTMLElement>(view === 'settings' ? '.settings-navigation [aria-current]' : '.library-navigation [aria-current]')
      ;(card ?? fallback)?.focus()
    }
    cameFrom = null
  }

  /** Library navigation and game filter. */
  function list(s: Scope, name?: string | null) {
    library?.stopSelecting()
    scope = s
    gameName = name ?? null
    go('library')
  }

  // A listed game that is gone (marked "not a game"): back to everything.
  $effect(() => {
    if (gameName && !game) {
      gameName = null
      if (scope === 'game') scope = 'all'
    }
  })

  // Audio devices come and go (a headset plugged in); re-read them each
  // time Settings opens. Encoder results are cached, so this is cheap.
  $effect(() => {
    if (view === 'settings') api.capabilities().then((c) => (caps = c))
  })

  async function loadLibrary() {
    try {
      const library = await api.library()
      allClips = library.clips
      games = library.games
      screenshots = library.screenshots
      retryFailedArt()
      libraryError = null
      writeLibraryCache(allClips, games)
      landArrivals()
    } catch (e) {
      libraryError = String(e)
    } finally {
      loading = false
    }
  }

  // Saves, the folder watcher and storage enforcement can all fire within
  // the same second; they cost one fetch.
  const refreshLibrary = coalesce(loadLibrary)

  /** A clip's images and duration arrived from the thumbnail pool. */
  function thumbnailReady(c: Clip) {
    allClips = allClips.map((x) => (x.path === c.path ? { ...x, ...c, title: x.title, favorite: x.favorite, labels: x.labels } : x))
    if (viewing?.path === c.path) viewing = { ...viewing, ...c, title: viewing.title, favorite: viewing.favorite, labels: viewing.labels }
  }

  async function refreshStatus() {
    try {
      status = await api.status()
    } catch {
      // The recorder may still be starting.
    }
  }

  async function refreshUsage() {
    try {
      usage = await api.storage()
    } catch {
      // Not critical; the bar just stays as it was.
    }
  }

  async function save() {
    saving = true
    try {
      await api.save()
    } catch (e) {
      failed('Couldn’t save replay', e, save)
    } finally {
      saving = false
    }
  }

  /** `from`: the list it was opened from; a file from anywhere else has none. */
  async function open(path: string, from?: string[]) {
    try {
      const clip = await api.openForEdit(path)
      // Read now, not on the click: a menu that asked has closed by now and
      // given focus back to its card.
      if (!covered && document.activeElement instanceof HTMLElement) cameFrom = document.activeElement
      viewing = clip
      // One viewer at a time: a screenshot open in front would hide the clip.
      shot = null
      queue = from ?? (queue.includes(path) ? queue : [])
    } catch (e) {
      notify(String(e), true)
    }
  }

  async function pickVideo() {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: 'Video', extensions: ['mp4', 'mkv', 'mov', 'webm'] }],
    })
    if (typeof path === 'string') open(path)
  }

  /** The size the viewer's Copy clip (or Save clip) fits under, when
   *  Settings → Sharing asks for one. */
  const limitMb = (saving: boolean) => (prefs.shareLimit && (!saving || prefs.shareLimitSave) ? prefs.shareMaxMb : null)

  /** A failure in the viewer: says what, and offers the same thing again.
   *  The trim is the viewer's own state, so it is still there to retry. */
  function failed(what: string, e: unknown, retry: () => void) {
    if (/cancelled/.test(String(e))) return
    const run = () => {
      closeToast()
      retry()
    }
    showToast({ text: `${what}: ${e}`, error: true, action: { label: 'Try again', run } }, 15000)
  }

  /** The selection as a new file next to the original (`…_edit.mp4`), which
   *  is never written to. Rejects with the backend's reason. */
  async function render(source: string, edit: Edit, maxMb: number | null, why: 'copy' | 'save') {
    busy = why
    renderPath = source
    copiedPath = null
    renderProgress = 0
    try {
      return await api.render(source, edit.start, edit.end, edit.volumes, maxMb, edit.vertical, edit.slow)
    } finally {
      busy = null
      renderPath = null
    }
  }

  /** Copy clip, in the viewer: the clip's file onto the clipboard, to paste
   *  into a chat or a folder. The file itself when nothing was changed and
   *  it meets the size limit; otherwise a render of the selection, kept in
   *  the library like any saved clip. Says so only once it is there: the
   *  button itself says Copied, and the size it was fitted under is said
   *  beside it. `prepared`: that render, made already, when only the
   *  clipboard failed. */
  async function copyReady(clip: Clip, edit?: Edit, untouched = true, prepared?: string) {
    if (copyBlocked) return
    const job = { path: clip.path, cancelled: false }
    copyJob = job
    copyingPath = clip.path
    const maxMb = limitMb(false)
    const asIs = !prepared && untouched && (maxMb === null || clip.size_bytes <= maxMb * 1e6)
    copyPhase = asIs || prepared ? 'copying' : 'preparing'
    renderProgress = 0
    let path = prepared ?? clip.path
    // Reads `path` when called: once the render exists, Try again only copies it.
    const retry = () => copyReady(clip, edit, untouched, path === clip.path ? undefined : path)
    try {
      if (!asIs && !prepared) {
        try {
          if (!edit) {
            const source = await api.openForEdit(clip.path)
            if (job.cancelled) return
            if (!source.duration || source.duration <= 0) throw new Error("Couldn't read the clip's length")
            edit = {
              start: 0, end: source.duration,
              volumes: Array(Math.max(0, source.audio_tracks - (source.mixed_audio ? 1 : 0))).fill(1),
              vertical: null, slow: null,
            }
          }
          path = await render(clip.path, edit, maxMb, 'copy')
        } catch (e) {
          if (!job.cancelled) failed("Couldn't prepare the clip", e, retry)
          return
        }
      }
      // Cancellation also covers a render finishing just after Cancel was pressed.
      if (job.cancelled) return
      copyPhase = 'copying'
      try {
        await api.copyClip(path)
        markCopied(clip.path)
        notify(!asIs && maxMb !== null ? `Clip copied, under ${maxMb} MB` : 'Clip copied')
      } catch (e) {
        failed("Couldn't copy the clip", String(e).replace(/^couldn't copy the clip: /, ''), retry)
      }
    } finally {
      if (copyJob === job) {
        copyJob = null
        copyingPath = null
      }
    }
  }

  function copySelection(edit: Edit, untouched: boolean) {
    if (viewing) return copyReady(viewing, edit, untouched)
  }

  function cancelWork() {
    if (copyJob) {
      copyJob.cancelled = true
      copyPhase = 'cancelling'
    }
    if (busy) api.cancelRender().catch((e) => notify(sentence(e), true))
  }

  /** Save clip, in the viewer: the selection as a new clip in the library. */
  async function saveSelection(edit: Edit, source = viewing?.path) {
    if (!source || copyBlocked) return
    try {
      const path = await render(source, edit, limitMb(true), 'save')
      const run = () => {
        closeToast()
        open(path)
      }
      showToast({ text: 'Clip saved', action: { label: 'Open', run } }, UNDO_MS)
    } catch (e) {
      failed("Couldn't save the clip", e, () => saveSelection(edit, source))
    }
  }

  /** Hide the clips now; delete them for real when the undo window closes. */
  function remove(list: (Clip | Screenshot)[]) {
    if (!list.length) return
    if (viewing && list.some((c) => c.path === viewing!.path)) viewing = null
    if (shot && list.some((c) => c.path === shot!.path)) shot = null
    // One Undo covers every delete still pending, so they all get the
    // full time again.
    for (const p of pending) {
      clearTimeout(p.timer)
      p.timer = setTimeout(() => commit(p), UNDO_MS)
    }
    const entry = { clips: list, timer: setTimeout(() => commit(entry), UNDO_MS) }
    pending = [...pending, entry]
    const n = pending.reduce((sum, p) => sum + p.clips.length, 0)
    const one = list[0]
    const label = n > 1 ? `Deleted ${n} clips` : 'title' in one ? (one.title ? `Deleted “${one.title}”` : 'Clip deleted') : 'Screenshot deleted'
    showToast({ text: label, action: { label: 'Undo', run: undo, key: 'Ctrl+Z' } }, UNDO_MS)
  }

  async function undo() {
    for (const p of pending) clearTimeout(p.timer)
    const back = pending.flatMap((p) => p.clips.map((c) => c.path))
    pending = []
    closeToast()
    // The restored cards are where they were: show where, and give the first
    // the keyboard if the library had it.
    await tick()
    const first = back.map((p) => document.querySelector<HTMLElement>(`[data-clip-path="${CSS.escape(p)}"]`)).find(Boolean)
    if (!first) return
    first.scrollIntoView({ block: 'nearest' })
    const active = document.activeElement
    if (!covered && (active === document.body || active?.closest('main'))) first.querySelector<HTMLElement>('.clip-open')?.focus({ preventScroll: true })
    for (const p of back) ring(p)
  }

  async function commit(entry: (typeof pending)[number]) {
    pending = pending.filter((p) => p !== entry)
    const failed: string[] = []
    for (const c of entry.clips) {
      try {
        await api.deleteClip(c.path)
      } catch (e) {
        failed.push(String(e))
      }
    }
    if (failed.length) notify(failed[0], true)
    refreshLibrary()
    refreshUsage()
  }

  /** Only library clips carry user metadata; opened files are read-only.
   *  Either separator: Windows paths use backslashes. */
  const inLibrary = (c: Clip) => {
    if (!settings) return false
    const dir = settings.clips_dir.replace(/[\\/]+$/, '')
    return c.path.startsWith(dir) && /[\\/]/.test(c.path.charAt(dir.length))
  }

  /** Title, favourite or labels. Applies optimistically, then takes the
   *  server's tidied version. */
  async function update(clip: Clip, patch: ClipPatch) {
    const optimistic: Clip = {
      ...clip,
      title: patch.title === undefined ? clip.title : patch.title.trim() || null,
      favorite: patch.favorite ?? clip.favorite,
      labels: patch.labels ?? clip.labels,
      cover: patch.cover === undefined ? clip.cover : patch.cover < 0 ? null : patch.cover,
    }
    const put = (c: Clip) => {
      allClips = allClips.map((x) => (x.path === c.path ? c : x))
      if (viewing?.path === c.path) viewing = c
    }
    put(optimistic)
    try {
      put(await api.updateClip(clip.path, patch))
    } catch (e) {
      put(clip)
      notify(String(e), true)
    }
  }

  /** Throws so the hotkey binder can show the reason. */
  async function changeSettings(s: Settings) {
    await api.setSettings(s)
    const dirChanged = s.clips_dir !== settings?.clips_dir
    const ignoredChanged = JSON.stringify(s.ignored_games) !== JSON.stringify(settings?.ignored_games)
    settings = s
    if (dirChanged) refreshLibrary()
    else if (ignoredChanged) refreshLibrary()
    refreshUsage()
  }

  /** Undo puts the list of ignored apps back as it was. */
  function ignored(name: string, before: Settings['ignored_games']) {
    const run = () => {
      closeToast()
      if (settings) changeSettings({ ...settings, ignored_games: before }).catch((e) => notify(String(e), true))
    }
    showToast({ text: `Ignoring ${name}`, action: { label: 'Undo', run, key: 'Ctrl+Z' } }, UNDO_MS)
  }

  /** From a game's clips: hide it and never detect it again. */
  async function notGame(g: GameEntry) {
    if (!settings) return
    const before = settings.ignored_games
    try {
      games = games.filter((x) => x.name !== g.name)
      await changeSettings({ ...settings, ignored_games: [...before, { name: g.name }] })
      ignored(g.name, before)
    } catch (e) {
      notify(String(e), true)
    }
  }

  /** "Not a game": stops within a scan. */
  async function ignoreGame() {
    const name = status?.game
    const before = settings?.ignored_games ?? []
    try {
      settings = await api.ignoreCurrentGame()
      ignored(name ?? 'that app', before)
    } catch (e) {
      notify(String(e), true)
    }
  }

  async function chooseScreen() {
    try {
      await api.chooseScreen()
      refreshStatus()
    } catch (e) {
      notify(String(e), true)
    }
  }

  function openSettings(c: Category) {
    category = c
    go('settings')
  }

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement || (e.target instanceof HTMLElement && e.target.isContentEditable)
    // Undo over confirm, from the keyboard too: Ctrl+Z takes back the
    // delete (or the ignored game) the notice offers.
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.code === 'KeyZ' && !typing && toast?.action?.label === 'Undo') {
      e.preventDefault()
      toast.action.run()
      return
    }
    if (typing || covered || e.ctrlKey || e.altKey || e.metaKey) return
    if (e.key in KEYS) go(KEYS[e.key])
  }

  $effect(() => applyPrefs(prefs))

  onMount(() => {
    initMedia()
    // Paint the last known library at once; the real listing replaces it.
    const cached = readLibraryCache()
    if (cached) {
      allClips = cached.clips
      games = cached.games
      loading = false
    }
    refreshStatus()
    loadLibrary()
    refreshUsage()
    api.settings().then((s) => (settings = s))
    // Encoders are probed when Settings opens (the effect above), not
    // here: a launch into the tray would test-encode with every encoder
    // for nothing, maybe while a game is running.
    // Everything else arrives as events. Coming back to the window is the one
    // moment worth re-checking, in case an event was missed while hidden.
    // Clips saved before the window first shows are not new arrivals.
    if (document.visibilityState === 'hidden') hiddenAt = Date.now()
    const onFocus = () => {
      if (document.visibilityState !== 'visible') {
        if (!hiddenAt) hiddenAt = Date.now()
        return
      }
      if (hiddenAt) {
        arriveAfter = hiddenAt
        hiddenAt = 0
      }
      refreshStatus()
      refreshLibrary()
      refreshUsage()
    }
    window.addEventListener('focus', onFocus)
    document.addEventListener('visibilitychange', onFocus)
    const tick = setInterval(() => {
      if (document.visibilityState !== 'visible') return
      const recording = status?.state === 'recording'
      if (recording || Date.now() - now >= 60_000) now = Date.now()
    }, 1000)
    const updates = onUpdateAvailable((u) => {
      announceUpdate(u)
      // Asking marks the prompt as shown, or a window rebuilt after the
      // tray would ask again after "Later".
      if (u.prompt) api.foundUpdate()
    })
    // The first check runs as the app starts, maybe before this page
    // listened: once listening, ask what it found.
    updates.then(() => api.foundUpdate()).then((u) => u && announceUpdate(u))
    api.updatedTo().then((v) => v && notify(`Updated to ${v}`))
    const unlisten = [
      updates,
      onStatusChanged((s) => {
        status = s
        // A game started or stopped: "playing now" and play time changed.
        // Not while the window is hidden in the tray: that would list the
        // whole library twice per change during a game, for nobody.
        // Coming back to the window refreshes (onFocus above).
        if (document.visibilityState === 'visible') refreshLibrary()
      }),
      onClipSaved((path) => {
        // A render (name_edit, name_edit2, … as library::is_edit) has its
        // own notice from render(); the flash is for clips from the game.
        if (!/_edit\d*\.[^.\\/]+$/.test(path)) {
          const run = () => {
            closeToast()
            open(path)
          }
          showToast({
            text: 'Clip saved',
            media: { path, thumbnail: null, game: status?.game ?? '', duration: null },
            action: { label: 'Copy', run: () => copySaved(path) },
            secondaryAction: { label: 'Open', run },
          }, 8000)
          flash = true
          clearTimeout(flashTimer)
          flashTimer = setTimeout(() => (flash = false), 1200)
          pendingArrivals.add(path)
        }
        refreshLibrary()
        refreshUsage()
      }),
      onScreenshotSaved((path) => {
        // Like a saved clip: its picture, Copy and Open. The listing that
        // follows fills in its size; until then a stand-in will do.
        const listed = (): Screenshot => screenshots.find((s) => s.path === path) ?? { path, game: status?.game ?? '', size: 0, modified_ms: Date.now() }
        showToast({
          text: 'Screenshot saved',
          media: { path, thumbnail: null, src: thumbUrl(path), game: status?.game ?? '', duration: null },
          action: { label: 'Copy', run: () => copyClip(listed()) },
          secondaryAction: {
            label: 'Open',
            run: () => {
              closeToast()
              viewing = null
              shot = listed()
            },
          },
        }, 8000)
      }),
      onMarked(() => notify('Moment marked')),
      onLibraryChanged(() => {
        refreshLibrary()
        refreshUsage()
      }),
      onSettingsChanged((s) => {
        settings = s
        refreshLibrary()
        refreshUsage()
      }),
      onThumbnailReady(thumbnailReady),
      onRenderProgress((p) => {
        if (renderPath === p.path) renderProgress = p.fraction
      }),
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload
        dragging = p.type === 'enter' || p.type === 'over'
        if (p.type === 'drop') {
          const file = p.paths.find((f) => VIDEO.test(f))
          if (file) open(file)
        }
      }),
    ]
    const motion = matchMedia('(prefers-reduced-motion: reduce)')
    const onMotion = () => applyPrefs(prefs)
    motion.addEventListener('change', onMotion)
    return () => {
      clearInterval(tick)
      window.removeEventListener('focus', onFocus)
      document.removeEventListener('visibilitychange', onFocus)
      unlisten.forEach((u) => u.then((f) => f()))
      motion.removeEventListener('change', onMotion)
    }
  })
</script>

<svelte:window {onkeydown} />

<div class="app-shell flex h-full flex-col">
  <!-- Keep the native window contract independent of the capture controls. -->
  <WindowControls split={!covered} />

  <div class="relative flex min-h-0 flex-1">
    <!-- A clip or screenshot open in its place: this can't be seen, clicked or tabbed to. -->
    <div class="workspace flex min-w-0 flex-1 {covered ? 'invisible' : ''}" inert={covered}>
      <LibraryNav {games} {clips} {scope} game={gameName} settings={view === 'settings'} {category} update={newRelease} onscope={list} onsettings={() => go('settings')} oncategory={(c) => (category = c)} onback={() => go('library')} onupdate={() => (updatePrompt = true)}>
        {#snippet capture()}<CaptureControl {status} {now} {flash} {saving} {hotkey} onsave={save} onchoosescreen={chooseScreen} onignore={ignoreGame} onrecording={() => openSettings('Recording')} />{/snippet}
      </LibraryNav>
      <!-- The library stays mounted so Settings preserves search and scroll. -->
      <main class="library-pane relative flex min-w-0 flex-1 flex-col">
        <div class="flex min-h-0 flex-1 flex-col {view === 'settings' ? 'invisible' : ''}" inert={view === 'settings'}>
          {#if libraryError}
            <header class="page-header">
              <h1 class="font-display text-page font-semibold">All clips</h1>
            </header>
            <div class="min-h-0 flex-1 overflow-y-auto px-8 pb-6">
              <StateMessage title="Couldn't read the clips folder" body={libraryError} error>
                <button class="btn" onclick={() => openSettings('Storage')}>Storage settings</button>
                <button class="btn-primary" onclick={refreshLibrary}>Try again</button>
              </StateMessage>
            </div>
          {:else}
            <ClipsView
              bind:this={library}
              {clips}
              screenshots={shots}
              {scope}
              {game}
              {games}
              behind={covered || view === 'settings'}
              {loading}
              {hotkey}
              {status}
              {settings}
              {copyingPath}
              {copiedPath}
              copyProgress={copyPhase === 'preparing' ? renderProgress : null}
              {copyBlocked}
              onopen={(c, from) => open(c.path, from)}
              onshot={(s) => {
                if (document.activeElement instanceof HTMLElement) cameFrom = document.activeElement
                shot = s
              }}
              ondelete={(c) => remove([c])}
              ondeletemany={remove}
              oncopy={copyClip}
              onreveal={(c) => api.reveal(c.path).catch((e) => notify(sentence(e), true))}
              onfavorite={(c) => update(c, { favorite: !c.favorite })}
              onnotgame={notGame}
              onpick={pickVideo}
              onchoosescreen={chooseScreen}
              onsettings={openSettings}
            />
          {/if}
          {#if settings?.ads}
            <div class="flex flex-none justify-center border-t border-line p-3">
              <AdSlot enabled width={728} height={90} />
            </div>
          {/if}
        </div>
        {#if view === 'settings'}
          <div class="settings absolute inset-0 flex flex-col bg-canvas">
            {#if settings}
              <SettingsView {settings} {usage} {caps} {status} {category} bind:update={newRelease} notice={null} ondismiss={closeToast} onchange={changeSettings} onerror={(e) => notify(e, true)} onupdate={() => (updatePrompt = true)} onchoosescreen={chooseScreen} />
            {/if}
          </div>
        {/if}
      </main>
      <!-- Always mounted while the library shows, so a notice can leave with its own exit. -->
      {#if !covered}
        <div class="capture-notice-slot"><CaptureNotice notice={displayNotice} {copyBlocked} ondismiss={closeToast} onpause={pauseNotice} onresume={resumeNotice} /></div>
      {/if}
    </div>

    {#if viewing}
      <ClipStudio
        clip={viewing}
        busy={renderPath === viewing.path ? busy : null}
        copied={copiedPath === viewing.path}
        {copyBlocked}
        progress={renderProgress}
        {position}
        notice={workNotice}
        ondismiss={closeToast}
        onback={closeViewer}
        onstep={step}
        oncopy={copySelection}
        onsave={saveSelection}
        oncancel={cancelWork}
        ondelete={() => viewing && remove([viewing])}
        onreveal={() => viewing && api.reveal(viewing.path).catch((e) => notify(sentence(e), true))}
        onupdate={inLibrary(viewing) ? (p) => viewing && update(viewing, p) : undefined}
        entry={games.find((g) => g.name === viewing?.game) ?? null}
        {clips}
        onopen={(c) => open(c.path)}
      />
    {/if}

    {#if shot}
      {@const s = shot}
      <ScreenshotViewer shot={s} notice={workNotice} {copyBlocked} ondismiss={closeToast} onclose={closeViewer} oncopy={() => copyClip(s)} onreveal={() => api.reveal(s.path).catch((e) => notify(sentence(e), true))} ondelete={() => remove([s])} />
    {/if}
  </div>
</div>

{#if dragging}
  <div class="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-canvas p-4">
    <div class="flex size-full items-center justify-center rounded-surface border border-dashed border-line-strong text-title font-semibold">Drop a video to open it</div>
  </div>
{/if}

{#if updatePrompt && newRelease}
  <UpdatePrompt info={newRelease} onclose={() => (updatePrompt = false)} />
{/if}

<style>
  .app-shell {
    background: var(--color-nav);
  }

  .library-pane {
    overflow: hidden;
    background: var(--color-canvas);
  }
  .capture-notice-slot { position: absolute; right: 24px; bottom: 24px; z-index: 35; width: min(360px, calc(100% - 276px)); }
</style>
