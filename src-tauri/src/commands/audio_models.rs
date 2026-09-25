//! Small ONNX models for the microphone chain (GTCRN denoiser, Silero VAD),
//! downloaded on demand into the app data dir with pinned SHA-256 checksums.
//! Both come from the sherpa-onnx release assets and total ~1.2 MB.

use std::path::PathBuf;

use serde::Serialize;
use tauri::ipc::Channel;

use super::download::{client, download_file, DownloadProgress, InFlightGuard};

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

/// Download every missing model, verifying each against its pinned SHA-256.
#[tauri::command]
pub async fn audio_models_download(on_progress: Channel<DownloadProgress>) -> Result<(), String> {
    let _guard = InFlightGuard::acquire("audio-models")?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create models dir: {e}"))?;
    let client = client()?;

    for m in &MODELS {
        if installed_path(m.id).is_some() {
            continue;
        }
        let emit = |phase: &str, received: u64, message: Option<String>| {
            let _ = on_progress.send(DownloadProgress {
                id: m.id.to_string(),
                phase: phase.to_string(),
                received,
                total: m.size,
                message,
            });
        };
        let result = download_file(&client, &[m.url], &dir.join(m.file), m.sha256, m.size, &|r| {
            emit("downloading", r, None)
        })
        .await;
        if let Err(e) = result {
            emit("error", 0, Some(e.clone()));
            return Err(e);
        }
        emit("done", m.size, None);
    }
    Ok(())
}
