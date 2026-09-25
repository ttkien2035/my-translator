pub mod audio;
pub mod edge_tts;
pub mod google_free_tts;
pub mod http_client;
pub mod local_pipeline;
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
