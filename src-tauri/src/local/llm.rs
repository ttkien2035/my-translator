//! Translation LLM for the Local engine: a GGUF instruct model (default
//! Qwen2.5-3B-Instruct Q4_K_M) through llama.cpp — Metal on Apple Silicon,
//! CPU elsewhere. One model per session; a fresh context per sentence, which
//! is cheap and keeps sentences independent (no KV cache to manage).

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::TokenToStringError;

/// Context window per translation: system prompt + glossary hits + sentence + output.
const N_CTX: u32 = 2048;
const MAX_NEW_TOKENS: usize = 256;
/// Glossary entries injected per sentence (only those whose source term occurs in it).
const MAX_GLOSSARY_HITS: usize = 12;

/// llama.cpp's global backend: initialised once per process, never torn down.
static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

fn backend() -> Result<&'static LlamaBackend, String> {
    BACKEND
        .get_or_init(|| {
            LlamaBackend::init()
                .map(|mut b| {
                    b.void_logs(); // llama.cpp is chatty; errors surface through Results
                    b
                })
                .map_err(|e| format!("llama backend init: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub struct TranslateRequest<'a> {
    pub text: &'a str,
    /// Language names for the prompt ("Chinese", "Vietnamese").
    pub source_lang: &'a str,
    pub target_lang: &'a str,
    pub glossary: &'a [(String, String)],
}

pub struct Llm {
    model: LlamaModel,
    template: LlamaChatTemplate,
    threads: i32,
}

impl Llm {
    pub fn load(path: &Path, threads: i32) -> Result<Self, String> {
        let backend = backend()?;
        // All layers on the GPU where Metal is compiled in; ignored on CPU builds.
        let gpu_layers = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            999
        } else {
            0
        };
        let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
        let model = LlamaModel::load_from_file(backend, path, &params)
            .map_err(|e| format!("load {}: {e}", path.display()))?;
        let template = model
            .chat_template(None)
            .map_err(|e| format!("model has no chat template: {e}"))?;
        Ok(Self {
            model,
            template,
            threads: threads.max(1),
        })
    }

    /// Translate one sentence. `cancel` aborts generation between tokens
    /// (returns an empty string) so a stop never waits for a full sentence.
    ///
    /// Greedy decoding only: measured on Qwen2.5-3B, "never copy source
    /// characters" instructions and correction turns made output *worse*
    /// and doubled latency, so a rare untranslated fragment is accepted. A
    /// larger GGUF (Settings › Model › Local) is the real fix.
    pub fn translate(&self, req: &TranslateRequest, cancel: &AtomicBool) -> Result<String, String> {
        let system = self.system_prompt(req);
        let messages = [("system", system.as_str()), ("user", req.text)];
        self.generate(&messages, LlamaSampler::greedy(), cancel)
    }

    /// One chat completion over `(role, content)` messages through the model's template.
    fn generate(
        &self,
        messages: &[(&str, &str)],
        mut sampler: LlamaSampler,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        let backend = backend()?;
        let messages = messages
            .iter()
            .map(|(role, content)| LlamaChatMessage::new(role.to_string(), content.to_string()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("message: {e}"))?;
        let prompt = self
            .model
            .apply_chat_template(&self.template, &messages, true)
            .map_err(|e| format!("apply chat template: {e}"))?;
        let tokens = self
            .model
            .str_to_token(&prompt, AddBos::Never) // the template carries its own special tokens
            .map_err(|e| format!("tokenize: {e}"))?;
        if tokens.is_empty() {
            return Ok(String::new());
        }
        let max_prompt = (N_CTX as usize).saturating_sub(MAX_NEW_TOKENS + 8);
        if tokens.len() > max_prompt {
            return Err(format!("sentence too long ({} tokens)", tokens.len()));
        }

        // The whole prompt goes in one batch.
        let n_batch = tokens.len().max(512);
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(n_batch as u32)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut ctx = self
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| format!("context: {e}"))?;

        let mut batch = LlamaBatch::new(n_batch, 1);
        let last = tokens.len() - 1;
        for (i, tok) in tokens.iter().enumerate() {
            batch
                .add(*tok, i as i32, &[0], i == last)
                .map_err(|e| format!("batch: {e}"))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| format!("prompt decode: {e}"))?;

        let mut out: Vec<u8> = Vec::with_capacity(512);
        // `pos` is the KV position of each newly generated token.
        for pos in (tokens.len() as i32..).take(MAX_NEW_TOKENS) {
            if cancel.load(Ordering::SeqCst) {
                return Ok(String::new());
            }
            let tok = sampler.sample(&ctx, -1);
            sampler.accept(tok);
            if self.model.is_eog_token(tok) {
                break;
            }
            // Bytes, not strings: a UTF-8 character (Chinese, Vietnamese
            // diacritics) can span several tokens.
            out.extend(self.piece_bytes(tok)?);
            batch.clear();
            batch
                .add(tok, pos, &[0], true)
                .map_err(|e| format!("batch: {e}"))?;
            ctx.decode(&mut batch).map_err(|e| format!("decode: {e}"))?;
        }
        Ok(clean_output(&String::from_utf8_lossy(&out)))
    }

    fn piece_bytes(&self, tok: LlamaToken) -> Result<Vec<u8>, String> {
        match self.model.token_to_piece_bytes(tok, 16, false, None) {
            Ok(b) => Ok(b),
            Err(TokenToStringError::InsufficientBufferSpace(n)) => self
                .model
                .token_to_piece_bytes(tok, usize::try_from(-n).unwrap_or(64).max(1), false, None)
                .map_err(|e| format!("token piece: {e}")),
            Err(e) => Err(format!("token piece: {e}")),
        }
    }

    /// System prompt: role, languages, and the glossary entries found in the sentence.
    fn system_prompt(&self, req: &TranslateRequest) -> String {
        let hits: Vec<String> = req
            .glossary
            .iter()
            .filter(|(src, _)| req.text.contains(src.as_str()))
            .take(MAX_GLOSSARY_HITS)
            .map(|(src, tgt)| format!("{src} → {tgt}"))
            .collect();

        let mut system = format!(
            "You are a professional interpreter for university lectures on finance, economics and accounting. \
             Translate the user's {src} sentence into {tgt}. Output only the translation — no explanations, \
             no quotes, no notes. Use precise, standard academic terminology in {tgt}; keep numbers, \
             formulas and proper names exact. If the sentence is a fragment, translate it as a fragment.",
            src = req.source_lang,
            tgt = req.target_lang,
        );
        if !hits.is_empty() {
            system.push_str("\nGlossary (must be followed):");
            for h in hits {
                system.push('\n');
                system.push_str(&h);
            }
        }
        system
    }
}

/// Strip wrapping quotes / a stray "Translation:" label some models add.
fn clean_output(s: &str) -> String {
    let mut t = s.trim();
    for prefix in ["Translation:", "Bản dịch:", "翻译："] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim();
        }
    }
    let t = t.trim_matches(|c| c == '"' || c == '“' || c == '”' || c == '「' || c == '」');
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runtime check against the real GGUF (`MT_TEST_GGUF`): exercises
    /// tokenize → batch → decode → greedy sampling → byte assembly end to end.
    #[test]
    #[ignore]
    fn translates_finance_sentence() {
        let Ok(path) = std::env::var("MT_TEST_GGUF") else {
            eprintln!("MT_TEST_GGUF not set; skipping");
            return;
        };
        let t = std::time::Instant::now();
        let llm = Llm::load(Path::new(&path), 8).expect("load");
        eprintln!("LLM loaded in {:?}", t.elapsed());
        let glossary = vec![
            ("资产负债表".to_string(), "bảng cân đối kế toán".to_string()),
            ("市盈率".to_string(), "hệ số giá trên lợi nhuận (P/E)".to_string()),
        ];
        let cancel = AtomicBool::new(false);
        for text in ["资产负债表反映企业在某一特定日期的财务状况。", "这个公式期末考试会考，大家注意一下。"] {
            let t = std::time::Instant::now();
            let out = llm
                .translate(
                    &TranslateRequest { text, source_lang: "Chinese", target_lang: "Vietnamese", glossary: &glossary },
                    &cancel,
                )
                .expect("translate");
            eprintln!("{text}\n → {out}   ({:?})", t.elapsed());
            assert!(!out.is_empty());
        }
    }
}
