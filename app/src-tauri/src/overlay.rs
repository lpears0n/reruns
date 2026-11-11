//! The in-game popup: "Recording <game>" when a game starts recording,
//! "Clip saved" / "Screenshot saved" on a save, "Moment marked" on a
//! marker. Windows only.
//!
//! It is a small transparent window kept on top of the game, not something
//! drawn into the game: hooking a game's rendering (the way Discord's or
//! Steam's overlays do) is what anti-cheat flags. The window never takes
//! focus (a focus change would knock a fullscreen game out of fullscreen),
//! lets clicks through, and is only shown for a few seconds, since any
//! window over a fullscreen game makes Windows compose the game while it is
//! visible. Recording captures only the game's window, so the popup never
//! appears in clips.
//!
//! Linux has no portable way to keep a window above a game on Wayland; the
//! desktop notification serves there.

use serde::Serialize;
use tauri::AppHandle;

/// Event the overlay page listens for.
#[cfg_attr(not(windows), allow(dead_code))]
pub const SHOW_EVENT: &str = "overlay-show";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Ready,
    Saved,
    Screenshot,
    Marked,
}

#[derive(Debug, Clone, Serialize)]
pub struct Popup {
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    /// Hotkey shown as a key cap ("Press F8 to clip").
    pub key: Option<String>,
    /// How long the window stays up, in milliseconds.
    pub ms: u64,
}

impl Popup {
    pub fn ready(game: &str, hotkey: &str) -> Self {
        Self {
            kind: Kind::Ready,
            title: format!("Recording {game}"),
            detail: String::new(),
            key: (!hotkey.is_empty()).then(|| hotkey.into()),
            ms: 3500,
        }
    }

    pub fn saved(kind: Kind, detail: &str) -> Self {
        Self {
            kind,
            title: match kind {
                Kind::Screenshot => "Screenshot saved",
                Kind::Marked => "Moment marked",
                _ => "Clip saved",
            }
            .into(),
            detail: detail.into(),
            key: None,
            ms: 2500,
        }
    }

    /// Short: a marker is pressed mid-fight.
    pub fn marked(game: &str) -> Self {
        Self {
            ms: 1500,
            ..Self::saved(Kind::Marked, game)
        }
    }
}

/// Size of the overlay window in design units: the card plus room for its
/// shadow and entrance. A unit is one pixel on a 1080-line screen; the page
/// zooms to fill whatever size the window is given (`DESIGN_WIDTH` in
/// overlay.ts matches `WIDTH`).
#[cfg_attr(not(windows), allow(dead_code))]
const WIDTH: f64 = 480.0;
#[cfg_attr(not(windows), allow(dead_code))]
const HEIGHT: f64 = 130.0;

/// Pixels per design unit over a game whose client area is `width` ×
/// `height` pixels: the popup takes the same share of the screen at any
/// resolution or display scaling. Measured by height (16:9 at most), so an
/// ultrawide screen doesn't blow it up; kept readable in a small window and
/// in proportion on a very large screen.
#[cfg_attr(not(windows), allow(dead_code))]
fn units(width: i32, height: i32) -> f64 {
    let lines = f64::from(height).min(f64::from(width) * 9.0 / 16.0);
    (lines / 1080.0).clamp(0.7, 3.0)
}

#[cfg(windows)]
pub use imp::set_game;
pub use imp::{init, show};

#[cfg(windows)]
mod imp {
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
        },
        time::Duration,
    };

    use tauri::{
        AppHandle, Emitter, Listener, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
        WebviewWindowBuilder,
    };
    use windows::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{
            GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, HWND_TOPMOST, SW_HIDE,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            SetWindowPos, ShowWindow,
        },
    };

    use super::{HEIGHT, Popup, SHOW_EVENT, WIDTH, units};

    const LABEL: &str = "overlay";

    /// Bumped by every show, so only the latest one's timer hides the window.
    static GENERATION: AtomicU64 = AtomicU64::new(0);

    /// The page says this once it listens for popups (see overlay.ts).
    const READY_EVENT: &str = "overlay-ready";

    /// The window is being made, or is there.
    static CREATED: AtomicBool = AtomicBool::new(false);
    /// The page is loaded and listening.
    static READY: AtomicBool = AtomicBool::new(false);
    /// The popup to show once the page is ready.
    static PENDING: Mutex<Option<Popup>> = Mutex::new(None);

    /// The window being recorded (0: none).
    static GAME: AtomicIsize = AtomicIsize::new(0);

    /// Tell the overlay which window is being recorded, so popups only
    /// show over that game.
    pub fn set_game(hwnd: Option<isize>) {
        GAME.store(hwnd.unwrap_or(0), Ordering::SeqCst);
    }

    /// The process a window belongs to (0 for a stale handle).
    fn pid(hwnd: HWND) -> u32 {
        let mut pid = 0;
        // SAFETY: writes the owning process id into `pid`.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    }

    /// Get ready to show popups. The window itself, a webview of its own,
    /// is only made for the first popup: a launch that never records a
    /// game, or with the popup turned off, never pays for it.
    pub fn init(app: &AppHandle) {
        let handle = app.clone();
        app.listen_any(READY_EVENT, move |_| {
            READY.store(true, Ordering::SeqCst);
            if let Some(popup) = PENDING.lock().unwrap().take() {
                present(&handle, popup);
            }
        });
    }

    /// Create the (hidden) overlay window.
    fn create(app: &AppHandle) -> tauri::Result<()> {
        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
            .title("Reruns overlay")
            .inner_size(WIDTH, HEIGHT)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .build()?;
        window.set_ignore_cursor_events(true)?;
        Ok(())
    }

    /// Show `popup` over the recorded game when it is in the foreground.
    /// Nothing happens when Reruns itself is in front: the app shows its
    /// own toast then. The
    /// first popup waits for the window to be made and its page to load.
    pub fn show(app: &AppHandle, popup: Popup) {
        if READY.load(Ordering::SeqCst) {
            return present(app, popup);
        }
        // The newest popup wins if several arrive before the page is up.
        *PENDING.lock().unwrap() = Some(popup);
        if CREATED.swap(true, Ordering::SeqCst) {
            return;
        }
        // Off this thread: it may be the event loop's, which the window
        // is made on.
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = create(&app) {
                tracing::warn!("in-game overlay unavailable: {e}");
            }
        });
    }

    fn present(app: &AppHandle, popup: Popup) {
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let Ok(overlay) = window.hwnd() else { return };
        // SAFETY: plain query.
        let fg = unsafe { GetForegroundWindow() };
        let ours = app
            .webview_windows()
            .values()
            .filter_map(|w| w.hwnd().ok())
            .any(|h| h.0 == fg.0);
        if fg.0.is_null() || ours {
            return;
        }
        // Only over the recorded game, not a launcher or another app in
        // front of it. By process: a game may put another of its windows
        // in front. shortcut: a popup that arrives while the game is behind
        // is dropped, not shown later; queue it if "Recording" goes missing.
        let game = GAME.load(Ordering::SeqCst);
        if game != 0 && pid(fg) != pid(HWND(game as *mut _)) {
            return;
        }

        // Sized for the game's resolution, flush with its client area's
        // top-right corner; the page's padding keeps the card off the edge.
        let Some(client) = client_rect(fg) else {
            return;
        };
        let s = units(client.right - client.left, client.bottom - client.top);
        let (width, height) = ((WIDTH * s).round() as u32, (HEIGHT * s).round() as u32);
        // Moved first: landing on a monitor with other display scaling
        // makes Windows rescale the window, and the size must win.
        let _ = window.set_position(PhysicalPosition::new(
            client.right - width as i32,
            client.top,
        ));
        let _ = window.set_size(PhysicalSize::new(width, height));

        let ms = popup.ms;
        let _ = app.emit_to(LABEL, SHOW_EVENT, popup);
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        // Show without activating; Tauri's own show() may take focus.
        let hwnd = overlay.0 as isize;
        let _ = app.run_on_main_thread(move || {
            let hwnd = HWND(hwnd as *mut _);
            // SAFETY: our own live window; no activation, no size or move.
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        });

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            if GENERATION.load(Ordering::SeqCst) != generation {
                return; // a newer popup owns the window now
            }
            let _ = app.run_on_main_thread(move || {
                // SAFETY: our own window.
                unsafe {
                    let _ = ShowWindow(HWND(hwnd as *mut _), SW_HIDE);
                }
            });
        });
    }

    /// The window's client area, in screen coordinates.
    fn client_rect(hwnd: HWND) -> Option<RECT> {
        let mut r = RECT::default();
        // SAFETY: plain queries on a window handle; a stale one just fails.
        unsafe {
            GetClientRect(hwnd, &mut r).ok()?;
            let mut p = POINT { x: 0, y: 0 };
            ClientToScreen(hwnd, &mut p).ok().ok()?;
            Some(RECT {
                left: p.x,
                top: p.y,
                right: p.x + r.right,
                bottom: p.y + r.bottom,
            })
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::AppHandle;

    use super::Popup;

    pub fn init(_app: &AppHandle) {}

    pub fn show(_app: &AppHandle, _popup: Popup) {}
}

/// Whether the overlay is turned on.
pub fn enabled(app: &AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .is_some_and(|s| s.settings.lock().unwrap().overlay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_popup_keeps_its_share_of_the_screen() {
        assert_eq!(units(1920, 1080), 1.0);
        assert_eq!(units(2560, 1440), 1440.0 / 1080.0);
        assert_eq!(units(3840, 2160), 2.0);
        // Ultrawide: by height, not width.
        assert_eq!(units(3440, 1440), units(2560, 1440));
        // A tall, narrow window: as if 16:9 at its width.
        assert_eq!(units(1600, 2000), 900.0 / 1080.0);
        // Small windows stay readable; huge screens stay in proportion.
        assert_eq!(units(800, 450), 0.7);
        assert_eq!(units(15360, 8640), 3.0);
    }
}
