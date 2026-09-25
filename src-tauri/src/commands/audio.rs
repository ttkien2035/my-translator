use super::audio_models;
use crate::audio::mic_pipeline::MicOptions;
use crate::audio::microphone::MicCapture;
use crate::audio::SystemAudioCapture;
use crate::settings::SettingsState;
use serde::Serialize;
use std::sync::mpsc;
use std::sync::Mutex;
use tauri::{
    ipc::{Channel, InvokeResponseBody},
    State,
};

/// State for tracking active audio captures
pub struct AudioState {
    pub system_audio: Mutex<SystemAudioCapture>,
    pub microphone: Mutex<MicCapture>,
    pub active_receiver: Mutex<Option<AudioForwarder>>,
}

/// Forwards audio from a receiver to a Tauri IPC channel
pub struct AudioForwarder {
    /// Handle to signal stop
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AudioForwarder {
    fn stop(&self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Serialize, Clone)]
pub struct PermissionStatus {
    pub screen_recording: String,
    pub microphone: String,
}

/// Start audio capture and forward data to the frontend via IPC channel
#[tauri::command]
pub fn start_capture(
    source: String,
    channel: Channel<InvokeResponseBody>,
    state: State<'_, AudioState>,
    settings: State<'_, SettingsState>,
) -> Result<(), String> {
    // Stop any existing capture first
    stop_capture_inner(&state);

    let receiver: mpsc::Receiver<Vec<u8>> = match source.as_str() {
        "system" => {
            let sys = state.system_audio.lock().map_err(|e| e.to_string())?;
            sys.start()?
        }
        "microphone" => {
            let mut mic = state.microphone.lock().map_err(|e| e.to_string())?;
            mic.start(mic_options(&settings))?
        }
        "both" => {
            // Start both sources and merge into a single receiver
            let sys = state.system_audio.lock().map_err(|e| e.to_string())?;
            let sys_rx = sys.start()?;
            let mut mic = state.microphone.lock().map_err(|e| e.to_string())?;
            let mic_rx = mic.start(mic_options(&settings))?;

            let (merged_tx, merged_rx) = mpsc::channel::<Vec<u8>>();
            let tx1 = merged_tx.clone();
            let tx2 = merged_tx;

            // Forward system audio to merged channel
            std::thread::spawn(move || {
                while let Ok(data) = sys_rx.recv() {
                    if tx1.send(data).is_err() {
                        break;
                    }
                }
            });
            // Forward mic audio to merged channel
            std::thread::spawn(move || {
                while let Ok(data) = mic_rx.recv() {
                    if tx2.send(data).is_err() {
                        break;
                    }
                }
            });

            merged_rx
        }
        _ => return Err(format!("Unknown source: {}", source)),
    };

    // Spawn a thread to forward audio data from receiver to IPC channel
    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    std::thread::spawn(move || {
        // 200 ms of 16 kHz s16le mono is 6400 B; headroom so a batch never grows.
        const BATCH_CAP: usize = 8192;
        let batch_interval = std::time::Duration::from_millis(200);
        let mut buffer: Vec<u8> = Vec::with_capacity(BATCH_CAP);
        let mut last_flush = std::time::Instant::now();

        // Hand the batch to the webview as raw bytes (not a JSON number array),
        // swapping in a fresh buffer instead of cloning. False once the channel
        // is closed.
        let flush = |buffer: &mut Vec<u8>| -> bool {
            if buffer.is_empty() {
                return true;
            }
            let batch = std::mem::replace(buffer, Vec::with_capacity(BATCH_CAP));
            channel.send(InvokeResponseBody::Raw(batch)).is_ok()
        };

        loop {
            if stop_flag_clone.load(std::sync::atomic::Ordering::SeqCst) {
                flush(&mut buffer);
                break;
            }

            match receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(data) => buffer.extend_from_slice(&data),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    flush(&mut buffer);
                    break;
                }
            }

            if last_flush.elapsed() >= batch_interval && !buffer.is_empty() {
                if !flush(&mut buffer) {
                    break; // webview channel closed
                }
                last_flush = std::time::Instant::now();
            }
        }
    });

    // Store the forwarder so we can stop it later
    let forwarder = AudioForwarder { stop_flag };
    let mut active = state.active_receiver.lock().map_err(|e| e.to_string())?;
    *active = Some(forwarder);

    Ok(())
}

/// Resolve the microphone chain from settings. Model stages are only enabled
/// when their file is actually installed, so a missing download degrades to
/// plain capture instead of failing to start.
fn mic_options(settings: &SettingsState) -> MicOptions {
    let s = settings
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    MicOptions {
        highpass: s.mic_highpass,
        agc: s.mic_agc,
        denoise_model: s
            .mic_denoise
            .then(|| audio_models::installed_path(audio_models::GTCRN_ID))
            .flatten(),
        vad_model: s
            .mic_vad
            .then(|| audio_models::installed_path(audio_models::SILERO_VAD_ID))
            .flatten(),
    }
}

/// Stop audio capture
#[tauri::command]
pub fn stop_capture(state: State<'_, AudioState>) -> Result<(), String> {
    stop_capture_inner(&state);
    Ok(())
}

fn stop_capture_inner(state: &AudioState) {
    // Stop the forwarder
    if let Ok(mut active) = state.active_receiver.lock() {
        if let Some(forwarder) = active.take() {
            forwarder.stop();
        }
    }

    // Stop system audio
    if let Ok(sys) = state.system_audio.lock() {
        sys.stop();
    }

    // Stop microphone
    if let Ok(mut mic) = state.microphone.lock() {
        mic.stop();
    }
}

/// Check audio capture permissions
#[tauri::command]
pub fn check_permissions() -> PermissionStatus {
    // Note: Actual permission checking on macOS requires Objective-C interop
    // For now, we return "unknown" and permissions will be prompted on first use
    PermissionStatus {
        screen_recording: "unknown".to_string(),
        microphone: "unknown".to_string(),
    }
}
