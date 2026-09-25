//! Models for the pure-Rust Local engine, downloaded on demand into the app
//! data dir with pinned SHA-256 checksums:
//!
//! - SenseVoice-small int8 (sherpa-onnx, zh/en/ja/ko/yue ASR), ~163 MB archive
//! - Qwen2.5-3B-Instruct Q4_K_M GGUF (llama.cpp translation LLM), ~2.1 GB
//!
//! Hugging Face is tried first, then the hf-mirror.com mirror (reachable from
//! mainland China without a VPN). A custom GGUF path in settings overrides the
//! bundled LLM choice.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::ipc::Channel;

use crate::commands::download::{download_file, extract_tar_bz2, DownloadProgress};

pub const SENSEVOICE_ID: &str = "sensevoice-int8";
pub const QWEN_ID: &str = "qwen2.5-3b-instruct-q4";

pub enum Kind {
    /// `.tar.bz2` whose single top-level folder is stripped into `dir`.
    Archive { dir: &'static str },
    /// A single file stored as `file`.
    File { file: &'static str },
}

pub struct LocalModel {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// Tried in order; the first that completes with the right hash wins.
    pub urls: &'static [&'static str],
    pub sha256: &'static str,
    pub size: u64,
}

pub const MODELS: [LocalModel; 2] = [
    LocalModel {
        id: SENSEVOICE_ID,
        label: "SenseVoice-small (nhận dạng)",
        kind: Kind::Archive { dir: "sensevoice-int8" },
        urls: &["https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"],
        sha256: "7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e",
        size: 163_002_883,
    },
    LocalModel {
        id: QWEN_ID,
        label: "Qwen2.5-3B-Instruct Q4 (dịch)",
        kind: Kind::File { file: "qwen2.5-3b-instruct-q4_k_m.gguf" },
        urls: &[
            "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
            "https://hf-mirror.com/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
        ],
        sha256: "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d",
        size: 2_104_932_768,
    },
];

/// Extracted archives larger than this are refused (defence against a bad mirror).
const MAX_EXTRACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn models_dir() -> PathBuf {
    crate::commands::app_support_dir().join("local-models")
}

fn entry(id: &str) -> Option<&'static LocalModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// Files the ASR needs inside the SenseVoice folder.
pub struct SenseVoiceFiles {
    pub model: PathBuf,
    pub tokens: PathBuf,
}

/// Installed SenseVoice files, if the archive was fully extracted.
pub fn sensevoice_files() -> Option<SenseVoiceFiles> {
    let Some(LocalModel { kind: Kind::Archive { dir }, .. }) = entry(SENSEVOICE_ID) else {
        return None;
    };
    let root = models_dir().join(dir);
    let files = SenseVoiceFiles {
        model: root.join("model.int8.onnx"),
        tokens: root.join("tokens.txt"),
    };
    (root.join(".complete").is_file() && files.model.is_file() && files.tokens.is_file())
        .then_some(files)
}

/// Installed GGUF: a custom path from settings when it exists, else the
/// bundled Qwen download (size-checked so a partial file never loads).
pub fn llm_path(custom: &str) -> Option<PathBuf> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
    }
    let m = entry(QWEN_ID)?;
    let Kind::File { file } = m.kind else {
        return None;
    };
    let p = models_dir().join(file);
    std::fs::metadata(&p)
        .map(|md| md.is_file() && md.len() == m.size)
        .unwrap_or(false)
        .then_some(p)
}

fn is_installed(m: &LocalModel) -> bool {
    match m.id {
        SENSEVOICE_ID => sensevoice_files().is_some(),
        QWEN_ID => llm_path("").is_some(),
        _ => false,
    }
}

#[derive(Serialize)]
pub struct LocalModelStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub installed: bool,
    pub size: u64,
}

#[tauri::command]
pub fn local_models_status() -> Vec<LocalModelStatus> {
    MODELS
        .iter()
        .map(|m| LocalModelStatus {
            id: m.id,
            label: m.label,
            installed: is_installed(m),
            size: m.size,
        })
        .collect()
}

/// Download every missing model (SHA-256 verified), extracting archives.
/// One download at a time — a second call while in flight is rejected.
#[tauri::command]
pub async fn local_models_download(on_progress: Channel<DownloadProgress>) -> Result<(), String> {
    let _guard = crate::commands::download::InFlightGuard::acquire("local-models")?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create models dir: {e}"))?;
    let client = crate::commands::download::client()?;

    for m in MODELS.iter() {
        if is_installed(m) {
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
        if let Err(e) = install_one(&client, m, &dir, &emit).await {
            emit("error", 0, Some(e.clone()));
            return Err(e);
        }
        emit("done", m.size, None);
    }
    Ok(())
}

async fn install_one(
    client: &reqwest::Client,
    m: &LocalModel,
    dir: &Path,
    emit: &impl Fn(&str, u64, Option<String>),
) -> Result<(), String> {
    match m.kind {
        Kind::File { file } => {
            download_file(client, m.urls, &dir.join(file), m.sha256, m.size, &|received| {
                emit("downloading", received, None)
            })
            .await
        }
        Kind::Archive { dir: sub } => {
            let archive = dir.join(format!("{sub}.tar.bz2"));
            download_file(client, m.urls, &archive, m.sha256, m.size, &|received| {
                emit("downloading", received, None)
            })
            .await?;
            emit("extracting", m.size, None);
            let target = dir.join(sub);
            let tmp = dir.join(format!(".tmp-{sub}"));
            let _ = std::fs::remove_dir_all(&tmp);
            // Blocking work (bz2 + tar of ~160 MB) off the async runtime.
            let extract_res = {
                let (archive, tmp) = (archive.clone(), tmp.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    extract_tar_bz2(&archive, &tmp, MAX_EXTRACT_BYTES)
                })
                .await
                .map_err(|e| format!("extract task failed: {e}"))?
            };
            let _ = std::fs::remove_file(&archive);
            if let Err(e) = extract_res {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(e);
            }
            // Sample wavs and export scripts aren't needed at runtime.
            let _ = std::fs::remove_dir_all(tmp.join("test_wavs"));
            let _ = std::fs::remove_file(tmp.join("export-onnx.py"));
            let _ = std::fs::remove_dir_all(&target);
            std::fs::rename(&tmp, &target).map_err(|e| format!("Failed to finalize {sub}: {e}"))?;
            std::fs::write(target.join(".complete"), b"1")
                .map_err(|e| format!("Failed to write completion marker: {e}"))?;
            Ok(())
        }
    }
}
