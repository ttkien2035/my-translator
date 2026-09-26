//! Models for the pure-Rust Local engine, downloaded on demand into the app
//! data dir with pinned SHA-256 checksums:
//!
//! - SenseVoice-small int8 (sherpa-onnx, zh/en/ja/ko/yue ASR), ~163 MB archive
//! - Tencent Hy-MT2-1.8B Q6_K GGUF (llama.cpp translation model), ~1.47 GB
//!
//! Hugging Face is tried first, then the hf-mirror.com mirror (reachable from
//! mainland China without a VPN). A custom GGUF path in settings overrides the
//! bundled LLM choice.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::ipc::Channel;

use crate::commands::download::{download_file, extract_tar_bz2, DownloadProgress};

pub const SENSEVOICE_ID: &str = "sensevoice-int8";
pub const LLM_ID: &str = "hy-mt2-1.8b-q6";

/// The former default LLM (Qwen2.5-3B-Instruct Q4_K_M, 2.1 GB). Deleted once
/// Hy-MT2 is installed, unless the user points the custom GGUF at it.
const LEGACY_LLM_FILE: &str = "qwen2.5-3b-instruct-q4_k_m.gguf";

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
    // Apache-2.0. Chosen over Qwen2.5-3B by a 25-sentence finance-lecture
    // benchmark: no untranslated Chinese (Qwen: 15/25), faster, smaller.
    // Q6_K rather than Q4_K_M: Q4 dropped digits ("3.2 lần" → "3 lần").
    LocalModel {
        id: LLM_ID,
        label: "Hy-MT2-1.8B Q6 (dịch)",
        kind: Kind::File { file: "Hy-MT2-1.8B-Q6_K.gguf" },
        urls: &[
            "https://huggingface.co/tencent/Hy-MT2-1.8B-GGUF/resolve/main/Hy-MT2-1.8B-Q6_K.gguf",
            "https://hf-mirror.com/tencent/Hy-MT2-1.8B-GGUF/resolve/main/Hy-MT2-1.8B-Q6_K.gguf",
        ],
        sha256: "d98fe604dec1f28f58f80d7d560f7177e584d3b8e5835862687660e5ff97cb40",
        size: 1_474_785_120,
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
/// bundled Hy-MT2 download (size-checked so a partial file never loads).
pub fn llm_path(custom: &str) -> Option<PathBuf> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
    }
    let m = entry(LLM_ID)?;
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
        LLM_ID => llm_path("").is_some(),
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
pub async fn local_models_download(
    on_progress: Channel<DownloadProgress>,
    settings: tauri::State<'_, crate::settings::SettingsState>,
) -> Result<(), String> {
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
    let custom_gguf = settings.0.lock().map(|s| s.local_llm_gguf.clone()).unwrap_or_default();
    remove_legacy_llm(&dir, &custom_gguf);
    Ok(())
}

/// Free the 2.1 GB of the former default once its replacement is in place.
/// Kept when it is the user's custom GGUF, or if anything is uncertain.
fn remove_legacy_llm(dir: &Path, custom_gguf: &str) {
    if llm_path("").is_none() {
        return;
    }
    let legacy = dir.join(LEGACY_LLM_FILE);
    if !legacy.is_file() || is_same_file(&legacy, Path::new(custom_gguf.trim())) {
        return;
    }
    let _ = std::fs::remove_file(&legacy);
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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
