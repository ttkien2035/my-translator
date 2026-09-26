//! Models for the pure-Rust Local engine, downloaded on demand into the app
//! data dir with pinned SHA-256 checksums:
//!
//! - X-ASR-zh-en Zipformer transducer int8, punctuation build (sherpa-onnx
//!   release asset), 136 MB archive
//! - Tencent Hy-MT2-1.8B Q6_K GGUF (llama.cpp translation model), ~1.47 GB
//!
//! Hugging Face is tried first, then the hf-mirror.com mirror (reachable from
//! mainland China without a VPN). A custom GGUF path in settings overrides the
//! bundled LLM choice.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::ipc::Channel;

use crate::commands::download::{download_file, extract_tar_bz2, DownloadProgress};

pub const ASR_ID: &str = "x-asr-zh-en-punct-int8";
pub const LLM_ID: &str = "hy-mt2-1.8b-q6";

/// Former models, deleted once their replacement is installed:
/// Qwen2.5-3B-Instruct Q4_K_M (2.1 GB; kept if it is the custom GGUF) and
/// SenseVoice-small int8 (230 MB).
const LEGACY_LLM_FILE: &str = "qwen2.5-3b-instruct-q4_k_m.gguf";
const LEGACY_ASR_DIR: &str = "sensevoice-int8";

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
    // Apache-2.0 (SJTU et al., 2026-06). Chosen over SenseVoice-small on
    // 480 lecture/meeting/classroom utterances and two 25-min lectures:
    // fewer errors (6.8 % vs 7.0 % overall, 14.6 % vs 16.6 % in a simulated
    // classroom, 8.2 % vs 8.9 % on a real lecture), punctuation, English
    // casing, and hotwords for the course glossary.
    LocalModel {
        id: ASR_ID,
        label: "X-ASR Zipformer zh-en (nhận dạng)",
        kind: Kind::Archive { dir: "x-asr-zh-en-punct-int8" },
        urls: &["https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-zipformer-transducer-zh-en-punct-int8-2026-06-03.tar.bz2"],
        sha256: "5d02c36d7b44e886b7c8f0d8e051f8713acab96c264bb6ef9e718be39a6a2224",
        size: 136_396_739,
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

/// Files the recogniser needs inside the X-ASR folder.
pub struct AsrFiles {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
    /// Written by `ensure_bpe_vocab` from the archive's `bpe.model`.
    pub bpe_vocab: PathBuf,
}

impl AsrFiles {
    pub fn in_dir(root: &Path) -> Self {
        Self {
            encoder: root.join("encoder-epoch-99-avg-1.int8.onnx"),
            decoder: root.join("decoder-epoch-99-avg-1.onnx"),
            joiner: root.join("joiner-epoch-99-avg-1.int8.onnx"),
            tokens: root.join("tokens.txt"),
            bpe_vocab: root.join("bpe.vocab"),
        }
    }

    fn all_present(&self) -> bool {
        [&self.encoder, &self.decoder, &self.joiner, &self.tokens, &self.bpe_vocab].iter().all(|p| p.is_file())
    }
}

/// Installed X-ASR files, if the archive was fully extracted.
pub fn asr_files() -> Option<AsrFiles> {
    let Some(LocalModel { kind: Kind::Archive { dir }, .. }) = entry(ASR_ID) else {
        return None;
    };
    let root = models_dir().join(dir);
    if !root.join(".complete").is_file() {
        return None;
    }
    // Installs made before bpe.vocab existed get it here.
    let _ = ensure_bpe_vocab(&root);
    let files = AsrFiles::in_dir(&root);
    files.all_present().then_some(files)
}

/// Write `bpe.vocab` next to `bpe.model` unless it is already there.
/// sherpa-onnx's hotword encoder reads the vocab file, not the model.
pub fn ensure_bpe_vocab(root: &Path) -> Result<PathBuf, String> {
    let vocab = root.join("bpe.vocab");
    if vocab.is_file() {
        return Ok(vocab);
    }
    let model = std::fs::read(root.join("bpe.model")).map_err(|e| format!("read bpe.model: {e}"))?;
    let text = super::spm::vocab_text(&super::spm::pieces(&model)?);
    let tmp = root.join(".bpe.vocab.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("write bpe.vocab: {e}"))?;
    std::fs::rename(&tmp, &vocab).map_err(|e| format!("finalize bpe.vocab: {e}"))?;
    Ok(vocab)
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
        ASR_ID => asr_files().is_some(),
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
    remove_legacy(&dir, &custom_gguf);
    Ok(())
}

/// Free the space of former defaults once their replacements are in place.
/// The old GGUF is kept when it is the user's custom model; anything
/// uncertain is kept.
fn remove_legacy(dir: &Path, custom_gguf: &str) {
    if llm_path("").is_some() {
        let legacy = dir.join(LEGACY_LLM_FILE);
        if legacy.is_file() && !is_same_file(&legacy, Path::new(custom_gguf.trim())) {
            let _ = std::fs::remove_file(&legacy);
        }
    }
    if asr_files().is_some() {
        let legacy = dir.join(LEGACY_ASR_DIR);
        if legacy.is_dir() {
            let _ = std::fs::remove_dir_all(&legacy);
        }
    }
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
            // Blocking work (bz2 + tar) off the async runtime.
            let extract_res = {
                let (archive, tmp) = (archive.clone(), tmp.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    extract_tar_bz2(&archive, &tmp, MAX_EXTRACT_BYTES)
                })
                .await
                .map_err(|e| format!("extract task failed: {e}"))?
            };
            let _ = std::fs::remove_file(&archive);
            let finish = || -> Result<(), String> {
                extract_res?;
                // Sample wavs and export scripts aren't needed at runtime.
                let _ = std::fs::remove_dir_all(tmp.join("test_wavs"));
                for f in ["export-onnx.py", "test_onnx.py"] {
                    let _ = std::fs::remove_file(tmp.join(f));
                }
                if m.id == ASR_ID {
                    ensure_bpe_vocab(&tmp)?;
                }
                Ok(())
            };
            if let Err(e) = finish() {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(e);
            }
            let _ = std::fs::remove_dir_all(&target);
            std::fs::rename(&tmp, &target).map_err(|e| format!("Failed to finalize {sub}: {e}"))?;
            std::fs::write(target.join(".complete"), b"1")
                .map_err(|e| format!("Failed to write completion marker: {e}"))?;
            Ok(())
        }
    }
}
