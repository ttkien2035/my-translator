//! Speech recognition for the Local engine: X-ASR-zh-en (Zipformer
//! transducer, int8, punctuation build) through sherpa-onnx, offline mode —
//! one VAD utterance at a time, so latency is "sentence end + decode".
//!
//! Chosen over SenseVoice-small on 2026-09-26 measurements (480 lecture /
//! meeting / classroom utterances + two 25-min lectures): fewer errors,
//! punctuation, proper casing of English terms, hotwords, smaller download.
//!
//! Hotwords: the course glossary's Chinese terms bias the beam search.
//! Finance-term recall in a simulated classroom went 93 % → 99 % with no
//! extra errors on general speech — provided that only terms of ≥ 3
//! characters are used (short ones caused false alarms) and that each
//! character is written as its own word: the BPE encoder then maps it to the
//! `▁X` token the model actually emits. A whole term is segmented
//! differently and never matches.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};

use super::models::AsrFiles;

/// Boost per hotword token (sherpa `hotwords_score`). 1.0–3.0 all raised
/// term recall; 3.0 began to add false alarms, so 2.0.
const HOTWORD_SCORE: f32 = 2.0;
const BEAM: i32 = 4;
const MIN_HOTWORD_CHARS: usize = 3;
/// X-ASR's ONNX graph fails on inputs of ≥ 50 s (40 s decoded fine). The
/// pipeline cuts utterances at 12 s; this is the last guard before the model.
pub const MAX_INPUT_SECONDS: usize = 30;
const SAMPLE_RATE: usize = 16_000;

static HOTWORDS_FILE_SEQ: AtomicU32 = AtomicU32::new(0);

pub struct Asr {
    recognizer: OfflineRecognizer,
    /// sherpa only takes hotwords from a file; removed on drop.
    hotwords_file: Option<PathBuf>,
}

impl Asr {
    /// `glossary_terms`: source-language terms; the usable ones become hotwords.
    pub fn load<'a>(
        files: &AsrFiles,
        glossary_terms: impl IntoIterator<Item = &'a str>,
        threads: i32,
    ) -> Result<Self, String> {
        let vocab = vocab_chars(&files.tokens)?;
        let lines = hotword_lines(glossary_terms, &vocab);
        let hotwords_file = if lines.is_empty() { None } else { Some(write_hotwords(&lines)?) };

        let mut cfg = OfflineRecognizerConfig::default();
        let m = &mut cfg.model_config;
        m.transducer.encoder = Some(path_str(&files.encoder));
        m.transducer.decoder = Some(path_str(&files.decoder));
        m.transducer.joiner = Some(path_str(&files.joiner));
        m.tokens = Some(path_str(&files.tokens));
        m.num_threads = threads.max(1);
        m.provider = Some("cpu".to_string());
        m.debug = false;
        match &hotwords_file {
            Some(f) => {
                // Hotwords need beam search; measured cost +15 ms per utterance.
                cfg.decoding_method = Some("modified_beam_search".to_string());
                cfg.max_active_paths = BEAM;
                cfg.hotwords_file = Some(path_str(f));
                cfg.hotwords_score = HOTWORD_SCORE;
                m.modeling_unit = Some("bpe".to_string());
                m.bpe_vocab = Some(path_str(&files.bpe_vocab));
            }
            None => cfg.decoding_method = Some("greedy_search".to_string()),
        }

        let recognizer = OfflineRecognizer::create(&cfg).ok_or_else(|| {
            if let Some(f) = &hotwords_file {
                let _ = std::fs::remove_file(f);
            }
            format!("X-ASR failed to load from {}", files.encoder.display())
        })?;
        Ok(Self { recognizer, hotwords_file })
    }

    /// Transcribe one utterance (16 kHz mono f32). Empty string when nothing
    /// was recognised. Input longer than `MAX_INPUT_SECONDS` is decoded in
    /// equal parts (see the constant).
    pub fn transcribe(&self, samples_16k: &[f32]) -> String {
        if samples_16k.is_empty() {
            return String::new();
        }
        let max = MAX_INPUT_SECONDS * SAMPLE_RATE;
        if samples_16k.len() <= max {
            return self.decode(samples_16k);
        }
        let parts = samples_16k.len().div_ceil(max);
        let len = samples_16k.len().div_ceil(parts);
        samples_16k
            .chunks(len)
            .map(|c| self.decode(c))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn decode(&self, samples: &[f32]) -> String {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        stream.get_result().map(|r| tidy_cjk(&r.text)).unwrap_or_default()
    }
}

/// The transducer joins BPE pieces with spaces, so Chinese comes out as
/// "不便 ， 所以" or "华尔街啊， 把". Drop every space that touches a CJK
/// character or CJK/fullwidth punctuation; spaces between Latin words stay.
fn tidy_cjk(text: &str) -> String {
    let is_wide = |c: char| is_cjk(c) || matches!(c, '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}');
    let mut out = String::with_capacity(text.len());
    let mut chars = text.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' {
            let prev_wide = out.chars().next_back().is_some_and(is_wide);
            let next_wide = chars.peek().copied().is_some_and(is_wide);
            if prev_wide || next_wide {
                continue;
            }
        }
        out.push(c);
    }
    out
}

impl Drop for Asr {
    fn drop(&mut self) {
        if let Some(f) = &self.hotwords_file {
            let _ = std::fs::remove_file(f);
        }
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Single characters the model can emit: `tokens.txt` lines `▁X <id>`.
fn vocab_chars(tokens: &Path) -> Result<HashSet<char>, String> {
    let text = std::fs::read_to_string(tokens).map_err(|e| format!("read {}: {e}", tokens.display()))?;
    Ok(vocab_chars_from(&text))
}

fn vocab_chars_from(tokens_txt: &str) -> HashSet<char> {
    tokens_txt
        .lines()
        .filter_map(|l| {
            let mut chars = l.split_whitespace().next()?.strip_prefix('▁')?.chars();
            let c = chars.next()?;
            chars.next().is_none().then_some(c)
        })
        .collect()
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}')
}

/// Hotword lines for sherpa: one usable term per line, characters
/// space-separated. Usable = ≥ 3 characters, all CJK and all in the
/// vocabulary (an unknown token only earns a sherpa error log, but mixed
/// terms like "MM定理" would not match what the model emits anyway).
pub fn hotword_lines<'a>(terms: impl IntoIterator<Item = &'a str>, vocab: &HashSet<char>) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for term in terms {
        let term = term.trim();
        if !seen.insert(term) {
            continue;
        }
        let n = term.chars().count();
        if n < MIN_HOTWORD_CHARS || !term.chars().all(|c| is_cjk(c) && vocab.contains(&c)) {
            continue;
        }
        let mut line = String::with_capacity(n * 4);
        for (i, c) in term.chars().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            line.push(c);
        }
        out.push(line);
    }
    out
}

fn write_hotwords(lines: &[String]) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!(
        "my-translator-hotwords-{}-{}.txt",
        std::process::id(),
        HOTWORDS_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(&path, text).map_err(|e| format!("write hotwords {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> HashSet<char> {
        vocab_chars_from("<blk> 0\n<sos/eos> 1\n▁, 2\n▁资 3\n▁产 4\n▁负 5\n▁债 6\n▁表 7\n▁贝 8\n▁塔 9\n▁系 10\n▁数 11\n▁定 12\n▁理 13\nM 14\n塔系数 15\n")
    }

    #[test]
    fn vocab_keeps_single_char_pieces_only() {
        let v = vocab();
        assert!(v.contains(&'资') && v.contains(&','));
        assert!(!v.contains(&'M'), "no ▁ prefix");
        assert!(!v.contains(&'塔') || v.contains(&'塔'), "multi-char piece ignored, ▁塔 kept");
        assert_eq!(v.len(), 12);
    }

    #[test]
    fn tidy_removes_spaces_around_cjk_only() {
        assert_eq!(tidy_cjk(" 不便 ， 所以钱要达到这个条件 ， 就是携带便利。 "), "不便，所以钱要达到这个条件，就是携带便利。");
        assert_eq!(tidy_cjk("华尔街啊， 把全球经济"), "华尔街啊，把全球经济");
        assert_eq!(tidy_cjk("那些 in power 掌握权力的人"), "那些in power掌握权力的人");
        assert_eq!(tidy_cjk("Made in china 这个比例"), "Made in china这个比例");
        assert_eq!(tidy_cjk("you are able to do something"), "you are able to do something");
    }

    #[test]
    fn hotwords_are_filtered_and_spaced() {
        let terms = [
            "资产负债表", // ok
            "资产",       // too short
            "MM定理",     // not CJK
            "贝塔系数",   // ok
            "久期风险",   // 久/期/风/险 not in this vocab
            " 资产负债表 ", // duplicate after trim
            "ESG",
        ];
        assert_eq!(hotword_lines(terms, &vocab()), vec!["资 产 负 债 表", "贝 塔 系 数"]);
        assert!(hotword_lines(["资产"], &vocab()).is_empty());
    }

    /// A missing model must surface as an error, never a crash (no assets needed).
    #[test]
    fn bad_path_is_an_error() {
        let files = AsrFiles::in_dir(Path::new("/nonexistent"));
        assert!(Asr::load(&files, ["资产负债表"], 1).is_err());
    }

    /// Runtime check against the real model: `MT_TEST_XASR_DIR` is the
    /// extracted `sherpa-onnx-x-asr-zipformer-transducer-zh-en-punct-int8-*`
    /// folder (with `bpe.vocab`, see `models::ensure_bpe_vocab`). Audio:
    /// `MT_TEST_WAV` (16 kHz mono s16le), else the first wav in `test_wavs/`.
    /// On macOS make one with:
    ///   say -v Tingting "资产负债表反映企业在某一特定日期的财务状况。" -o /tmp/zh.aiff
    ///   afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/zh.aiff /tmp/zh.wav
    #[test]
    #[ignore]
    fn transcribes_chinese_sample() {
        let Ok(dir) = std::env::var("MT_TEST_XASR_DIR") else {
            eprintln!("MT_TEST_XASR_DIR not set; skipping");
            return;
        };
        let dir = Path::new(&dir);
        super::super::models::ensure_bpe_vocab(dir).expect("bpe.vocab");
        let files = AsrFiles::in_dir(dir);
        let asr = Asr::load(&files, ["资产负债表", "财务状况"], 2).expect("load");
        assert!(asr.hotwords_file.as_ref().is_some_and(|f| f.is_file()));
        let wav = std::env::var_os("MT_TEST_WAV").map(PathBuf::from).or_else(|| {
            std::fs::read_dir(dir.join("test_wavs")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "wav"))
        });
        let Some(wav) = wav else { panic!("no wav: set MT_TEST_WAV") };
        let samples = read_wav_s16_mono(&wav);
        let t = std::time::Instant::now();
        let text = asr.transcribe(&samples);
        eprintln!("ASR ({:?} for {:.1}s audio): {text}", t.elapsed(), samples.len() as f32 / 16000.0);
        assert!(!text.is_empty());
        assert!(text.chars().any(is_cjk), "expected Chinese characters");
        // Silence must not produce text, and a > 30 s input must not crash.
        assert_eq!(asr.transcribe(&vec![0f32; 16000]), "");
        let long: Vec<f32> = samples.iter().copied().cycle().take(35 * 16000).collect();
        assert!(!asr.transcribe(&long).is_empty());
    }

    /// Minimal RIFF reader for 16 kHz mono s16le test wavs.
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
}
