//! Microphone processing chain. Runs on a dedicated thread so the cpal
//! callback stays real-time safe: the callback only hands raw frames over a
//! bounded queue and never touches DSP, models or IPC.
//!
//! raw frames (device rate, N ch) → mono → resample 16 kHz → high-pass
//!   → [GTCRN denoiser] → 32 ms windows: [Silero VAD] → [AGC] → gate → s16le
//!
//! Memory: every scratch buffer is allocated once in `MicProcessor::new` and
//! reused; the only per-callback allocations are the two `Vec`s that cross
//! thread boundaries (raw frames in, s16le out), both bounded by queue depth.
//! When capture stops the sender side of `rx` is dropped, `run` returns, and
//! the processor — models included — is freed with it.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use sherpa_onnx::{
    OfflineSpeechDenoiserGtcrnModelConfig, OfflineSpeechDenoiserModelConfig, OnlineSpeechDenoiser,
    OnlineSpeechDenoiserConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use super::TARGET_SAMPLE_RATE;

/// Silero VAD (v5) is trained on 32 ms windows at 16 kHz; AGC uses the same grid.
const WINDOW: usize = 512;
/// Windows kept before speech onset so the first syllable isn't clipped (256 ms).
const PRE_ROLL_WINDOWS: usize = 8;
/// Digital silence appended once speech ends, so the recognizer still gets an
/// endpoint cue although non-speech audio itself is not sent (~192 ms).
const TAIL_SILENCE_WINDOWS: usize = 6;

/// Per-session microphone options, resolved from settings in `start_capture`.
#[derive(Clone, Debug, Default)]
pub struct MicOptions {
    /// macOS only: capture through Apple's Voice-Processing I/O unit (system
    /// echo cancellation, noise suppression and AGC). Ignored elsewhere.
    pub voice_processing: bool,
    /// 1st-order high-pass at 80 Hz: removes rumble/handling noise.
    pub highpass: bool,
    /// Software automatic gain control (lifts a distant lecturer to a steady level).
    pub agc: bool,
    /// Path to `gtcrn_simple.onnx`; `None` disables denoising.
    pub denoise_model: Option<PathBuf>,
    /// Path to `silero_vad.onnx`; `None` disables the VAD gate.
    pub vad_model: Option<PathBuf>,
}

/// Thread body: consume raw device frames until the stream is dropped.
pub fn run(
    rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<Vec<u8>>,
    source_rate: u32,
    channels: usize,
    opts: MicOptions,
) {
    let mut proc = match MicProcessor::new(source_rate, channels, &opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[mic] pipeline init failed: {e}");
            return;
        }
    };
    let mut out: Vec<u8> = Vec::with_capacity(OUT_CAP);
    for frames in rx {
        if let Err(e) = proc.process(&frames, &mut out) {
            eprintln!("[mic] {e}");
            out.clear();
            continue;
        }
        if out.is_empty() {
            continue;
        }
        let batch = std::mem::replace(&mut out, Vec::with_capacity(OUT_CAP));
        if tx.send(batch).is_err() {
            break; // forwarder gone — capture was stopped
        }
    }
}

/// Typical output per callback is ≤ 40 ms of 16 kHz s16le (1280 B); 4 KiB
/// leaves headroom for pre-roll + tail silence bursts without regrowth.
const OUT_CAP: usize = 4096;

pub struct MicProcessor {
    channels: usize,
    /// Scratch: mixed-down input at device rate.
    mono: Vec<f32>,
    /// Device-rate samples waiting for a full resampler chunk.
    pending: Vec<f32>,
    /// `None` when the device already runs at 16 kHz.
    resampler: Option<Resample16k>,
    /// Scratch: 16 kHz samples produced by this call.
    buf16: Vec<f32>,
    highpass: Option<HighPass>,
    denoiser: Option<OnlineSpeechDenoiser>,
    /// 16 kHz samples waiting for a full 32 ms window.
    win_pending: Vec<f32>,
    vad: Option<VadGate>,
    agc: Option<Agc>,
}

impl MicProcessor {
    pub fn new(source_rate: u32, channels: usize, opts: &MicOptions) -> Result<Self, String> {
        let channels = channels.max(1);
        let resampler = if source_rate == TARGET_SAMPLE_RATE {
            None
        } else {
            Some(Resample16k::new(source_rate)?)
        };
        let denoiser = opts.denoise_model.as_ref().and_then(|path| {
            let cfg = OnlineSpeechDenoiserConfig {
                model: OfflineSpeechDenoiserModelConfig {
                    gtcrn: OfflineSpeechDenoiserGtcrnModelConfig {
                        model: Some(path.to_string_lossy().into_owned()),
                    },
                    num_threads: 1,
                    ..Default::default()
                },
            };
            match OnlineSpeechDenoiser::create(&cfg) {
                Some(d) if d.sample_rate() == TARGET_SAMPLE_RATE as i32 => Some(d),
                Some(d) => {
                    eprintln!(
                        "[mic] denoiser expects {} Hz, pipeline is {} Hz — disabled",
                        d.sample_rate(),
                        TARGET_SAMPLE_RATE
                    );
                    None
                }
                None => {
                    eprintln!("[mic] denoiser model failed to load: {}", path.display());
                    None
                }
            }
        });
        let vad = opts.vad_model.as_ref().and_then(|path| {
            let cfg = VadModelConfig {
                silero_vad: SileroVadModelConfig {
                    model: Some(path.to_string_lossy().into_owned()),
                    threshold: 0.5,
                    // Hangover: speech stays "on" this long after the last voiced window.
                    min_silence_duration: 0.5,
                    min_speech_duration: 0.1,
                    window_size: WINDOW as i32,
                    max_speech_duration: 20.0,
                },
                sample_rate: TARGET_SAMPLE_RATE as i32,
                num_threads: 1,
                provider: Some("cpu".to_string()),
                debug: false,
                ..Default::default()
            };
            // 10 s ring buffer inside the detector; segments are cleared as they
            // appear, so this is the upper bound on its memory.
            let vad = VoiceActivityDetector::create(&cfg, 10.0);
            if vad.is_none() {
                eprintln!("[mic] VAD model failed to load: {}", path.display());
            }
            vad.map(VadGate::new)
        });

        // Scratch sizes: a cpal callback is ~10–20 ms; 50 ms of headroom.
        let frames_per_callback = (source_rate as usize / 20).max(WINDOW);
        Ok(Self {
            channels,
            mono: Vec::with_capacity(frames_per_callback),
            pending: Vec::with_capacity(frames_per_callback * 2),
            resampler,
            buf16: Vec::with_capacity(frames_per_callback),
            highpass: opts.highpass.then(|| HighPass::new(TARGET_SAMPLE_RATE, 80.0)),
            denoiser,
            win_pending: Vec::with_capacity(frames_per_callback + WINDOW),
            vad,
            agc: opts.agc.then(Agc::new),
        })
    }

    /// Process one callback's interleaved frames; appends s16le 16 kHz to `out`.
    pub fn process(&mut self, frames: &[f32], out: &mut Vec<u8>) -> Result<(), String> {
        // 1. Mono mixdown at device rate.
        self.mono.clear();
        if self.channels == 1 {
            self.mono.extend_from_slice(frames);
        } else {
            let inv = 1.0 / self.channels as f32;
            self.mono.extend(
                frames
                    .chunks_exact(self.channels)
                    .map(|frame| frame.iter().sum::<f32>() * inv),
            );
        }

        // 2. To 16 kHz.
        self.buf16.clear();
        match &mut self.resampler {
            Some(r) => {
                self.pending.extend_from_slice(&self.mono);
                r.process(&mut self.pending, &mut self.buf16)?;
            }
            None => self.buf16.extend_from_slice(&self.mono),
        }
        if self.buf16.is_empty() {
            return Ok(());
        }

        // 3. High-pass (cheapest at 16 kHz, after decimation).
        if let Some(hp) = &mut self.highpass {
            hp.process(&mut self.buf16);
        }

        // 4. Denoise. sherpa returns an owned buffer per call; it is freed here.
        if let Some(d) = &self.denoiser {
            let den = d.run(&self.buf16, TARGET_SAMPLE_RATE as i32);
            self.buf16.clear();
            self.buf16.extend_from_slice(&den.samples);
        }

        // 5. 32 ms windows: VAD decision → AGC → gate → s16le.
        self.win_pending.extend_from_slice(&self.buf16);
        let mut consumed = 0;
        while self.win_pending.len() - consumed >= WINDOW {
            let window = &mut self.win_pending[consumed..consumed + WINDOW];
            let speech = match &mut self.vad {
                Some(g) => g.detect(window),
                None => true,
            };
            if let Some(a) = &mut self.agc {
                a.process(window, speech);
            }
            match &mut self.vad {
                Some(g) => g.gate(window, speech, out),
                None => push_s16le(window, out),
            }
            consumed += WINDOW;
        }
        self.win_pending.drain(..consumed);
        Ok(())
    }
}

/// Anti-aliased device-rate → 16 kHz conversion with state carried across
/// calls (no phase reset at callback boundaries).
struct Resample16k {
    inner: SincFixedIn<f32>,
    chunk: usize,
    in_buf: Vec<Vec<f32>>,
    out_buf: Vec<Vec<f32>>,
}

impl Resample16k {
    fn new(source_rate: u32) -> Result<Self, String> {
        // 10 ms of input per chunk keeps added latency at ~10 ms.
        let chunk = (source_rate as usize / 100).max(160);
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.92,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let inner = SincFixedIn::<f32>::new(
            TARGET_SAMPLE_RATE as f64 / source_rate as f64,
            1.0,
            params,
            chunk,
            1,
        )
        .map_err(|e| format!("resampler init ({source_rate} Hz → 16 kHz): {e}"))?;
        let in_buf = inner.input_buffer_allocate(true);
        let out_buf = inner.output_buffer_allocate(true);
        Ok(Self {
            inner,
            chunk,
            in_buf,
            out_buf,
        })
    }

    /// Consume whole chunks from `pending`, appending 16 kHz samples to `out`.
    fn process(&mut self, pending: &mut Vec<f32>, out: &mut Vec<f32>) -> Result<(), String> {
        let mut consumed = 0;
        while pending.len() - consumed >= self.chunk {
            self.in_buf[0].copy_from_slice(&pending[consumed..consumed + self.chunk]);
            let (_, produced) = self
                .inner
                .process_into_buffer(&self.in_buf, &mut self.out_buf, None)
                .map_err(|e| format!("resample: {e}"))?;
            out.extend_from_slice(&self.out_buf[0][..produced]);
            consumed += self.chunk;
        }
        pending.drain(..consumed);
        Ok(())
    }
}

/// 1st-order IIR high-pass (DC/rumble removal).
struct HighPass {
    alpha: f32,
    prev_in: f32,
    prev_out: f32,
}

impl HighPass {
    fn new(sample_rate: u32, cutoff_hz: f32) -> Self {
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        let dt = 1.0 / sample_rate as f32;
        Self {
            alpha: rc / (rc + dt),
            prev_in: 0.0,
            prev_out: 0.0,
        }
    }

    fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let out = self.alpha * (self.prev_out + *s - self.prev_in);
            self.prev_in = *s;
            self.prev_out = out;
            *s = out;
        }
    }
}

/// RMS-based automatic gain control on 32 ms windows. Only adapts while the
/// VAD reports speech (or, without VAD, while the window is above the noise
/// floor), so silence is never amplified into hiss.
struct Agc {
    gain: f32,
}

impl Agc {
    const TARGET_RMS: f32 = 0.1; // −20 dBFS
    const MAX_GAIN: f32 = 8.0; // +18 dB
    const NOISE_FLOOR_RMS: f32 = 0.003; // −50 dBFS
    const ATTACK: f32 = 0.5; // per window, when gain must drop (fast)
    const RELEASE: f32 = 0.05; // per window, when gain may rise (slow)

    fn new() -> Self {
        Self { gain: 1.0 }
    }

    fn process(&mut self, window: &mut [f32], speech: bool) {
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
        if speech && rms > Self::NOISE_FLOOR_RMS {
            let desired = (Self::TARGET_RMS / rms).clamp(1.0, Self::MAX_GAIN);
            let k = if desired < self.gain {
                Self::ATTACK
            } else {
                Self::RELEASE
            };
            self.gain += (desired - self.gain) * k;
        }
        if (self.gain - 1.0).abs() > 1e-3 {
            for s in window.iter_mut() {
                *s = (*s * self.gain).clamp(-1.0, 1.0);
            }
        }
    }
}

/// Speech gate: forwards audio only inside speech (plus pre-roll and a short
/// silent tail). Non-speech windows are kept in a fixed-size ring for pre-roll.
struct VadGate {
    vad: VoiceActivityDetector,
    pre_roll: VecDeque<[f32; WINDOW]>,
    in_speech: bool,
}

impl VadGate {
    fn new(vad: VoiceActivityDetector) -> Self {
        Self {
            vad,
            pre_roll: VecDeque::with_capacity(PRE_ROLL_WINDOWS),
            in_speech: false,
        }
    }

    /// Feed one window; true while inside a speech segment (hangover included).
    fn detect(&mut self, window: &[f32]) -> bool {
        self.vad.accept_waveform(window);
        let speech = self.vad.detected();
        // Segments are a by-product we never read; keep the queue empty so it
        // can't grow over a long lecture.
        if !self.vad.is_empty() {
            self.vad.clear();
        }
        speech
    }

    fn gate(&mut self, window: &[f32], speech: bool, out: &mut Vec<u8>) {
        if speech {
            if !self.in_speech {
                self.in_speech = true;
                for w in self.pre_roll.drain(..) {
                    push_s16le(&w, out);
                }
            }
            push_s16le(window, out);
        } else {
            if self.in_speech {
                self.in_speech = false;
                out.resize(out.len() + TAIL_SILENCE_WINDOWS * WINDOW * 2, 0);
            }
            if self.pre_roll.len() == PRE_ROLL_WINDOWS {
                self.pre_roll.pop_front();
            }
            let mut w = [0f32; WINDOW];
            w.copy_from_slice(window);
            self.pre_roll.push_back(w);
        }
    }
}

fn push_s16le(samples: &[f32], out: &mut Vec<u8>) {
    out.reserve(samples.len() * 2);
    for &s in samples {
        let s16 = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&s16.to_le_bytes());
    }
}
