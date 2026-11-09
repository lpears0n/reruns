<!--
  Watch a clip, or trim it (in/out, audio mix, render) on the same screen.
  Playback controls overlay the video's bottom edge and hide while playing.
-->
<script lang="ts">
  import { ArrowLeft, CircleHelp, Maximize, Minimize, Pause, Pencil, Play, Plus, Repeat, Star, Volume2, VolumeX, X } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { api, type Clip, type ClipPatch, type ExportPlan, videoUrl } from './api'
  import { size, timecode, trackName, when } from './format'
  import { playbackStats } from './playbackStats'
  import Timeline from './Timeline.svelte'

  let {
    clip,
    rendering,
    progress = 0,
    onback,
    onrender,
    oncancel,
    ondelete,
    onshare,
    oncopy,
    onupdate,
  }: {
    clip: Clip
    rendering: boolean
    /** Fraction of the render done, while `rendering`. */
    progress?: number
    onback: () => void
    /** `maxMb`: fit the file under this size; null keeps full quality.
     *  `vertical`: crop to 9:16 at this position (0 left, 1 right).
     *  `slow`: play this range slower (factor 2 = half speed). */
    onrender: (
      start: number,
      end: number,
      volumes: number[],
      maxMb: number | null,
      vertical: number | null,
      slow: { from: number; to: number; factor: number } | null,
    ) => void
    oncancel: () => void
    ondelete: () => void
    onshare: () => void
    /** Put the clip on the clipboard as a file. */
    oncopy: () => void
    /** Absent for videos opened from outside the library. */
    onupdate?: (patch: ClipPatch) => void
  } = $props()

  const subtitle = $derived(
    [
      clip.title ? clip.game : null,
      when(clip.created_ms),
      size(clip.size_bytes),
      clip.is_edit ? 'edited' : null,
    ]
      .filter(Boolean)
      .join(' · '),
  )

  // Title editing and label entry.
  let editingTitle = $state(false)
  let titleDraft = $state('')
  let titleInput: HTMLInputElement | undefined = $state()
  let addingLabel = $state(false)
  let labelDraft = $state('')
  let labelInput: HTMLInputElement | undefined = $state()

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
    const l = labelDraft.trim()
    labelDraft = ''
    if (l) onupdate?.({ labels: [...clip.labels, l] })
  }

  function removeLabel(l: string) {
    onupdate?.({ labels: clip.labels.filter((x) => x !== l) })
  }

  let video: HTMLVideoElement
  let duration = $state(0)
  let time = $state(0)
  let paused = $state(true)
  let muted = $state(false)
  let volume = $state(1)
  let aspect = $state(16 / 9)
  let fullscreen = $state(false)
  let trimming = $state(false)
  let start = $state(0)
  let end = $state(0)
  /** Percent per audio track. */
  let levels = $state<number[]>([])
  let armed = $state(false)
  let speed = $state(1)
  // Export shape: a 9:16 window (its position across the picture) for
  // TikTok, Shorts and Reels, and a slow-motion range inside the trim.
  let vertical = $state<number | null>(null)
  let slowFactor = $state(1)
  let slowFrom = $state(0)
  let slowTo = $state(0)
  const slowAdds = $derived(
    slowFactor > 1 ? Math.max(0, Math.min(slowTo, end) - Math.max(slowFrom, start)) * (slowFactor - 1) : 0,
  )
  /** How long the rendered file will be. */
  const outLength = $derived(end - start + slowAdds)

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

  // Controls show while paused, or briefly after the pointer moves.
  let active = $state(true)
  let idleTimer: ReturnType<typeof setTimeout>
  const showControls = $derived(paused || active || trimming)

  function poke() {
    active = true
    clearTimeout(idleTimer)
    idleTimer = setTimeout(() => (active = false), 2200)
  }

  // Reset when a different clip is loaded, and only then: the parent swaps
  // in a new clip object whenever this one is starred, renamed, labelled
  // or its thumbnail arrives, which mustn't throw away a trim in progress.
  let shownPath: string | null = null
  $effect(() => {
    const path = clip.path
    if (path === shownPath) return
    shownPath = path
    trimming = false
    start = 0
    end = untrack(() => clip.duration) ?? 0
  })

  // Track count can arrive later (thumbnail pool); keep one level per
  // source track. A clip that starts with game and mic mixed into one has
  // a level for each of the tracks after it, not for the mix.
  const sourceTracks = $derived(Math.max(0, clip.audio_tracks - (clip.mixed_audio ? 1 : 0)))
  $effect(() => {
    if (levels.length !== sourceTracks) levels = Array.from({ length: sourceTracks }, () => 100)
  })

  function loaded() {
    duration = video.duration
    if (video.videoWidth && video.videoHeight) aspect = video.videoWidth / video.videoHeight
    if (!end || end > duration) end = duration
  }

  function tick() {
    time = video.currentTime
    const rate = trimming && slowFactor > 1 && time >= slowFrom && time < slowTo ? speed / slowFactor : speed
    if (video.playbackRate !== rate) video.playbackRate = rate
    // While trimming, preview only the selection.
    if (trimming && !paused && time >= end) {
      if (loop) seek(start)
      else {
        video.pause()
        video.currentTime = start
      }
    }
  }

  // The video's timeupdate event comes only ~4 times a second, which made
  // the progress line jump. While playing, follow the playhead on every
  // display frame instead.
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
      seek(trimming ? start : 0)
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

  function cycleSpeed() {
    setSpeed(SPEEDS[(SPEEDS.indexOf(speed) + 1) % SPEEDS.length])
  }

  function nudgeSpeed(dir: 1 | -1) {
    const i = Math.min(SPEEDS.length - 1, Math.max(0, SPEEDS.indexOf(speed) + dir))
    setSpeed(SPEEDS[i])
  }

  function seek(t: number) {
    video.currentTime = t
    time = t
  }

  function toggle() {
    if (video.paused) {
      if (trimming && (time >= end - 0.05 || time < start)) seek(start)
      video.play()
    } else {
      video.pause()
    }
    poke()
  }

  async function setFullscreen(on: boolean) {
    fullscreen = on
    try {
      await getCurrentWindow().setFullscreen(on)
    } catch {
      // Without the window going fullscreen it still fills the app.
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return
    if (help && e.code !== 'Escape' && e.key !== '?') return
    if ((e.ctrlKey || e.metaKey) && e.code === 'KeyC') {
      // Text selected on the page copies as text, as usual.
      if (window.getSelection()?.toString()) return
      e.preventDefault()
      oncopy()
      return
    }
    if (e.code === 'Space' || e.code === 'KeyK') {
      e.preventDefault()
      toggle()
    } else if (e.code === 'KeyF') {
      setFullscreen(!fullscreen)
    } else if (e.code === 'KeyM') {
      muted = !muted
    } else if (e.key === '?') {
      help = !help
    } else if (e.code === 'Escape') {
      if (help) help = false
      else if (fullscreen) setFullscreen(false)
      else if (trimming) trimming = false
      else onback()
    } else if (e.code === 'ArrowLeft') {
      seek(Math.max(0, time - 5))
      poke()
    } else if (e.code === 'ArrowRight') {
      seek(Math.min(duration, time + 5))
      poke()
    } else if (e.code === 'KeyJ') {
      seek(Math.max(0, time - 10))
      poke()
    } else if (e.code === 'KeyL') {
      seek(Math.min(duration, time + 10))
      poke()
    } else if (e.code === 'Home') {
      seek(trimming ? start : 0)
      poke()
    } else if (e.code === 'End') {
      seek(trimming ? end : duration)
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
    } else if (e.code === 'KeyT') {
      trimming = !trimming
    } else if (e.code === 'KeyS' && onupdate) {
      onupdate({ favorite: !clip.favorite })
    } else if (e.code === 'KeyN' && onupdate) {
      e.preventDefault()
      startTitle()
    } else if (trimming && e.code === 'KeyI') {
      start = Math.min(time, end - 0.1)
    } else if (trimming && e.code === 'KeyO') {
      end = Math.max(time, start + 0.1)
    }
  }

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

  // Export size: full quality, or fitted under an upload limit.
  const SIZES: [number, string][] = [
    [0, 'Full quality'],
    [10, '10 MB · Discord'],
    [50, '50 MB · Nitro Basic'],
    [500, '500 MB · Nitro'],
  ]
  const SIZE_KEY = 'reruns.export-size'
  let sizeMb = $state(readSize())
  let plan = $state<ExportPlan | null>(null)
  let planError = $state<string | null>(null)

  function readSize(): number {
    try {
      const v = Number(localStorage.getItem(SIZE_KEY))
      return SIZES.some(([mb]) => mb === v) ? v : 0
    } catch {
      return 0
    }
  }

  function setSize(mb: number) {
    sizeMb = mb
    try {
      localStorage.setItem(SIZE_KEY, String(mb))
    } catch {
      // Remembering the choice is a convenience.
    }
  }

  $effect(() => {
    const mb = sizeMb
    const seconds = outLength
    const audio = clip.audio_tracks > 0
    if (!trimming || mb === 0 || seconds < 0.1) {
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

  const planLabel = $derived(
    plan ? `${plan.max_height}p${plan.fps ?? ''} · ${(plan.video_kbps / 1000).toFixed(1)} Mb/s` : null,
  )

  const SHORTCUTS: [string, string][] = [
    ['Space / K', 'Play or pause'],
    ['J / L', 'Back or forward 10 s'],
    ['← / →', 'Back or forward 5 s'],
    [', / .', 'Previous or next frame'],
    ['< / >', 'Slower or faster'],
    ['Home / End', 'Jump to start or end'],
    ['[ / ]', 'Previous or next marked moment'],
    ['R', 'Repeat'],
    ['S', 'Star (favourite)'],
    ['N', 'Rename'],
    ['M', 'Mute'],
    ['F', 'Fullscreen'],
    ['T', 'Trim'],
    ['Ctrl+C', 'Copy clip'],
    ['I / O', 'Set trim start / end'],
    ['Esc', 'Back'],
    ['?', 'This list'],
  ]

  // The <video> element plays the first audio track; mirror its level when
  // trimming, unless that track is the mix (no one level applies to it).
  $effect(() => {
    if (!video) return
    video.volume = trimming && !clip.mixed_audio ? Math.min(1, (levels[0] ?? 100) / 100) : volume
    video.muted = muted
  })

  $effect(() => () => {
    clearTimeout(idleTimer)
    if (fullscreen) getCurrentWindow().setFullscreen(false).catch(() => {})
  })
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col gap-4">
  <div class="flex items-center gap-2">
    <button class="btn-icon -ml-1.5" onclick={onback} title="Back (Esc)" aria-label="Back">
      <ArrowLeft size={20} />
    </button>
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        {#if editingTitle}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            bind:this={titleInput}
            bind:value={titleDraft}
            class="field display-face h-8 w-full max-w-lg text-title font-semibold"
            placeholder={clip.game}
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
          <button
            class="group/title display-face flex min-w-0 items-center gap-2 truncate text-title font-semibold {onupdate ? '' : 'cursor-default'}"
            onclick={startTitle}
            title={onupdate ? 'Rename (N)' : undefined}
            disabled={!onupdate}
          >
            <span class="truncate">{clip.title ?? clip.game}</span>
            {#if onupdate}<Pencil size={13} class="flex-none text-muted opacity-0 transition-opacity duration-fast group-hover/title:opacity-100" />{/if}
          </button>
        {/if}
        {#if onupdate}
          <button
            class="btn-icon size-7 flex-none {clip.favorite ? 'text-accent' : 'text-muted'}"
            onclick={() => onupdate({ favorite: !clip.favorite })}
            title={clip.favorite ? 'Remove from favourites (S)' : 'Add to favourites (S)'}
            aria-label={clip.favorite ? 'Remove from favourites' : 'Add to favourites'}
            aria-pressed={clip.favorite}
          >
            <Star size={16} fill={clip.favorite ? 'currentColor' : 'none'} />
          </button>
        {/if}
      </div>
      <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-secondary text-muted tabular-nums">
        <span class="truncate">{subtitle}</span>
        {#if onupdate || clip.labels.length}
          <span class="flex flex-wrap items-center gap-1">
            {#each clip.labels as l (l)}
              <span class="chip gap-1 pr-1">
                {l}
                {#if onupdate}
                  <button class="rounded-full hover:text-white" onclick={() => removeLabel(l)} aria-label="Remove label {l}">
                    <X size={11} />
                  </button>
                {/if}
              </span>
            {/each}
            {#if onupdate}
              {#if addingLabel}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  bind:this={labelInput}
                  bind:value={labelDraft}
                  class="chip h-5 w-28 bg-canvas outline-none focus:border-accent"
                  placeholder="New label"
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
                  aria-label="New label"
                />
              {:else}
                <button class="chip gap-0.5 border-dashed text-muted hover:text-fg" onclick={() => (addingLabel = true)}>
                  <Plus size={11} /> Label
                </button>
              {/if}
            {/if}
          </span>
        {/if}
      </div>
    </div>
    <div class="flex items-center text-body font-medium">
      <button class="action {trimming ? 'text-accent' : ''}" onclick={() => (trimming = !trimming)} title="Trim (T)">
        {trimming ? 'Done' : 'Trim'}
      </button>
      <button class="action" onclick={oncopy} title="Copy (Ctrl+C)">Copy</button>
      <button class="action" onclick={onshare}>Show in folder</button>
      <button
        class="action {armed ? 'text-danger' : ''}"
        onclick={() => (armed ? ondelete() : (armed = true))}
        onmouseleave={() => (armed = false)}
      >
        {armed ? 'Delete clip?' : 'Delete'}
      </button>
      <button class="btn-icon ml-1" onclick={() => (help = !help)} title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts">
        <CircleHelp size={18} />
      </button>
    </div>
  </div>

  <!-- Sized to the clip's own aspect ratio, so ultrawide clips have no bars. -->
  <div class="flex min-h-0 flex-1 items-center justify-center [container-type:size]">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="player group relative overflow-hidden bg-black {fullscreen ? 'fixed inset-0 z-50' : 'rounded-surface'}"
      class:hide-cursor={!showControls}
      style:aspect-ratio={fullscreen ? undefined : aspect}
      style:width={fullscreen ? undefined : `min(100cqw, calc(100cqh * ${aspect}))`}
      onpointermove={poke}
      onpointerleave={() => (active = false)}
    >
      <!-- svelte-ignore a11y_media_has_caption, a11y_click_events_have_key_events -->
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

      {#if trimming && vertical !== null}
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

      <!-- Controls and the progress line, over the video's bottom edge. -->
      <div
        class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/70 via-black/25 to-transparent px-4 pt-10 pb-2 transition-opacity duration-normal
          {showControls ? 'opacity-100' : 'pointer-events-none opacity-0'}"
      >
        <Timeline
          {duration}
          {time}
          trim={trimming}
          markers={clip.markers}
          bind:start
          bind:end
          slow={slowFactor > 1}
          bind:slowFrom
          bind:slowTo
          onseek={seek}
        />
        <div class="mt-1 flex items-center gap-1 text-white">
          <button class="glyph" onclick={toggle} aria-label={paused ? 'Play' : 'Pause'} title="Play/Pause (Space)">
            {#if paused}<Play size={20} fill="currentColor" strokeWidth={0} />{:else}<Pause size={20} fill="currentColor" strokeWidth={0} />{/if}
          </button>
          <span class="ml-1 text-secondary tabular-nums">
            {timecode(time)}<span class="text-white/55">{' / '}{timecode(duration)}</span>
          </span>
          <div class="flex-1"></div>
          <button class="glyph w-auto px-2 text-secondary font-semibold tabular-nums" onclick={cycleSpeed} title="Playback speed (< >)" aria-label="Playback speed {speed}x">
            {speed}×
          </button>
          <button class="glyph {loop ? 'text-accent' : ''}" onclick={() => (loop = !loop)} title="Repeat (R)" aria-label="Repeat" aria-pressed={loop}>
            <Repeat size={17} />
          </button>
          <div class="volume flex items-center">
            <input
              type="range"
              min="0"
              max="1"
              step="0.05"
              bind:value={volume}
              oninput={() => (muted = false)}
              class="slider w-0 opacity-0 transition-[width,opacity] duration-normal"
              style:--fill="{(muted ? 0 : volume) * 100}%"
              aria-label="Volume"
            />
            <button class="glyph" onclick={() => (muted = !muted)} aria-label={muted ? 'Unmute' : 'Mute'} title="Mute (M)">
              {#if muted || volume === 0}<VolumeX size={19} />{:else}<Volume2 size={19} />{/if}
            </button>
          </div>
          <button
            class="glyph"
            onclick={() => setFullscreen(!fullscreen)}
            aria-label={fullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            title="Fullscreen (F)"
          >
            {#if fullscreen}<Minimize size={18} />{:else}<Maximize size={18} />{/if}
          </button>
        </div>
        </div>
    </div>
  </div>

  {#if trimming}
    <!-- Trimming is done on the line over the video; this is the rest. -->
    <div class="flex flex-wrap items-center gap-x-5 gap-y-3 text-secondary">
      <button class="action -ml-2 tabular-nums" onclick={() => (start = Math.min(time, end - 0.1))} title="Set start to playhead (I)">
        <span class="text-muted">In</span>&nbsp;{timecode(start)}
      </button>
      <button class="action -ml-3 tabular-nums" onclick={() => (end = Math.max(time, start + 0.1))} title="Set end to playhead (O)">
        <span class="text-muted">Out</span>&nbsp;{timecode(end)}
      </button>
      {#each levels as _, i (i)}
        <label class="flex items-center gap-2.5">
          <span class="text-muted">{trackName(i)}</span>
          <input type="range" min="0" max="200" step="5" bind:value={levels[i]} class="w-24" style:--fill="{levels[i] / 2}%" />
          <span class="w-10 tabular-nums">{levels[i]}%</span>
        </label>
      {/each}
      <!-- The export half; it moves to its own line when the row is full. -->
      <div class="ml-auto flex items-center gap-5 whitespace-nowrap">
      <label class="flex items-center gap-2.5">
        <span class="text-muted">Format</span>
        <select
          class="field w-36 flex-none"
          value={vertical === null ? 'original' : 'vertical'}
          onchange={(e) => (vertical = e.currentTarget.value === 'vertical' ? (vertical ?? 0.5) : null)}
          aria-label="Export format"
        >
          <option value="original">Original</option>
          <option value="vertical">Vertical 9:16</option>
        </select>
      </label>
      <label class="flex items-center gap-2.5" title="Slows the striped range (muted)">
        <span class="text-muted">Slow-mo</span>
        <select class="field w-24 flex-none" value={slowFactor} onchange={(e) => setSlow(Number(e.currentTarget.value))} aria-label="Slow motion">
          <option value={1}>Off</option>
          <option value={2}>½×</option>
          <option value={4}>¼×</option>
        </select>
      </label>
      <label class="flex items-center gap-2.5">
        <span class="text-muted">Size</span>
        <select class="field w-44 flex-none" value={sizeMb} onchange={(e) => setSize(Number(e.currentTarget.value))} aria-label="Export size">
          {#each SIZES as [mb, label] (mb)}
            <option value={mb}>{label}</option>
          {/each}
        </select>
      </label>
      {#if planError}
        <span class="max-w-64 whitespace-normal text-danger" role="status">{planError.charAt(0).toUpperCase() + planError.slice(1)}</span>
      {:else if planLabel}
        <span class="text-muted tabular-nums" role="status" title="How the clip will be fitted">{planLabel}</span>
      {/if}
      <span class="text-muted">Length <span class="font-semibold text-fg tabular-nums">{timecode(outLength)}</span></span>
      {#if rendering}
        <div class="flex items-center gap-3">
          <div class="h-1.5 w-32 overflow-hidden rounded-full bg-line-strong" role="progressbar" aria-valuenow={Math.round(progress * 100)} aria-valuemin={0} aria-valuemax={100}>
            <div class="h-full rounded-full bg-accent transition-[width] duration-normal" style:width="{progress * 100}%"></div>
          </div>
          <span class="w-9 text-fg tabular-nums">{Math.round(progress * 100)}%</span>
          <button class="btn h-9" onclick={oncancel}>Cancel</button>
        </div>
      {:else}
        <button
          class="btn-primary"
          disabled={end - start < 0.1 || planError !== null}
          onclick={() =>
            onrender(
              start,
              end,
              levels.map((l) => l / 100),
              sizeMb || null,
              vertical,
              slowFactor > 1 ? { from: slowFrom, to: slowTo, factor: slowFactor } : null,
            )}
        >
          Render clip
        </button>
      {/if}
      </div>
    </div>
  {/if}
</div>

{#if help}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-60 flex items-center justify-center bg-black/60" onclick={() => (help = false)}>
    <div class="floating w-md p-5" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Keyboard shortcuts" tabindex="-1">
      <div class="display-face mb-3 text-title font-semibold">Keyboard shortcuts</div>
      <dl class="grid grid-cols-[auto_1fr] gap-x-5 gap-y-1.5 text-secondary">
        {#each SHORTCUTS as [keys, what] (keys)}
          <dt class="flex gap-1 whitespace-nowrap">
            {#each keys.split(' / ') as k, i (i)}
              {#if i > 0}<span class="text-muted">/</span>{/if}<kbd>{k}</kbd>
            {/each}
          </dt>
          <dd class="self-center text-fg-2">{what}</dd>
        {/each}
      </dl>
    </div>
  </div>
{/if}

<style>
  /* Borderless text actions in the header. */
  .action {
    padding: 6px 10px;
    border-radius: var(--radius-ctl);
    color: var(--color-fg-2);
    transition:
      color var(--duration-fast),
      background var(--duration-fast);
  }

  .action:hover {
    color: var(--color-fg);
    background: var(--color-raised);
  }

  /* Bare icon controls on the video. */
  .glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    opacity: 0.9;
    transition: opacity var(--duration-fast);
  }

  .glyph:hover {
    opacity: 1;
  }

  /* Volume slider slides out next to the speaker icon. */
  .volume:hover .slider,
  .volume:focus-within .slider {
    width: 84px;
    opacity: 1;
    margin-right: 4px;
  }

  .slider::-webkit-slider-thumb {
    width: 10px;
    height: 10px;
  }

  .hide-cursor,
  .hide-cursor video {
    cursor: none;
  }

  /* Seek bar on the video: lighter rail. */
  .crop-frame {
    cursor: ew-resize;
    box-shadow:
      inset 0 0 0 2px var(--color-accent),
      0 0 0 1px rgb(0 0 0 / 40%);
    touch-action: none;
  }

  .player :global(.rail) {
    background: rgba(255, 255, 255, 0.25);
  }
</style>
