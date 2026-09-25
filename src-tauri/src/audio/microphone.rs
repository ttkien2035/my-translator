//! Microphone capture via cpal. The audio callback only forwards raw frames
//! to the processing thread (see `mic_pipeline`); all DSP happens off the
//! real-time thread. Stopping drops the stream, which closes the raw queue,
//! which ends the processing thread and frees its models.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;

use super::mic_pipeline::{self, MicOptions};

/// Raw-frame queue depth. Callbacks arrive every ~10 ms; 64 entries is ~0.6 s
/// of slack before frames are dropped (the callback never blocks).
const RAW_QUEUE_DEPTH: usize = 64;

/// Microphone capture: default input device → PCM s16le 16 kHz mono.
pub struct MicCapture {
    /// Held to keep the stream alive; dropping it stops capture.
    stream: Option<cpal::Stream>,
}

// SAFETY: MicCapture is only accessed through the Mutex in AudioState, and
// the cpal::Stream is created and dropped on the same (main) thread via sync
// Tauri commands. cpal::Stream is !Send only because some backends require
// same-thread use; we never move it across threads.
unsafe impl Send for MicCapture {}

impl MicCapture {
    pub fn new() -> Self {
        Self { stream: None }
    }

    /// Start capturing. Returns a receiver of processed s16le 16 kHz chunks.
    pub fn start(&mut self, opts: MicOptions) -> Result<mpsc::Receiver<Vec<u8>>, String> {
        if self.stream.is_some() {
            return Err("Already capturing".to_string());
        }

        let host = cpal::default_host();
        let input_devices: Vec<String> = host
            .input_devices()
            .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default();
        println!("[Mic] Available input devices: {:?}", input_devices);
        if input_devices.is_empty() {
            return Err(
                "No microphone found. Connect an external microphone or headset.".to_string(),
            );
        }

        let device = host
            .default_input_device()
            .ok_or("No default microphone found. Connect an external microphone or headset.")?;
        println!("[Mic] Device: {:?}", device.name().unwrap_or_default());

        // Default config first; otherwise the first supported config at 48 kHz.
        let default_config = device
            .default_input_config()
            .or_else(|e| {
                println!("[Mic] default_input_config failed: {e}, trying supported configs");
                let mut configs = device
                    .supported_input_configs()
                    .map_err(|e2| format!("No supported input configs: {e2}"))?;
                configs
                    .find(|c| c.sample_format() == cpal::SampleFormat::F32)
                    .or_else(|| {
                        device
                            .supported_input_configs()
                            .ok()
                            .and_then(|mut c| c.next())
                    })
                    .map(|c| {
                        let rate =
                            if c.min_sample_rate().0 <= 48000 && c.max_sample_rate().0 >= 48000 {
                                cpal::SampleRate(48000)
                            } else {
                                c.max_sample_rate()
                            };
                        c.with_sample_rate(rate)
                    })
                    .ok_or_else(|| format!("No suitable input config found (original: {e})"))
            })
            .map_err(|e| format!("Failed to get default input config: {e}"))?;

        let source_rate = default_config.sample_rate().0;
        let channels = default_config.channels() as usize;
        println!(
            "[Mic] Config: rate={source_rate}, channels={channels}, format={:?}, opts={opts:?}",
            default_config.sample_format()
        );

        // Raw frames: callback → DSP thread (bounded). Processed PCM: DSP → forwarder.
        let (raw_tx, raw_rx) = mpsc::sync_channel::<Vec<f32>>(RAW_QUEUE_DEPTH);
        let (pcm_tx, pcm_rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("mic-dsp".into())
            .spawn(move || mic_pipeline::run(raw_rx, pcm_tx, source_rate, channels, opts))
            .map_err(|e| format!("Failed to spawn mic DSP thread: {e}"))?;

        let stream_config = cpal::StreamConfig {
            channels: default_config.channels(),
            sample_rate: default_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let err_fn = |err| eprintln!("[Mic] input error: {err}");

        let stream = match default_config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| forward(&raw_tx, data.to_vec()),
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    forward(&raw_tx, data.iter().map(|&s| s as f32 / 32768.0).collect())
                },
                err_fn,
                None,
            ),
            format => return Err(format!("Unsupported sample format: {format:?}")),
        }
        .map_err(|e| format!("Failed to build input stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start mic stream: {e}"))?;
        self.stream = Some(stream);
        Ok(pcm_rx)
    }

    /// Stop capturing: dropping the stream closes the raw queue, which ends
    /// the DSP thread and, through its dropped sender, the forwarder.
    pub fn stop(&mut self) {
        self.stream = None;
    }
}

/// Real-time callback side: never block. A full queue means the DSP thread is
/// behind; dropping this callback's frames is the only safe option.
fn forward(tx: &mpsc::SyncSender<Vec<f32>>, frames: Vec<f32>) {
    if let Err(mpsc::TrySendError::Full(_)) = tx.try_send(frames) {
        // dropped
    }
}

impl Default for MicCapture {
    fn default() -> Self {
        Self::new()
    }
}
