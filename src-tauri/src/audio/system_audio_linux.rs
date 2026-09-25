//! Linux placeholder for system-audio capture. The app targets macOS
//! (ScreenCaptureKit) and Windows (WASAPI); this stub keeps the crate
//! type-checking on Linux (`cargo check` in WSL/CI) without pretending to
//! capture anything. Microphone capture via cpal still works here.

use std::sync::mpsc;

pub struct SystemAudioCapture;

impl SystemAudioCapture {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self) -> Result<mpsc::Receiver<Vec<u8>>, String> {
        Err("System audio capture is not supported on Linux — use the microphone source".to_string())
    }

    pub fn stop(&self) {}
}

impl Default for SystemAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}
