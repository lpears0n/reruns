<script lang="ts">
  import { CircleAlert, Download, PanelLeft } from '@lucide/svelte'
  import { getCurrentWebview } from '@tauri-apps/api/webview'
  import { open as openDialog } from '@tauri-apps/plugin-dialog'
  import { onMount } from 'svelte'
  import AdSlot from './lib/AdSlot.svelte'
  import {
    api,
    type Capabilities,
    type Clip,
    type ClipPatch,
    type GameEntry,
    initMedia,
    onClipSaved,
    onScreenshotSaved,
    onMarked,
    onUpdateAvailable,
    onLibraryChanged,
    onSettingsChanged,
    onRenderProgress,
    onStatusChanged,
    onThumbnailReady,
    type Settings,
    type Status,
    type UpdateInfo,
    type Usage,
  } from './lib/api'
  import { retryFailedArt } from './lib/Art.svelte'
  import { readLibraryCache, writeLibraryCache } from './lib/cache'
  import ClipsView from './lib/ClipsView.svelte'
  import { coalesce } from './lib/coalesce'
  import ClipViewer from './lib/ClipViewer.svelte'
  import GamesView from './lib/GamesView.svelte'
  import GameView from './lib/GameView.svelte'
  import { applyPrefs, prefs, setPrefs } from './lib/prefs.svelte'
  import SettingsView, { type Category } from './lib/SettingsView.svelte'
  import Sidebar, { type NavId } from './lib/Sidebar.svelte'
  import StateMessage from './lib/StateMessage.svelte'
  import StatusBar from './lib/StatusBar.svelte'
  import Toast, { type ToastData } from './lib/Toast.svelte'
  import UpdatePrompt from './lib/UpdatePrompt.svelte'
  import WindowControls from './lib/WindowControls.svelte'

  type View = 'games' | 'game' | 'clips' | 'settings'
  const VIDEO = /\.(mp4|mkv|mov|webm)$/i
  /** How long a deleted clip can be brought back from the toast. */
  const UNDO_MS = 6000

  let view = $state<View>('games')
  /** The game page being shown, by name (entries are refreshed often). */
  let gameName = $state<string | null>(null)
  /** Clip open in the viewer, layered over the current view. */
  let viewing = $state<Clip | null>(null)
  let category = $state<Category>('General')

  let status = $state<Status | null>(null)
  let settings = $state<Settings | null>(null)
  /** A newer release, found by the background check or Settings. */
  let newRelease = $state<UpdateInfo | null>(null)
  /** The update dialog: opened by the start-up check, or the header icon. */
  let updatePrompt = $state(false)
  function announceUpdate(u: UpdateInfo) {
    newRelease = u
    if (u.prompt) updatePrompt = true
  }
  let usage = $state<Usage | null>(null)
  let caps = $state<Capabilities | null>(null)
  let innerWidth = $state(1280)

  // The sidebar folds to icons by itself where room matters more than
  // labels: a narrow window, Settings (which has its own section list) and
  // the clip viewer. There the toggle opens it for the moment (`peek`);
  // everywhere else it follows the saved choice (Ctrl+B).
  const autoCollapse = $derived(innerWidth < 1100 || view === 'settings' || viewing !== null)
  let peek = $state(false)
  $effect(() => {
    void autoCollapse
    peek = false
  })
  const sidebarCollapsed = $derived(autoCollapse ? !peek : prefs.sidebarCollapsed)
  function toggleSidebar() {
    if (autoCollapse) peek = !peek
    else setPrefs({ sidebarCollapsed: !prefs.sidebarCollapsed })
  }
  // Raw: replaced wholesale on every refresh; a deep proxy over thousands of
  // clips is slow and unnecessary.
  let allClips = $state.raw<Clip[]>([])
  let games = $state.raw<GameEntry[]>([])
  let loading = $state(true)
  /** Wall clock for live counters (buffer fill, "playing for"); ticks once
   *  a second while recording, otherwise once a minute. No IPC involved. */
  let now = $state(Date.now())
  /** Why the library couldn't be read, shown in place of the views. */
  let libraryError = $state<string | null>(null)
  let rendering = $state(false)
  let renderProgress = $state(0)
  let dragging = $state(false)
  let saving = $state(false)
  let flash = $state(false)
  let toast = $state<ToastData | null>(null)
  let toastTimer: ReturnType<typeof setTimeout>

  // Deletes are deferred so they can be undone; the clips vanish from the
  // UI at once and are only removed from disk when the toast expires.
  // Raw state: entries are compared by identity when undone or committed,
  // which a deep proxy would break. The array is always reassigned.
  let pending = $state.raw<{ clips: Clip[]; timer: ReturnType<typeof setTimeout> }[]>([])
  const hidden = $derived(new Set(pending.flatMap((p) => p.clips.map((c) => c.path))))
  const clips = $derived(hidden.size ? allClips.filter((c) => !hidden.has(c.path)) : allClips)

  const game = $derived(games.find((g) => g.name === gameName) ?? null)
  const hotkey = $derived(settings?.hotkey ?? 'F8')
  const nav = $derived<NavId>(view === 'game' ? 'games' : view)
  const title = $derived(
    view === 'games' ? 'Games' : view === 'game' ? (game?.name ?? 'Games') : view === 'clips' ? 'All clips' : 'Settings',
  )

  async function copyClip(c: Clip) {
    try {
      await api.copyClip(c.path)
      notify('Clip copied')
    } catch (e) {
      notify(String(e), true)
    }
  }

  function notify(text: string, error = false) {
    showToast({ text, error })
  }

  function showToast(t: ToastData, ms = 3000) {
    toast = t
    clearTimeout(toastTimer)
    toastTimer = setTimeout(() => (toast = null), ms)
  }

  function go(v: View) {
    view = v
    viewing = null
  }

  // Audio devices come and go (a headset plugged in); re-read them each
  // time Settings opens. Encoder results are cached, so this is cheap.
  $effect(() => {
    if (view === 'settings') api.capabilities().then((c) => (caps = c))
  })

  async function loadLibrary() {
    try {
      allClips = await api.clips()
      games = await api.games()
      retryFailedArt()
      libraryError = null
      writeLibraryCache(allClips, games)
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
      notify(String(e), true)
    } finally {
      saving = false
    }
  }

  async function open(path: string) {
    try {
      viewing = await api.openForEdit(path)
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

  async function render(
    start: number,
    end: number,
    volumes: number[],
    maxMb: number | null,
    vertical: number | null,
    slow: { from: number; to: number; factor: number } | null,
  ) {
    if (!viewing) return
    rendering = true
    renderProgress = 0
    try {
      await api.render(viewing.path, start, end, volumes, maxMb, vertical, slow)
    } catch (e) {
      if (!/cancelled/.test(String(e))) notify(String(e), true)
    } finally {
      rendering = false
    }
  }

  /** Hide the clips now; delete them for real when the undo window closes. */
  function remove(list: Clip[]) {
    if (!list.length) return
    if (viewing && list.some((c) => c.path === viewing!.path)) viewing = null
    const entry = { clips: list, timer: setTimeout(() => commit(entry), UNDO_MS) }
    pending = [...pending, entry]
    const label = list.length === 1 ? `Deleted ${list[0].title ?? list[0].game} clip` : `Deleted ${list.length} clips`
    showToast({ text: label, action: { label: 'Undo', run: () => undo(entry) } }, UNDO_MS)
  }

  function undo(entry: (typeof pending)[number]) {
    clearTimeout(entry.timer)
    pending = pending.filter((p) => p !== entry)
    toast = null
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
    else if (ignoredChanged) api.games().then((g) => (games = g), () => {})
    refreshUsage()
  }

  /** From the Games page: hide it and never detect it again. */
  async function notGame(g: GameEntry) {
    if (!settings) return
    try {
      games = games.filter((x) => x.name !== g.name)
      await changeSettings({ ...settings, ignored_games: [...settings.ignored_games, { name: g.name }] })
      notify(`Ignoring ${g.name}. Undo in Settings → Recording.`)
    } catch (e) {
      notify(String(e), true)
    }
  }

  /** "Not a game": stops within a scan; undo under Settings → Recording. */
  async function ignoreGame() {
    const name = status?.game
    try {
      settings = await api.ignoreCurrentGame()
      notify(`Ignoring ${name ?? 'that app'}. Undo in Settings → Recording.`)
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
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement
    if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
      e.preventDefault()
      toggleSidebar()
    } else if (!typing && !viewing && !e.ctrlKey && !e.altKey && !e.metaKey) {
      if (e.key === '1') go('games')
      else if (e.key === '2') go('clips')
      else if (e.key === '3') go('settings')
    }
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
    const onFocus = () => {
      if (document.visibilityState !== 'visible') return
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
    const updates = onUpdateAvailable(announceUpdate)
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
        notify(`Saved ${path.split(/[\\/]/).pop()}`)
        flash = true
        setTimeout(() => (flash = false), 400)
        refreshLibrary()
        refreshUsage()
      }),
      onScreenshotSaved((path) => {
        notify(`Screenshot saved as ${path.split(/[\\/]/).pop()}`)
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
        if (viewing?.path === p.path) renderProgress = p.fraction
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

<svelte:window {onkeydown} bind:innerWidth />

<div class="flex h-full">
  <Sidebar active={nav} collapsed={sidebarCollapsed} {usage} onnavigate={go} onstorage={() => openSettings('Storage')} />

  <div class="flex min-w-0 flex-1 flex-col">
    <!-- On Windows this is the title bar too: drag it, double-click to maximize. -->
    <header class="flex h-14 flex-none items-center gap-3 border-b border-line bg-panel pr-4 pl-3" data-tauri-drag-region>
      <button
        class="btn-icon text-muted"
        onclick={toggleSidebar}
        aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        title="Toggle sidebar (Ctrl+B)"
      >
        <PanelLeft size={18} />
      </button>
      <h1 class="display-face truncate text-title font-semibold" data-tauri-drag-region>{title}</h1>
      <div class="h-full min-w-0 flex-1" data-tauri-drag-region></div>
      {#if newRelease}
        <button
          class="update-badge btn-icon flex-none text-accent"
          onclick={() => (updatePrompt = true)}
          title="Update to {newRelease.version}"
          aria-label="Update to {newRelease.version}"
        >
          <Download size={18} />
        </button>
      {/if}
      <StatusBar {status} {now} {flash} {saving} {hotkey} onsave={save} onopen={pickVideo} onchoosescreen={chooseScreen} onignore={ignoreGame} />
      <WindowControls />
    </header>

    <main class="flex min-h-0 flex-1 flex-col bg-canvas">
      <div class="min-h-0 flex-1 overflow-y-auto p-6">
        {#if viewing}
          <ClipViewer
            clip={viewing}
            {rendering}
            progress={renderProgress}
            onback={() => (viewing = null)}
            onrender={render}
            oncancel={() => api.cancelRender()}
            ondelete={() => viewing && remove([viewing])}
            onshare={() => viewing && api.reveal(viewing.path)}
            oncopy={() => viewing && copyClip(viewing)}
            onupdate={inLibrary(viewing) ? (p) => viewing && update(viewing, p) : undefined}
          />
        {:else if libraryError && view !== 'settings'}
          <StateMessage icon={CircleAlert} title="Couldn't read the clips folder" body={libraryError} error>
            <button class="btn" onclick={() => openSettings('Storage')}>Storage settings</button>
            <button class="btn-primary" onclick={refreshLibrary}>Try again</button>
          </StateMessage>
        {:else if view === 'game' && game}
          <GameView
            {game}
            {clips}
            onback={() => go('games')}
            onopen={(c) => open(c.path)}
            ondelete={(c) => remove([c])}
            onshare={(c) => api.reveal(c.path)}
            oncopy={copyClip}
            onfavorite={(c) => update(c, { favorite: !c.favorite })}
          />
        {:else if view === 'clips'}
          <ClipsView
            {clips}
            {loading}
            {hotkey}
            onopen={(c) => open(c.path)}
            ondelete={(c) => remove([c])}
            ondeletemany={remove}
            onshare={(c) => api.reveal(c.path)}
            oncopy={copyClip}
            onfavorite={(c) => update(c, { favorite: !c.favorite })}
          />
        {:else if view === 'settings' && settings}
          <SettingsView {settings} {usage} {caps} {status} bind:category bind:update={newRelease} onchange={changeSettings} onchoosescreen={chooseScreen} />
        {:else}
          <GamesView
            {games}
            {loading}
            {hotkey}
            {status}
            {settings}
            {now}
            onopen={(g) => {
              gameName = g.name
              go('game')
            }}
            onnotgame={notGame}
            onchoosescreen={chooseScreen}
            onsettings={openSettings}
          />
        {/if}
      </div>
      {#if settings?.ads}
        <div class="flex justify-center border-t border-line p-3">
          <AdSlot enabled width={728} height={90} />
        </div>
      {/if}
    </main>
  </div>
</div>

{#if dragging}
  <div class="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-canvas/85">
    <div class="display-face rounded-surface border-2 border-accent px-10 py-6 text-page font-semibold">Drop to open</div>
  </div>
{/if}

{#if toast}
  <Toast data={toast} onclose={() => (toast = null)} />
{/if}

{#if updatePrompt && newRelease}
  <UpdatePrompt info={newRelease} onclose={() => (updatePrompt = false)} />
{/if}

<style>
  /* A slow breath, not a blink: a soft accent halo behind the icon swells
     and fades, so it's noticeable without nagging and the button itself
     stays still under the pointer. Turned off with reduced motion
     (app.css). */
  .update-badge {
    position: relative;
    isolation: isolate;
  }
  .update-badge::before {
    content: '';
    position: absolute;
    inset: 3px;
    z-index: -1;
    border-radius: 999px;
    background: var(--color-accent);
    opacity: 0;
    animation: breathe 2.4s ease-in-out infinite;
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: 0.06;
      transform: scale(0.75);
    }
    50% {
      opacity: 0.24;
      transform: scale(1.1);
    }
  }
</style>
