pub mod mic_pipeline;
pub mod microphone;
pub mod resampler;

#[cfg(target_os = "macos")]
pub mod mic_vpio;
#[cfg(target_os = "macos")]
pub mod system_audio;

#[cfg(target_os = "windows")]
pub mod wasapi;

#[cfg(target_os = "linux")]
pub mod system_audio_linux;

// Re-export SystemAudioCapture from the correct platform module
#[cfg(target_os = "macos")]
pub use system_audio::SystemAudioCapture;

#[cfg(target_os = "windows")]
pub use wasapi::SystemAudioCapture;

#[cfg(target_os = "linux")]
pub use system_audio_linux::SystemAudioCapture;

/// Target audio format for every engine: PCM s16le, 16 kHz, mono.
pub const TARGET_SAMPLE_RATE: u32 = 16000;
