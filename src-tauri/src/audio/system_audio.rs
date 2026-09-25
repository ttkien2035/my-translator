use screencapturekit::prelude::*;
use std::sync::mpsc;
use std::sync::Mutex;

use super::TARGET_SAMPLE_RATE;

/// ScreenCaptureKit is asked for 48 kHz; we decimate by this factor to 16 kHz.
const SCK_SAMPLE_RATE: u32 = 48_000;
const DECIMATION: usize = (SCK_SAMPLE_RATE / TARGET_SAMPLE_RATE) as usize; // 3

/// Audio handler that receives CMSampleBuffer callbacks from ScreenCaptureKit
/// and sends PCM data through a channel.
struct AudioHandler {
    sender: mpsc::Sender<Vec<u8>>,
}

impl SCStreamOutputTrait for AudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
        match output_type {
            SCStreamOutputType::Audio => {
                if let Some(audio_buffer_list) = sample.audio_buffer_list() {
                    // ScreenCaptureKit with stereo config may deliver audio as:
                    // - 2 separate mono buffers (deinterleaved L/R), OR
                    // - 1 interleaved stereo buffer
                    // We only need ONE channel for speech, so take just the first buffer
                    let mut iter = audio_buffer_list.into_iter();
                    if let Some(audio_buffer) = iter.next() {
                        let raw_data = audio_buffer.data();

                        if raw_data.is_empty() {
                            return;
                        }

                        // Bytes are native-endian f32 (first channel only). Read
                        // them safely — no alignment assumption — and in one pass
                        // decimate 48 kHz → 16 kHz and convert to s16le, into a
                        // single pre-sized buffer (one allocation per callback).
                        let sample_count = raw_data.len() / 4;
                        let mut pcm_s16 =
                            Vec::with_capacity((sample_count / DECIMATION + 1) * 2);
                        for i in (0..sample_count).step_by(DECIMATION) {
                            let b = &raw_data[i * 4..i * 4 + 4];
                            let sample = f32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
                            let s16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                            pcm_s16.extend_from_slice(&s16.to_le_bytes());
                        }

                        if !pcm_s16.is_empty() {
                            let _ = self.sender.send(pcm_s16);
                        }
                    }
                }
            }
            _ => {
                // Ignore video frames
            }
        }
    }
}

/// System audio capture using ScreenCaptureKit
/// Captures all system audio output and converts to PCM s16le 16kHz mono.
pub struct SystemAudioCapture {
    /// Held while a stream runs. Each start gets its own sender; the thread
    /// that owns the `SCStream` blocks on the matching receiver and stops the
    /// stream the moment the sender is dropped. No polling, and a thread from
    /// a previous start can never be confused by the next start's state.
    stop_handle: Mutex<Option<mpsc::Sender<()>>>,
}

impl SystemAudioCapture {
    pub fn new() -> Self {
        Self {
            stop_handle: Mutex::new(None),
        }
    }

    /// Start capturing system audio.
    /// Returns a receiver that yields PCM s16le 16kHz mono audio chunks.
    pub fn start(&self) -> Result<mpsc::Receiver<Vec<u8>>, String> {
        if self.is_capturing() {
            return Err("Already capturing".to_string());
        }

        // Get available displays
        let content = SCShareableContent::get().map_err(|e| {
            format!(
                "Failed to get shareable content (Screen Recording permission needed): {}",
                e
            )
        })?;

        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or("No displays found".to_string())?;

        // Create content filter for the main display
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        // Configure: audio only, 48kHz stereo (ScreenCaptureKit native rate)
        // Downsampling to 16kHz mono happens in AudioHandler
        let config = SCStreamConfiguration::new()
            .with_width(2) // minimal video (required by API)
            .with_height(2)
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true) // Prevent TTS audio feedback loop
            .with_sample_rate(SCK_SAMPLE_RATE)
            .with_channel_count(2);

        // Create channel for audio data
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();

        let handler = AudioHandler { sender };

        // Create and start the stream
        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(handler, SCStreamOutputType::Audio);

        stream
            .start_capture()
            .map_err(|e| format!("Failed to start system audio capture: {}", e))?;

        // The stream lives on its own thread, which parks on `stop_rx` and
        // stops the stream as soon as `stop()` drops the sender.
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _ = stop_rx.recv(); // Err(Disconnected) once the sender is dropped
            let _ = stream.stop_capture();
        });
        *self.lock_handle() = Some(stop_tx);

        Ok(receiver)
    }

    /// Stop capturing. Dropping the sender wakes the stream thread immediately.
    pub fn stop(&self) {
        self.lock_handle().take();
    }

    pub fn is_capturing(&self) -> bool {
        self.lock_handle().is_some()
    }

    fn lock_handle(&self) -> std::sync::MutexGuard<'_, Option<mpsc::Sender<()>>> {
        // A poisoned lock only means a thread panicked while holding it; the
        // Option inside is still valid.
        self.stop_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for SystemAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}
