pub mod audio;
pub mod audio_models;
pub mod download;
pub mod edge_tts;
pub mod google_free_tts;
pub mod http_client;
pub mod local_tts;
pub mod microsoft_tts;
pub mod tiktok_tts;
pub mod openai_realtime;
pub mod qwen_realtime;
pub mod session_store;
pub mod settings;
pub mod transcript;

/// Session id carried in the `x-session-id` header of raw-body audio invokes
/// (`openai_realtime_send_audio`, `qwen_realtime_send_audio`), so the PCM
/// itself travels as bytes instead of a JSON number array.
pub(crate) fn session_id_from_headers(headers: &http::HeaderMap) -> Result<u64, String> {
    headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| "missing or invalid x-session-id header".to_string())
}

/// Per-user app data dir: `~/Library/Application Support/My Translator` on
/// macOS, `%APPDATA%\My Translator` on Windows, `~/.local/share/My Translator`
/// on Linux. Holds the MLX venv, downloaded models and logs.
pub(crate) fn app_support_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        // The pre-rename name, kept on purpose: downloaded models live here and
        // renaming the app (now MeowLaoshi) must not orphan them.
        .join("My Translator")
}
