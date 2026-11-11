//! Loopback HTTP server for `<video>` playback.
//!
//! WebKitGTK's media player can't stream from Tauri's custom `asset://`
//! scheme (images work, video doesn't), so videos are served over
//! `http://127.0.0.1:<port>/<token>/<percent-encoded path>` with Range
//! support. The random token keeps other local processes from browsing, and
//! only allowed roots (the clips folder, files opened for editing) are served.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Clone)]
pub struct MediaServer {
    pub base_url: String,
    token: String,
    allowed: Arc<RwLock<Vec<PathBuf>>>,
}

impl MediaServer {
    pub async fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let server = Self {
            base_url: format!("http://127.0.0.1:{port}/{token}/"),
            token,
            allowed: Arc::default(),
        };
        let s = server.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    continue;
                };
                let s = s.clone();
                tokio::spawn(async move {
                    if let Err(e) = s.serve(sock).await {
                        tracing::debug!("media server: {e:#}");
                    }
                });
            }
        });
        Ok(server)
    }

    /// Allow a directory (recursively) or a single file.
    pub fn allow(&self, path: &Path) {
        if let Ok(p) = path.canonicalize() {
            let mut a = self.allowed.write().unwrap();
            if !a.contains(&p) {
                a.push(p);
            }
        }
    }

    fn is_allowed(&self, path: &Path) -> bool {
        self.allowed
            .read()
            .unwrap()
            .iter()
            .any(|root| path.starts_with(root))
    }

    async fn serve(&self, mut sock: TcpStream) -> Result<()> {
        let mut buf = Vec::with_capacity(2048);
        let mut chunk = [0u8; 2048];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = sock.read(&mut chunk).await?;
            if n == 0 || buf.len() > 16 * 1024 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let head = String::from_utf8_lossy(&buf);
        let mut lines = head.lines();
        let mut first = lines.next().unwrap_or_default().split_whitespace();
        let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
        let range = lines
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case("range"))
            .map(|(_, v)| v.trim().to_owned());

        let Some(encoded) = target
            .strip_prefix('/')
            .and_then(|t| t.strip_prefix(self.token.as_str()))
            .and_then(|t| t.strip_prefix('/'))
        else {
            return respond(&mut sock, "404 Not Found", &[]).await;
        };
        let path = PathBuf::from(percent_decode(encoded));
        let Ok(path) = path.canonicalize() else {
            return respond(&mut sock, "404 Not Found", &[]).await;
        };
        if !self.is_allowed(&path) || !matches!(method, "GET" | "HEAD") {
            return respond(&mut sock, "403 Forbidden", &[]).await;
        }

        let mut file = tokio::fs::File::open(&path).await?;
        let len = file.metadata().await?.len();
        let (start, end) = match range.as_deref().and_then(|r| parse_range(r, len)) {
            Some(r) => r,
            None if range.is_some() => {
                let cr = format!("bytes */{len}");
                return respond(
                    &mut sock,
                    "416 Range Not Satisfiable",
                    &[("Content-Range", &cr)],
                )
                .await;
            }
            None => (0, len.saturating_sub(1)),
        };
        let count = if len == 0 { 0 } else { end - start + 1 };
        let content_len = count.to_string();
        let content_range = format!("bytes {start}-{end}/{len}");
        let mut headers = vec![
            ("Content-Type", content_type(&path)),
            ("Content-Length", content_len.as_str()),
            ("Accept-Ranges", "bytes"),
            ("Cache-Control", "no-store"),
        ];
        let status = if range.is_some() {
            headers.push(("Content-Range", &content_range));
            "206 Partial Content"
        } else {
            "200 OK"
        };
        respond(&mut sock, status, &headers).await?;
        if method == "GET" && count > 0 {
            file.seek(std::io::SeekFrom::Start(start)).await?;
            tokio::io::copy(&mut file.take(count), &mut sock).await?;
        }
        sock.shutdown().await.ok();
        Ok(())
    }
}

async fn respond(sock: &mut TcpStream, status: &str, headers: &[(&str, &str)]) -> Result<()> {
    let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if !headers.iter().any(|(k, _)| *k == "Content-Length") {
        out.push_str("Content-Length: 0\r\n");
    }
    out.push_str("\r\n");
    sock.write_all(out.as_bytes()).await?;
    Ok(())
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` → inclusive (start, end).
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (len.checked_sub(n.min(len))?, len.checked_sub(1)?)
        }
        (a, "") => (a.parse().ok()?, len.checked_sub(1)?),
        (a, b) => (
            a.parse().ok()?,
            b.parse::<u64>().ok()?.min(len.checked_sub(1)?),
        ),
    };
    (start <= end && start < len).then_some((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    // The system's random source on every platform. This read
    // /dev/urandom before, which Windows doesn't have, so the token
    // there was only the time in nanoseconds: guessable.
    getrandom::fill(&mut bytes).expect("the system has a random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        // Not the clock: tokens made together share no long prefix.
        assert_ne!(a[..16], b[..16]);
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    #[test]
    fn decodes_paths() {
        assert_eq!(
            percent_decode("%2Fhome%2Fx%2FSteam%20Game%20(1)%2Fa.mp4"),
            "/home/x/Steam Game (1)/a.mp4"
        );
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
    }

    #[tokio::test]
    async fn serves_allowed_files_with_ranges_only() {
        let dir = std::env::temp_dir().join(format!("reruns-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a b.mp4");
        std::fs::write(&file, b"0123456789").unwrap();
        let server = MediaServer::start().await.unwrap();

        let url_path = |p: &Path| {
            format!(
                "{}{}",
                server
                    .base_url
                    .trim_start_matches("http://127.0.0.1:")
                    .split_once('/')
                    .unwrap()
                    .1,
                p.to_string_lossy().replace('/', "%2F").replace(' ', "%20")
            )
        };
        let port = server
            .base_url
            .split(':')
            .nth(2)
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let get = |path: String, range: Option<&str>| {
            let port = port.to_owned();
            let range = range.map(str::to_owned);
            async move {
                let mut s = TcpStream::connect(format!("127.0.0.1:{port}"))
                    .await
                    .unwrap();
                let mut req = format!("GET /{path} HTTP/1.1\r\nHost: x\r\n");
                if let Some(r) = range {
                    req.push_str(&format!("Range: {r}\r\n"));
                }
                req.push_str("\r\n");
                s.write_all(req.as_bytes()).await.unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                out
            }
        };

        let denied = get(url_path(&file), None).await;
        assert!(denied.starts_with("HTTP/1.1 403"), "{denied}");

        server.allow(&dir);
        let full = get(url_path(&file), None).await;
        assert!(
            full.starts_with("HTTP/1.1 200") && full.ends_with("0123456789"),
            "{full}"
        );
        let part = get(url_path(&file), Some("bytes=2-4")).await;
        assert!(
            part.starts_with("HTTP/1.1 206")
                && part.contains("Content-Range: bytes 2-4/10")
                && part.ends_with("234"),
            "{part}"
        );

        let wrong_token = get(
            format!("nottoken/{}", url_path(&file).split_once('/').unwrap().1),
            None,
        )
        .await;
        assert!(wrong_token.starts_with("HTTP/1.1 404"), "{wrong_token}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
