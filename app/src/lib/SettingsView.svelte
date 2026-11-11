<script lang="ts" module>
  export const categories = ['General', 'Recording', 'Audio', 'Hotkeys', 'Storage', 'Appearance'] as const
  export type Category = (typeof categories)[number]
</script>

<script lang="ts">
  import { AudioLines, Check, FolderOpen, HardDrive, Keyboard, Palette, Play, SlidersHorizontal, Video } from '@lucide/svelte'
  import { open as openDialog } from '@tauri-apps/plugin-dialog'
  import {
    api,
    type Capabilities,
    type Codec,
    type Encoder,
    type HotkeyBackend,
    type Settings,
    type Sound,
    type Status,
    type UpdateInfo,
    type Usage,
    type AudioLevel,
    onAudioLevel,
    onUpdateProgress,
  } from './api'
  import { bytes } from './format'
  import HotkeyBinder from './HotkeyBinder.svelte'
  import LevelMeter from './LevelMeter.svelte'
  import { ACCENTS, type Accent, prefs, setPrefs } from './prefs.svelte'
  import {
    CODEC_LABEL,
    ENCODER_LABEL,
    FPS_OPTIONS,
    activePreset,
    bytesFor,
    presets,
    supportedCodecs,
  } from './quality'
  import Row from './Row.svelte'
  import Segmented from './Segmented.svelte'
  import Toggle from './Toggle.svelte'

  let {
    settings,
    usage,
    caps,
    status,
    category = $bindable(),
    update: available = $bindable(null),
    onchange,
    onchoosescreen,
  }: {
    settings: Settings
    usage: Usage | null
    caps: Capabilities | null
    status: Status | null
    category: Category
    /** A newer release, once one has been found. */
    update?: UpdateInfo | null
    onchange: (s: Settings) => Promise<void>
    onchoosescreen: () => void
  } = $props()

  // Which hotkey service the desktop provided (loaded when Hotkeys opens).
  let hotkeyBackend = $state<HotkeyBackend | null | undefined>(undefined)
  $effect(() => {
    if (category === 'Hotkeys') api.hotkeyBackend().then((b) => (hotkeyBackend = b))
  })
  // Live levels while Audio is open, for the devices a recording would
  // use; restarted when one of them changes, stopped on leaving the page.
  let levels = $state<Partial<Record<AudioLevel['track'], AudioLevel>>>({})
  $effect(() => {
    if (category !== 'Audio') return
    // Restart on any change to what is recorded.
    void [settings.desktop_audio, settings.desktop_device, settings.mic, settings.mic_device]
    levels = {}
    const unlisten = onAudioLevel((l) => (levels = { ...levels, [l.track]: l }))
    api.startAudioMeters().catch(() => {})
    return () => {
      api.stopAudioMeters().catch(() => {})
      unlisten.then((f) => f())
    }
  })

  let copied = $state(false)
  async function copyCommand() {
    await navigator.clipboard.writeText('rerunsctl save')
    copied = true
    setTimeout(() => (copied = false), 1500)
  }

  const icons = {
    General: SlidersHorizontal,
    Recording: Video,
    Audio: AudioLines,
    Hotkeys: Keyboard,
    Storage: HardDrive,
    Appearance: Palette,
  }

  const lengths = [15, 30, 60, 120, 180, 300]
  const audioRates = [128, 160, 192, 256, 320]
  // Same steps as Medal's PC Storage limit.
  const limits = [1, 5, 10, 25, 50, 100, 250, 500]

  const update = (patch: Partial<Settings>) => onchange({ ...settings, ...patch })

  // Recording
  const hz = $derived(caps?.display_hz ? Math.round(caps.display_hz) : null)
  const codecs = $derived(supportedCodecs(caps))
  const preset = $derived(activePreset(settings, caps))
  const clipBytes = $derived(bytesFor(settings, settings.replay_seconds))
  const encoderOptions = $derived(
    (caps?.encoders ?? []).map((e) => ({ ...e, ok: e.codecs.includes(settings.codec) })),
  )
  const autoEncoder = $derived(encoderOptions.find((e) => e.ok)?.encoder)

  function setCodec(codec: Codec) {
    // Drop a manual encoder choice that can't do the new codec.
    const keep = settings.encoder && caps?.encoders.find((e) => e.encoder === settings.encoder)?.codecs.includes(codec)
    update({ codec, encoder: keep ? settings.encoder : null })
  }

  function applyPreset(id: string) {
    const p = presets(caps).find((p) => p.id === id)
    if (p) update({ fps: p.fps, bitrate_kbps: p.bitrate_kbps, codec: p.codec, encoder: null })
  }

  const lengthLabel = (s: number) => (s < 60 ? `${s} seconds` : `${s / 60} minute${s > 60 ? 's' : ''}`)

  // Storage
  let customLimit = $state(false)
  const limitChoice = $derived(
    settings.storage_limit_gb == null
      ? 'none'
      : customLimit || !limits.includes(settings.storage_limit_gb)
        ? 'custom'
        : String(settings.storage_limit_gb),
  )

  function chooseLimit(v: string) {
    customLimit = v === 'custom'
    if (v === 'none') update({ storage_limit_gb: null })
    else if (v !== 'custom') update({ storage_limit_gb: Number(v) })
    else if (settings.storage_limit_gb == null) update({ storage_limit_gb: 20 })
  }

  // Updates
  let support = $state<{ configured: boolean; version: string } | null>(null)
  let checking = $state(false)
  let checked = $state(false)
  let installing = $state<number | null | undefined>(undefined)
  let updateError = $state<string | null>(null)
  $effect(() => {
    if (category === 'General' && !support) api.updateSupport().then((s) => (support = s))
  })

  async function checkNow() {
    checking = true
    updateError = null
    try {
      available = await api.checkUpdate()
      checked = true
    } catch (e) {
      updateError = String(e)
    } finally {
      checking = false
    }
  }

  async function install() {
    updateError = null
    installing = null
    const stop = await onUpdateProgress((f) => (installing = f))
    try {
      // Restarts into the new version, or waits for the game to close.
      const game = await api.installUpdate()
      if (game) updateError = `Downloaded. Installs when ${game} closes.`
    } catch (e) {
      updateError = String(e)
    } finally {
      installing = undefined
      stop()
    }
  }

  const updateText = $derived(
    !support
      ? ''
      : !support.configured
        ? 'This build can’t update itself.'
        : updateError
          ? updateError
          : available
            ? `Version ${available.version} available.`
            : checked
              ? 'Up to date.'
              : '',
  )

  // Logs: the error names the folder, so it can be found by hand.
  let logError = $state<string | null>(null)
  async function openLogs() {
    logError = null
    try {
      await api.openLogFolder()
    } catch (e) {
      logError = String(e)
    }
  }

  // Save sounds
  const SOUND_FILES = ['wav', 'mp3', 'ogg', 'oga', 'opus', 'flac', 'm4a']
  let soundError = $state<string | null>(null)
  const soundName = (s: Sound | null) =>
    s === null ? 'Same as clips' : s.kind === 'builtin' ? 'The built-in beep' : s.name || 'Your sound'

  async function chooseSound(slot: 'clip' | 'screenshot') {
    soundError = null
    const file = await openDialog({ filters: [{ name: 'Sounds', extensions: SOUND_FILES }] })
    if (typeof file !== 'string') return
    try {
      const sound = await api.importSound(file, slot)
      await update(slot === 'clip' ? { clip_sound: sound } : { screenshot_sound: sound })
      api.previewSound(sound, settings.sound_volume)
    } catch (e) {
      soundError = String(e)
    }
  }

  const previewSound = (s: Sound | null) => api.previewSound(s ?? settings.clip_sound, settings.sound_volume)

  async function browse() {
    const dir = await openDialog({ directory: true, defaultPath: settings.clips_dir })
    if (typeof dir === 'string') update({ clips_dir: dir })
  }

  const drive = $derived.by(() => {
    if (!usage || !usage.disk_total) return null
    const t = usage.disk_total
    const other = Math.max(0, t - usage.disk_free - usage.clips_bytes)
    return { clips: (usage.clips_bytes / t) * 100, other: (other / t) * 100 }
  })
</script>

<div class="mx-auto flex h-full max-w-(--container-settings) gap-8">
  <nav class="w-44 flex-none space-y-0.5">
    {#each categories as c (c)}
      {@const Icon = icons[c]}
      <button
        class="flex h-9 w-full items-center gap-3 rounded-ctl px-3 text-left text-body font-medium transition-colors duration-fast
          {category === c ? 'bg-raised text-fg' : 'text-fg-2 hover:bg-raised hover:text-fg'}"
        onclick={() => (category = c)}
      >
        <Icon size={16} class={category === c ? 'text-accent' : ''} />
        {c}
      </button>
    {/each}
  </nav>

  <div class="min-w-0 flex-1 space-y-6 pb-10">
    <h2 class="display-face text-page font-semibold">{category}</h2>

    {#if category === 'General'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Startup</h3>
        <div class="card divide-y divide-line">
          <Row title="Launch at login">
            <Toggle label="Launch at login" checked={settings.launch_at_login} onchange={(launch_at_login) => update({ launch_at_login })} />
          </Row>
          <Row title="Start in tray" disabled={!settings.launch_at_login}>
            <Toggle
              label="Start in tray"
              disabled={!settings.launch_at_login}
              checked={settings.start_in_tray}
              onchange={(start_in_tray) => update({ start_in_tray })}
            />
          </Row>
          <Row title="Closing the window">
            <select
              class="field"
              value={settings.close_to_tray ? 'tray' : 'quit'}
              onchange={(e) => update({ close_to_tray: e.currentTarget.value === 'tray' })}
            >
              <option value="tray">Minimizes to tray</option>
              <option value="quit">Quits Reruns</option>
            </select>
          </Row>
          <Row title="Show on Discord" description={'“Clipping <game>” on your profile while recording.'}>
            <Toggle
              label="Show on Discord"
              checked={settings.discord_presence}
              onchange={(discord_presence) => update({ discord_presence })}
            />
          </Row>
        </div>
      </section>
      <section class="space-y-2">
        <h3 class="section-title px-1">When a clip is saved</h3>
        <div class="card divide-y divide-line">
          <Row title="Show a notification">
            <Toggle label="Show a notification" checked={settings.notify_on_save} onchange={(notify_on_save) => update({ notify_on_save })} />
          </Row>
          <Row title="Play a sound">
            <Toggle label="Play a sound" checked={settings.sound_on_save} onchange={(sound_on_save) => update({ sound_on_save })} />
          </Row>
          {#if settings.sound_on_save}
            <Row title="Clip sound" description={soundName(settings.clip_sound)}>
              <div class="flex items-center gap-2">
                <button class="btn h-9 w-9 px-0" onclick={() => previewSound(settings.clip_sound)} aria-label="Play the clip sound" title="Play">
                  <Play size={15} />
                </button>
                {#if settings.clip_sound.kind === 'custom'}
                  <button class="btn h-9" onclick={() => update({ clip_sound: { kind: 'builtin' } })}>Use default</button>
                {/if}
                <button class="btn h-9" onclick={() => chooseSound('clip')}>Choose file…</button>
              </div>
            </Row>
            <Row title="Screenshot sound" description={soundName(settings.screenshot_sound)}>
              <div class="flex items-center gap-2">
                <button
                  class="btn h-9 w-9 px-0"
                  onclick={() => previewSound(settings.screenshot_sound)}
                  aria-label="Play the screenshot sound"
                  title="Play"
                >
                  <Play size={15} />
                </button>
                {#if settings.screenshot_sound !== null}
                  <button class="btn h-9" onclick={() => update({ screenshot_sound: null })}>Same as clips</button>
                {/if}
                <button class="btn h-9" onclick={() => chooseSound('screenshot')}>Choose file…</button>
              </div>
            </Row>
            <Row title="Sound volume">
              <div class="flex items-center gap-3">
                <input
                  type="range"
                  min="0"
                  max="200"
                  step="5"
                  value={settings.sound_volume}
                  aria-label="Sound volume"
                  class="w-36"
                  style:--fill="{settings.sound_volume / 2}%"
                  onchange={async (e) => {
                    await update({ sound_volume: Number(e.currentTarget.value) })
                    previewSound(settings.clip_sound)
                  }}
                />
                <span class="w-10 text-right tabular-nums text-fg-2">{settings.sound_volume}%</span>
              </div>
            </Row>
            {#if soundError}
              <div class="px-4 py-3 text-secondary text-danger" role="alert">{soundError}</div>
            {/if}
          {/if}
          {#if status?.backend === 'windows'}
            <Row
              title="In-game popup"
              description="“Ready” and “Clip saved” over the game. Not recorded."
            >
              <Toggle label="In-game popup" checked={settings.overlay} onchange={(overlay) => update({ overlay })} />
            </Row>
          {/if}
        </div>
      </section>
      <section class="space-y-2">
        <h3 class="section-title px-1">Updates</h3>
        <div class="card divide-y divide-line">
          <Row title={support ? `Reruns ${support.version}` : 'Reruns'} description={updateText}>
            {#if installing !== undefined}
              <div class="flex items-center gap-3">
                <div
                  class="h-1.5 w-32 overflow-hidden rounded-full bg-line-strong"
                  role="progressbar"
                  aria-label="Downloading the update"
                  aria-valuenow={installing === null ? undefined : Math.round(installing * 100)}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div class="h-full rounded-full bg-accent transition-[width] duration-normal" style:width="{(installing ?? 0.1) * 100}%"></div>
                </div>
                <span class="text-secondary text-fg-2">Updating…</span>
              </div>
            {:else if available}
              <button class="btn-primary h-9" onclick={install}>Update and restart</button>
            {:else}
              <button class="btn h-9" disabled={!support?.configured || checking} onclick={checkNow}>
                {checking ? 'Checking…' : 'Check now'}
              </button>
            {/if}
          </Row>
          <Row title="Check automatically">
            <Toggle
              label="Check for updates automatically"
              checked={settings.check_for_updates}
              disabled={!support?.configured}
              onchange={(check_for_updates) => update({ check_for_updates })}
            />
          </Row>
        </div>
      </section>
      <section class="space-y-2">
        <h3 class="section-title px-1">Troubleshooting</h3>
        <div class="card divide-y divide-line">
          <Row title="Logs" description={logError ?? undefined}>
            <button class="btn h-9" onclick={openLogs} aria-label="Open the log folder">Open folder</button>
          </Row>
        </div>
      </section>
    {:else if category === 'Recording'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Quality</h3>
        <div class="card divide-y divide-line">
          <Row title="Preset">
            <Segmented
              label="Preset"
              value={preset}
              onchange={applyPreset}
              options={[
                ...presets(caps).map((p) => ({ value: p.id as string, label: p.label })),
                ...(preset === 'custom' ? [{ value: 'custom', label: 'Custom' }] : []),
              ]}
            />
          </Row>
          <Row
            title="Frame rate"
            description={hz ? `Your display: ${hz} Hz` : undefined}
          >
            <select class="field w-44" value={settings.fps} onchange={(e) => update({ fps: Number(e.currentTarget.value) })}>
              {#each FPS_OPTIONS as f (f)}
                <option value={f}>{f} FPS{hz && f > hz + 1 ? ' · above display' : ''}</option>
              {/each}
            </select>
          </Row>
          <Row title="Bitrate">
            <input
              type="range"
              min="5"
              max="100"
              step="5"
              class="w-44"
              style:--fill="{((settings.bitrate_kbps / 1000 - 5) / 95) * 100}%"
              value={settings.bitrate_kbps / 1000}
              onchange={(e) => update({ bitrate_kbps: Number(e.currentTarget.value) * 1000 })}
              oninput={(e) => e.currentTarget.style.setProperty('--fill', `${((Number(e.currentTarget.value) - 5) / 95) * 100}%`)}
            />
            <span class="w-20 text-right text-body font-medium tabular-nums">{settings.bitrate_kbps / 1000} Mbps</span>
          </Row>
          <Row
            title="Codec"
            description={settings.codec === 'h264'
              ? 'Plays everywhere.'
              : settings.codec === 'hevc'
                ? 'A third smaller. Some apps can’t play it.'
                : 'Smallest files. Needs a recent player.'}
          >
            <Segmented
              label="Codec"
              value={settings.codec}
              onchange={setCodec}
              options={(['h264', 'hevc', 'av1'] as Codec[]).map((c) => ({
                value: c,
                label: CODEC_LABEL[c],
                disabled: !codecs.has(c),
                title: codecs.has(c) ? undefined : 'Not supported by your GPU',
              }))}
            />
          </Row>
          <Row title="Encoder">
            <select
              class="field w-48"
              value={settings.encoder ?? 'auto'}
              onchange={(e) => {
                const v = e.currentTarget.value
                update({ encoder: v === 'auto' ? null : (v as Encoder) })
              }}
            >
              <option value="auto">Auto{autoEncoder ? ` · ${ENCODER_LABEL[autoEncoder]}` : ''}</option>
              {#each encoderOptions as e (e.encoder)}
                <option value={e.encoder} disabled={!e.ok}>
                  {ENCODER_LABEL[e.encoder]}{e.ok ? '' : ' · unsupported'}
                </option>
              {/each}
            </select>
          </Row>
        </div>
        <p class="px-1 text-secondary text-muted">
          {lengthLabel(settings.replay_seconds)} ≈ <span class="font-medium text-fg-2">{bytes(clipBytes)}</span>, on disk and in RAM.
          {#if clipBytes > 2e9}
            <span class="text-accent">That’s a lot of RAM.</span>
          {/if}
        </p>
      </section>

      <section class="space-y-2">
        <h3 class="section-title px-1">Capture</h3>
        <div class="card divide-y divide-line">
          <Row title="Replay length" description="Applies immediately.">
            <select
              class="field w-44"
              value={settings.replay_seconds}
              onchange={(e) => update({ replay_seconds: Number(e.currentTarget.value) })}
            >
              {#each lengths as s (s)}
                <option value={s}>{lengthLabel(s)}</option>
              {/each}
            </select>
          </Row>
          <Row title="Record cursor">
            <Toggle label="Record cursor" checked={settings.cursor} onchange={(cursor) => update({ cursor })} />
          </Row>
          <Row
            title="GPU direct capture"
            description={status?.capture_path === 'zerocopy'
              ? 'Active.'
              : status?.capture_path === 'copy' && settings.zero_copy
                ? 'Not available on this driver.'
                : undefined}
          >
            <Toggle label="GPU direct capture" checked={settings.zero_copy} onchange={(zero_copy) => update({ zero_copy })} />
          </Row>
          {#if status?.backend !== 'windows'}
          <Row
            title="Capture method"
            description={status?.backend === 'kwin' ? 'Game window, no prompts.' : status?.backend === 'portal' ? 'The screen you chose.' : undefined}
          >
            <select
              class="field w-48"
              value={settings.capture_backend ?? 'auto'}
              onchange={(e) => {
                const v = e.currentTarget.value
                update({ capture_backend: v === 'auto' ? null : (v as 'kwin' | 'portal') })
              }}
            >
              <option value="auto">Automatic</option>
              <option value="kwin">Game window (KDE)</option>
              <option value="portal">Screen (any desktop)</option>
            </select>
          </Row>
          {:else}
            <Row
              title="Capture method"
              description="Game window only."
            />
          {/if}
          {#if status?.backend === 'windows'}
            <Row
              title="HDR compatibility"
              description="Fixes blown-out HDR clips. Records the screen area, so overlays can show up."
            >
              <Toggle
                label="HDR compatibility"
                checked={settings.hdr_compatibility}
                onchange={(hdr_compatibility) => update({ hdr_compatibility })}
              />
            </Row>
            {#if settings.hdr_compatibility}
              <Row
                title="Capture the window in HDR"
                description="Experimental. Records only the game window."
              >
                <Toggle
                  label="Capture the window in HDR"
                  checked={settings.hdr_window_capture}
                  onchange={(hdr_window_capture) => update({ hdr_window_capture })}
                />
              </Row>
            {/if}
          {/if}
          {#if status?.hdr}
            <Row
              title="HDR display"
              description={status.hdr_tonemapped
                ? `SDR white ${Math.round(status.hdr.sdr_white_nits)} nits. Tone-mapped.`
                : `SDR white ${Math.round(status.hdr.sdr_white_nits)} nits. Highlights blow out${status.backend === 'windows' ? ' without HDR compatibility' : ''}.`}
            />
          {/if}
          {#if settings.ignored_games.length}
            <Row title="Not games" description="Never recorded. Remove one to record it again.">
              <div class="flex max-w-80 flex-wrap justify-end gap-1.5">
                {#each settings.ignored_games as g, i (g.exe ?? g.name)}
                  <span class="chip" title={g.exe ? `${g.exe}.exe` : g.name}>
                    {g.name}
                    <button
                      class="ml-0.5 text-muted hover:text-fg"
                      aria-label="Detect {g.name} again"
                      onclick={() => update({ ignored_games: settings.ignored_games.filter((_, j) => j !== i) })}
                    >
                      ×
                    </button>
                  </span>
                {/each}
              </div>
            </Row>
          {/if}
          {#if status?.backend === 'portal'}
            <Row
              title="Screen"
              description={status.needs_screen
                ? 'Your desktop asks once.'
                : undefined}
            >
              <button class="btn {status.needs_screen ? 'border-accent/60 text-accent' : ''}" onclick={onchoosescreen}>
                {status.needs_screen ? 'Choose screen' : 'Change screen'}
              </button>
            </Row>
          {/if}
        </div>
        <p class="px-1 text-secondary text-muted">
          {status?.backend === 'windows' ? 'Changes apply from the next game.' : 'Changes apply from the next game; capture method after a restart.'}
        </p>
      </section>
    {:else if category === 'Audio'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Sources</h3>
        <div class="card divide-y divide-line">
          <Row title="Game and desktop audio">
            <Toggle label="Game and desktop audio" checked={settings.desktop_audio} onchange={(desktop_audio) => update({ desktop_audio })} />
          </Row>
          <Row
            title="Output device"
            disabled={!settings.desktop_audio}
          >
            <div class="flex w-64 flex-col gap-2">
              <select
                class="field w-64"
                aria-label="Output device"
                disabled={!settings.desktop_audio}
                value={settings.desktop_device ?? ''}
                onchange={(e) => update({ desktop_device: e.currentTarget.value || null })}
              >
                <option value="">System default</option>
                {#each caps?.output_devices ?? [] as d (d.id)}
                  <option value={d.id}>{d.name}</option>
                {/each}
                {#if settings.desktop_device && caps && !caps.output_devices.some((d) => d.id === settings.desktop_device)}
                  <option value={settings.desktop_device}>Not connected</option>
                {/if}
              </select>
              {#if settings.desktop_audio}
                <LevelMeter label="Game and desktop audio" peakDb={levels.desktop?.peak_db ?? null} error={levels.desktop?.error} />
              {/if}
            </div>
          </Row>
          <Row title="Microphone" description="Saved as a separate track.">
            <Toggle label="Microphone" checked={settings.mic} onchange={(mic) => update({ mic })} />
          </Row>
          <Row title="Input device" disabled={!settings.mic}>
            <div class="flex w-64 flex-col gap-2">
              <select
                class="field w-64"
                aria-label="Input device"
                disabled={!settings.mic}
                value={settings.mic_device ?? ''}
                onchange={(e) => update({ mic_device: e.currentTarget.value || null })}
              >
                <option value="">System default</option>
                {#each caps?.mic_devices ?? [] as d (d.id)}
                  <option value={d.id}>{d.name}</option>
                {/each}
                {#if settings.mic_device && caps && !caps.mic_devices.some((d) => d.id === settings.mic_device)}
                  <option value={settings.mic_device}>Not connected</option>
                {/if}
              </select>
              {#if settings.mic}
                <LevelMeter label="Microphone" peakDb={levels.mic?.peak_db ?? null} error={levels.mic?.error} />
              {/if}
            </div>
          </Row>
        </div>
        <p class="px-1 text-secondary text-muted">
          Unplugged devices fall back to the default.
        </p>
      </section>
      <section class="space-y-2">
        <h3 class="section-title px-1">Quality</h3>
        <div class="card divide-y divide-line">
          <Row title="Audio bitrate" description="Per track.">
            <select
              class="field w-[140px]"
              value={settings.audio_bitrate_kbps}
              onchange={(e) => update({ audio_bitrate_kbps: Number(e.currentTarget.value) })}
            >
              {#each audioRates as r (r)}
                <option value={r}>{r} kbps</option>
              {/each}
            </select>
          </Row>
        </div>
        <p class="px-1 text-secondary text-muted">Changes apply from the next game.</p>
      </section>
    {:else if category === 'Hotkeys'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Clipping</h3>
        <div class="card divide-y divide-line">
          {#if hotkeyBackend !== null}
            <Row
              title="Clip that"
              description={hotkeyBackend === 'portal'
                ? 'Your desktop may ask to confirm it.'
                : hotkeyBackend === 'native'
                  ? 'If nothing happens, another app may own this key.'
                  : undefined}
            >
              <HotkeyBinder value={settings.hotkey} onbind={(hotkey) => update({ hotkey })} />
            </Row>
            <Row
              title="Screenshot"
              description="Saved as PNG with the clips."
            >
              <HotkeyBinder value={settings.screenshot_hotkey} onbind={(screenshot_hotkey) => update({ screenshot_hotkey })} />
            </Row>
            <Row
              title="Mark a moment"
              description="Shows on the clip’s timeline."
            >
              <HotkeyBinder value={settings.marker_hotkey} onbind={(marker_hotkey) => update({ marker_hotkey })} />
            </Row>
          {/if}
          <Row
            title="Command"
            description={hotkeyBackend === null
              ? 'No global hotkeys on this desktop. Bind this to a key in its keyboard settings.'
              : hotkeyBackend === 'native'
                ? 'For scripts, Stream Deck or AutoHotkey.'
                : 'Bind it to a key, or use it in scripts.'}
          >
            <code class="rounded-ctl border border-line bg-canvas px-2.5 py-1.5 text-secondary text-fg-2" title="Also: rerunsctl screenshot, rerunsctl mark">rerunsctl save</code>
            <button class="btn" onclick={copyCommand}>{copied ? 'Copied' : 'Copy'}</button>
          </Row>
        </div>
        {#if hotkeyBackend === 'kglobalaccel'}
          <p class="px-1 text-secondary text-muted">Also in System Settings → Shortcuts.</p>
        {/if}
      </section>
    {:else if category === 'Storage'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Location</h3>
        <div class="card divide-y divide-line">
          <Row title="Clips folder" description="One folder per game.">
            <input class="field w-[280px] cursor-pointer" readonly value={settings.clips_dir} onclick={browse} title={settings.clips_dir} />
            <button class="btn" onclick={browse}>Browse</button>
            <button class="btn w-9 px-0" onclick={() => api.openFolder()} title="Open folder" aria-label="Open folder">
              <FolderOpen size={16} />
            </button>
          </Row>
          {#if usage && drive}
            <div class="px-4 py-4">
              <div class="flex items-baseline justify-between text-secondary">
                <span class="font-medium text-fg">Drive</span>
                <span class="text-muted tabular-nums">{bytes(usage.disk_free)} free of {bytes(usage.disk_total)}</span>
              </div>
              <div class="mt-2.5 flex h-1.5 overflow-hidden rounded-full bg-line">
                <div class="bg-accent" style:width="{Math.max(drive.clips, usage.clips_bytes ? 0.5 : 0)}%"></div>
                <div class="bg-line-strong" style:width="{drive.other}%"></div>
              </div>
              <div class="mt-2.5 flex gap-5 text-caption text-muted">
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-xs bg-accent"></span>Clips · {bytes(usage.clips_bytes)}</span>
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-xs bg-line-strong"></span>Other files</span>
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-xs bg-line"></span>Free</span>
              </div>
            </div>
          {/if}
        </div>
      </section>

      <section class="space-y-2">
        <h3 class="section-title px-1">File limits</h3>
        <div class="card divide-y divide-line">
          <Row title="Storage limit" description="Oldest clips go first.">
            {#if limitChoice === 'custom'}
              <input
                class="field w-20 text-right tabular-nums"
                type="number"
                min="1"
                step="1"
                value={settings.storage_limit_gb ?? 20}
                onchange={(e) => {
                  const v = Number(e.currentTarget.value)
                  if (v >= 1) update({ storage_limit_gb: v })
                }}
              />
              <span class="text-secondary text-muted">GB</span>
            {/if}
            <select class="field w-[140px]" value={limitChoice} onchange={(e) => chooseLimit(e.currentTarget.value)}>
              <option value="none">No limit</option>
              {#each limits as l (l)}
                <option value={String(l)}>{l} GB</option>
              {/each}
              <option value="custom">Custom</option>
            </select>
          </Row>
          <Row
            title="Protect edited clips"
            description="Never remove clips you rendered."
            disabled={settings.storage_limit_gb == null}
          >
            <Toggle
              label="Protect edited clips"
              disabled={settings.storage_limit_gb == null}
              checked={settings.protect_edits}
              onchange={(protect_edits) => update({ protect_edits })}
            />
          </Row>
          <Row
            title="Protect starred clips"
            description="Never remove clips you starred."
            disabled={settings.storage_limit_gb == null}
          >
            <Toggle
              label="Protect starred clips"
              disabled={settings.storage_limit_gb == null}
              checked={settings.protect_favorites}
              onchange={(protect_favorites) => update({ protect_favorites })}
            />
          </Row>
          <Row title="Move deleted clips to Trash">
            <Toggle label="Move deleted clips to Trash" checked={settings.use_trash} onchange={(use_trash) => update({ use_trash })} />
          </Row>
        </div>
      </section>
    {:else if category === 'Appearance'}
      <section class="space-y-2">
        <h3 class="section-title px-1">Theme</h3>
        <div class="card divide-y divide-line">
          <Row title="Accent colour">
            <div class="flex items-center gap-2" role="radiogroup" aria-label="Accent colour">
              {#each Object.entries(ACCENTS) as [id, a] (id)}
                <button
                  role="radio"
                  aria-checked={prefs.accent === id}
                  aria-label={a.label}
                  title={a.label}
                  class="flex size-7 items-center justify-center rounded-full text-on-accent ring-offset-2 ring-offset-panel transition-shadow duration-fast hover:ring-2 hover:ring-line-strong
                    {prefs.accent === id ? 'ring-2 ring-white' : ''}"
                  style:background={a.css}
                  onclick={() => setPrefs({ accent: id as Accent })}
                >
                  {#if prefs.accent === id}<Check size={14} strokeWidth={3} />{/if}
                </button>
              {/each}
            </div>
          </Row>
          <Row title="Card size">
            <Segmented
              label="Card size"
              value={prefs.density}
              onchange={(density) => setPrefs({ density })}
              options={[
                { value: 'comfortable', label: 'Comfortable' },
                { value: 'compact', label: 'Compact' },
              ]}
            />
          </Row>
        </div>
      </section>
      <section class="space-y-2">
        <h3 class="section-title px-1">Motion</h3>
        <div class="card divide-y divide-line">
          <Row title="Preview clips on hover">
            <Toggle label="Preview clips on hover" checked={prefs.hoverPreviews} onchange={(hoverPreviews) => setPrefs({ hoverPreviews })} />
          </Row>
          <Row title="Reduce motion" description="When off, follows your system.">
            <Toggle label="Reduce motion" checked={prefs.reduceMotion} onchange={(reduceMotion) => setPrefs({ reduceMotion })} />
          </Row>
        </div>
      </section>
    {/if}
  </div>
</div>
