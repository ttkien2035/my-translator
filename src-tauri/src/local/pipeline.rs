//! Local engine session: audio → Silero VAD utterances → X-ASR (Zipformer)
//! recognition → Hy-MT2 (llama.cpp) translation → events to a sink (the
//! webview Channel in the app, a closure in tests).
//!
//! Two worker threads, both owned by the session:
//! - `local-asr`: owns VAD + recognizer; drains the audio queue, emits one
//!   utterance text per detected speech segment (junk segments filtered).
//! - `local-llm`: owns the translator; translates utterances from a small
//!   drop-oldest queue (staying current beats a growing delay).
//!
//! Stopping = dropping the `Session`: cancel is raised and the audio sender
//! closes → ASR thread finishes → marks the utterance queue done → LLM
//! thread finishes and emits `Closed`. `Session::finish` is the graceful
//! variant (no cancel: flush the last utterance, translate what is queued).
//! Models are freed with the threads; nothing runs while idle.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};

use serde::Serialize;
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use super::asr::Asr;
use super::llm::{Llm, TranslateRequest};
use super::models::AsrFiles;

/// Audio queue depth (200 ms capture batches): ~10 s before frames are dropped.
const AUDIO_QUEUE_CHUNKS: usize = 50;
/// Utterances waiting for the LLM; beyond this the oldest is skipped.
pub const UTTERANCE_QUEUE_MAX: usize = 4;
const SAMPLE_RATE: u32 = 16_000;
/// Segments quieter than this (RMS, dBFS) are noise that slipped past VAD.
const MIN_SEGMENT_DBFS: f32 = -45.0;
/// Audio prepended to each VAD segment. Silero's own padding clips the
/// initial consonant of an utterance that follows silence ("开放" → "放");
/// 300 ms restores it and stays inside the ≥ 350 ms pause VAD needs to end
/// the previous utterance, so no tail of it leaks in.
const PRE_ROLL_SAMPLES: usize = SAMPLE_RATE as usize * 3 / 10;
/// Recent audio kept for pre-roll: a segment's start lies at most
/// HARD_CUT (12 s) + min_silence (0.35 s) behind the newest sample.
const HISTORY_SAMPLES: usize = SAMPLE_RATE as usize * 16;
/// sherpa's `max_speech_duration` only relaxes the end-of-speech rule (a
/// shorter pause then suffices); under continuous babble — a classroom —
/// a segment kept growing for 46 s in tests: that much translation delay,
/// and X-ASR's graph fails at ≥ 50 s. So the pipeline cuts on its own: at a
/// quiet capture chunk once SOFT_CUT is reached, unconditionally at HARD_CUT.
const SOFT_CUT_SAMPLES: u64 = SAMPLE_RATE as u64 * 8;
const HARD_CUT_SAMPLES: u64 = SAMPLE_RATE as u64 * 12;
/// A capture chunk this far below the utterance's loudest chunk is a pause.
const CUT_QUIET_DB: f32 = 15.0;

/// Ring of the most recent input samples, indexed by absolute sample number
/// (the same numbering VAD uses for `SpeechSegment::start`).
struct History {
    buf: VecDeque<f32>,
    /// Absolute index one past the newest sample.
    end: u64,
}

impl History {
    fn new() -> Self {
        Self {
            buf: VecDeque::with_capacity(HISTORY_SAMPLES),
            end: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.buf.extend(samples.iter().copied());
        self.end += samples.len() as u64;
        let excess = self.buf.len().saturating_sub(HISTORY_SAMPLES);
        self.buf.drain(..excess);
    }

    /// Append the stored samples in `[from, to)` to `out` (clamped to what is kept).
    fn copy_range(&self, from: u64, to: u64, out: &mut Vec<f32>) {
        let first = self.end - self.buf.len() as u64;
        let (from, to) = (from.max(first), to.min(self.end));
        if from < to {
            let (a, b) = ((from - first) as usize, (to - first) as usize);
            out.extend(self.buf.range(a..b));
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalEvent {
    /// `state`: "loading" | "asr_ready" | "ready" | "backlog_skipped"
    Status { state: String, message: Option<String> },
    Result { src: String, tgt: String },
    Error { code: String, message: String },
    Closed { reason: String },
}

/// Where session events go. Shared by both worker threads, hence `Sync`.
pub type EventSink = Arc<dyn Fn(LocalEvent) + Send + Sync>;

/// Translation backend. `Llm` in the app; tests substitute a stub (e.g. one
/// that sleeps to exercise the backlog path) through `start_with_translator`.
pub trait Translator: Send {
    fn translate(&self, req: &TranslateRequest, cancel: &AtomicBool) -> Result<String, String>;
}

impl Translator for Llm {
    fn translate(&self, req: &TranslateRequest, cancel: &AtomicBool) -> Result<String, String> {
        Llm::translate(self, req, cancel)
    }
}

/// Builds the translator on the `local-llm` thread (so a slow model load
/// never blocks the caller).
pub type TranslatorFactory = Box<dyn FnOnce() -> Result<Box<dyn Translator>, String> + Send>;

pub struct SessionConfig {
    pub asr: AsrFiles,
    pub llm_model: std::path::PathBuf,
    pub vad_model: std::path::PathBuf,
    /// Prompt language names ("Chinese", "Vietnamese").
    pub source_lang_name: String,
    pub target_lang_name: String,
    /// Course glossary (source → target). Sources become ASR hotwords;
    /// pairs found in a sentence go into that sentence's prompt.
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
    /// The newest item is never the one dropped.
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

    /// Graceful end of input: close the audio queue *without* cancelling, so
    /// the trailing utterance is flushed and everything queued is translated
    /// before `Closed`. A later drop of the session no longer cancels.
    pub fn finish(&mut self) {
        self.audio_tx.take();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Abrupt stop (unless `finish` ran): abort any in-flight generation,
        // then close the audio queue so the workers unwind and free models.
        if self.audio_tx.take().is_some() {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }
}

/// Start a session that reports events to `sink`, translating with the real
/// LLM (`cfg.llm_model`, warmed up after load).
pub fn start_with_sink(
    cfg: SessionConfig,
    sink: Box<dyn Fn(LocalEvent) + Send + Sync>,
) -> Result<Session, String> {
    let llm_path = cfg.llm_model.clone();
    let factory: TranslatorFactory = Box::new(move || {
        let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 8) as i32;
        let llm = Llm::load(&llm_path, threads)?;
        llm.warm_up();
        Ok(Box::new(llm) as Box<dyn Translator>)
    });
    start_with_translator(cfg, sink, factory)
}

/// Start a session with an explicit translator (tests: stubs; the app goes
/// through `start_with_sink`).
pub fn start_with_translator(
    cfg: SessionConfig,
    sink: Box<dyn Fn(LocalEvent) + Send + Sync>,
    make_translator: TranslatorFactory,
) -> Result<Session, String> {
    let sink: EventSink = Arc::from(sink);
    let (audio_tx, audio_rx) = sync_channel::<Vec<u8>>(AUDIO_QUEUE_CHUNKS);
    let cancel = Arc::new(AtomicBool::new(false));
    let queue = Arc::new(UtteranceQueue::new());
    let cfg = Arc::new(cfg);

    // ASR worker
    {
        let (sink, queue, cfg, cancel) = (sink.clone(), queue.clone(), cfg.clone(), cancel.clone());
        std::thread::Builder::new()
            .name("local-asr".into())
            .spawn(move || {
                asr_worker(&cfg, audio_rx, &queue, &sink, &cancel);
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
                llm_worker(&cfg, make_translator, &queue, &sink, &cancel);
                sink(LocalEvent::Closed {
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

fn status(sink: &EventSink, state: &str, message: impl Into<Option<String>>) {
    sink(LocalEvent::Status {
        state: state.into(),
        message: message.into(),
    });
}

/// Utterance length guard (see `SOFT_CUT_SAMPLES`): tracks how long VAD has
/// been inside speech and the loudest capture chunk so far.
#[derive(Default)]
struct Cutter {
    /// Absolute sample index when speech was first seen (None outside speech).
    started: Option<u64>,
    peak_dbfs: f32,
}

impl Cutter {
    /// `now`: absolute index one past the newest sample; `chunk_dbfs`: level of
    /// the chunk just fed to VAD. True = cut the utterance here.
    fn update(&mut self, now: u64, chunk_dbfs: f32) -> bool {
        let started = *self.started.get_or_insert_with(|| {
            self.peak_dbfs = f32::NEG_INFINITY;
            now
        });
        self.peak_dbfs = self.peak_dbfs.max(chunk_dbfs);
        should_cut(now - started, chunk_dbfs, self.peak_dbfs)
    }

    fn reset(&mut self) {
        self.started = None;
    }
}

fn should_cut(len: u64, chunk_dbfs: f32, peak_dbfs: f32) -> bool {
    len >= HARD_CUT_SAMPLES || (len >= SOFT_CUT_SAMPLES && chunk_dbfs <= peak_dbfs - CUT_QUIET_DB)
}

/// RMS level of a segment in dBFS (−∞ for silence).
fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return f32::NEG_INFINITY;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    10.0 * mean_sq.log10()
}

/// Text with punctuation, symbols and whitespace removed — what's left is
/// what the speaker actually said (CJK ideographs count as alphanumeric).
fn content_of(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Filters junk utterances before they reach the LLM: a recogniser can
/// return e.g. "没。" for a quiet segment, and VAD can emit the same short
/// phrase twice at a boundary.
#[derive(Default)]
struct UtteranceFilter {
    last_content: String,
}

impl UtteranceFilter {
    /// Cheap pre-check on audio, before spending ASR time.
    fn audio_ok(samples: &[f32]) -> bool {
        rms_dbfs(samples) >= MIN_SEGMENT_DBFS
    }

    /// Accept the recognised text? Rejects ≤ 1 content character and an exact
    /// repeat of the previous accepted utterance.
    fn accept(&mut self, text: &str) -> bool {
        let content = content_of(text);
        if content.chars().count() <= 1 || content == self.last_content {
            return false;
        }
        self.last_content = content;
        true
    }
}

fn asr_worker(
    cfg: &SessionConfig,
    audio_rx: Receiver<Vec<u8>>,
    queue: &UtteranceQueue,
    sink: &EventSink,
    cancel: &AtomicBool,
) {
    status(sink, "loading", Some("Đang nạp nhận dạng (X-ASR)…".into()));
    let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 4) as i32;
    let asr = match Asr::load(&cfg.asr, cfg.glossary.iter().map(|(src, _)| src.as_str()), threads) {
        Ok(a) => a,
        Err(e) => {
            sink(LocalEvent::Error { code: "asr_load".into(), message: e });
            return;
        }
    };
    let vad_cfg = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(cfg.vad_model.to_string_lossy().into_owned()),
            threshold: 0.5,
            // Utterance boundaries: a 350 ms pause ends a sentence; past 8 s
            // VAD accepts a shorter pause (the hard cut is `Cutter`, below).
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
        sink(LocalEvent::Error {
            code: "vad_load".into(),
            message: format!("Silero VAD failed to load from {}", cfg.vad_model.display()),
        });
        return;
    };
    status(sink, "asr_ready", None);

    let mut filter = UtteranceFilter::default();
    let mut history = History::new();
    let mut utterance: Vec<f32> = Vec::with_capacity(SAMPLE_RATE as usize * 9);
    let drain = |vad: &VoiceActivityDetector, history: &History, utterance: &mut Vec<f32>, filter: &mut UtteranceFilter| {
        while let Some(seg) = vad.front() {
            let samples = seg.samples();
            let text = if UtteranceFilter::audio_ok(samples) {
                // Pre-roll from our own history, then the segment itself.
                let start = seg.start().max(0) as u64;
                utterance.clear();
                history.copy_range(start.saturating_sub(PRE_ROLL_SAMPLES as u64), start, utterance);
                utterance.extend_from_slice(samples);
                asr.transcribe(utterance)
            } else {
                String::new()
            };
            vad.pop();
            if text.is_empty() || !filter.accept(&text) {
                continue;
            }
            let dropped = queue.push(text);
            if dropped > 0 {
                status(sink, "backlog_skipped", Some(dropped.to_string()));
            }
        }
    };

    let mut samples: Vec<f32> = Vec::with_capacity(8192);
    let mut cutter = Cutter::default();
    for pcm in audio_rx {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        samples.clear();
        samples.reserve(pcm.len() / 2);
        for i in 0..pcm.len() / 2 {
            samples.push(i16::from_le_bytes([pcm[2 * i], pcm[2 * i + 1]]) as f32 / 32768.0);
        }
        history.push(&samples);
        vad.accept_waveform(&samples);
        if vad.detected() {
            if cutter.update(history.end, rms_dbfs(&samples)) {
                // Ends the current segment where it stands; VAD keeps running.
                vad.flush();
                cutter.reset();
            }
        } else {
            cutter.reset();
        }
        drain(&vad, &history, &mut utterance, &mut filter);
    }
    if !cancel.load(Ordering::SeqCst) {
        vad.flush();
        drain(&vad, &history, &mut utterance, &mut filter);
    }
}

fn llm_worker(
    cfg: &SessionConfig,
    make_translator: TranslatorFactory,
    queue: &UtteranceQueue,
    sink: &EventSink,
    cancel: &AtomicBool,
) {
    // First load on Apple Silicon compiles ggml's Metal kernels (~15 s once).
    let loading = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "Đang khởi tạo Metal (lần đầu ~15 s)…"
    } else {
        "Đang nạp model dịch…"
    };
    status(sink, "loading", Some(loading.into()));
    let translator = match make_translator() {
        Ok(t) => t,
        Err(e) => {
            sink(LocalEvent::Error { code: "llm_load".into(), message: e });
            // No translator → stop the ASR side instead of transcribing for nothing.
            cancel.store(true, Ordering::SeqCst);
            return;
        }
    };
    status(sink, "ready", None);

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
        match translator.translate(&req, cancel) {
            Ok(tgt) if !tgt.is_empty() => sink(LocalEvent::Result { src, tgt }),
            Ok(_) => {}
            Err(e) => sink(LocalEvent::Error { code: "translate".into(), message: e }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutter_soft_and_hard_caps() {
        let s = SAMPLE_RATE as u64;
        // Under 8 s: never, however quiet the chunk.
        assert!(!should_cut(7 * s, -80.0, -20.0));
        // 8–12 s: only at a chunk ≥ 15 dB under the utterance's peak.
        assert!(!should_cut(9 * s, -30.0, -20.0));
        assert!(should_cut(9 * s, -35.0, -20.0));
        // 12 s: always.
        assert!(should_cut(12 * s, -20.0, -20.0));
        // Stateful wrapper: start is the first speech chunk, peak tracks the loudest.
        let mut c = Cutter::default();
        assert!(!c.update(3200, -20.0)); // speech starts at 3200
        assert!(!c.update(3200 + 9 * s, -25.0)); // 9 s in, not quiet enough
        assert!(c.update(3200 + 10 * s, -36.0)); // quiet chunk → cut
        c.reset();
        assert!(!c.update(3200 + 11 * s, -36.0), "after a cut the count restarts");
    }

    #[test]
    fn rms_levels() {
        assert_eq!(rms_dbfs(&[]), f32::NEG_INFINITY);
        assert_eq!(rms_dbfs(&[0.0; 1600]), f32::NEG_INFINITY);
        // Full-scale square wave = 0 dBFS; 0.001 ≈ −60 dBFS.
        assert!(rms_dbfs(&[1.0, -1.0, 1.0, -1.0]).abs() < 1e-3);
        assert!((rms_dbfs(&[0.001; 1600]) + 60.0).abs() < 0.1);
        assert!(!UtteranceFilter::audio_ok(&[0.001; 1600]));
        assert!(UtteranceFilter::audio_ok(&[0.05; 1600]));
    }

    #[test]
    fn filter_rejects_junk_and_repeats() {
        let mut f = UtteranceFilter::default();
        assert!(!f.accept("没。"), "one content char after punctuation strip");
        assert!(!f.accept("。，"));
        assert!(!f.accept(" a "));
        assert!(f.accept("资产负债表。"));
        assert!(!f.accept("资产负债表！"), "same content as previous");
        assert!(f.accept("市盈率很高。"));
        assert!(f.accept("资产负债表。"), "only the immediately previous one is deduplicated");
        assert!(f.accept("OK"), "two latin chars pass");
    }

    #[test]
    fn history_ring_ranges() {
        let mut h = History::new();
        let chunk: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        h.push(&chunk);
        let mut out = Vec::new();
        h.copy_range(990, 1000, &mut out);
        assert_eq!(out, (990..1000).map(|i| i as f32).collect::<Vec<_>>());
        out.clear();
        h.copy_range(0, 5, &mut out);
        assert_eq!(out, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        // Overflow the ring: the oldest samples are gone, ranges clamp.
        for _ in 0..HISTORY_SAMPLES / 1000 + 1 {
            h.push(&chunk);
        }
        assert_eq!(h.buf.len(), HISTORY_SAMPLES);
        out.clear();
        h.copy_range(0, 10, &mut out);
        assert!(out.is_empty(), "long-gone range yields nothing");
        out.clear();
        h.copy_range(h.end - 3, h.end + 100, &mut out);
        assert_eq!(out, vec![997.0, 998.0, 999.0]);
    }

    #[test]
    fn queue_drops_oldest_and_keeps_newest() {
        let q = UtteranceQueue::new();
        let mut dropped = 0;
        for i in 0..10 {
            dropped += q.push(format!("s{i}"));
        }
        assert_eq!(dropped, 10 - UTTERANCE_QUEUE_MAX);
        q.finish();
        let rest: Vec<String> = std::iter::from_fn(|| q.pop()).collect();
        assert_eq!(rest, vec!["s6", "s7", "s8", "s9"]);
    }
}
