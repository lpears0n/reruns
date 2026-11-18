<script lang="ts" module>
  /** The pages, listed in the navigation while Settings is open. */
  export const CATEGORIES = ['General', 'Recording', 'Audio', 'Hotkeys', 'Storage', 'Sharing', 'Appearance'] as const
  export type Category = (typeof CATEGORIES)[number]
</script>

<script lang="ts">
  import { FolderOpen, Play, X } from '@lucide/svelte'
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
  } from './api'
  import Fader from './Fader.svelte'
  import { bytes } from './format'
  import HotkeyBinder from './HotkeyBinder.svelte'
  import LevelMeter from './LevelMeter.svelte'
  import { prefs, setPrefs } from './prefs.svelte'
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
  import type { Notice } from './StatusBar.svelte'
  import Toggle from './Toggle.svelte'

  let {
    settings,
    usage,
    caps,
    status,
    category,
    notice,
    ondismiss,
    update: available = $bindable(null),
    onchange,
    onerror,
    onupdate,
    onchoosescreen,
  }: {
    settings: Settings
    usage: Usage | null
    caps: Capabilities | null
    status: Status | null
    category: Category
    /** What just happened (a setting that didn't save): said in the header, as there is no capture strip here. */
    notice: Notice | null
    ondismiss: () => void
    /** A newer release, once one has been found. */
    update?: UpdateInfo | null
    onchange: (s: Settings) => Promise<void>
    /** A setting didn't save. */
    onerror: (text: string) => void
    /** Open the update prompt, which installs it. */
    onupdate: () => void
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

  const lengths = [15, 30, 60, 120, 180, 300]
  const audioRates = [128, 160, 192, 256, 320]
  // Same steps as Medal's PC Storage limit.
  const limits = [1, 5, 10, 25, 50, 100, 250, 500]

  /** Throws: for callers that show the reason next to the control. */
  const save = (patch: Partial<Settings>) => onchange({ ...settings, ...patch })
  const update = (patch: Partial<Settings>) => save(patch).catch((e) => onerror(String(e)))

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

  /** The bitrate under the thumb while it is dragged, in Mbps. */
  let bitrateDraft = $state<number | null>(null)

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

  // Sharing: the sizes chat apps cap uploads at.
  const shareSizes = [10, 25, 50, 100, 500]
  let customShare = $state(false)
  const shareChoice = $derived(customShare || !shareSizes.includes(prefs.shareMaxMb) ? 'custom' : String(prefs.shareMaxMb))

  // Updates
  let support =$state<{ configured: boolean; version: string } | null>(null)
  let checking = $state(false)
  let checked = $state(false)
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
  const soundName = (s: Sound) => (s.kind === 'builtin' ? 'Built-in chime' : s.name || 'Custom sound')
  /** Screenshots have a shutter of their own, unless clips use a file of
   *  the user's: then they follow it, as they did before the shutter. */
  const screenshotSoundName = (s: Sound | null) =>
    s?.kind === 'custom'
      ? s.name || 'Custom sound'
      : s === null && settings.clip_sound.kind === 'custom'
        ? 'Same as clips'
        : 'Built-in shutter'

  async function chooseSound(slot: 'clip' | 'screenshot') {
    soundError = null
    const file = await openDialog({ filters: [{ name: 'Sounds', extensions: SOUND_FILES }] })
    if (typeof file !== 'string') return
    try {
      const sound = await api.importSound(file, slot)
      await save(slot === 'clip' ? { clip_sound: sound } : { screenshot_sound: sound })
      api.previewSound(sound, slot, settings.sound_volume)
    } catch (e) {
      soundError = String(e)
    }
  }

  const previewSound = (slot: 'clip' | 'screenshot') => api.previewSound(null, slot, settings.sound_volume)

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

<!-- One column: the page's name, and its preferences under it, at a
     readable width. The navigation holds the pages. -->
<header class="page-header">
  <h1 class="min-w-0 flex-1 truncate font-display text-page font-semibold">{category}</h1>
  {#if notice}
    {#key notice.persistent ? 'work' : notice.text}
      <div class="said flex max-w-[60%] min-w-0 items-center gap-1 text-secondary {notice.error ? 'text-danger' : ''}" role="status" aria-live="polite">
        <span class="truncate" title={notice.text}>{notice.text}</span>
        {#if notice.action}
          <button class="btn-text" onclick={notice.action.run}>{notice.action.label}</button>
        {/if}
        {#if notice.secondaryAction}<button class="btn-text" onclick={notice.secondaryAction.run}>{notice.secondaryAction.label}</button>{/if}
        {#if !notice.persistent}
          <button class="btn-icon-sm text-muted" onclick={ondismiss} aria-label="Dismiss"><X size={15} /></button>
        {/if}
      </div>
    {/key}
  {/if}
</header>

<div class="min-h-0 flex-1 overflow-y-auto px-8 pb-12">
    <div class="max-w-(--container-settings) space-y-9">

    {#if category === 'General'}
      <section>
        <h2 class="text-title font-semibold">Startup</h2>
        <div class="mt-2 flex flex-col">
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
      <section>
        <h2 class="text-title font-semibold">When a clip is saved</h2>
        <div class="mt-2 flex flex-col">
          <Row title="Show a notification">
            <Toggle label="Show a notification" checked={settings.notify_on_save} onchange={(notify_on_save) => update({ notify_on_save })} />
          </Row>
          <Row title="Play a sound">
            <Toggle label="Play a sound" checked={settings.sound_on_save} onchange={(sound_on_save) => update({ sound_on_save })} />
          </Row>
          {#if settings.sound_on_save}
            <Row title="Clip sound" description={soundName(settings.clip_sound)}>
              <div class="flex items-center gap-2">
                <button class="btn-icon-sm" onclick={() => previewSound('clip')} aria-label="Play the clip sound" title="Play">
                  <Play size={12} fill="currentColor" strokeWidth={0} />
                </button>
                {#if settings.clip_sound.kind === 'custom'}
                  <button class="btn-sm" onclick={() => update({ clip_sound: { kind: 'builtin' } })}>Use default</button>
                {/if}
                <button class="btn-sm" onclick={() => chooseSound('clip')}>Choose file…</button>
              </div>
            </Row>
            <Row title="Screenshot sound" description={screenshotSoundName(settings.screenshot_sound)}>
              <div class="flex items-center gap-2">
                <button
                  class="btn-icon-sm"
                  onclick={() => previewSound('screenshot')}
                  aria-label="Play the screenshot sound"
                  title="Play"
                >
                  <Play size={12} fill="currentColor" strokeWidth={0} />
                </button>
                {#if settings.screenshot_sound !== null}
                  <button class="btn-sm" onclick={() => update({ screenshot_sound: null })}>Use default</button>
                {/if}
                <button class="btn-sm" onclick={() => chooseSound('screenshot')}>Choose file…</button>
              </div>
            </Row>
            <Row title="Sound volume">
              <div class="flex items-center gap-4">
                <Fader
                  label="Sound volume"
                  class="w-36!"
                  value={settings.sound_volume}
                  max={200}
                  step={5}
                  detent={100}
                  text="{settings.sound_volume}%"
                  onchange={async (v) => {
                    await update({ sound_volume: v })
                    previewSound('clip')
                  }}
                />
                <span class="w-10 text-right tabular-nums text-fg-2">{settings.sound_volume}%</span>
              </div>
            </Row>
            {#if soundError}
              <div class="py-3 text-danger [overflow-wrap:anywhere]" role="alert">{soundError}</div>
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
      <section>
        <h2 class="text-title font-semibold">Updates</h2>
        <div class="mt-2 flex flex-col">
          <Row title={support ? `Reruns ${support.version}` : 'Reruns'} description={updateText}>
            {#if available}
              <button class="btn-primary" onclick={onupdate}>Update…</button>
            {:else}
              <button class="btn-sm" disabled={!support?.configured || checking} onclick={checkNow}>
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
      <section>
        <h2 class="text-title font-semibold">Troubleshooting</h2>
        <div class="mt-2 flex flex-col">
          <Row title="Logs" description={logError ?? undefined}>
            <button class="btn-sm" onclick={openLogs} aria-label="Open the log folder">Open folder</button>
          </Row>
        </div>
      </section>
    {:else if category === 'Recording'}
      <section>
        <h2 class="text-title font-semibold">Quality</h2>
        <p class="mt-1 text-secondary text-fg-2">
          {lengthLabel(settings.replay_seconds)} ≈ <span class="text-fg tabular-nums">{bytes(clipBytes)}</span>, on disk and in RAM.
          {#if clipBytes > 2e9}
            <span class="text-warn">That’s a lot of RAM.</span>
          {/if}
        </p>
        <div class="mt-2 flex flex-col">
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
            <Fader
              label="Bitrate"
              class="w-44!"
              value={bitrateDraft ?? settings.bitrate_kbps / 1000}
              min={5}
              max={100}
              step={5}
              text="{bitrateDraft ?? settings.bitrate_kbps / 1000} Mbps"
              oninput={(v) => (bitrateDraft = v)}
              onchange={async (v) => {
                await update({ bitrate_kbps: v * 1000 })
                bitrateDraft = null
              }}
            />
            <span class="w-16 text-right tabular-nums">{bitrateDraft ?? settings.bitrate_kbps / 1000} Mbps</span>
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
      </section>

      <section>
        <h2 class="text-title font-semibold">Capture</h2>
        <p class="mt-1 text-secondary text-fg-2">
          {status?.backend === 'windows' ? 'Changes apply from the next game.' : 'Changes apply from the next game; capture method after a restart.'}
        </p>
        <div class="mt-2 flex flex-col">
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
            <!-- 0: Windows didn't say (the display was asleep); 80 is assumed until it does. -->
            {@const white = status.hdr.sdr_white_nits > 0 ? `SDR white ${Math.round(status.hdr.sdr_white_nits)} nits.` : 'SDR white unknown yet, 80 nits assumed.'}
            <Row
              title="HDR display"
              description={status.hdr_tonemapped
                ? `${white} Tone-mapped.`
                : `${white} Highlights blow out${status.backend === 'windows' ? ' without HDR compatibility' : ''}.`}
            />
          {/if}
          {#if settings.ignored_games.length}
            <Row title="Not games" description="Never recorded. Remove one to record it again.">
              <div class="flex max-w-80 flex-wrap justify-end gap-2">
                {#each settings.ignored_games as g, i (g.exe ?? g.name)}
                  <span class="inline-flex h-8 max-w-56 items-center rounded-ctl bg-raised pr-1 pl-3 text-secondary" title={g.exe ? `${g.exe}.exe` : g.name}>
                    <span class="truncate">{g.name}</span>
                    <button
                      class="press ml-1 flex size-6 flex-none items-center justify-center rounded-chip text-fg-2 hover:bg-hover hover:text-fg"
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
              <button class={status.needs_screen ? 'btn-primary' : 'btn-sm'} onclick={onchoosescreen}>
                {status.needs_screen ? 'Choose screen' : 'Change screen'}
              </button>
            </Row>
          {/if}
        </div>
      </section>
    {:else if category === 'Audio'}
      <section>
        <h2 class="text-title font-semibold">Sources</h2>
        <p class="mt-1 text-secondary text-fg-2">
          Unplugged devices fall back to the default.
        </p>
        <div class="mt-2 flex flex-col">
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
      </section>
      <section>
        <h2 class="text-title font-semibold">Quality</h2>
        <p class="mt-1 text-secondary text-fg-2">Changes apply from the next game.</p>
        <div class="mt-2 flex flex-col">
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
      </section>
    {:else if category === 'Hotkeys'}
      <section>
        <h2 class="text-title font-semibold">Clipping</h2>
        {#if hotkeyBackend === 'kglobalaccel'}
          <p class="mt-1 text-secondary text-fg-2">Also in System Settings → Shortcuts.</p>
        {/if}
        <div class="mt-2 flex flex-col">
          {#if hotkeyBackend !== null}
            <Row
              title="Clip that"
              description={hotkeyBackend === 'portal'
                ? 'Your desktop may ask to confirm it.'
                : hotkeyBackend === 'native'
                  ? 'If nothing happens, another app may own this key.'
                  : undefined}
            >
              <HotkeyBinder value={settings.hotkey} onbind={(hotkey) => save({ hotkey })} />
            </Row>
            <Row
              title="Screenshot"
              description="Saved as PNG with the clips."
            >
              <HotkeyBinder value={settings.screenshot_hotkey} onbind={(screenshot_hotkey) => save({ screenshot_hotkey })} />
            </Row>
            <Row
              title="Mark a moment"
              description="Shows on the clip’s timeline."
            >
              <HotkeyBinder value={settings.marker_hotkey} onbind={(marker_hotkey) => save({ marker_hotkey })} />
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
            <code class="flex h-8 items-center rounded-ctl border border-line-strong px-3 text-secondary text-fg-2 select-text"title="Also: rerunsctl screenshot, rerunsctl mark">rerunsctl save</code>
            <button class="btn-sm" onclick={copyCommand}>{copied ? 'Copied' : 'Copy'}</button>
          </Row>
        </div>
      </section>
    {:else if category === 'Storage'}
      <section>
        <h2 class="text-title font-semibold">Location</h2>
        <div class="mt-2 flex flex-col">
          <Row title="Clips folder" description="One folder per game.">
            <input class="field w-[260px] cursor-pointer truncate" readonly value={settings.clips_dir} onclick={browse} title={settings.clips_dir} />
            <button class="btn-sm" onclick={browse}>Browse</button>
            <button class="btn-icon-sm" onclick={() => api.openFolder()} title="Open folder" aria-label="Open folder">
              <FolderOpen size={14} />
            </button>
          </Row>
          {#if usage && drive}
            <div class="py-3">
              <div class="flex items-baseline justify-between">
                <span class="text-fg">Drive</span>
                <span class="text-muted tabular-nums">{bytes(usage.disk_free)} free of {bytes(usage.disk_total)}</span>
              </div>
              <!-- Segments on a track: clips, everything else, and the track itself is what is free. -->
              <div class="mt-2 flex h-2 gap-px overflow-hidden rounded-chip bg-raised">
                <div class="bg-fg" style:width="{Math.max(drive.clips, usage.clips_bytes ? 0.5 : 0)}%"></div>
                <div class="bg-faint" style:width="{drive.other}%"></div>
              </div>
              <div class="meta mt-2 flex gap-5">
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-[2px] bg-fg"></span>Clips, {bytes(usage.clips_bytes)}</span>
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-[2px] bg-faint"></span>Other files</span>
                <span class="flex items-center gap-1.5"><span class="size-2 rounded-[2px] bg-raised"></span>Free</span>
              </div>
            </div>
          {/if}
        </div>
      </section>

      <section>
        <h2 class="text-title font-semibold">File limits</h2>
        <div class="mt-2 flex flex-col">
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
              <span class="text-muted">GB</span>
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
            title="Protect favorites"
            description="The limit never removes them."
            disabled={settings.storage_limit_gb == null}
          >
            <Toggle
              label="Protect favorites"
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
    {:else if category === 'Sharing'}
      <section>
        <h2 class="text-title font-semibold">File size</h2>
        <p class="mt-1 text-secondary text-fg-2">A clip over the limit is re-encoded to fit. The original stays as it is.</p>
        <div class="mt-2 flex flex-col">
          <Row title="Limit copied clips">
            <Toggle label="Limit copied clips" checked={prefs.shareLimit} onchange={(shareLimit) => setPrefs({ shareLimit })} />
          </Row>
          <Row title="Maximum size" disabled={!prefs.shareLimit}>
            {#if shareChoice === 'custom'}
              <input
                class="field w-20 text-right tabular-nums"
                type="number"
                min="1"
                step="1"
                aria-label="Maximum size in megabytes"
                disabled={!prefs.shareLimit}
                value={prefs.shareMaxMb}
                onchange={(e) => {
                  const v = Number(e.currentTarget.value)
                  if (v >= 1) setPrefs({ shareMaxMb: v })
                }}
              />
              <span class="text-muted">MB</span>
            {/if}
            <select
              class="field w-[140px]"
              aria-label="Maximum size"
              disabled={!prefs.shareLimit}
              value={shareChoice}
              onchange={(e) => {
                const v = e.currentTarget.value
                customShare = v === 'custom'
                if (!customShare) setPrefs({ shareMaxMb: Number(v) })
              }}
            >
              {#each shareSizes as m (m)}
                <option value={String(m)}>{m} MB</option>
              {/each}
              <option value="custom">Custom</option>
            </select>
          </Row>
          <Row title="Limit saved clips too" disabled={!prefs.shareLimit}>
            <Toggle
              label="Limit saved clips too"
              disabled={!prefs.shareLimit}
              checked={prefs.shareLimitSave}
              onchange={(shareLimitSave) => setPrefs({ shareLimitSave })}
            />
          </Row>
        </div>
      </section>
    {:else if category === 'Appearance'}
      <section>
        <h2 class="text-title font-semibold">Library</h2>
        <div class="mt-2 flex flex-col">
          <Row title="Tags" description="Tag clips and filter the library by tag.">
            <Toggle label="Tags" checked={prefs.tagging} onchange={(tagging) => setPrefs({ tagging })} />
          </Row>
        </div>
      </section>
      <section>
        <h2 class="text-title font-semibold">Motion</h2>
        <div class="mt-2 flex flex-col">
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

<style>
  .said {
    transition: opacity var(--duration-normal) var(--ease-out);
    @starting-style {
      opacity: 0;
    }
  }
</style>
