//! Small ONNX models for the microphone chain (GTCRN denoiser, Silero VAD),
//! downloaded on demand into the app data dir with pinned SHA-256 checksums.
//! Both come from the sherpa-onnx release assets and total ~1.2 MB.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;

pub struct AudioModel {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub const GTCRN_ID: &str = "gtcrn";
pub const SILERO_VAD_ID: &str = "silero-vad";

pub const MODELS: [AudioModel; 2] = [
    AudioModel {
        id: GTCRN_ID,
        file: "gtcrn_simple.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx",
        sha256: "e77603ac0c23dac3227dd2d7135b3a585cbee2679048aecfa886657d3ae1b534",
        size: 535_638,
    },
    AudioModel {
        id: SILERO_VAD_ID,
        file: "silero_vad.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
        sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
        size: 643_854,
    },
];

pub fn models_dir() -> PathBuf {
    super::app_support_dir().join("audio-models")
}

fn entry(id: &str) -> Option<&'static AudioModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// Path of an installed model, or `None`. A size check catches partial files
/// without hashing on every capture start.
pub fn installed_path(id: &str) -> Option<PathBuf> {
    let m = entry(id)?;
    let path = models_dir().join(m.file);
    let ok = std::fs::metadata(&path)
        .map(|md| md.is_file() && md.len() == m.size)
        .unwrap_or(false);
    ok.then_some(path)
}

#[derive(Serialize)]
pub struct AudioModelStatus {
    pub id: &'static str,
    pub installed: bool,
    pub size: u64,
}

#[tauri::command]
pub fn audio_models_status() -> Vec<AudioModelStatus> {
    MODELS
        .iter()
        .map(|m| AudioModelStatus {
            id: m.id,
            installed: installed_path(m.id).is_some(),
            size: m.size,
        })
        .collect()
}

#[derive(Serialize, Clone)]
pub struct AudioModelProgress {
    pub id: String,
    /// "downloading" | "done" | "error"
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub message: Option<String>,
}

/// One download at a time: a second click while in flight is rejected rather
/// than racing on the same `.part` file.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

struct InFlightGuard;
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Download every missing model, verifying each against its pinned SHA-256.
#[tauri::command]
pub async fn audio_models_download(
    on_progress: Channel<AudioModelProgress>,
) -> Result<(), String> {
    if IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Already downloading".into());
    }
    let _guard = InFlightGuard;

    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create models dir: {e}"))?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;

    for m in &MODELS {
        if installed_path(m.id).is_some() {
            continue;
        }
        let emit = |phase: &str, received: u64, message: Option<String>| {
            let _ = on_progress.send(AudioModelProgress {
                id: m.id.to_string(),
                phase: phase.to_string(),
                received,
                total: m.size,
                message,
            });
        };
        if let Err(e) = download_one(&client, m, &dir, &emit).await {
            emit("error", 0, Some(e.clone()));
            return Err(e);
        }
        emit("done", m.size, None);
    }
    Ok(())
}

async fn download_one(
    client: &reqwest::Client,
    m: &AudioModel,
    dir: &std::path::Path,
    emit: &impl Fn(&str, u64, Option<String>),
) -> Result<(), String> {
    let part = dir.join(format!("{}.part", m.file));
    let cleanup = || {
        let _ = std::fs::remove_file(&part);
    };

    let resp = client
        .get(m.url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download HTTP {}", resp.status().as_u16()));
    }

    let mut file =
        std::fs::File::create(&part).map_err(|e| format!("Failed to create temp file: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                cleanup();
                return Err(format!("Download interrupted: {e}"));
            }
        };
        hasher.update(&chunk);
        if let Err(e) = file.write_all(&chunk) {
            cleanup();
            return Err(format!("Write failed: {e}"));
        }
        received += chunk.len() as u64;
        // Throttle progress events to every 64 KiB so IPC isn't flooded.
        if received - last_emit >= 64 * 1024 {
            last_emit = received;
            emit("downloading", received, None);
        }
    }
    drop(file);

    let digest = hex::encode(hasher.finalize());
    if !digest.eq_ignore_ascii_case(m.sha256) {
        cleanup();
        return Err(format!(
            "{} failed integrity check (SHA-256 mismatch)",
            m.file
        ));
    }
    if received != m.size {
        cleanup();
        return Err(format!("{} has unexpected size {received}", m.file));
    }
    std::fs::rename(&part, dir.join(m.file)).map_err(|e| {
        cleanup();
        format!("Failed to finalize {}: {e}", m.file)
    })?;
    Ok(())
}
