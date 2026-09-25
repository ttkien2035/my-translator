//! Speech recognition for the Local engine: SenseVoice-small (int8) through
//! sherpa-onnx. Offline (non-streaming) model — we feed it one VAD utterance
//! at a time, which keeps latency at "sentence end + ~0.3 s".

use std::path::Path;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};

pub struct Asr {
    recognizer: OfflineRecognizer,
}

impl Asr {
    /// `language`: "auto" | "zh" | "en" | "ja" | "ko" | "yue".
    pub fn load(model: &Path, tokens: &Path, language: &str, threads: i32) -> Result<Self, String> {
        let language = match language {
            "zh" | "en" | "ja" | "ko" | "yue" => language,
            _ => "auto",
        };
        let mut cfg = OfflineRecognizerConfig::default();
        cfg.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model.to_string_lossy().into_owned()),
            language: Some(language.to_string()),
            // Inverse text normalisation: "二零二四" → "2024", useful for figures.
            use_itn: true,
        };
        cfg.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
        cfg.model_config.num_threads = threads.max(1);
        cfg.model_config.provider = Some("cpu".to_string());
        cfg.model_config.debug = false;
        cfg.decoding_method = Some("greedy_search".to_string());

        let recognizer = OfflineRecognizer::create(&cfg)
            .ok_or_else(|| format!("SenseVoice failed to load from {}", model.display()))?;
        Ok(Self { recognizer })
    }

    /// Transcribe one utterance (16 kHz mono f32). Empty string when nothing
    /// was recognised.
    pub fn transcribe(&self, samples_16k: &[f32]) -> String {
        if samples_16k.is_empty() {
            return String::new();
        }
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16_000, samples_16k);
        self.recognizer.decode(&stream);
        stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal RIFF reader for the 16 kHz mono s16le test wavs shipped with SenseVoice.
    fn read_wav_s16_mono(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("read wav");
        let mut pos = 12;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
            if id == b"data" {
                let data = &bytes[pos + 8..(pos + 8 + size).min(bytes.len())];
                return (0..data.len() / 2)
                    .map(|i| i16::from_le_bytes([data[2 * i], data[2 * i + 1]]) as f32 / 32768.0)
                    .collect();
            }
            pos += 8 + size + (size & 1);
        }
        panic!("no data chunk");
    }

    /// Without gdb on this box: turn a C-side abort into a printed Rust backtrace.
    fn install_abort_backtrace() {
        extern "C" fn on_abort(_sig: libc::c_int) {
            let bt = std::backtrace::Backtrace::force_capture();
            eprintln!("=== SIGABRT backtrace ===\n{bt}");
            unsafe { libc::_exit(134) };
        }
        unsafe {
            libc::signal(libc::SIGABRT, on_abort as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }

    /// A missing model must surface as an error, never a crash (no assets needed).
    #[test]
    fn bad_path_is_an_error() {
        let r = Asr::load(Path::new("/nonexistent/model.onnx"), Path::new("/nonexistent/tokens.txt"), "zh", 1);
        assert!(r.is_err());
    }

    /// Bisect helper: model load + drop only.
    #[test]
    #[ignore]
    fn loads_only() {
        install_abort_backtrace();
        let Ok(dir) = std::env::var("MT_TEST_SENSEVOICE_DIR") else { return };
        let dir = Path::new(&dir);
        let asr = Asr::load(&dir.join("model.int8.onnx"), &dir.join("tokens.txt"), "zh", 2).expect("load");
        eprintln!("loaded OK");
        drop(asr);
        eprintln!("dropped OK");
    }

    /// Bisect helper: load + empty stream decode (no audio).
    #[test]
    #[ignore]
    fn stream_roundtrip_silence() {
        let Ok(dir) = std::env::var("MT_TEST_SENSEVOICE_DIR") else { return };
        let dir = Path::new(&dir);
        let asr = Asr::load(&dir.join("model.int8.onnx"), &dir.join("tokens.txt"), "zh", 2).expect("load");
        let silence = vec![0f32; 16000];
        let text = asr.transcribe(&silence);
        eprintln!("silence → {text:?}");
    }

    /// Runtime check against the real model: `MT_TEST_SENSEVOICE_DIR` must
    /// contain model.int8.onnx, tokens.txt and test_wavs/zh.wav.
    #[test]
    #[ignore]
    fn transcribes_chinese_sample() {
        install_abort_backtrace();
        let Ok(dir) = std::env::var("MT_TEST_SENSEVOICE_DIR") else {
            eprintln!("MT_TEST_SENSEVOICE_DIR not set; skipping");
            return;
        };
        let dir = Path::new(&dir);
        let asr = Asr::load(&dir.join("model.int8.onnx"), &dir.join("tokens.txt"), "zh", 2).expect("load");
        let samples = read_wav_s16_mono(&dir.join("test_wavs/zh.wav"));
        let t = std::time::Instant::now();
        let text = asr.transcribe(&samples);
        eprintln!("ASR ({:?} for {:.1}s audio): {text}", t.elapsed(), samples.len() as f32 / 16000.0);
        assert!(!text.is_empty());
        assert!(text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)), "expected Chinese characters");
    }
}
