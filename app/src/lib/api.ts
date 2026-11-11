// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.

import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type RecorderState = 'unavailable' | 'waiting' | 'recording' | 'retrying'

/** Who provides the global hotkey: KDE, the portal, or the OS itself (Windows). */
export type HotkeyBackend = 'kglobalaccel' | 'portal' | 'native'

export interface Status {
  state: RecorderState
  game: string | null
  steam_app_id: number | null
  session_start_ms: number | null
  buffered_seconds: number
  replay_seconds: number
  encoder: string | null
  /** While recording: the usual encoder refused, so another one is used.
   *  `reason` is the driver's words. It is tried again at the next game,
   *  unless `set_aside` (it kept failing while recording): then only after
   *  Reruns restarts. */
  encoder_fallback: { preferred: Encoder; using: Encoder; reason: string; set_aside: boolean } | null
  /** While recording: frames go straight into GPU memory, or through RAM. */
  capture_path: 'zerocopy' | 'copy' | null
  error: string | null
  /** "kwin": game windows (KDE). "portal": the chosen screen. "windows": the game's monitor. */
  backend: 'kwin' | 'portal' | 'windows' | null
  /** Portal capture needs a screen chosen once. */
  needs_screen: boolean
  /** While recording: the game's display, when it runs in HDR. */
  hdr: { sdr_white_nits: number; peak_nits: number } | null
  /** While recording: HDR is tone-mapped into the clips. */
  hdr_tonemapped: boolean
}

export interface Clip {
  path: string
  game: string
  name: string
  created_ms: number
  size_bytes: number
  duration: number | null
  audio_tracks: number
  is_edit: boolean
  steam_app_id: number | null
  thumbnail: string | null
  /** One row of `sprite_frames` frames for hover scrubbing. */
  sprite: string | null
  sprite_frames: number
  /** User-given name; show the game or time when null. */
  title: string | null
  favorite: boolean
  labels: string[]
  /** Moments marked while recording, in seconds from the start. */
  markers: number[]
  /** The first audio track is game and mic mixed (what players play); the
   *  sources' own tracks follow, and the editor's levels are for those. */
  mixed_audio?: boolean
}

/** How a size-limited export will be encoded. */
export interface ExportPlan {
  video_kbps: number
  audio_kbps: number
  /** 16:9 height of the picture class (wider pictures keep its pixel count). */
  max_height: number
  /** Frame rate it drops to; null keeps the clip's. */
  fps: number | null
}

/** Fields of a clip the user can change; omitted ones are left alone. */
export interface ClipPatch {
  /** Empty string clears the title. */
  title?: string
  favorite?: boolean
  labels?: string[]
}

/** A newer release than the running one. */
export interface UpdateInfo {
  version: string
  current_version: string
  notes: string | null
  /** Found at start-up: ask once whether to update. */
  prompt: boolean
}

/** A save sound: the built-in beep, or a file the user imported. */
export type Sound = { kind: 'builtin' } | { kind: 'custom'; path: string; name: string }

export interface Settings {
  replay_seconds: number
  mic: boolean
  clips_dir: string
  hotkey: string
  /** Empty = unbound. */
  screenshot_hotkey: string
  /** Empty = unbound. */
  marker_hotkey: string
  capture_backend: 'kwin' | 'portal' | 'windows' | null
  launch_at_login: boolean
  start_in_tray: boolean
  close_to_tray: boolean
  notify_on_save: boolean
  sound_on_save: boolean
  /** Percent of the sound file's own level, 0–200. */
  sound_volume: number
  /** Played when a clip saves or an edit renders. */
  clip_sound: Sound
  /** Played when a screenshot saves; null = same as clips. */
  screenshot_sound: Sound | null
  /** Windows: popup over the game when recording starts and on a save. */
  overlay: boolean
  /** Look for new releases now and then (builds that can update only). */
  check_for_updates: boolean
  /** "Clipping <game>" on the user's Discord profile while recording. */
  discord_presence: boolean
  fps: number
  bitrate_kbps: number
  codec: Codec
  encoder: Encoder | null
  cursor: boolean
  /** Try DMA-BUF capture into GPU memory first; falls back by itself. */
  zero_copy: boolean
  /** Windows: tone-map HDR games (captures the game's area of the screen). */
  hdr_compatibility: boolean
  /** Windows, with HDR compatibility: capture the window itself in HDR (experimental). */
  hdr_window_capture: boolean
  /** Apps marked "not a game"; never recorded. */
  ignored_games: { name: string; exe?: string }[]
  desktop_audio: boolean
  /** Output whose sound is recorded; null = the system default. */
  desktop_device: string | null
  mic_device: string | null
  audio_bitrate_kbps: number
  storage_limit_gb: number | null
  protect_edits: boolean
  protect_favorites: boolean
  use_trash: boolean
  ads: boolean
}

export type Codec = 'h264' | 'hevc' | 'av1'
export type Encoder = 'nvenc' | 'vaapi' | 'amf' | 'qsv' | 'mf' | 'x264'

export interface Capabilities {
  display_hz: number | null
  encoders: { encoder: Encoder; codecs: Codec[] }[]
  mic_devices: { id: string; name: string }[]
  output_devices: { id: string; name: string }[]
}

export interface Session {
  game: string
  steam_app_id: number | null
  start_ms: number
  end_ms: number
}

export interface GameEntry {
  name: string
  steam_app_id: number | null
  art: { capsule: string | null; hero: string | null; logo: string | null }
  fallback_image: string | null
  last_played_ms: number | null
  last_activity_ms: number
  total_play_ms: number
  clip_count: number
  clip_bytes: number
  sessions: Session[]
  playing: boolean
}

export interface Usage {
  clips_bytes: number
  clip_count: number
  disk_free: number
  disk_total: number
  limit_bytes: number | null
}

export const api = {
  storage: () => invoke<Usage>('get_storage'),
  capabilities: () => invoke<Capabilities>('get_capabilities'),
  chooseScreen: () => invoke<void>('choose_screen'),
  hotkeyBackend: () => invoke<HotkeyBackend | null>('hotkey_backend'),
  status: () => invoke<Status>('get_status'),
  save: () => invoke<string>('save_clip'),
  screenshot: () => invoke<string>('save_screenshot'),
  ignoreCurrentGame: () => invoke<Settings>('ignore_current_game'),
  clips: () => invoke<Clip[]>('list_clips'),
  games: () => invoke<GameEntry[]>('list_games'),
  deleteClip: (path: string) => invoke<void>('delete_clip', { path }),
  updateClip: (path: string, patch: ClipPatch) => invoke<Clip>('update_clip', { path, patch }),
  reveal: (path: string) => invoke<void>('reveal_clip', { path }),
  openFolder: () => invoke<void>('open_clips_folder'),
  /** Rejects with a message naming the folder when it can't be opened. */
  openLogFolder: () => invoke<void>('open_log_folder'),
  settings: () => invoke<Settings>('get_settings'),
  setSettings: (settings: Settings) => invoke<void>('set_settings', { settings }),
  openForEdit: (path: string) => invoke<Clip>('open_for_edit', { path }),
  /** `maxMb`: fit the file under this size (null = full quality); `vertical`:
   *  crop to 9:16 at this position (0–1); `slow`: play a range slower. */
  render: (
    path: string,
    start: number,
    end: number,
    volumes: number[],
    maxMb: number | null,
    vertical: number | null = null,
    slow: { from: number; to: number; factor: number } | null = null,
  ) => invoke<string>('render_clip', { path, start, end, volumes, maxMb, vertical, slow }),
  /** How a clip of `seconds` would be fitted under `maxMb`; rejects with how long fits. */
  exportPlan: (maxMb: number, seconds: number, audio: boolean) =>
    invoke<ExportPlan>('export_plan', { maxMb, seconds, audio }),
  cancelRender: () => invoke<void>('cancel_render'),
  copyClip: (path: string) => invoke<void>('copy_clip', { path }),
  startAudioMeters: () => invoke<void>('start_audio_meters'),
  stopAudioMeters: () => invoke<void>('stop_audio_meters'),
  /** Check a picked sound file and copy it in; resolves to the sound to store. */
  importSound: (path: string, slot: 'clip' | 'screenshot') => invoke<Sound>('import_sound', { path, slot }),
  previewSound: (sound: Sound, volume: number) => invoke<void>('preview_sound', { sound, volume }),
  /** Whether this build can update itself, and its version. */
  updateSupport: () => invoke<{ configured: boolean; version: string }>('update_support'),
  /** What the automatic checks found so far (also sent as `update-available`). */
  foundUpdate: () => invoke<UpdateInfo | null>('found_update'),
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  /** Downloads, installs and restarts; resolves only on failure. */
  /** Restarts into the new version, unless a game is being recorded: then
   *  it answers with the game's name and installs when that game closes. */
  installUpdate: () => invoke<string | null>('install_update'),
  /** Stop the download, or drop an update waiting for a game to close. */
  cancelUpdate: () => invoke<void>('cancel_update'),
  /** The version this launch was just updated to; answers once. */
  updatedTo: () => invoke<string | null>('updated_to'),
}

export const onRenderProgress = (f: (p: { path: string; fraction: number }) => void): Promise<UnlistenFn> =>
  listen<{ path: string; fraction: number }>('render-progress', (e) => f(e.payload))

/** A live level from Settings → Audio's meters: dBFS, or why the device didn't open. */
export type AudioLevel = { track: 'desktop' | 'mic'; peak_db: number | null; error: string | null }

export const onAudioLevel = (f: (l: AudioLevel) => void): Promise<UnlistenFn> =>
  listen<AudioLevel>('audio-level', (e) => f(e.payload))

export const onStatusChanged = (f: (s: Status) => void): Promise<UnlistenFn> =>
  listen<Status>('status-changed', (e) => f(e.payload))

export const onLibraryChanged = (f: () => void): Promise<UnlistenFn> =>
  listen('library-changed', () => f())

/** The backend changed the settings itself (the clips folder moved). */
export const onSettingsChanged = (f: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>('settings-changed', (e) => f(e.payload))

export const onClipSaved = (f: (path: string) => void): Promise<UnlistenFn> =>
  listen<string>('clip-saved', (e) => f(e.payload))

export const onScreenshotSaved = (f: (path: string) => void): Promise<UnlistenFn> =>
  listen<string>('screenshot-saved', (e) => f(e.payload))

export const onUpdateAvailable = (f: (u: UpdateInfo) => void): Promise<UnlistenFn> =>
  listen<UpdateInfo>('update-available', (e) => f(e.payload))

/** Download progress of an update being installed, 0–1 (null: size unknown). */
export const onUpdateProgress = (f: (fraction: number | null) => void): Promise<UnlistenFn> =>
  listen<number | null>('update-progress', (e) => f(e.payload))

/** A moment was marked in the recording. */
export const onMarked = (f: (m: { game: string; at: number }) => void): Promise<UnlistenFn> =>
  listen<{ game: string; at: number }>('moment-marked', (e) => f(e.payload))

/** A clip's thumbnail, sprite and duration were generated in the background. */
export const onThumbnailReady = (f: (c: Clip) => void): Promise<UnlistenFn> =>
  listen<Clip>('thumbnail-ready', (e) => f(e.payload))

/** URL the webview can load a local image from. */
export const fileUrl = (path: string) => convertFileSrc(path)

// WebKitGTK can't stream <video> from the asset protocol, so videos come
// from the app's loopback media server.
let mediaBase = ''
export async function initMedia() {
  mediaBase = await invoke<string>('media_base')
}
export const videoUrl = (path: string) => mediaBase + encodeURIComponent(path)
