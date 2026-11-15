<!--
  The viewer: a clip opened takes the library's place, opaque, from the
  window's strip to its bottom edge. In it, one workspace about 1080 px wide
  (narrower when the window or the picture's height asks), a little above
  the middle: a quiet header (back, what the clip is, one menu), the picture
  at its own shape, the one timeline that both seeks and trims, and right
  under it a row with playback on the left and what to do with the clip on
  the right: Copy clip, Save clip. Fullscreen is a button away.

  Everything else is in the header's menu or on a key: favorite, rename,
  delete, speed, loop, fullscreen, the saved clip's audio mix, 9:16, slow
  motion, exact trim points. Left and Right step to the clip before and
  after without leaving.
-->
<script lang="ts" module>
  /** What Copy clip and Save clip act on: the selection, and how it is
   *  shaped. `volumes`: a level per source track (1 = as recorded).
   *  `vertical`: crop to 9:16 at this position (0 left, 1 right). `slow`:
   *  play this range slower (factor 2 = half speed). */
  export interface Edit {
    start: number
    end: number
    volumes: number[]
    vertical: number | null
    slow: { from: number; to: number; factor: number } | null
  }

  interface Draft {
    start: number
    end: number
    levels: number[]
    vertical: number | null
    slowFactor: number
    slowFrom: number
    slowTo: number
  }

  // An edit in progress, by clip, for this run of the app: going back to the
  // library and opening the clip again finds the trim as it was left.
  const drafts = new Map<string, Draft>()

  // Playback volume is the listener's, not the clip's: it stays as set from
  // one clip to the next.
  const heard = $state({ volume: 1, muted: false })
</script>

<script lang="ts">
  import { ArrowLeft, Check, Ellipsis, Maximize, Minimize, Pause, Play, Volume1, Volume2, VolumeX, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { api, type Clip, type ClipPatch, type ExportPlan, fileUrl, videoUrl } from './api'
  import Fader from './Fader.svelte'
  import { duration as length, gameName, parseTime, size, span, timecode, trackName, when } from './format'
  import { playbackStats } from './playbackStats'
  import Popover from './Popover.svelte'
  import { prefs } from './prefs.svelte'
  import type { Notice } from './StatusBar.svelte'
  import Timeline from './Timeline.svelte'
  import { waveform } from './waveform'

  let {
    clip,
    busy,
    copied = false,
    progress = 0,
    position = null,
    notice,
    ondismiss,
    onback,
    onstep,
    oncopy,
    onsave,
    oncancel,
    ondelete,
    onreveal,
    onupdate,
  }: {
    clip: Clip
    /** A render is running, for Copy clip or for Save clip. */
    busy: 'copy' | 'save' | null
    /** Copy clip just put the clip on the clipboard. */
    copied?: boolean
    /** Fraction of the render done, while `busy`. */
    progress?: number
    /** Where this clip is in the list it was opened from; null for a file
     *  opened from outside it. */
    position?: { index: number; total: number } | null
    /** What just happened, said beside the actions (there is no strip here). */
    notice: Notice | null
    ondismiss: () => void
    onback: () => void
    /** To the clip before (-1) or after (1) in the list. */
    onstep: (dir: 1 | -1) => void
    /** The selection onto the clipboard, as a file. `untouched`: nothing
     *  differs from the clip as it is, so the file itself can go. */
    oncopy: (edit: Edit, untouched: boolean) => void
    /** The selection as a new clip; the original is left alone. */
    onsave: (edit: Edit) => void
    oncancel: () => void
    ondelete: () => void
    /** Show the file in the system's file manager. */
    onreveal: () => void
    /** Absent for videos opened from outside the library. */
    onupdate?: (patch: ClipPatch) => void
  } = $props()

  const game = $derived(gameName(clip.game))
  const file = $derived(clip.path.split(/[\\/]/).pop() ?? '')

  // Title editing and tag entry.
  let editingTitle = $state(false)
  let titleDraft = $state('')
  let titleInput: HTMLInputElement | undefined = $state()
  let addingLabel = $state(false)
  let labelDraft = $state('')

  function startTitle() {
    if (!onupdate) return
    titleDraft = clip.title ?? ''
    editingTitle = true
    queueMicrotask(() => titleInput?.select())
  }

  function commitTitle() {
    if (!editingTitle) return
    editingTitle = false
    if ((clip.title ?? '') !== titleDraft.trim()) onupdate?.({ title: titleDraft })
  }

  function addLabel() {
    const l = labelDraft.trim().replace(/^#+/, '')
    labelDraft = ''
    if (l && !clip.labels.includes(l)) onupdate?.({ labels: [...clip.labels, l] })
  }

  function removeLabel(l: string) {
    onupdate?.({ labels: clip.labels.filter((x) => x !== l) })
  }

  let video: HTMLVideoElement
  let duration = $state(0)
  let time = $state(0)
  let paused = $state(true)
  let aspect = $state(16 / 9)
  /** The picture's own size, once the video says. */
  let pixels = $state('')
  let fullscreen = $state(false)
  let start = $state(0)
  let end = $state(0)
  /** The handles have been moved in from the ends. */
  const trimmed = $derived(start > 0.05 || end < duration - 0.05)
  let speed = $state(1)
  // The saved clip's shape: a 9:16 window (its position across the picture)
  // for TikTok, Shorts and Reels, and a slow-motion range inside the trim.
  let vertical = $state<number | null>(null)
  let slowFactor = $state(1)
  let slowFrom = $state(0)
  let slowTo = $state(0)
  const slow = $derived(slowFactor > 1 ? { from: slowFrom, to: slowTo, factor: slowFactor } : null)
  const slowAdds = $derived(
    slowFactor > 1 ? Math.max(0, Math.min(slowTo, end) - Math.max(slowFrom, start)) * (slowFactor - 1) : 0,
  )
  /** How long the saved file will be. */
  const outLength = $derived(end - start + slowAdds)

  // One level per source track, in percent, 100 unless changed. The track
  // count can arrive later (thumbnail pool). A clip that starts with game
  // and mic mixed into one has a level for each of the tracks after it, not
  // for the mix.
  const sourceTracks = $derived(Math.max(0, clip.audio_tracks - (clip.mixed_audio ? 1 : 0)))
  let levelEdits = $state<number[]>([])
  const levels = $derived(Array.from({ length: sourceTracks }, (_, i) => levelEdits[i] ?? 100))
  const mixed = $derived(levels.some((l) => l !== 100))

  /** Nothing would differ from the clip as it is. */
  const untouched = $derived(!trimmed && vertical === null && !slow && !mixed)
  const edit = $derived<Edit>({ start, end, volumes: levels.map((l) => l / 100), vertical, slow })
  /** What the selection is, in one line: shown once something has changed. */
  const summary = $derived(
    [
      trimmed && `${span(end - start)} selected`,
      vertical !== null && '9:16',
      slow && `${slow.factor === 2 ? '½' : '¼'}× slow motion`,
      mixed && 'audio mix changed',
    ]
      .filter(Boolean)
      .join('\u2003'),
  )

  function reset() {
    start = 0
    end = duration
    vertical = null
    slowFactor = 1
    levelEdits = []
  }

  function setSlow(factor: number) {
    // First time on: the middle third of the trim.
    if (factor > 1 && (slowFactor === 1 || slowTo <= slowFrom)) {
      const third = (end - start) / 3
      slowFrom = start + third
      slowTo = end - third
    }
    slowFactor = factor
  }

  // Width of the 9:16 frame as a share of the picture's width.
  const frameWidth = $derived(Math.min(1, 9 / 16 / aspect))
  let cropDrag: { x: number; at: number; width: number } | null = null
  function cropDown(e: PointerEvent) {
    e.stopPropagation()
    const box = (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect()
    cropDrag = { x: e.clientX, at: vertical ?? 0.5, width: box.width * (1 - frameWidth) }
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }
  function cropMove(e: PointerEvent) {
    if (!cropDrag || cropDrag.width <= 0) return
    vertical = Math.min(1, Math.max(0, cropDrag.at + (e.clientX - cropDrag.x) / cropDrag.width))
  }
  let loop = $state(false)
  let help = $state(false)
  const SPEEDS = [0.25, 0.5, 1, 1.5, 2]
  /** One frame at 60 fps; fine enough to land on a moment. */
  const FRAME = 1 / 60

  // Fullscreen: the controls over the picture show while paused, or
  // briefly after the pointer moves.
  let active = $state(true)
  let idleTimer: ReturnType<typeof setTimeout>
  const showControls = $derived(paused || active)

  function poke() {
    active = true
    clearTimeout(idleTimer)
    idleTimer = setTimeout(() => (active = false), 2200)
  }

  // Start over when a different clip is loaded (stepped to, or opened), and
  // only then: the parent swaps in a new clip object whenever this one is
  // favorited, renamed, tagged or its thumbnail arrives, which mustn't
  // throw away a trim in progress. A clip edited earlier comes back as it
  // was left.
  let shownPath: string | null = null
  $effect(() => {
    const path = clip.path
    if (path === shownPath) return
    shownPath = path
    untrack(() => {
      const d = drafts.get(path)
      time = 0
      duration = clip.duration ?? 0
      start = d?.start ?? 0
      end = d?.end ?? duration
      vertical = d?.vertical ?? null
      slowFactor = d?.slowFactor ?? 1
      slowFrom = d?.slowFrom ?? 0
      slowTo = d?.slowTo ?? 0
      levelEdits = d?.levels ?? []
      pixels = ''
      editingTitle = addingLabel = false
    })
  })
  $effect(() => {
    const d: Draft = { start, end, levels: $state.snapshot(levelEdits), vertical, slowFactor, slowFrom, slowTo }
    // Not before the clip's length is known: a kept trim would read as none.
    if (!shownPath || !duration) return
    if (untouched) drafts.delete(shownPath)
    else drafts.set(shownPath, d)
  })

  // The audio's shape under the filmstrip, read once per clip. Tagged with
  // its clip, so one clip's waveform is never drawn under another.
  let wave = $state.raw<{ path: string; peaks: Float32Array | null } | null>(null)
  $effect(() => {
    const path = clip.path
    let stale = false
    waveform(path, untrack(() => clip.size_bytes)).then((peaks) => {
      if (!stale) wave = { path, peaks }
    })
    return () => {
      stale = true
    }
  })
  /** Undefined while it is being read; null when there is none to draw. */
  const peaks = $derived(!clip.audio_tracks ? null : wave?.path === clip.path ? wave.peaks : undefined)

  function loaded() {
    duration = video.duration
    if (video.videoWidth && video.videoHeight) {
      aspect = video.videoWidth / video.videoHeight
      pixels = `${video.videoWidth} × ${video.videoHeight}`
    }
    if (!end || end > duration) end = duration
  }

  function tick() {
    time = video.currentTime
    const rate = slowFactor > 1 && time >= slowFrom && time < slowTo ? speed / slowFactor : speed
    if (video.playbackRate !== rate) video.playbackRate = rate
    // A trimmed clip plays only what is kept, once, unless Loop is on.
    if (trimmed && !paused && time >= end) {
      if (loop) seek(start)
      else {
        video.pause()
        video.currentTime = start
      }
    }
  }

  // The video's timeupdate event comes only ~4 times a second, which made
  // the playhead jump. While playing, follow it on every display frame
  // instead.
  $effect(() => {
    if (paused || !video) return
    let id = requestAnimationFrame(function frame() {
      tick()
      id = requestAnimationFrame(frame)
    })
    return () => cancelAnimationFrame(id)
  })

  function ended() {
    if (loop) {
      seek(trimmed ? start : 0)
      video.play()
    }
  }

  function step(frames: number) {
    video.pause()
    seek(Math.min(duration, Math.max(0, time + frames * FRAME)))
    poke()
  }

  function setSpeed(v: number) {
    speed = v
    video.playbackRate = v
    poke()
  }

  function nudgeSpeed(dir: 1 | -1) {
    const i = Math.min(SPEEDS.length - 1, Math.max(0, SPEEDS.indexOf(speed) + dir))
    setSpeed(SPEEDS[i])
  }

  function seek(t: number) {
    video.currentTime = t
    time = t
  }

  /** Play previews the selection: from its start when the playhead is outside it. */
  function toggle() {
    if (video.paused) {
      if (trimmed && (time >= end - 0.05 || time < start)) seek(start)
      video.play()
    } else {
      video.pause()
    }
    poke()
  }

  const setIn = () => (start = Math.max(0, Math.min(time, end - 0.1)))
  const setOut = () => (end = Math.min(duration, Math.max(time, start + 0.1)))

  async function setFullscreen(on: boolean) {
    fullscreen = on
    try {
      await getCurrentWindow().setFullscreen(on)
    } catch {
      // Without the window going fullscreen it still fills the app.
    }
  }

  // A size to fit under (Settings → Sharing): how the selection would be
  // encoded, or why it can't be (too long for the size).
  const limit = $derived(prefs.shareLimit ? prefs.shareMaxMb : null)
  /** Copy clip can send the file itself. */
  const copyAsIs = $derived(untouched && (limit === null || clip.size_bytes <= limit * 1e6))
  let plan = $state<ExportPlan | null>(null)
  let planError = $state<string | null>(null)
  $effect(() => {
    const mb = limit
    const seconds = outLength
    const audio = clip.audio_tracks > 0
    if (mb === null || seconds < 0.1 || (copyAsIs && !prefs.shareLimitSave)) {
      plan = null
      planError = null
      return
    }
    let stale = false
    api.exportPlan(mb, seconds, audio).then(
      (p) => {
        if (stale) return
        plan = p
        planError = null
      },
      (e) => {
        if (stale) return
        plan = null
        planError = String(e)
      },
    )
    return () => {
      stale = true
    }
  })
  const fit = $derived(plan && limit !== null ? `Fits under ${limit} MB at ${plan.max_height}p${plan.fps ?? ''}, ${(plan.video_kbps / 1000).toFixed(1)} Mb/s.` : '')

  const valid = $derived(end - start >= 0.1)
  const canCopy = $derived(valid && !busy && (copyAsIs || !planError))
  const canSave = $derived(valid && !busy && !(prefs.shareLimitSave && planError))
  const copy = () => canCopy && oncopy(edit, untouched)
  const save = () => canSave && onsave(edit)

  /** Seek to the previous (-1) or next (1) marked moment. A small margin
   *  lets repeated presses walk past the one just reached. */
  function jumpToMarker(dir: 1 | -1) {
    const marks = clip.markers ?? []
    const target =
      dir > 0
        ? marks.find((m) => m > time + 0.25)
        : [...marks].reverse().find((m) => m < time - 0.25)
    if (target === undefined) return
    seek(target)
    poke()
  }

  // The header's menu, and the two small panels it opens in its place.
  let more: HTMLButtonElement | undefined = $state()
  let mix: ReturnType<typeof Popover> | undefined = $state()
  let points: ReturnType<typeof Popover> | undefined = $state()
  let startText = $state('')
  let endText = $state('')
  let pointsError = $state('')

  function openPoints() {
    startText = timecode(start)
    endText = timecode(end)
    pointsError = ''
    if (more) points?.openFrom(more)
  }

  function applyPoints(e: SubmitEvent) {
    e.preventDefault()
    const a = parseTime(startText)
    const b = parseTime(endText)
    if (a === null || b === null) pointsError = 'Write a time like 0:07.50.'
    else if (b > duration + 0.005) pointsError = `The clip ends at ${timecode(duration)}.`
    else if (b - a < 0.1) pointsError = 'The end has to come after the start.'
    else {
      start = a
      end = Math.min(b, duration)
      seek(a)
      points?.close()
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return
    if (help && e.code !== 'Escape' && e.key !== '?') return
    if ((e.ctrlKey || e.metaKey) && e.code === 'KeyC') {
      // Text selected on the page copies as text, as usual.
      if (window.getSelection()?.toString()) return
      e.preventDefault()
      copy()
      return
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return
    if (e.code === 'Space' || e.code === 'KeyK') {
      // Space on a focused button presses that button.
      if (e.code === 'Space' && e.target instanceof HTMLButtonElement) return
      e.preventDefault()
      toggle()
    } else if (e.code === 'KeyF') {
      setFullscreen(!fullscreen)
    } else if (e.code === 'KeyM') {
      heard.muted = !heard.muted
    } else if (e.key === '?') {
      help = !help
    } else if (e.code === 'Escape') {
      if (help) help = false
      else if (fullscreen) setFullscreen(false)
      else onback()
    } else if (e.code === 'ArrowLeft') {
      e.preventDefault()
      onstep(-1)
    } else if (e.code === 'ArrowRight') {
      e.preventDefault()
      onstep(1)
    } else if (e.code === 'KeyJ') {
      seek(Math.max(0, time - 5))
      poke()
    } else if (e.code === 'KeyL') {
      seek(Math.min(duration, time + 5))
      poke()
    } else if (e.code === 'Home') {
      seek(start)
      poke()
    } else if (e.code === 'End') {
      seek(end)
      poke()
    } else if (e.key === '[' || e.key === ']') {
      jumpToMarker(e.key === ']' ? 1 : -1)
    } else if (e.key === ',') {
      step(-1)
    } else if (e.key === '.') {
      step(1)
    } else if (e.key === '<') {
      nudgeSpeed(-1)
    } else if (e.key === '>') {
      nudgeSpeed(1)
    } else if (e.code === 'KeyR') {
      loop = !loop
      poke()
    } else if (e.code === 'KeyS' && onupdate) {
      onupdate({ favorite: !clip.favorite })
    } else if (e.code === 'KeyN' && onupdate) {
      e.preventDefault()
      startTitle()
    } else if (e.code === 'KeyI') {
      setIn()
    } else if (e.code === 'KeyO') {
      setOut()
    }
  }

  const SHORTCUTS: [string, string][] = [
    ['Space or K', 'Play or pause'],
    ['J, L', 'Back or forward 5 seconds'],
    [', and .', 'Previous or next frame'],
    ['I, O', 'Set the trim start or end here'],
    ['Home, End', 'Go to the trim start or end'],
    ['[ and ]', 'Previous or next marked moment'],
    ['← →', 'Previous or next clip'],
    ['< and >', 'Slower or faster'],
    ['R', 'Loop'],
    ['S', 'Favorite'],
    ['N', 'Rename'],
    ['M', 'Mute'],
    ['F', 'Fullscreen'],
    ['Ctrl+C', 'Copy clip'],
    ['Esc', 'Back to the library'],
    ['?', 'This list'],
  ]

  // The <video> element plays the first audio track; its level in the mix
  // is heard too, unless that track is the mix (no one level applies to it).
  $effect(() => {
    if (!video) return
    video.volume = heard.volume * (clip.mixed_audio ? 1 : Math.min(1, (levels[0] ?? 100) / 100))
    video.muted = heard.muted
  })

  // The workspace's size. The picture is as wide as the column allows and
  // as tall as leaves room for everything under it; the column then follows
  // the picture (never narrower than NARROWEST), so the header, timeline and
  // row line up with the picture's edges. Only a resize recomputes this.
  let boxW = $state(0)
  let boxH = $state(0)
  const COLUMN = 1080
  const NARROWEST = 560
  /** The viewer's own side padding (`px-6`). */
  const SIDE = 24
  /** Everything stacked with the picture: header 56, gaps 20 + 14 + 10,
   *  timeline 60, row 40, and the least space above and below, 24 each. */
  const AROUND = 56 + 20 + 14 + 60 + 10 + 40 + 24 + 24
  const room = $derived(Math.max(0, boxW - SIDE * 2))
  const pictureW = $derived(Math.min(room, COLUMN, Math.max(120, boxH - AROUND) * aspect))
  const columnW = $derived(Math.min(room, COLUMN, Math.max(pictureW, NARROWEST)))

  // Keys go to the viewer first, not to what it replaced.
  let root: HTMLDivElement
  $effect(() => root.focus())

  $effect(() => () => {
    clearTimeout(idleTimer)
    if (fullscreen) getCurrentWindow().setFullscreen(false).catch(() => {})
  })
</script>

<svelte:window {onkeydown} />

{#snippet timeline(lane: Float32Array | null | undefined)}
  <Timeline
    {duration}
    {time}
    sprite={clip.sprite ? fileUrl(clip.sprite) : null}
    frames={clip.sprite_frames}
    peaks={lane}
    markers={clip.markers}
    bind:start
    bind:end
    slow={slowFactor > 1}
    bind:slowFrom
    bind:slowTo
    onseek={seek}
  />
{/snippet}

{#snippet check(on: boolean)}
  <span class="flex w-4 flex-none justify-center text-accent">{#if on}<Check size={15} strokeWidth={2.5} />{/if}</span>
{/snippet}

<div
  bind:this={root}
  bind:clientWidth={boxW}
  bind:clientHeight={boxH}
  class="viewer absolute inset-0 z-30 flex flex-col items-center overflow-hidden bg-canvas px-6 outline-none"
  role="dialog"
  aria-modal="true"
  aria-label={clip.title ?? `${game} clip`}
  tabindex="-1"
>
  <!-- What is left over goes a third above, two thirds below: a little
       above the middle, where the eye expects a workspace. -->
  <div class="min-h-6 flex-1"></div>
  <div class="workspace flex flex-none flex-col" style:width="{columnW}px">
  <!-- Back, what this is, one menu. -->
  <header class="flex h-14 flex-none items-center gap-2">
    <button class="btn-icon -ml-2" onclick={onback} aria-label="Back to the library" title="Back to the library (Esc)">
      <ArrowLeft size={20} />
    </button>

    <div class="min-w-0 flex-1">
      {#if editingTitle}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:this={titleInput}
          bind:value={titleDraft}
          class="field -ml-3 block w-96 max-w-full text-title font-semibold"
          placeholder={game}
          maxlength="120"
          autofocus
          onblur={commitTitle}
          onkeydown={(e) => {
            e.stopPropagation()
            if (e.key === 'Enter') commitTitle()
            else if (e.key === 'Escape') editingTitle = false
          }}
          aria-label="Clip title"
        />
      {:else}
        <h1 class="truncate text-heading font-semibold" title={clip.title ?? game}>{clip.title ?? game}</h1>
        {#if addingLabel}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            bind:value={labelDraft}
            class="field -ml-2 block h-6 w-40 px-2 text-secondary"
            placeholder="New tag"
            maxlength="40"
            autofocus
            onblur={() => {
              addLabel()
              addingLabel = false
            }}
            onkeydown={(e) => {
              e.stopPropagation()
              if (e.key === 'Enter') addLabel()
              else if (e.key === 'Escape') {
                labelDraft = ''
                addingLabel = false
              }
            }}
            aria-label="New tag"
          />
        {:else}
          <!-- Fields apart by space, not joined by dots. -->
          <p class="flex min-w-0 gap-3 overflow-hidden text-secondary whitespace-nowrap text-fg-2">
            {#if clip.title}<span class="truncate">{game}</span>{/if}
            <span class="flex-none tabular-nums">{when(clip.created_ms)}</span>
            {#if prefs.tagging}{#each clip.labels as l (l)}<span class="tag flex-none text-muted">#{l}</span>{/each}{/if}
          </p>
        {/if}
      {/if}
    </div>

    <Popover label="Clip options" class="max-w-80">
      {#snippet trigger(props)}
        <button {...props} bind:this={more} class="btn-icon -mr-2" aria-label="Clip options" title="Clip options">
          <Ellipsis size={20} />
        </button>
      {/snippet}
      {#if onupdate}
        <button class="menu-item" role="menuitem" onclick={() => onupdate({ favorite: !clip.favorite })}>
          {@render check(false)}{clip.favorite ? 'Remove from favorites' : 'Add to favorites'}<span class="menu-key">S</span>
        </button>
        <button class="menu-item" role="menuitem" onclick={startTitle}>{@render check(false)}Rename<span class="menu-key">N</span></button>
        {#if prefs.tagging}
          <button class="menu-item" role="menuitem" onclick={() => (addingLabel = true)}>{@render check(false)}Add a tag…</button>
          {#each clip.labels as l (l)}
            <button class="menu-item" role="menuitem" onclick={() => removeLabel(l)}>{@render check(false)}<span class="truncate">Remove tag #{l}</span></button>
          {/each}
        {/if}
      {/if}
      <button class="menu-item" role="menuitem" onclick={onreveal}>{@render check(false)}Show in folder</button>

      <div class="menu-rule"></div>
      <div class="pt-1 pr-2.5 pb-1.5 pl-9 text-caption text-muted">Playback speed</div>
      <div class="grid grid-cols-5 gap-1 px-1 pb-1">
        {#each SPEEDS as s (s)}
          <button class="menu-item justify-center px-0 tabular-nums {speed === s ? 'bg-hover font-semibold' : 'text-fg-2'}" role="menuitemradio" aria-checked={speed === s} onclick={() => setSpeed(s)}>{s}×</button>
        {/each}
      </div>
      <button class="menu-item" role="menuitemcheckbox" aria-checked={loop} onclick={() => (loop = !loop)}>
        {@render check(loop)}Loop<span class="menu-key">R</span>
      </button>
      <button class="menu-item" role="menuitem" onclick={() => setFullscreen(true)}>{@render check(false)}Fullscreen<span class="menu-key">F</span></button>

      <div class="menu-rule"></div>
      <div class="pt-1 pr-2.5 pb-1.5 pl-9 text-caption text-muted">The copied or saved clip</div>
      <button class="menu-item" role="menuitem" onclick={openPoints}>{@render check(false)}Set trim points…</button>
      {#if sourceTracks}
        <button class="menu-item" role="menuitem" onclick={() => more && mix?.openFrom(more)}>{@render check(mixed)}Audio mix…</button>
      {/if}
      <button class="menu-item" role="menuitemcheckbox" aria-checked={vertical !== null} onclick={() => (vertical = vertical === null ? 0.5 : null)}>
        {@render check(vertical !== null)}Vertical 9:16
      </button>
      <div class="flex items-center gap-1 py-0.5 pr-1 pl-2.5">
        {@render check(slowFactor > 1)}
        <span class="mr-auto pl-2.5">Slow motion</span>
        {#each [[1, 'Off'], [2, '½×'], [4, '¼×']] as const as [factor, name] (factor)}
          <button class="menu-item w-12 justify-center px-0 {slowFactor === factor ? 'bg-hover font-semibold' : 'text-fg-2'}" role="menuitemradio" aria-checked={slowFactor === factor} aria-label="Slow motion {name}" onclick={() => setSlow(factor)}>{name}</button>
        {/each}
      </div>

      <div class="menu-rule"></div>
      {#if position}
        <button class="menu-item" role="menuitem" disabled={position.index <= 0 || !!busy} onclick={() => onstep(-1)}>{@render check(false)}Previous clip<span class="menu-key">←</span></button>
        <button class="menu-item" role="menuitem" disabled={position.index >= position.total - 1 || !!busy} onclick={() => onstep(1)}>{@render check(false)}Next clip<span class="menu-key">→</span></button>
      {/if}
      <button class="menu-item" role="menuitem" onclick={() => (help = true)}>{@render check(false)}Keyboard shortcuts<span class="menu-key">?</span></button>
      <div class="menu-rule"></div>
      <button class="menu-item text-danger" role="menuitem" onclick={ondelete}>{@render check(false)}Delete</button>
      <div class="menu-rule"></div>
      <!-- What the file is: read here, not carried across the header. -->
      <div class="pt-0.5 pr-2.5 pb-1.5 pl-9 text-caption text-muted select-text">
        <div class="truncate" title={clip.path}>{file}</div>
        <div class="tabular-nums">{[length(clip.duration ?? duration), size(clip.size_bytes), pixels, clip.is_edit && 'Trimmed'].filter(Boolean).join(' · ')}</div>
      </div>
    </Popover>

    <!-- Opened from the menu, in its place. -->
    <Popover bind:this={points} label="Trim points" menu={false}>
      <form class="flex w-64 flex-col gap-3 p-4" onsubmit={applyPoints}>
        <div class="font-semibold">Trim points</div>
        <label class="flex items-center justify-between gap-3 text-fg-2">
          Start
          <input class="field w-32 text-right tabular-nums" bind:value={startText} aria-label="Trim start" aria-invalid={!!pointsError} />
        </label>
        <label class="flex items-center justify-between gap-3 text-fg-2">
          End
          <input class="field w-32 text-right tabular-nums" bind:value={endText} aria-label="Trim end" aria-invalid={!!pointsError} />
        </label>
        {#if pointsError}<p class="text-secondary text-danger" role="alert">{pointsError}</p>{/if}
        <button class="btn-sm self-end" type="submit">Apply</button>
      </form>
    </Popover>

    <Popover bind:this={mix} label="Audio mix" menu={false}>
      <div class="flex w-80 flex-col gap-3 p-4">
        <div>
          <div class="font-semibold">Audio mix</div>
          <p class="mt-0.5 text-secondary text-fg-2">Levels in the copied or saved clip. Playback volume doesn't change them.</p>
        </div>
        {#each levels as level, i (i)}
          <div class="flex items-center gap-3">
            <span class="w-12 flex-none text-fg-2">{trackName(i)}</span>
            <Fader label="{trackName(i)} level" class="flex-1!" value={level} max={200} step={5} detent={100} text="{level}%" oninput={(v) => (levelEdits = levels.with(i, v))} />
            <span class="w-11 flex-none text-right tabular-nums {level === 100 ? 'text-fg-2' : ''}">{level}%</span>
          </div>
        {/each}
        {#if mixed}<button class="btn-text -mr-2 self-end" onclick={() => (levelEdits = [])}>Reset levels</button>{/if}
      </div>
    </Popover>
  </header>

  <!-- The picture at its own shape; its frame is exactly the footage, no
       black panel round it. (No container query or transform up the tree:
       either would trap the fullscreen picture inside it.) -->
  <div class="mt-5 flex flex-none justify-center">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class={['player overflow-hidden bg-black', fullscreen ? 'fixed inset-0 z-50' : 'relative rounded-frame', { 'hide-cursor': fullscreen && !showControls }]}
      style:aspect-ratio={fullscreen ? undefined : aspect}
      style:width={fullscreen ? undefined : `${pictureW}px`}
      onpointermove={poke}
      onpointerleave={() => (active = false)}
    >
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={video}
        bind:paused
        src={videoUrl(clip.path)}
        onloadedmetadata={loaded}
        ontimeupdate={tick}
        onended={ended}
        onclick={toggle}
        ondblclick={() => setFullscreen(!fullscreen)}
        preload="auto"
        use:playbackStats={clip.path}
        class="size-full object-contain"
      ></video>

      {#if vertical !== null}
        <!-- The 9:16 window that will be kept; drag it sideways. -->
        <div class="pointer-events-none absolute inset-0">
          <div class="absolute inset-y-0 left-0 bg-black/60" style:width="{vertical * (1 - frameWidth) * 100}%"></div>
          <div class="absolute inset-y-0 right-0 bg-black/60" style:width="{(1 - vertical) * (1 - frameWidth) * 100}%"></div>
          <div
            class="crop-frame pointer-events-auto absolute inset-y-0"
            style:left="{vertical * (1 - frameWidth) * 100}%"
            style:width="{frameWidth * 100}%"
            onpointerdown={cropDown}
            onpointermove={cropMove}
            onpointerup={() => (cropDrag = null)}
            role="slider"
            aria-label="Vertical frame position"
            aria-valuemin={0}
            aria-valuemax={1}
            aria-valuenow={vertical}
            tabindex="-1"
          ></div>
        </div>
      {/if}

      {#if fullscreen}
        <!-- Fullscreen has no row under it: the timeline and the essentials
             over the picture's bottom edge, hidden while it plays. -->
        <div class="absolute inset-x-0 bottom-0 bg-black/75 px-2 pt-4 pb-2 transition-opacity duration-normal {showControls ? 'opacity-100' : 'pointer-events-none opacity-0'}">
          {@render timeline(null)}
          <div class="mt-2 flex items-center gap-3 px-4 text-white">
            <button class="btn-icon text-white hover:not-disabled:bg-white/15" onclick={toggle} aria-label={paused ? 'Play' : 'Pause'}>
              {#if paused}<Play size={18} fill="currentColor" strokeWidth={0} />{:else}<Pause size={18} fill="currentColor" strokeWidth={0} />{/if}
            </button>
            <span class="tabular-nums">{length(time)} <span class="text-white/60">/ {length(duration)}</span></span>
            <div class="flex-1"></div>
            <button class="btn-icon text-white hover:not-disabled:bg-white/15" onclick={() => setFullscreen(false)} aria-label="Exit fullscreen" title="Exit fullscreen (F)">
              <Minimize size={18} />
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>

  {#if !fullscreen}
    <div class="mt-3.5 flex-none">{@render timeline(peaks)}</div>
  {/if}

  <!-- Playback on the left; what to do with the clip on the right. -->
  <div class="relative mt-2.5 flex h-10 flex-none items-center gap-1" data-deck>
    <button class="btn-icon -ml-2 size-10 text-fg" onclick={toggle} aria-label={paused ? 'Play' : 'Pause'} title="{paused ? 'Play' : 'Pause'} (Space)">
      {#if paused}<Play size={18} fill="currentColor" strokeWidth={0} />{:else}<Pause size={18} fill="currentColor" strokeWidth={0} />{/if}
    </button>
    <span class="flex-none pr-2 pl-0.5 whitespace-nowrap tabular-nums" aria-label="Time">{length(time)} <span class="text-fg-2">/ {length(duration)}</span></span>

    <Popover label="Playback volume" menu={false} side="top" align="start">
      {#snippet trigger(props)}
        <button {...props} class="btn-icon" aria-label="Playback volume" title="Playback volume (M mutes)">
          {#if heard.muted || heard.volume === 0}<VolumeX size={19} />{:else if heard.volume < 0.5}<Volume1 size={19} />{:else}<Volume2 size={19} />{/if}
        </button>
      {/snippet}
      <div class="flex items-center gap-2 py-2 pr-4 pl-2">
        <button class="btn-icon-sm hover:not-disabled:bg-hover" onclick={() => (heard.muted = !heard.muted)} aria-label={heard.muted ? 'Unmute' : 'Mute'} aria-pressed={heard.muted} title="{heard.muted ? 'Unmute' : 'Mute'} (M)">
          {#if heard.muted}<VolumeX size={17} />{:else}<Volume2 size={17} />{/if}
        </button>
        <Fader label="Playback volume" class="w-36!" value={heard.muted ? 0 : heard.volume} max={1} step={0.05} text="{Math.round((heard.muted ? 0 : heard.volume) * 100)}%" oninput={(v) => ((heard.volume = v), (heard.muted = false))} />
        <span class="w-10 flex-none text-right text-secondary text-fg-2 tabular-nums">{Math.round((heard.muted ? 0 : heard.volume) * 100)}%</span>
      </div>
    </Popover>
    <button class="btn-icon" onclick={() => setFullscreen(true)} aria-label="Fullscreen" title="Fullscreen (F)">
      <Maximize size={17} />
    </button>

    <!-- What is selected, once it isn't the whole clip as it is. -->
    <div class="flex min-w-0 flex-initial items-center gap-1 pl-3">
      {#if summary}
        <span class="truncate text-secondary text-fg-2 tabular-nums" title={summary}>{summary}</span>
        <button class="btn-text" onclick={reset} title="Back to the whole clip as it is">Reset</button>
      {/if}
    </div>

    <div class="min-w-2 flex-1"></div>

    <!-- What is happening or just happened. Said here, beside the actions:
         progress with its Cancel, then the outcome, then why it can't be done. -->
    <div class="flex min-w-0 flex-initial items-center gap-1 text-secondary" role="status" aria-live="polite">
      {#if busy}
        <span class="truncate tabular-nums">{busy === 'save' ? 'Saving clip' : 'Preparing clip'}… {Math.round(progress * 100)}%</span>
        <button class="btn-text" onclick={oncancel}>Cancel</button>
      {:else if notice}
        {#key notice.text}
          <span class="said truncate font-medium {notice.error ? 'text-danger' : ''}" title={notice.text}>{notice.text}</span>
          {#if notice.action}
            <button class="btn-text" onclick={notice.action.run}>{notice.action.label}</button>
          {/if}
          <button class="btn-icon-sm text-muted" onclick={ondismiss} aria-label="Dismiss">
            <X size={15} />
          </button>
        {/key}
      {:else if planError}
        <span class="truncate text-danger" title={planError}>{planError.charAt(0).toUpperCase() + planError.slice(1)}</span>
      {/if}
      <!-- The button says Copied; this says it to a screen reader. -->
      {#if copied && !notice}<span class="sr-only">Clip copied</span>{/if}
    </div>

    <!-- Both labels share one cell, so the button keeps its width. -->
    <button class="btn-primary ml-2" disabled={!canCopy} onclick={copy} title="Put this clip on the clipboard as a file, to paste into a chat or a folder (Ctrl+C). {copyAsIs ? '' : fit}">
      <span class="grid">
        <span class="col-start-1 row-start-1 text-center {copied ? 'invisible' : ''}">Copy clip</span>
        <span class="col-start-1 row-start-1 inline-flex items-center justify-center gap-1.5 {copied ? '' : 'invisible'}"><Check size={16} />Copied</span>
      </span>
    </button>
    <button class="btn" disabled={!canSave} onclick={save} title="Save the selection as a new clip in the library. The original stays as it is. {prefs.shareLimitSave ? fit : ''}">
      Save clip
    </button>

    {#if busy}
      <div class="absolute inset-x-0 -bottom-1.5 h-0.5 origin-left bg-accent transition-transform duration-normal" style:transform="scaleX({progress})" role="progressbar" aria-label={busy === 'save' ? 'Saving clip' : 'Preparing clip'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(progress * 100)}></div>
    {/if}
  </div>
  </div>
  <div class="min-h-6 flex-[2]"></div>
</div>

{#if help}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-60 flex items-center justify-center bg-black/60" onclick={() => (help = false)}>
    <div class="dialog-surface max-h-[calc(100vh-32px)] w-md overflow-y-auto p-6" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Keyboard shortcuts" tabindex="-1">
      <h2 class="mb-4 text-title font-semibold">Keyboard shortcuts</h2>
      <dl class="grid grid-cols-[auto_1fr] gap-x-8 gap-y-2">
        {#each SHORTCUTS as [keys, what] (keys)}
          <dt class="font-medium whitespace-nowrap">{keys}</dt>
          <dd class="text-fg-2">{what}</dd>
        {/each}
      </dl>
    </div>
  </div>
{/if}

<style>
  /* The library becomes the viewer: a short fade, and gone at once (Esc
     is a key; keys are never made to wait). */
  .viewer {
    transition: opacity var(--duration-normal) var(--ease-out);
    @starting-style {
      opacity: 0;
    }
  }

  .said {
    transition: opacity var(--duration-normal) var(--ease-out);
    @starting-style {
      opacity: 0;
    }
  }

  .hide-cursor,
  .hide-cursor video {
    cursor: none;
  }

  .crop-frame {
    cursor: ew-resize;
    box-shadow:
      inset 0 0 0 2px var(--color-accent),
      0 0 0 1px rgb(0 0 0 / 40%);
    touch-action: none;
  }
</style>
