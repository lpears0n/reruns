// Puts the NSIS installer named by RERUNS_SETUP_PAYLOAD (an absolute path)
// inside Setup, with the product and version it installs and the bytes a
// full install writes. Without it Setup still builds, and says it has nothing
// to install.
#[path = "src/nsi.rs"]
mod nsi;

use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=RERUNS_SETUP_PAYLOAD");
    let mut bytes = 0u64;
    match env::var_os("RERUNS_SETUP_PAYLOAD").map(PathBuf::from) {
        Some(payload) => {
            println!("cargo:rerun-if-changed={}", payload.display());
            fs::copy(&payload, out.join("payload.exe"))
                .unwrap_or_else(|e| panic!("RERUNS_SETUP_PAYLOAD {}: {e}", payload.display()));
            // The script of the last NSIS bundle in this target folder (OUT_DIR
            // is <target>/<profile>/build/<crate>/out), which is the payload's.
            let script = out.join("../../../nsis/x64/installer.nsi");
            if let Ok(text) = fs::read_to_string(&script) {
                println!("cargo:rerun-if-changed={}", script.display());
                bytes = nsi::installed_files(&text)
                    .iter()
                    .filter_map(|f| fs::metadata(f).ok())
                    .map(|m| m.len())
                    .sum();
            }
        }
        None => fs::write(out.join("payload.exe"), b"").unwrap(),
    }
    println!("cargo:rustc-env=RERUNS_EXPECTED_BYTES={bytes}");

    let app = "../src-tauri/tauri.conf.json";
    println!("cargo:rerun-if-changed={app}");
    let app: serde_json::Value = serde_json::from_str(&fs::read_to_string(app).unwrap()).unwrap();
    for (key, var) in [
        ("version", "RERUNS_VERSION"),
        ("productName", "RERUNS_PRODUCT"),
    ] {
        println!("cargo:rustc-env={var}={}", app[key].as_str().unwrap());
    }
    // tauri is a Windows-only dependency (Cargo.toml), and tauri_build needs
    // what tauri's own build script reports.
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        tauri_build::build();
    }
}
