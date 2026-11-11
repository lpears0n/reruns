mod clips_folder;
mod discord;
mod discord_icons;
mod games;
mod hotkeys;
mod ipc_server;
mod library;
mod logging;
mod media_server;
mod overlay;
mod service;
mod settings;
mod storage;
mod system;
mod thumbs;
mod updates;
mod watcher;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use clip_core::{Codec, Encoder, export::Edit};
use serde::Serialize;

use service::{Service, Status};
use settings::Settings;
use tauri::{
    AppHandle, Emitter, Listener, Manager, RunEvent, State, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_opener::OpenerExt;

pub(crate) struct AppState {
    pub(crate) service: Service,
    pub(crate) settings: Mutex<Settings>,
    hotkeys: hotkeys::Hotkeys,
    encoder: OnceLock<Encoder>,
    /// Which encoders handle which codecs; probing test-encodes, so do it once.
    encoders: tokio::sync::OnceCell<Vec<EncoderSupport>>,
    media: media_server::MediaServer,
    /// Set by `cancel_render` to stop the render in progress (one at a time).
    render_cancel: Arc<AtomicBool>,
    /// An edit is being rendered: an update must not restart the app now.
    pub(crate) rendering: Arc<AtomicBool>,
    thumbs: thumbs::Thumbnailer,
    /// Folder watcher for the current clips folder; replaced when it changes.
    watcher: Mutex<Option<watcher::Handle>>,
    /// Live audio levels while Settings → Audio is open. Starts and stops
    /// take turns (an async lock, held across the device work): old meters
    /// close their devices before new ones open them, and a stop that
    /// arrives while meters are starting removes them once they have.
    meter: tokio::sync::Mutex<Option<clip_core::meter::Meter>>,
    /// Kept open: on Linux the copied file list is served by this process
    /// for as long as the clipboard object lives.
    clipboard: Mutex<Option<arboard::Clipboard>>,
    discord: discord::Presence,
}

/// Settings the backend changed by itself (the clips folder moving); the
/// window replaces its copy with the payload.
pub const SETTINGS_CHANGED: &str = "settings-changed";

/// The folder to save a new clip or screenshot in: the clips folder, or
/// when that can't be written to ([`clips_folder`]), `~/Reruns` from now
/// on. The clips folder setting moves with it, so the library (which
/// lists one folder) shows what was saved; the user is told which folder
/// failed, to pick it again once it works (a drive plugged back in).
pub(crate) fn save_dir(app: &AppHandle) -> PathBuf {
    let dir = app.state::<AppState>().clips_dir();
    match clips_folder::fallback().and_then(|f| clips_folder::instead_of(&dir, &f)) {
        Some(fallback) => {
            move_clips_dir(app, &dir, &fallback);
            fallback
        }
        None => dir,
    }
}

/// Start-up: a default clips folder that can't be written to moves now,
/// not at the first save. A folder the user picked waits for a save: its
/// drive may not be there yet.
fn check_default_clips_dir(app: &AppHandle) {
    let dir = app.state::<AppState>().clips_dir();
    if dir != clip_core::default_clips_dir() {
        return;
    }
    if let Some(to) = clips_folder::fallback().and_then(|f| clips_folder::instead_of(&dir, &f)) {
        move_clips_dir(app, &dir, &to);
    }
}

/// The clips folder can't be written to: use `to` from now on.
fn move_clips_dir(app: &AppHandle, from: &Path, to: &Path) {
    let state = app.state::<AppState>();
    let settings = {
        let mut s = state.settings.lock().unwrap();
        s.clips_dir = to.to_owned();
        s.clone()
    };
    if let Err(e) = settings.save() {
        tracing::warn!("saving settings: {e:#}");
    }
    tracing::warn!(
        from = %from.display(),
        to = %to.display(),
        "can't write to the clips folder; moved it"
    );
    allow_clips_dir(app, to);
    watch_clips_dir(app, to);
    let _ = app.emit(SETTINGS_CHANGED, &settings);
    // The default folder failing is Controlled folder access, all but
    // always; a folder the user picked may as well be a missing drive.
    let why = if cfg!(windows) && from == clip_core::default_clips_dir() {
        format!("Windows blocked saving in {}.", from.display())
    } else {
        format!("Can't write to {}.", from.display())
    };
    notify_later(app, format!("Clips now save in {}", to.display()), why);
}

fn notify_later(app: &AppHandle, summary: String, body: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = system::notify(&app, &summary, &body).await {
            tracing::debug!("notify: {e:#}");
        }
    });
}

/// Put a clip on the clipboard as a file, so pasting into Discord, a chat
/// or a folder brings the video itself.
#[tauri::command]
fn copy_clip(state: State<'_, AppState>, path: PathBuf) -> CmdResult<()> {
    if !path.is_file() {
        return Err("clip not found".into());
    }
    let mut clipboard = state.clipboard.lock().unwrap();
    if clipboard.is_none() {
        *clipboard = Some(arboard::Clipboard::new().map_err(|e| format!("no clipboard: {e}"))?);
    }
    clipboard
        .as_mut()
        .unwrap()
        .set()
        .file_list(&[&path])
        .map_err(|e| format!("couldn't copy the clip: {e}"))
}

/// One audio meter reading for the page: `track` is "desktop" or "mic";
/// `peak_db` is dBFS, or `error` says why the device didn't open.
#[derive(Clone, serde::Serialize)]
struct AudioLevel {
    track: &'static str,
    peak_db: Option<f64>,
    error: Option<String>,
}

/// Start (or restart, after a device change) the level meters for the
/// audio tracks the current settings would record. Off the main thread:
/// listing and opening audio devices can take a moment, which froze the
/// window as the Audio page opened.
#[tauri::command]
async fn start_audio_meters(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let mut slot = state.meter.lock().await;
    let sources = state.settings.lock().unwrap().recorder_config().audio;
    let names: Vec<&'static str> = sources.iter().map(|s| s.name()).collect();
    let old = slot.take();
    let meter = tauri::async_runtime::spawn_blocking(move || {
        // The old meters close their devices before the new ones open them.
        drop(old);
        clip_core::meter::Meter::start(&sources, move |r| {
            let level = match r {
                clip_core::meter::Reading::Level { index, peak_db } => AudioLevel {
                    track: names[index],
                    peak_db: Some(peak_db),
                    error: None,
                },
                clip_core::meter::Reading::Failed { index, error } => AudioLevel {
                    track: names[index],
                    peak_db: None,
                    error: Some(error),
                },
            };
            let _ = app.emit("audio-level", level);
        })
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))?;
    *slot = Some(meter);
    Ok(())
}

#[tauri::command]
async fn stop_audio_meters(state: State<'_, AppState>) -> CmdResult<()> {
    let mut slot = state.meter.lock().await;
    if let Some(m) = slot.take() {
        // Off the async runtime: closing devices can block.
        let _ = tauri::async_runtime::spawn_blocking(move || drop(m)).await;
    }
    Ok(())
}

/// Emitted a few times a second while an edit renders.
const RENDER_PROGRESS: &str = "render-progress";

#[derive(Clone, Serialize)]
struct RenderProgress {
    path: PathBuf,
    fraction: f64,
}

#[tauri::command]
fn cancel_render(state: State<'_, AppState>) {
    state.render_cancel.store(true, Ordering::Relaxed);
}

/// Trimmer playback stats from `HTMLVideoElement.getVideoPlaybackQuality()`,
/// logged to diagnose stutter in the webview's video path.
#[tauri::command]
fn log_playback(path: PathBuf, total: u64, dropped: u64, corrupted: u64, seconds: f64) {
    tracing::info!(
        path = %path.display(),
        total,
        dropped,
        corrupted,
        seconds,
        fps = total as f64 / seconds.max(0.001),
        "trimmer playback"
    );
}

/// Portal capture: show the desktop's screen picker once.
#[tauri::command]
async fn choose_screen(state: State<'_, AppState>) -> CmdResult<()> {
    state.service.choose_screen().await
}

/// How the global hotkey is provided: "kglobalaccel", "portal", or null when
/// the desktop has neither (the UI then explains `rerunsctl save`).
#[tauri::command]
async fn hotkey_backend(state: State<'_, AppState>) -> CmdResult<Option<hotkeys::Backend>> {
    Ok(state.hotkeys.backend().await)
}

#[derive(Serialize)]
struct Library {
    clips: Vec<library::Clip>,
    /// Recently played games with clips, sessions and Steam art.
    games: Vec<games::GameEntry>,
}

/// Every clip and the games they belong to, from one pass over the
/// clips folder (the page used to ask for each list separately, and each
/// read every clip's file and sidecar).
#[tauri::command]
async fn list_library(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Library> {
    let status = state.service.status().await.ok();
    let dir = state.clips_dir();
    let ignored = state.settings.lock().unwrap().ignored_games.clone();
    let library = tauri::async_runtime::spawn_blocking(move || {
        let clips = library::list(&dir);
        let playing = status
            .as_ref()
            .and_then(|s| Some((s.game.clone()?, s.steam_app_id, s.session_start_ms?)));
        let games = games::list(
            &clips,
            playing.as_ref().map(|(n, id, t)| (n.as_str(), *id, *t)),
            &ignored,
        );
        Library { clips, games }
    })
    .await
    .map_err(err)?;
    // Images are made in the background; each arrives as a thumbnail-ready event.
    for c in library.clips.iter().filter(|c| !library::is_cached(c)) {
        state.thumbs.request(c.path.clone(), c.game.clone());
    }
    // Again here in case Steam was installed after start-up.
    allow_steam_art(&app);
    Ok(library)
}

/// Base URL for `<video>` sources; append the percent-encoded file path.
#[tauri::command]
fn media_base(state: State<'_, AppState>) -> String {
    state.media.base_url.clone()
}

#[derive(Debug, Clone, Serialize)]
struct EncoderSupport {
    encoder: Encoder,
    codecs: Vec<Codec>,
}

/// What this machine can do, for the settings UI.
#[derive(Debug, Clone, Serialize)]
struct Capabilities {
    display_hz: Option<f64>,
    encoders: Vec<EncoderSupport>,
    mic_devices: Vec<clip_core::AudioDevice>,
    output_devices: Vec<clip_core::AudioDevice>,
}

#[tauri::command]
async fn get_capabilities(state: State<'_, AppState>) -> CmdResult<Capabilities> {
    let encoders = state
        .encoders
        .get_or_init(|| async {
            tauri::async_runtime::spawn_blocking(|| {
                Encoder::ALL
                    .into_iter()
                    .map(|encoder| EncoderSupport {
                        encoder,
                        codecs: Codec::ALL
                            .into_iter()
                            .filter(|&c| encoder.offered(c))
                            .collect(),
                    })
                    .collect()
            })
            .await
            .unwrap_or_default()
        })
        .await
        .clone();
    let (display_hz, mic_devices, output_devices) = tauri::async_runtime::spawn_blocking(|| {
        (
            clip_core::display::max_refresh_hz().ok().flatten(),
            clip_core::input_devices(),
            clip_core::output_devices(),
        )
    })
    .await
    .map_err(err)?;
    Ok(Capabilities {
        display_hz,
        encoders,
        mic_devices,
        output_devices,
    })
}

impl AppState {
    pub(crate) fn clips_dir(&self) -> PathBuf {
        self.settings.lock().unwrap().clips_dir.clone()
    }
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> CmdResult<Status> {
    state.service.status().await
}

#[tauri::command]
async fn save_clip(app: AppHandle, state: State<'_, AppState>) -> CmdResult<PathBuf> {
    state.service.save(save_dir(&app)).await
}

/// "Not a game": stop recording the current game and never detect it
/// again. Returns the updated settings (the entry is on their list).
#[tauri::command]
async fn ignore_current_game(state: State<'_, AppState>) -> CmdResult<Settings> {
    let entry = state
        .service
        .ignore_entry()
        .await?
        .ok_or("no game is being recorded")?;
    let settings = {
        let mut s = state.settings.lock().unwrap();
        if !s.ignored_games.contains(&entry) {
            s.ignored_games.push(entry);
        }
        s.clone()
    };
    settings.save().map_err(|e| format!("{e:#}"))?;
    state.service.configure(settings.recorder_config())?;
    Ok(settings)
}

/// Check a sound file the user picked and copy it in for `slot` ("clip" or
/// "screenshot"). Returns the sound for the settings to point at.
#[tauri::command]
async fn import_sound(path: PathBuf, slot: String) -> CmdResult<settings::Sound> {
    if !matches!(slot.as_str(), "clip" | "screenshot") {
        return Err(format!("unknown sound slot {slot:?}"));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let copy = tauri::async_runtime::spawn_blocking(move || system::import_sound(&path, &slot))
        .await
        .map_err(err)?
        .map_err(|e| format!("{e:#}"))?;
    Ok(settings::Sound::Custom { path: copy, name })
}

/// Play a sound now, for the Preview buttons in Settings.
#[tauri::command]
fn preview_sound(sound: settings::Sound, volume: u32) {
    system::play_sound(&sound, volume);
}

#[tauri::command]
async fn save_screenshot(app: AppHandle, state: State<'_, AppState>) -> CmdResult<PathBuf> {
    state.service.screenshot(save_dir(&app)).await
}

#[tauri::command]
async fn delete_clip(state: State<'_, AppState>, path: PathBuf) -> CmdResult<()> {
    let use_trash = state.settings.lock().unwrap().use_trash;
    library::delete(&state.clips_dir(), &path, use_trash).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn update_clip(
    state: State<'_, AppState>,
    path: PathBuf,
    patch: library::MetaPatch,
) -> CmdResult<library::Clip> {
    let dir = state.clips_dir();
    tauri::async_runtime::spawn_blocking(move || library::update_meta(&dir, &path, patch))
        .await
        .map_err(err)?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn get_storage(state: State<'_, AppState>) -> CmdResult<storage::Usage> {
    let (dir, limit) = {
        let s = state.settings.lock().unwrap();
        (s.clips_dir.clone(), s.storage_policy().limit_bytes)
    };
    tauri::async_runtime::spawn_blocking(move || storage::usage(&dir, limit))
        .await
        .map_err(err)
}

use watcher::LIBRARY_CHANGED;

/// Apply the storage limit off the async runtime; tell the UI if anything
/// was removed.
fn enforce_storage(app: &AppHandle) {
    let (dir, policy) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (s.clips_dir.clone(), s.storage_policy())
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || match storage::enforce(&dir, policy) {
        Ok(removed) if !removed.is_empty() => {
            let _ = app.emit(LIBRARY_CHANGED, removed.len());
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("storage limit: {e:#}"),
    });
}

#[tauri::command]
fn reveal_clip(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(path).map_err(err)
}

#[tauri::command]
fn open_clips_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.clips_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(err)
}

/// Show `reruns.log` in the file manager, for attaching to a bug report.
/// The error names the folder, so it can still be found by hand.
fn open_logs(app: &AppHandle) -> CmdResult<()> {
    let log = logging::log_path();
    if app.opener().reveal_item_in_dir(&log).is_ok() {
        return Ok(());
    }
    // No log file yet, or a Linux desktop with neither FileManager1 nor the
    // portal to select it: open the folder instead.
    let dir = log.parent().unwrap_or(&log);
    std::fs::create_dir_all(dir)
        .map_err(err)
        .and_then(|()| {
            app.opener()
                .open_path(dir.to_string_lossy(), None::<&str>)
                .map_err(err)
        })
        .map_err(|e| format!("Can't open {}: {e}", dir.display()))
}

#[tauri::command]
fn open_log_folder(app: AppHandle) -> CmdResult<()> {
    open_logs(&app)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<()> {
    let (old_bindings, old_autostart) = {
        let s = state.settings.lock().unwrap();
        (s.hotkey_bindings(), s.launch_at_login)
    };
    if settings.launch_at_login != old_autostart {
        system::set_autostart(settings.launch_at_login).map_err(|e| format!("{e:#}"))?;
    }
    for b in settings.hotkey_bindings() {
        if !old_bindings.contains(&b) {
            state.hotkeys.set_keys(&app, b.action, &b.keys).await?;
        }
    }
    settings.save().map_err(|e| format!("{e:#}"))?;
    allow_clips_dir(&app, &settings.clips_dir);
    if settings.clips_dir != state.settings.lock().unwrap().clips_dir {
        watch_clips_dir(&app, &settings.clips_dir);
    }
    state.service.configure(settings.recorder_config())?;
    state.discord.set_enabled(settings.discord_presence);
    *state.settings.lock().unwrap() = settings;
    enforce_storage(&app);
    Ok(())
}

/// Load any video for editing (also from outside the library).
#[tauri::command]
async fn open_for_edit(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> CmdResult<library::Clip> {
    app.asset_protocol_scope().allow_file(&path).map_err(err)?;
    state.media.allow(&path);
    let clip = tauri::async_runtime::spawn_blocking(move || {
        let game = path
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        library::load(&path, &game)
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))?;
    if !library::is_cached(&clip) {
        state.thumbs.request(clip.path.clone(), clip.game.clone());
    }
    Ok(clip)
}

/// Trim `path` to `start..end` seconds, mixing audio tracks at `volumes`.
/// A slow-motion range from the editor, in seconds; `factor` 2 = half speed.
#[derive(serde::Deserialize)]
struct SlowRange {
    from: f64,
    to: f64,
    factor: f64,
}

// Each argument is a field of the page's request.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn render_clip(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    start: f64,
    end: f64,
    volumes: Vec<f64>,
    max_mb: Option<f64>,
    vertical: Option<f64>,
    slow: Option<SlowRange>,
) -> CmdResult<PathBuf> {
    let (out, cfg) = {
        let s = state.settings.lock().unwrap();
        (library::edit_path(&s.clips_dir, &path), s.video_config())
    };
    let encoder = match state.encoder.get() {
        Some(e) => *e,
        None => {
            let e = tauri::async_runtime::spawn_blocking(|| Encoder::probe(Codec::H264))
                .await
                .map_err(err)?
                .ok_or("no working H.264 encoder")?;
            *state.encoder.get_or_init(|| e)
        }
    };
    // The page's levels are for the sources' own tracks. A clip that
    // starts with them mixed into one leaves that track out (at 0), or the
    // export would play everything twice.
    let mut volumes = volumes;
    if library::read_meta(&path).is_some_and(|m| m.mixed_audio) {
        volumes.insert(0, 0.0);
    }
    let edit = Edit {
        start: clip_core::gst::ClockTime::from_nseconds((start.max(0.0) * 1e9) as u64),
        end: clip_core::gst::ClockTime::from_nseconds((end.max(0.0) * 1e9) as u64),
        volumes,
        max_bytes: max_mb.filter(|mb| *mb > 0.0).map(megabytes),
        vertical: vertical.map(|x| x.clamp(0.0, 1.0)),
        slow: slow.map(|s| clip_core::export::Slow {
            from: clip_core::gst::ClockTime::from_nseconds((s.from.max(0.0) * 1e9) as u64),
            to: clip_core::gst::ClockTime::from_nseconds((s.to.max(0.0) * 1e9) as u64),
            factor: s.factor,
        }),
    };
    let result = out.clone();
    let source = path.clone();
    let cancel = state.render_cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let progress_app = app.clone();
    let progress_path = path.clone();
    let control = clip_core::export::Control {
        on_progress: Some(Box::new(move |fraction| {
            let _ = progress_app.emit(
                RENDER_PROGRESS,
                RenderProgress {
                    path: progress_path.clone(),
                    fraction,
                },
            );
        })),
        cancel: Some(cancel),
    };
    let rendering = state.rendering.clone();
    rendering.store(true, Ordering::Relaxed);
    let rendered = tauri::async_runtime::spawn_blocking(move || {
        clip_core::export::render_with(&path, &out, &edit, encoder, &cfg, control)
    })
    .await;
    rendering.store(false, Ordering::Relaxed);
    let done = rendered
        .map_err(err)
        .and_then(|r| r.map_err(|e| format!("{e:#}")))
        .map(|()| {
            library::copy_meta(&source, &result, start, end);
            let _ = app.emit(service::CLIP_SAVED, &result);
            result
        });
    // A window hidden in the tray was kept for this render: free it once
    // its page has had the result.
    free_main_later(&app);
    // An update that was waiting for this render (however it ended).
    tauri::async_runtime::spawn(updates::install_waiting(app.clone()));
    done
}

/// How a clip of `seconds` would be fitted under `max_mb`, for the export
/// size picker; an error says how long a clip may be.
#[tauri::command]
fn export_plan(max_mb: f64, seconds: f64, audio: bool) -> CmdResult<clip_core::export::SizePlan> {
    clip_core::export::plan_size(megabytes(max_mb), seconds, audio).map_err(|e| format!("{e:#}"))
}

/// Upload limits are in decimal megabytes.
fn megabytes(mb: f64) -> u64 {
    (mb * 1e6) as u64
}

/// Let the webview load videos and thumbnails through the asset protocol.
/// (Re)start the folder watcher on `dir`.
fn watch_clips_dir(app: &AppHandle, dir: &std::path::Path) {
    let state = app.state::<AppState>();
    let handle = match watcher::watch(app.clone(), dir) {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::warn!("can't watch {}: {e:#}", dir.display());
            None
        }
    };
    *state.watcher.lock().unwrap() = handle;
}

fn allow_clips_dir(app: &AppHandle, dir: &std::path::Path) {
    let scope = app.asset_protocol_scope();
    for d in [dir.to_owned(), library::cache_dir()] {
        let _ = std::fs::create_dir_all(&d);
        if let Err(e) = scope.allow_directory(&d, true) {
            tracing::warn!("asset scope for {}: {e}", d.display());
        }
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.media.allow(dir);
    }
}

/// Steam's art lives outside the clips folder. Allowed from start-up, not
/// per file once listed: the window paints its cached library first, and
/// Steam moves the files when a game's art changes.
fn allow_steam_art(app: &AppHandle) {
    let scope = app.asset_protocol_scope();
    for root in clip_core::game::steam_roots() {
        let dir = root.join("appcache").join("librarycache");
        if let Err(e) = scope.allow_directory(&dir, true) {
            tracing::warn!("asset scope for {}: {e}", dir.display());
        }
    }
}

const MAIN: &str = "main";

/// Hidden in the tray this long, the window is closed for real: its
/// webview is the app's largest piece of memory (100–250 MB) and all of
/// it is idle while a game is played. Opening it again builds a new one,
/// which paints from the cached library.
const FREE_MAIN_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// Bumped whenever the window is shown or hidden, so a pending
/// [`free_main_later`] only acts if nothing happened since.
static MAIN_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Show the main window, building it first if there is none (a launch
/// into the tray makes none, and one left hidden is closed after a
/// while).
pub(crate) fn show_main(app: &AppHandle) {
    MAIN_GENERATION.fetch_add(1, Ordering::SeqCst);
    let window = match app.get_webview_window(MAIN) {
        Some(w) => w,
        None => match create_main(app) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("can't open the window: {e}");
                return;
            }
        },
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn create_main(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let window = WebviewWindowBuilder::from_config(app, config)?.build()?;
    // Windows: the app's header is the title bar (its own minimize,
    // maximize and close), so the system frame goes before the window is
    // first shown. Windows 11 keeps the rounded corners, shadow and
    // resize edges of an undecorated window.
    #[cfg(windows)]
    let _ = window.set_decorations(false);
    Ok(window)
}

/// Close the hidden main window now, unless it is in use: visible, or
/// showing an edit being rendered.
fn free_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    if window.is_visible().unwrap_or(true)
        || app.state::<AppState>().rendering.load(Ordering::Relaxed)
    {
        return;
    }
    tracing::info!("closing the hidden window to free its memory");
    let _ = window.destroy();
}

/// [`free_main`] once the window has stayed hidden for [`FREE_MAIN_AFTER`].
fn free_main_later(app: &AppHandle) {
    let generation = MAIN_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FREE_MAIN_AFTER).await;
        if MAIN_GENERATION.load(Ordering::SeqCst) == generation {
            free_main(&app);
        }
    });
}

/// What was saved, for its notification, sound and popup.
#[derive(Clone, Copy)]
enum Saved {
    Clip,
    /// Rendered in the app: no popup over a game.
    Edit,
    Screenshot,
}

impl Saved {
    fn summary(self) -> &'static str {
        match self {
            Saved::Clip => "Clip saved",
            Saved::Edit => "Edit rendered",
            Saved::Screenshot => "Screenshot saved",
        }
    }

    fn popup(self) -> Option<overlay::Kind> {
        match self {
            Saved::Clip => Some(overlay::Kind::Saved),
            Saved::Edit => None,
            Saved::Screenshot => Some(overlay::Kind::Screenshot),
        }
    }
}

/// Notification, sound and overlay for a newly saved clip, edit or
/// screenshot, as configured.
fn announce_saved(app: &AppHandle, saved: Saved, path: &std::path::Path) {
    let (notify, sound) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        let sound = match saved {
            Saved::Clip | Saved::Edit => s.clip_sound.clone(),
            Saved::Screenshot => s.screenshot_sound().clone(),
        };
        (
            s.notify_on_save,
            s.sound_on_save.then_some((sound, s.sound_volume)),
        )
    };
    if let Some((sound, volume)) = sound {
        system::play_sound(&sound, volume);
    }
    let summary = saved.summary();
    let game = path
        .parent()
        .and_then(std::path::Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(kind) = saved.popup()
        && overlay::enabled(app)
    {
        overlay::show(app, overlay::Popup::saved(kind, &game));
    }
    if notify {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = system::notify(&app, summary, &game).await {
                tracing::debug!("notify: {e:#}");
            }
        });
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Reruns", true, None::<&str>)?;
    let save = MenuItem::with_id(app, "save", "Save clip", true, None::<&str>)?;
    let shot = MenuItem::with_id(app, "screenshot", "Save screenshot", true, None::<&str>)?;
    let mark = MenuItem::with_id(app, "mark", "Mark a moment", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, "logs", "Open log folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &save, &shot, &mark, &logs, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Reruns")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "save" | "screenshot" | "mark" => {
                let action = match event.id.as_ref() {
                    "save" => clip_core::Action::SaveClip,
                    "screenshot" => clip_core::Action::Screenshot,
                    _ => clip_core::Action::Marker,
                };
                let app = app.clone();
                tauri::async_runtime::spawn(async move { hotkeys::fire(&app, action).await });
            }
            "logs" => {
                if let Err(e) = open_logs(app) {
                    tracing::warn!("{e}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// The installer puts GStreamer's libraries next to the executable (where
/// the loader finds them) and its plugins under `gst-plugins/`; tell
/// GStreamer about the latter before it initialises. A machine with its own
/// GStreamer install, or a dev build, has nothing there and is left alone.
#[cfg(windows)]
fn point_gstreamer_at_bundled_plugins() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let plugins = exe.with_file_name("gst-plugins");
    if plugins.is_dir() {
        // SAFETY: called on the main thread before any other thread exists.
        unsafe {
            std::env::set_var("GST_PLUGIN_PATH", &plugins);
            // Only the bundled set: a stray system install of a different
            // version must not be mixed in.
            std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &plugins);
        }
        // The plugin list GStreamer caches, in a file of this version's
        // own. Its default is one file per user shared by every GStreamer
        // on the PC (another install rewrites it for its own plugins, and
        // each then rescans at its next start) in a folder disk clean-up
        // empties.
        if std::env::var_os("GST_REGISTRY").is_none()
            && let Some(dir) = dirs::data_local_dir().map(|d| d.join("gg.reruns.app"))
        {
            let name = registry_file_name(env!("CARGO_PKG_VERSION"));
            let _ = std::fs::create_dir_all(&dir);
            // Earlier versions' lists are no use to anyone.
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let file = entry.file_name();
                    let file = file.to_string_lossy();
                    if is_registry_file(&file) && *file != name {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            // SAFETY: as above.
            unsafe {
                std::env::set_var("GST_REGISTRY", dir.join(name));
            }
        }
    }
}

#[cfg(windows)]
fn registry_file_name(version: &str) -> String {
    format!("gst-registry-{version}.bin")
}

#[cfg(windows)]
fn is_registry_file(name: &str) -> bool {
    name.starts_with("gst-registry-") && name.ends_with(".bin")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log = %logging::log_path().display(),
        "Reruns starting"
    );
    // Before anything slow: until the socket is ours, a second launch would
    // also find no instance and start recording alongside this one.
    let control = match ipc_server::claim() {
        Ok(ipc_server::Claim::Ours(listener)) => Some(listener),
        Ok(ipc_server::Claim::Running) => {
            tracing::info!("Reruns is already running; showed its window");
            return;
        }
        Err(e) => {
            tracing::warn!("control socket unavailable: {e:#}");
            None
        }
    };
    #[cfg(windows)]
    point_gstreamer_at_bundled_plugins();
    if let Err(e) = clip_core::init() {
        logging::fatal(&format!("GStreamer failed to initialise: {e:#}"));
    }

    let context = tauri::generate_context!();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init());
    // Only builds made with an update signing key can update themselves.
    let builder = match updates::plugin(context.config()) {
        Some(updater) => builder.plugin(updater),
        None => builder,
    };
    #[cfg(windows)]
    let builder = builder
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_notification::init());
    builder
        // Remember the window's size and position between launches. The
        // window starts hidden and VISIBLE is skipped so a tray launch stays
        // in the tray.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE
                        // The frame is chosen per platform below, not restored.
                        - tauri_plugin_window_state::StateFlags::DECORATIONS,
                )
                // The overlay is placed over the game each time it shows.
                .with_denylist(&["overlay"])
                .build(),
        )
        .setup(move |app| {
            let settings = Settings::load();
            let media = tauri::async_runtime::block_on(media_server::MediaServer::start())?;
            let token_path = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("reruns/portal-restore-token");
            let service = Service::spawn(
                app.handle().clone(),
                settings.recorder_config(),
                settings.capture_backend,
                token_path,
            );
            let bindings = settings.hotkey_bindings();
            let discord = discord::Presence::start(settings.discord_presence);
            app.manage(AppState {
                service,
                settings: Mutex::new(settings),
                hotkeys: hotkeys::Hotkeys::default(),
                encoder: OnceLock::new(),
                encoders: tokio::sync::OnceCell::new(),
                media,
                render_cancel: Arc::default(),
                rendering: Arc::default(),
                thumbs: thumbs::Thumbnailer::start(app.handle().clone()),
                watcher: Mutex::new(None),
                meter: tokio::sync::Mutex::new(None),
                clipboard: Mutex::new(None),
                discord,
            });
            {
                let handle = app.handle().clone();
                app.listen_any(service::STATUS_CHANGED, move |event| {
                    let status: serde_json::Value =
                        serde_json::from_str(event.payload()).unwrap_or_default();
                    let playing = (status["state"] == "recording")
                        .then(|| status["game"].as_str())
                        .flatten()
                        .map(|game| discord::Activity {
                            game: game.to_owned(),
                            steam_app_id: status["steam_app_id"]
                                .as_u64()
                                .and_then(|id| u32::try_from(id).ok()),
                            exe: status["exe"].as_str().map(str::to_owned),
                            since_ms: status["session_start_ms"].as_u64(),
                        });
                    handle.state::<AppState>().discord.set_playing(playing);
                });
            }
            check_default_clips_dir(app.handle());
            allow_clips_dir(app.handle(), &app.state::<AppState>().clips_dir());
            allow_steam_art(app.handle());
            watch_clips_dir(app.handle(), &app.state::<AppState>().clips_dir());
            updates::spawn_checks(app.handle());
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    state.hotkeys.start(handle.clone(), bindings).await;
                });
            }
            {
                let handle = app.handle().clone();
                app.listen_any(service::CLIP_SAVED, move |event| {
                    enforce_storage(&handle);
                    let path: PathBuf = serde_json::from_str(event.payload()).unwrap_or_default();
                    let saved = if library::is_edit(&path) {
                        Saved::Edit
                    } else {
                        handle.state::<AppState>().discord.clip_saved();
                        Saved::Clip
                    };
                    announce_saved(&handle, saved, &path);
                });
            }
            {
                let handle = app.handle().clone();
                app.listen_any(service::SCREENSHOT_SAVED, move |event| {
                    let path: PathBuf = serde_json::from_str(event.payload()).unwrap_or_default();
                    announce_saved(&handle, Saved::Screenshot, &path);
                });
            }
            {
                let handle = app.handle().clone();
                app.listen_any(service::MARKED, move |event| {
                    if overlay::enabled(&handle) {
                        let m: service::Marked =
                            serde_json::from_str(event.payload()).unwrap_or_default();
                        overlay::show(&handle, overlay::Popup::marked(&m.game));
                    }
                });
            }
            {
                let handle = app.handle().clone();
                app.listen_any(service::RECORDING_STARTED, move |event| {
                    // A game is on: a window left in the tray goes, once
                    // any delete it holds for Undo has had its few seconds.
                    let app = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        free_main(&app);
                    });
                    if !overlay::enabled(&handle) {
                        return;
                    }
                    let game: String = serde_json::from_str(event.payload()).unwrap_or_default();
                    let hotkey = handle
                        .state::<AppState>()
                        .settings
                        .lock()
                        .unwrap()
                        .hotkey
                        .clone();
                    overlay::show(&handle, overlay::Popup::ready(&game, &hotkey));
                });
            }
            overlay::init(app.handle());
            // The window starts hidden (tauri.conf.json) so a login launch
            // can stay in the tray without flashing.
            let autostarted = std::env::args().any(|a| a == system::AUTOSTART_ARG);
            // After "Update and restart" the window comes back, even from
            // a copy that was started at login.
            let updated = updates::take_marker(app.handle());
            let in_tray = autostarted
                && !updated
                && app
                    .state::<AppState>()
                    .settings
                    .lock()
                    .unwrap()
                    .start_in_tray;
            if !in_tray {
                show_main(app.handle());
            }
            enforce_storage(app.handle());
            setup_tray(app.handle())?;
            if let Some(listener) = control {
                ipc_server::serve(app.handle().clone(), listener);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window
                    .app_handle()
                    .state::<AppState>()
                    .settings
                    .lock()
                    .unwrap()
                    .close_to_tray;
                if to_tray {
                    // Keep recording in the tray.
                    api.prevent_close();
                    let _ = window.hide();
                    free_main_later(window.app_handle());
                } else {
                    window.app_handle().exit(0);
                }
            }
            // The page is gone without its clean-up having run: close
            // the audio meters it may have left open.
            if let WindowEvent::Destroyed = event
                && window.label() == MAIN
            {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let meter = app.state::<AppState>().meter.lock().await.take();
                    if let Some(m) = meter {
                        let _ = tauri::async_runtime::spawn_blocking(move || drop(m)).await;
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            save_clip,
            save_screenshot,
            export_plan,
            import_sound,
            preview_sound,
            updates::update_support,
            updates::found_update,
            updates::check_update,
            updates::install_update,
            updates::cancel_update,
            updates::updated_to,
            ignore_current_game,
            list_library,
            delete_clip,
            update_clip,
            get_storage,
            get_capabilities,
            choose_screen,
            hotkey_backend,
            media_base,
            log_playback,
            reveal_clip,
            open_clips_folder,
            open_log_folder,
            get_settings,
            set_settings,
            open_for_edit,
            render_clip,
            cancel_render,
            start_audio_meters,
            stop_audio_meters,
            copy_clip,
        ])
        .build(context)
        .expect("error while running Reruns")
        .run(|_, event| {
            // With no window left (closed to free memory, or never made
            // for a launch into the tray) Tauri would quit; Reruns lives
            // in the tray. An explicit exit carries a code and goes ahead.
            if let RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
            {
                api.prevent_exit();
            }
        });
}
