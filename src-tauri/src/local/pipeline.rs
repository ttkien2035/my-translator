//! Local engine session: audio → Silero VAD utterances → SenseVoice ASR →
//! Qwen (llama.cpp) translation → events to the webview.
//!
//! Two worker threads, both owned by the session:
//! - `local-asr`: owns VAD + recognizer; drains the audio queue, emits one
//!   utterance text per detected speech segment.
//! - `local-llm`: owns the LLM; translates utterances from a small
//!   drop-oldest queue (staying current beats a growing delay).
//!
//! Stopping = dropping the `Session`: the audio sender closes → ASR thread
//! finishes → marks the utterance queue done → LLM thread finishes. Models are
//! freed with the threads; nothing runs while idle.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};

use serde::Serialize;
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};
use tauri::ipc::Channel;

use super::asr::Asr;
use super::llm::{Llm, TranslateRequest};

/// Audio queue depth (200 ms capture batches): ~10 s before frames are dropped.
const AUDIO_QUEUE_CHUNKS: usize = 50;
/// Utterances waiting for the LLM; beyond this the oldest is skipped.
const UTTERANCE_QUEUE_MAX: usize = 4;
const SAMPLE_RATE: u32 = 16_000;

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalEvent {
    /// `state`: "loading" | "ready" | "backlog_skipped"
    Status { state: String, message: Option<String> },
    Result { src: String, tgt: String },
    Error { code: String, message: String },
    Closed { reason: String },
}

pub struct SessionConfig {
    pub asr_model: std::path::PathBuf,
    pub asr_tokens: std::path::PathBuf,
    pub llm_model: std::path::PathBuf,
    pub vad_model: std::path::PathBuf,
    /// SenseVoice language code ("auto", "zh", …) and prompt language names.
    pub asr_language: String,
    pub source_lang_name: String,
    pub target_lang_name: String,
    pub glossary: Vec<(String, String)>,
}

/// Drop-oldest queue of recognised utterances between the two workers.
struct UtteranceQueue {
    inner: Mutex<(VecDeque<String>, bool)>, // (items, producer done)
    cv: Condvar,
}

impl UtteranceQueue {
    fn new() -> Self {
        Self {
            inner: Mutex::new((VecDeque::with_capacity(UTTERANCE_QUEUE_MAX + 1), false)),
            cv: Condvar::new(),
        }
    }

    /// Push; returns how many older items were dropped to stay within bounds.
    fn push(&self, text: String) -> usize {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.0.push_back(text);
        let mut dropped = 0;
        while g.0.len() > UTTERANCE_QUEUE_MAX {
            g.0.pop_front();
            dropped += 1;
        }
        self.cv.notify_one();
        dropped
    }

    fn finish(&self) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.1 = true;
        self.cv.notify_all();
    }

    /// Blocks until an item is available; `None` once finished and drained.
    fn pop(&self) -> Option<String> {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(t) = g.0.pop_front() {
                return Some(t);
            }
            if g.1 {
                return None;
            }
            g = self.cv.wait(g).unwrap_or_else(|p| p.into_inner());
        }
    }
}

pub struct Session {
    audio_tx: Option<SyncSender<Vec<u8>>>,
    cancel: Arc<AtomicBool>,
}

impl Session {
    /// Hot path: enqueue one s16le 16 kHz chunk. Never blocks; a full queue
    /// drops the chunk (ASR is ~15× real time, so this only happens if the
    /// machine is badly overloaded).
    pub fn push_audio(&self, pcm: Vec<u8>) -> Result<(), String> {
        let Some(tx) = &self.audio_tx else {
            return Err("session stopped".into());
        };
        match tx.try_send(pcm) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
            Err(TrySendError::Disconnected(_)) => Err("pipeline ended".into()),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Abort any in-flight generation, then close the audio queue so the
        // workers unwind and free their models.
        self.cancel.store(true, Ordering::SeqCst);
        self.audio_tx.take();
    }
}

pub fn start(cfg: SessionConfig, events: Channel<LocalEvent>) -> Result<Session, String> {
    let (audio_tx, audio_rx) = sync_channel::<Vec<u8>>(AUDIO_QUEUE_CHUNKS);
    let cancel = Arc::new(AtomicBool::new(false));
    let queue = Arc::new(UtteranceQueue::new());
    let cfg = Arc::new(cfg);

    // ASR worker
    {
        let (events, queue, cfg, cancel) = (events.clone(), queue.clone(), cfg.clone(), cancel.clone());
        std::thread::Builder::new()
            .name("local-asr".into())
            .spawn(move || {
                asr_worker(&cfg, audio_rx, &queue, &events, &cancel);
                queue.finish();
            })
            .map_err(|e| format!("spawn local-asr: {e}"))?;
    }
    // LLM worker
    {
        let (queue, cancel) = (queue.clone(), cancel.clone());
        std::thread::Builder::new()
            .name("local-llm".into())
            .spawn(move || {
                llm_worker(&cfg, &queue, &events, &cancel);
                let _ = events.send(LocalEvent::Closed {
                    reason: "session_ended".into(),
                });
            })
            .map_err(|e| format!("spawn local-llm: {e}"))?;
    }

    Ok(Session {
        audio_tx: Some(audio_tx),
        cancel,
    })
}

fn status(events: &Channel<LocalEvent>, state: &str, message: impl Into<Option<String>>) {
    let _ = events.send(LocalEvent::Status {
        state: state.into(),
        message: message.into(),
    });
}

fn asr_worker(
    cfg: &SessionConfig,
    audio_rx: Receiver<Vec<u8>>,
    queue: &UtteranceQueue,
    events: &Channel<LocalEvent>,
    cancel: &AtomicBool,
) {
    status(events, "loading", Some("Đang nạp SenseVoice…".into()));
    let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 4) as i32;
    let asr = match Asr::load(&cfg.asr_model, &cfg.asr_tokens, &cfg.asr_language, threads) {
        Ok(a) => a,
        Err(e) => {
            let _ = events.send(LocalEvent::Error { code: "asr_load".into(), message: e });
            return;
        }
    };
    let vad_cfg = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(cfg.vad_model.to_string_lossy().into_owned()),
            threshold: 0.5,
            // Utterance boundaries: a 350 ms pause ends a sentence; a monologue
            // is cut at 8 s so latency stays bounded.
            min_silence_duration: 0.35,
            min_speech_duration: 0.25,
            window_size: 512,
            max_speech_duration: 8.0,
        },
        sample_rate: SAMPLE_RATE as i32,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        debug: false,
        ..Default::default()
    };
    let Some(vad) = VoiceActivityDetector::create(&vad_cfg, 30.0) else {
        let _ = events.send(LocalEvent::Error {
            code: "vad_load".into(),
            message: format!("Silero VAD failed to load from {}", cfg.vad_model.display()),
        });
        return;
    };
    status(events, "asr_ready", None);

    let mut samples: Vec<f32> = Vec::with_capacity(8192);
    let drain = |vad: &VoiceActivityDetector| {
        while let Some(seg) = vad.front() {
            let text = asr.transcribe(seg.samples());
            vad.pop();
            if text.is_empty() {
                continue;
            }
            let dropped = queue.push(text);
            if dropped > 0 {
                status(events, "backlog_skipped", Some(dropped.to_string()));
            }
        }
    };

    for pcm in audio_rx {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        samples.clear();
        samples.reserve(pcm.len() / 2);
        for i in 0..pcm.len() / 2 {
            samples.push(i16::from_le_bytes([pcm[2 * i], pcm[2 * i + 1]]) as f32 / 32768.0);
        }
        vad.accept_waveform(&samples);
        drain(&vad);
    }
    if !cancel.load(Ordering::SeqCst) {
        vad.flush();
        drain(&vad);
    }
}

fn llm_worker(
    cfg: &SessionConfig,
    queue: &UtteranceQueue,
    events: &Channel<LocalEvent>,
    cancel: &AtomicBool,
) {
    status(events, "loading", Some("Đang nạp Qwen2.5 (dịch)…".into()));
    let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 8) as i32;
    let llm = match Llm::load(&cfg.llm_model, threads) {
        Ok(l) => l,
        Err(e) => {
            let _ = events.send(LocalEvent::Error { code: "llm_load".into(), message: e });
            return;
        }
    };
    status(events, "ready", None);

    while let Some(src) = queue.pop() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let req = TranslateRequest {
            text: &src,
            source_lang: &cfg.source_lang_name,
            target_lang: &cfg.target_lang_name,
            glossary: &cfg.glossary,
        };
        match llm.translate(&req, cancel) {
            Ok(tgt) if !tgt.is_empty() => {
                let _ = events.send(LocalEvent::Result { src, tgt });
            }
            Ok(_) => {}
            Err(e) => {
                let _ = events.send(LocalEvent::Error { code: "translate".into(), message: e });
            }
        }
    }
}
