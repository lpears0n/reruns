// Reruns Setup: one small window in the app's own look. It installs Reruns
// by running the NSIS installer it carries silently, so the install folder,
// registry entries, shortcuts, uninstaller and updater all stay exactly as
// that installer leaves them.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(test)]
mod nsi;

use std::cmp::Ordering;

/// What the button offers, given the version already installed (if any).
#[cfg_attr(not(windows), allow(dead_code))]
fn action(installed: Option<&str>, version: &str) -> &'static str {
    let Some(installed) = installed else {
        return "install";
    };
    match (
        semver::Version::parse(installed),
        semver::Version::parse(version),
    ) {
        (Ok(have), Ok(new)) => match have.cmp(&new) {
            Ordering::Less => "update",
            Ordering::Equal => "reinstall",
            Ordering::Greater => "install",
        },
        _ => "update",
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Reruns Setup only runs on Windows.");
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::time::Duration;
    use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Threading::{GetProcessIoCounters, IO_COUNTERS};
    use windows::core::HSTRING;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    /// The NSIS installer built just before Setup (empty in a build without one).
    const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));
    const VERSION: &str = env!("RERUNS_VERSION");
    const PRODUCT: &str = env!("RERUNS_PRODUCT");
    /// What a full install writes, from the installer's file list; 0 if unknown.
    const EXPECTED_BYTES: &str = env!("RERUNS_EXPECTED_BYTES");

    #[derive(serde::Serialize)]
    struct Info {
        version: &'static str,
        installed: Option<String>,
        action: &'static str,
    }

    fn uninstall_key() -> Option<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(format!(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{PRODUCT}"
            ))
            .ok()
    }

    #[tauri::command]
    fn info() -> Info {
        let installed: Option<String> =
            uninstall_key().and_then(|k| k.get_value("DisplayVersion").ok());
        let action = super::action(installed.as_deref(), VERSION);
        Info {
            version: VERSION,
            installed,
            action,
        }
    }

    #[tauri::command]
    async fn install(window: tauri::WebviewWindow) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || {
            run_installer(|p| {
                let _ = window.emit("progress", p);
            })
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Where NSIS installs: over the existing copy, else its per-user default.
    fn install_dir() -> PathBuf {
        uninstall_key()
            .and_then(|k| k.get_value::<String, _>("InstallLocation").ok())
            .map(|s| PathBuf::from(s.trim_matches('"')))
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(PRODUCT)))
            .unwrap_or_default()
    }

    fn free_bytes(dir: &Path) -> Option<u64> {
        let root: PathBuf = dir.components().take(2).collect();
        let mut free = 0u64;
        unsafe {
            GetDiskFreeSpaceExW(
                &HSTRING::from(root.as_os_str()),
                Some(&mut free),
                None,
                None,
            )
        }
        .ok()?;
        Some(free)
    }

    /// Bytes the installer has written so far, files and all.
    fn written(child: &Child) -> u64 {
        let mut io = IO_COUNTERS::default();
        match unsafe { GetProcessIoCounters(HANDLE(child.as_raw_handle()), &mut io) } {
            Ok(()) => io.WriteTransferCount,
            Err(_) => 0,
        }
    }

    fn unpack() -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!("reruns-setup-{}", std::process::id()));
        let exe = dir.join(format!("{PRODUCT} installer.exe"));
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&exe, PAYLOAD))
            .map_err(|e| format!("Couldn't unpack the installer: {e}"))?;
        Ok(exe)
    }

    fn run_installer(progress: impl Fn(f64)) -> Result<(), String> {
        if PAYLOAD.is_empty() {
            return Err("This Setup was built without an installer inside.".into());
        }
        let expected: u64 = EXPECTED_BYTES.parse().unwrap_or(0);
        let dir = install_dir();
        if let Some(free) = free_bytes(&dir)
            && free < expected
        {
            let drive = dir
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            return Err(format!(
                "Not enough space on drive {}",
                drive.unwrap_or_default()
            ));
        }
        let exe = unpack()?;
        let mut installer = Command::new(&exe);
        installer.arg("/S");
        // shortcut: lets a test install on a PC already running Reruns skip
        // starting the copy it installed; drop it once tests run in a VM.
        if std::env::var_os("RERUNS_SETUP_NO_LAUNCH").is_none() {
            installer.arg("/R"); // NSIS starts Reruns once it has installed
        }
        let result = installer.spawn().and_then(|mut child| {
            loop {
                if let Some(status) = child.try_wait()? {
                    return Ok(status);
                }
                if expected > 0 {
                    progress((written(&child) as f64 / expected as f64).min(0.99));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let _ = std::fs::remove_dir_all(exe.parent().unwrap());
        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!(
                "Setup couldn't finish (error {}).",
                status.code().unwrap_or(-1)
            )),
            Err(e) => Err(format!("Couldn't start the installer: {e}")),
        }
    }

    pub fn run() {
        // Without WebView2 this window can't be drawn: hand over to the
        // installer's own pages, which fetch WebView2 first.
        if tauri::webview_version().is_err() {
            if let Ok(exe) = unpack() {
                let _ = Command::new(&exe).status();
                let _ = std::fs::remove_dir_all(exe.parent().unwrap());
            }
            return;
        }
        tauri::Builder::default()
            .invoke_handler(tauri::generate_handler![info, install])
            .setup(|app| {
                // Undecorated with its shadow: Windows 11 gives it 8px corners,
                // and the dark theme makes its border dark.
                WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                    .title("Reruns Setup")
                    .inner_size(520.0, 300.0)
                    .resizable(false)
                    .maximizable(false)
                    .decorations(false)
                    .theme(Some(tauri::Theme::Dark))
                    .background_color(tauri::window::Color(0x1c, 0x1d, 0x1d, 0xff))
                    .center()
                    // Keeps WebView2's profile out of %LOCALAPPDATA%.
                    .data_directory(std::env::temp_dir().join("reruns-setup-webview"))
                    .build()?;
                Ok(())
            })
            .run(tauri::generate_context!())
            .expect("Reruns Setup failed to start");
    }
}

#[cfg(test)]
mod tests {
    use super::action;

    #[test]
    fn offers_what_fits_the_installed_version() {
        assert_eq!(action(None, "2.0.0"), "install");
        assert_eq!(action(Some("1.2.0"), "2.0.0"), "update");
        assert_eq!(action(Some("2.0.0-beta.1"), "2.0.0"), "update");
        assert_eq!(action(Some("2.0.0"), "2.0.0"), "reinstall");
        assert_eq!(action(Some("2.1.0"), "2.0.0"), "install");
        assert_eq!(action(Some("odd"), "2.0.0"), "update");
    }
}
