//! Shared model downloader: streamed to a `.part` file, SHA-256 verified,
//! size-checked, renamed into place; mirrors tried in order; progress events
//! throttled so IPC isn't flooded. Also a hardened `.tar.bz2` extractor.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::Path;
use std::sync::Mutex;

use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub id: String,
    /// "downloading" | "extracting" | "done" | "error"
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub message: Option<String>,
}

/// Keys of downloads currently in flight (one per model family).
static IN_FLIGHT: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

/// RAII token proving no other download of `key` is running.
pub struct InFlightGuard(&'static str);

impl InFlightGuard {
    pub fn acquire(key: &'static str) -> Result<Self, String> {
        let mut set = IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner());
        if !set.get_or_insert_with(HashSet::new).insert(key) {
            return Err("Already downloading".into());
        }
        Ok(Self(key))
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = IN_FLIGHT.lock() {
            if let Some(s) = set.as_mut() {
                s.remove(self.0);
            }
        }
    }
}

/// Client for large downloads: no total timeout, but bounded connect and
/// per-read so a stalled socket can't hang the stream forever.
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| format!("client build failed: {e}"))
}

/// Download `urls[..]` (first success wins) to `dest`, verifying SHA-256 and
/// size. `on_progress(received_bytes)` is called at most every 256 KiB.
pub async fn download_file(
    client: &reqwest::Client,
    urls: &[&str],
    dest: &Path,
    expected_sha256: &str,
    expected_size: u64,
    on_progress: &impl Fn(u64),
) -> Result<(), String> {
    let mut last_err = String::from("no download URL configured");
    for url in urls {
        match download_one(client, url, dest, expected_sha256, expected_size, on_progress).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("[download] {url}: {e}");
                last_err = e;
            }
        }
    }
    Err(last_err)
}

async fn download_one(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_size: u64,
    on_progress: &impl Fn(u64),
) -> Result<(), String> {
    let part = dest.with_extension(match dest.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.part"),
        None => "part".to_string(),
    });
    let cleanup = || {
        let _ = std::fs::remove_file(&part);
    };

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }

    let mut file = std::fs::File::create(&part).map_err(|e| format!("create temp file: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                cleanup();
                return Err(format!("download interrupted: {e}"));
            }
        };
        hasher.update(&chunk);
        if let Err(e) = file.write_all(&chunk) {
            cleanup();
            return Err(format!("write failed: {e}"));
        }
        received += chunk.len() as u64;
        if received - last_emit >= 256 * 1024 {
            last_emit = received;
            on_progress(received);
        }
        if received > expected_size {
            cleanup();
            return Err(format!("file larger than expected ({received} > {expected_size})"));
        }
    }
    drop(file);

    if received != expected_size {
        cleanup();
        return Err(format!("unexpected size {received} (expected {expected_size})"));
    }
    let digest = hex::encode(hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha256) {
        cleanup();
        return Err("integrity check failed (SHA-256 mismatch)".into());
    }
    std::fs::rename(&part, dest).map_err(|e| {
        cleanup();
        format!("finalize failed: {e}")
    })?;
    on_progress(received);
    Ok(())
}

/// Decode a `.tar.bz2` into `dest`, stripping the single top-level directory,
/// rejecting any non-normal path component (`..`, absolute, prefixes) and
/// symlinks, and enforcing `max_bytes` on the decompressed total.
pub fn extract_tar_bz2(archive: &Path, dest: &Path, max_bytes: u64) -> Result<(), String> {
    use std::path::Component;

    let f = std::fs::File::open(archive).map_err(|e| format!("open archive: {e}"))?;
    let mut tar = tar::Archive::new(BzDecoder::new(f));
    std::fs::create_dir_all(dest).map_err(|e| format!("mkdir: {e}"))?;
    let mut total: u64 = 0;

    for entry in tar.entries().map_err(|e| format!("read archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("archive entry: {e}"))?;
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            continue; // symlinks, devices, etc. are never extracted
        }
        total += entry.header().size().unwrap_or(0);
        if total > max_bytes {
            return Err("archive larger than allowed".into());
        }
        let path = entry.path().map_err(|e| format!("entry path: {e}"))?.into_owned();
        let mut comps = path.components();
        comps.next(); // strip the top-level directory
        let mut rel = std::path::PathBuf::new();
        for c in comps {
            match c {
                Component::Normal(p) => rel.push(p),
                _ => return Err(format!("unsafe path in archive: {}", path.display())),
            }
        }
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if kind.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| format!("create: {e}"))?;
            std::io::copy(&mut entry, &mut f).map_err(|e| format!("extract: {e}"))?;
        }
    }
    Ok(())
}
