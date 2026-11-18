//! Reads which files a Tauri NSIS script installs, so Setup knows how many
//! bytes a full install writes: its progress is those bytes written so far.

/// Source paths of the files `nsi` installs, with `${MAINBINARYSRCPATH}`
/// filled in. Files it only drops in $TEMP (the WebView2 bootstrapper) are
/// left out.
pub fn installed_files(nsi: &str) -> Vec<String> {
    let main = nsi
        .lines()
        .find_map(|l| l.trim().strip_prefix("!define MAINBINARYSRCPATH "))
        .map(|v| v.trim().trim_matches('"').to_string());
    nsi.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("File ") && !l.contains("$TEMP"))
        // The source is the line's last quoted string.
        .filter_map(|l| l.rsplit('"').nth(1))
        .map(|s| match &main {
            Some(m) => s.replace("${MAINBINARYSRCPATH}", m),
            None => s.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_what_lands_in_the_app_folder() {
        let nsi = r#"!define MAINBINARYSRCPATH "C:\t\reruns.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebview2Setup.exe" "C:\wv2.exe"
  File "${MAINBINARYSRCPATH}"
    File /a "/oname=gst-plugins\gstapp.dll" "C:\g\gstapp.dll"
  ; File "C:\commented.dll"
"#;
        assert_eq!(
            super::installed_files(nsi),
            [r"C:\t\reruns.exe", r"C:\g\gstapp.dll"]
        );
    }
}
