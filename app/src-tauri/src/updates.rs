//! Updates from GitHub releases, through Tauri's updater: signed bundles,
//! checked against a public key built into the app.
//!
//! The updater is only switched on in builds that carry that key (release
//! builds once the owner has set it up; see docs/RELEASING.md). Other
//! builds report updates as not set up, and nothing is ever checked.
//!
//! Only the check runs by itself: one small request as the app starts
//! (whose result the app offers in a prompt) and then every six hours, so
//! nothing is downloaded while a game is being played. Downloading and
//! installing happen when the user asks, and end with a restart.
//!
//! Installing ends this process on the spot (Windows: the installer is
//! started and the app exits), which would throw away the replay buffer
//! of a game being recorded. So an update downloaded while a game is
//! recorded waits ([`Pending`]) and installs when that recording stops.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{AppState, service};

/// An update was found; the payload is an [`UpdateInfo`].
pub const UPDATE_AVAILABLE: &str = "update-available";
/// Download progress while installing, 0..=1 (`null` when the size is
/// unknown).
pub const UPDATE_PROGRESS: &str = "update-progress";

const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Longest a check may take: one small file.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest a download may take before it is given up as stalled.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// A downloaded update that waits for the recording to stop, and whether
/// the download in progress was cancelled.
#[derive(Default)]
pub struct Pending {
    update: Mutex<Option<(Update, Vec<u8>)>>,
    cancel: tokio::sync::Notify,
    downloading: AtomicBool,
}

/// The version this launch was updated to, until the page has said so.
#[derive(Default)]
pub struct Updated(Mutex<Option<String>>);
/// After a failed check, as at login before the network is up: try again
/// this soon, then twice as long each time, up to [`CHECK_EVERY`].
const RETRY_FIRST: Duration = Duration::from_secs(60);

/// The newest update the timed checks found. The first check runs as the
/// app starts and can finish before the page listens for
/// [`UPDATE_AVAILABLE`], so the page also asks for it ([`found_update`]).
#[derive(Default)]
pub struct Found(Mutex<Option<UpdateInfo>>);

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release notes, as written on the release.
    pub notes: Option<String>,
    /// Found by the check at start-up: the app asks once whether to update.
    pub prompt: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateSupport {
    /// This build can update itself.
    pub configured: bool,
    pub version: String,
}

/// Whether `config` (as built) has an updater public key.
pub fn configured(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

/// The updater plugin, for builds that can use it.
pub fn plugin(
    config: &tauri::Config,
) -> Option<tauri::plugin::TauriPlugin<tauri::Wry, tauri_plugin_updater::Config>> {
    configured(config).then(|| tauri_plugin_updater::Builder::new().build())
}

/// Check by itself, at once and then on a timer, when this build can update
/// and the user hasn't turned it off.
pub fn spawn_checks(app: &AppHandle) {
    app.manage(Found::default());
    app.manage(Pending::default());
    if !configured(app.config()) {
        return;
    }
    // A downloaded update goes in when the game being recorded closes.
    {
        let handle = app.clone();
        app.listen_any(service::STATUS_CHANGED, move |event| {
            let status: serde_json::Value =
                serde_json::from_str(event.payload()).unwrap_or_default();
            if matches!(status["state"].as_str(), Some("recording" | "retrying")) {
                return;
            }
            let waiting = handle.state::<Pending>().update.lock().unwrap().take();
            if let Some((update, bytes)) = waiting {
                install(&handle, &update, bytes);
            }
        });
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Until a check answers: what it finds is offered in a prompt.
        let mut starting = true;
        let mut failures = 0;
        loop {
            let enabled = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .check_for_updates;
            let mut wait = CHECK_EVERY;
            if !enabled {
                starting = false;
            } else {
                match check(&app).await {
                    Ok(update) => {
                        if let Some(mut info) = update {
                            info.prompt = starting;
                            announce(&app, info);
                        }
                        starting = false;
                        failures = 0;
                    }
                    Err(e) => {
                        tracing::info!("update check: {e}");
                        wait = retry_after(failures);
                        failures += 1;
                    }
                }
            }
            tokio::time::sleep(wait).await;
        }
    });
}

/// Keep `info` for the page and tell it, unless that version was already
/// announced.
fn announce(app: &AppHandle, info: UpdateInfo) {
    let found = app.state::<Found>();
    let mut found = found.0.lock().unwrap();
    if found.as_ref().is_some_and(|f| f.version == info.version) {
        return;
    }
    tracing::info!(version = %info.version, "update available");
    let _ = app.emit(UPDATE_AVAILABLE, &info);
    *found = Some(info);
}

/// How long to wait after the `failures`-th failed check in a row (from 0).
fn retry_after(failures: u32) -> Duration {
    RETRY_FIRST
        .saturating_mul(1 << failures.min(16))
        .min(CHECK_EVERY)
}

async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if !configured(app.config()) {
        return Err("updates aren't set up in this build".into());
    }
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        prompt: false,
    }))
}

#[tauri::command]
pub fn update_support(app: AppHandle) -> UpdateSupport {
    UpdateSupport {
        configured: configured(app.config()),
        version: app.package_info().version.to_string(),
    }
}

/// The update the timed checks found, if any.
#[tauri::command]
pub fn found_update(found: State<Found>) -> Option<UpdateInfo> {
    let mut found = found.0.lock().unwrap();
    let info = found.clone();
    // Asked about once per launch: the window is rebuilt after a while
    // in the tray, and its page asks again each time.
    if let Some(f) = found.as_mut() {
        f.prompt = false;
    }
    info
}

/// Look for a newer release now.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    check(&app).await
}

/// Download the newest release and restart into it. While a game is
/// being recorded the install waits for it to close instead (installing
/// ends this process, and the replay buffer with it): the answer is then
/// that game's name. Otherwise only a failure comes back.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<Option<String>, String> {
    if !configured(app.config()) {
        return Err("updates aren't set up in this build".into());
    }
    if app.state::<AppState>().rendering.load(Ordering::Relaxed) {
        return Err("An edit is rendering. Update when it's done.".into());
    }
    let pending = app.state::<Pending>();
    if pending.downloading.swap(true, Ordering::SeqCst) {
        return Err("The update is already downloading.".into());
    }
    let downloaded = download(&app, &pending).await;
    pending.downloading.store(false, Ordering::SeqCst);
    let (update, bytes) = downloaded?;

    let status = app.state::<AppState>().service.status().await.ok();
    if let Some(status) = status
        && matches!(
            status.state,
            service::State::Recording | service::State::Retrying
        )
    {
        let game = status.game.unwrap_or_else(|| "the game".into());
        tracing::info!(version = %update.version, %game, "update downloaded; waiting for the recording to stop");
        *pending.update.lock().unwrap() = Some((update, bytes));
        return Ok(Some(game));
    }
    install(&app, &update, bytes);
    Ok(None)
}

/// Stop the download in progress, or forget an update that is waiting
/// for a game to close.
#[tauri::command]
pub fn cancel_update(pending: State<Pending>) {
    pending.cancel.notify_waiters();
    pending.update.lock().unwrap().take();
}

async fn download(app: &AppHandle, pending: &Pending) -> Result<(Update, Vec<u8>), String> {
    let update = app
        .updater_builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("already up to date")?;
    tracing::info!(version = %update.version, "downloading update");
    let progress = app.clone();
    let mut done = 0u64;
    let bytes = tokio::select! {
        bytes = update.download(
            move |chunk, total| {
                done += chunk as u64;
                let fraction = total.map(|t| (done as f64 / t.max(1) as f64).min(1.0));
                let _ = progress.emit(UPDATE_PROGRESS, fraction);
            },
            || {},
        ) => bytes.map_err(|e| e.to_string())?,
        () = pending.cancel.notified() => return Err("Download cancelled".into()),
    };
    Ok((update, bytes))
}

/// Install `bytes` and restart into the new version. On Windows this
/// starts the installer and ends the process; it returns only on failure.
fn install(app: &AppHandle, update: &Update, bytes: Vec<u8>) {
    tracing::info!(version = %update.version, "installing update");
    // The relaunch gets this launch's arguments, `--autostart` included,
    // and would come back hidden in the tray: leave word that it follows
    // an update, so it opens its window and says so.
    let marker = marker();
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&marker, &update.version);
    match update.install(bytes) {
        Ok(()) => app.restart(),
        Err(e) => {
            let _ = std::fs::remove_file(&marker);
            tracing::warn!("installing the update failed: {e}");
            let _ = app.emit(UPDATE_FAILED, e.to_string());
        }
    }
}

/// Installing failed after the download (the command had already
/// answered if it was waiting for a game); the payload is the reason.
pub const UPDATE_FAILED: &str = "update-failed";

fn marker() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reruns/updated-to")
}

/// Call once at start-up: whether this launch follows an update to the
/// version now running. A marker for any other version is from an
/// install that didn't happen, and is dropped.
pub fn take_marker(app: &AppHandle) -> bool {
    let marker = marker();
    let version = std::fs::read_to_string(&marker).ok();
    let _ = std::fs::remove_file(&marker);
    let current = app.package_info().version.to_string();
    let updated = version
        .map(|v| v.trim().to_owned())
        .filter(|v| *v == current);
    let yes = updated.is_some();
    app.manage(Updated(Mutex::new(updated)));
    yes
}

/// The version this launch was just updated to; answers once.
#[tauri::command]
pub fn updated_to(updated: State<Updated>) -> Option<String> {
    updated.0.lock().unwrap().take()
}

#[cfg(test)]
mod tests {
    use super::{CHECK_EVERY, configured, retry_after};
    use std::time::Duration;

    /// A failed check (no network yet at login) is tried again within a
    /// minute, then less often, never later than the usual check.
    #[test]
    fn a_failed_check_is_retried_soon() {
        let minutes = |f| retry_after(f).as_secs() / 60;
        assert_eq!([0, 1, 2, 3].map(minutes), [1, 2, 4, 8]);
        assert_eq!(retry_after(9), CHECK_EVERY);
        assert_eq!(retry_after(u32::MAX), CHECK_EVERY);
        assert!(retry_after(0) <= Duration::from_secs(60));
    }

    fn config(plugins: serde_json::Value) -> tauri::Config {
        let mut c: tauri::Config = serde_json::from_value(serde_json::json!({
            "identifier": "gg.reruns.test",
        }))
        .unwrap();
        c.plugins = serde_json::from_value(plugins).unwrap();
        c
    }

    #[test]
    fn needs_a_public_key() {
        assert!(!configured(&config(serde_json::json!({}))));
        assert!(!configured(&config(
            serde_json::json!({ "updater": { "pubkey": "  ", "endpoints": [] } })
        )));
        assert!(configured(&config(
            serde_json::json!({ "updater": { "pubkey": "dW50cnVzdGVk", "endpoints": [] } })
        )));
    }

    /// The shape scripts/release-config.mjs writes is what the plugin reads.
    #[test]
    fn release_config_is_a_valid_updater_config() {
        let c: tauri_plugin_updater::Config = serde_json::from_value(serde_json::json!({
            "pubkey": "dW50cnVzdGVk",
            "endpoints": ["https://github.com/lpears0n/reruns/releases/latest/download/latest.json"],
            "windows": { "installMode": "passive" },
        }))
        .unwrap();
        assert_eq!(c.endpoints.len(), 1);
    }
}
