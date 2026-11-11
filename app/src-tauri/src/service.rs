//! Background task that owns the recorder. The UI talks to it through
//! [`Service`]; it reports changes as Tauri events.

use std::path::PathBuf;

use clip_core::{
    RecorderConfig,
    auto::{AutoRecorder, Backend, Change},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};

use crate::{games, library};

/// Event names emitted to the frontend.
pub const STATUS_CHANGED: &str = "status-changed";
pub const CLIP_SAVED: &str = "clip-saved";
pub const SCREENSHOT_SAVED: &str = "screenshot-saved";
/// The clip hotkey was pressed while a game is recorded: what the clip
/// will hold is fixed now, and writing the file follows. The sound plays
/// on this, at the key press, not a second later when the file is done.
pub const CLIP_SAVING: &str = "clip-saving";
/// The same for a screenshot.
pub const SCREENSHOT_SAVING: &str = "screenshot-saving";
/// A save announced by one of the two events above didn't work; the
/// payload is the reason.
pub const SAVE_FAILED: &str = "save-failed";
/// A game started recording; the payload is its name.
pub const RECORDING_STARTED: &str = "recording-started";
/// A moment was marked; the payload is a [`Marked`].
pub const MARKED: &str = "moment-marked";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Marked {
    pub game: String,
    /// How far into the replay buffer the mark is, in seconds.
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Automatic capture isn't possible (e.g. not KDE); see `error`.
    Unavailable,
    /// Watching for a game window.
    Waiting,
    Recording,
    /// A game's capture failed and is waiting to be restarted; `game` and
    /// `error` say which and why.
    Retrying,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub state: State,
    pub game: Option<String>,
    pub steam_app_id: Option<u32>,
    /// When the current play session began (ms since epoch).
    pub session_start_ms: Option<u64>,
    pub buffered_seconds: f64,
    pub replay_seconds: u32,
    pub encoder: Option<String>,
    /// While recording: the usual encoder refused, so another one is used
    /// (tried again at the next game).
    pub encoder_fallback: Option<clip_core::EncoderFallback>,
    /// "zerocopy" (DMA-BUF into GPU memory) or "copy", while recording.
    pub capture_path: Option<clip_core::CapturePath>,
    pub error: Option<String>,
    /// How games are captured: "kwin" (window, KDE) or "portal" (screen).
    pub backend: Option<Backend>,
    /// Portal capture needs a screen chosen once before it can record.
    pub needs_screen: bool,
    /// While recording: the game's display, if it is running in HDR.
    pub hdr: Option<clip_core::HdrDisplay>,
    /// While recording: HDR is tone-mapped into the clips.
    pub hdr_tonemapped: bool,
    /// The recorded game's executable (lower-case file stem), when known.
    pub exe: Option<String>,
}

enum Cmd {
    Status(oneshot::Sender<Status>),
    Save(PathBuf, oneshot::Sender<Result<PathBuf, String>>),
    Screenshot(PathBuf, oneshot::Sender<Result<PathBuf, String>>),
    Mark(oneshot::Sender<Result<f64, String>>),
    Configure(RecorderConfig),
    ChooseScreen(oneshot::Sender<Result<(), String>>),
    /// What to add to the ignore list for the game being recorded.
    IgnoreEntry(oneshot::Sender<Option<clip_core::game::IgnoredGame>>),
}

#[derive(Clone)]
pub struct Service {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl Service {
    /// `prefer`: capture backend to use (`None` = KWin when available, else
    /// the portal). `token_path`: where the portal's restore token is kept.
    pub fn spawn(
        app: AppHandle,
        cfg: RecorderConfig,
        prefer: Option<Backend>,
        token_path: PathBuf,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(run(app, cfg, prefer, token_path, rx));
        Self { tx }
    }

    /// Show the desktop's screen picker once (portal capture).
    pub async fn choose_screen(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::ChooseScreen(tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())?
    }

    pub async fn status(&self) -> Result<Status, String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Status(tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())
    }

    /// Save the replay buffer into `clips_dir/<game>/`.
    pub async fn save(&self, clips_dir: PathBuf) -> Result<PathBuf, String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Save(clips_dir, tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())?
    }

    /// Save the newest frame as a PNG into `clips_dir/<game>/`.
    pub async fn screenshot(&self, clips_dir: PathBuf) -> Result<PathBuf, String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Screenshot(clips_dir, tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())?
    }

    /// Mark this moment in the recording; saved clips show it. Returns how
    /// far into the replay buffer it is, in seconds.
    pub async fn mark(&self) -> Result<f64, String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Mark(tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())?
    }

    /// The ignore-list entry for the game being recorded, if any.
    pub async fn ignore_entry(&self) -> Result<Option<clip_core::game::IgnoredGame>, String> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::IgnoreEntry(tx))?;
        rx.await.map_err(|_| "recorder stopped".to_string())
    }

    pub fn configure(&self, cfg: RecorderConfig) -> Result<(), String> {
        self.send(Cmd::Configure(cfg))
    }

    fn send(&self, cmd: Cmd) -> Result<(), String> {
        self.tx
            .send(cmd)
            .map_err(|_| "recorder stopped".to_string())
    }
}

async fn run(
    app: AppHandle,
    cfg: RecorderConfig,
    prefer: Option<Backend>,
    token_path: PathBuf,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
) {
    let replay_seconds = cfg.replay_seconds;
    let (mut auto, mut events) = match AutoRecorder::new(cfg, prefer, token_path).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("automatic capture unavailable: {e:#}");
            let status = Status {
                state: State::Unavailable,
                game: None,
                steam_app_id: None,
                session_start_ms: None,
                buffered_seconds: 0.0,
                replay_seconds,
                encoder: None,
                encoder_fallback: None,
                capture_path: None,
                error: Some(format!("{e:#}")),
                backend: None,
                needs_screen: false,
                hdr: None,
                hdr_tonemapped: false,
                exe: None,
            };
            while let Some(cmd) = cmds.recv().await {
                match cmd {
                    Cmd::Status(r) => drop(r.send(status.clone())),
                    Cmd::Save(_, r) | Cmd::Screenshot(_, r) => {
                        drop(r.send(Err("automatic capture is unavailable".into())))
                    }
                    Cmd::Mark(r) => drop(r.send(Err("automatic capture is unavailable".into()))),
                    Cmd::Configure(_) => {}
                    Cmd::ChooseScreen(r) => {
                        drop(r.send(Err("automatic capture is unavailable".into())))
                    }
                    Cmd::IgnoreEntry(r) => drop(r.send(None)),
                }
            }
            return;
        }
    };
    let mut replay_seconds = replay_seconds;
    // The game being played and when its session started.
    let mut session: Option<(clip_core::game::Game, u64)> = None;
    // What the page was last told, so a change it would show gets sent.
    let mut shown: Option<Shown> = None;

    loop {
        tokio::select! {
            ev = events.recv() => {
                let Some(ev) = ev else { break };
                let changed = match auto.handle(ev).await {
                    Ok(Some(change)) => {
                        if let Change::Started(g) = &change {
                            let _ = app.emit(RECORDING_STARTED, &g.name);
                        }
                        apply(change, &mut session);
                        true
                    }
                    Ok(None) => false,
                    Err(e) => {
                        tracing::error!("{e:#}");
                        false
                    }
                };
                // Also when nothing started or stopped: a capture that failed
                // and is being retried (or came back) changes what the page
                // shows, and it hears only through this event.
                let now = status(&auto, replay_seconds, session.as_ref());
                let key = Shown::of(&now);
                if changed || shown.as_ref() != Some(&key) {
                    shown = Some(key);
                    let _ = app.emit(STATUS_CHANGED, now);
                }
            }
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Cmd::Status(r) => drop(r.send(status(&auto, replay_seconds, session.as_ref()))),
                    Cmd::Save(dir, r) => {
                        let announced = auto.current_game().is_some();
                        if announced {
                            let _ = app.emit(CLIP_SAVING, ());
                        }
                        let res = auto.save(&dir, None).await.map_err(|e| format!("{e:#}"));
                        if let (true, Err(e)) = (announced, &res) {
                            let _ = app.emit(SAVE_FAILED, e);
                        }
                        if let Ok(saved) = &res {
                            if let Some(g) = auto.current_game() {
                                let meta = library::ClipMeta {
                                    game: g.name.clone(),
                                    steam_app_id: g.steam_app_id,
                                    markers: saved.markers.clone(),
                                    mixed_audio: saved.mixed_audio,
                                    ..Default::default()
                                };
                                if let Err(e) = library::write_meta(&saved.path, &meta) {
                                    tracing::warn!("clip metadata: {e:#}");
                                }
                            }
                            let _ = app.emit(CLIP_SAVED, &saved.path);
                        }
                        drop(r.send(res.map(|s| s.path)));
                    }
                    Cmd::Screenshot(dir, r) => {
                        let announced = auto.current_game().is_some();
                        if announced {
                            let _ = app.emit(SCREENSHOT_SAVING, ());
                        }
                        let res = auto.screenshot(&dir).await.map_err(|e| format!("{e:#}"));
                        if let (true, Err(e)) = (announced, &res) {
                            let _ = app.emit(SAVE_FAILED, e);
                        }
                        if let Ok(p) = &res {
                            let _ = app.emit(SCREENSHOT_SAVED, p);
                        }
                        drop(r.send(res));
                    }
                    Cmd::ChooseScreen(r) => {
                        let res = match auto.choose_screen().await {
                            Ok(change) => {
                                if let Some(c) = change {
                                    apply(c, &mut session);
                                }
                                Ok(())
                            }
                            Err(e) => Err(format!("{e:#}")),
                        };
                        drop(r.send(res));
                        let _ = app.emit(STATUS_CHANGED, status(&auto, replay_seconds, session.as_ref()));
                    }
                    Cmd::Mark(r) => {
                        let res = auto.mark().map_err(|e| format!("{e:#}"));
                        if let Ok(at) = res {
                            let game = auto.current_game().map(|g| g.name.clone()).unwrap_or_default();
                            let _ = app.emit(MARKED, Marked { game, at });
                        }
                        drop(r.send(res));
                    }
                    Cmd::IgnoreEntry(r) => drop(r.send(auto.ignore_entry())),
                    Cmd::Configure(cfg) => {
                        replay_seconds = cfg.replay_seconds;
                        auto.set_config(cfg);
                        // The status bar shows the replay length.
                        let _ = app.emit(STATUS_CHANGED, status(&auto, replay_seconds, session.as_ref()));
                    }
                }
            }
        }
    }
    end_session(session.take());
    let _ = auto.shutdown().await;
}

/// The parts of [`Status`] the page shows that change without a game
/// starting or stopping (the buffered length changes all the time and is
/// left out).
#[derive(PartialEq)]
struct Shown {
    state: State,
    game: Option<String>,
    encoder: Option<String>,
    encoder_fallback: Option<clip_core::EncoderFallback>,
    capture_path: Option<clip_core::CapturePath>,
    error: Option<String>,
    hdr_tonemapped: bool,
}

impl Shown {
    fn of(s: &Status) -> Self {
        Self {
            state: s.state.clone(),
            game: s.game.clone(),
            encoder: s.encoder.clone(),
            encoder_fallback: s.encoder_fallback.clone(),
            capture_path: s.capture_path,
            error: s.error.clone(),
            hdr_tonemapped: s.hdr_tonemapped,
        }
    }
}

/// Track play sessions as recording starts and stops.
fn apply(change: Change, session: &mut Option<(clip_core::game::Game, u64)>) {
    match change {
        Change::Started(g) => {
            tracing::info!("recording {}", g.name);
            end_session(session.take());
            *session = Some((g, games::now_ms()));
        }
        Change::Stopped(g) => {
            tracing::info!("stopped recording {}", g.name);
            end_session(session.take());
        }
    }
}

/// Log a finished play session.
fn end_session(session: Option<(clip_core::game::Game, u64)>) {
    let Some((g, start_ms)) = session else { return };
    let s = games::Session {
        game: g.name,
        steam_app_id: g.steam_app_id,
        start_ms,
        end_ms: games::now_ms(),
    };
    if let Err(e) = games::record(&s) {
        tracing::warn!("session log: {e:#}");
    }
}

fn status(
    auto: &AutoRecorder,
    replay_seconds: u32,
    session: Option<&(clip_core::game::Game, u64)>,
) -> Status {
    let rec = auto.status();
    let retrying = if rec.is_none() { auto.retrying() } else { None };
    Status {
        state: if rec.is_some() {
            State::Recording
        } else if retrying.is_some() {
            State::Retrying
        } else {
            State::Waiting
        },
        game: auto
            .current_game()
            .map(|g| g.name.clone())
            .or_else(|| retrying.as_ref().map(|r| r.game.name.clone())),
        steam_app_id: auto.current_game().and_then(|g| g.steam_app_id),
        session_start_ms: session.map(|(_, t)| *t),
        buffered_seconds: rec.as_ref().map_or(0.0, |s| s.buffered_seconds),
        replay_seconds: rec.as_ref().map_or(replay_seconds, |s| s.replay_seconds),
        encoder: auto
            .encoder()
            .and_then(|e| e.element(auto.codec()))
            .map(str::to_owned),
        encoder_fallback: auto.encoder_fallback().cloned(),
        capture_path: rec.as_ref().map(|s| s.path),
        error: rec
            .and_then(|s| s.error)
            .or_else(|| retrying.map(|r| r.reason)),
        backend: Some(auto.backend()),
        needs_screen: auto.needs_screen(),
        hdr: auto.hdr(),
        hdr_tonemapped: auto.hdr_tonemapped(),
        exe: auto.ignore_entry().and_then(|g| g.exe),
    }
}
