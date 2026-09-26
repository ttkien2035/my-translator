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

/// Per-user app data dir, the same folder as settings and sessions:
/// `~/Library/Application Support/<APP_ID>` on macOS, `%APPDATA%\<APP_ID>` on
/// Windows, `~/.local/share/<APP_ID>` on Linux. Holds downloaded models.
/// Open the OS privacy page for `pane` ("microphone" | "screen"), so a
/// "permission denied" dialog can take the user straight there. Only these
/// fixed URLs are ever opened.
#[tauri::command]
pub fn open_privacy_settings(pane: String) -> Result<(), String> {
    let url = privacy_url(&pane).ok_or_else(|| format!("unknown privacy pane {pane}"))?;
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(url).spawn().map(|_| ()).map_err(|e| format!("open {url}: {e}"))
}

fn privacy_url(pane: &str) -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        match pane {
            "microphone" => Some("ms-settings:privacy-microphone"),
            _ => None,
        }
    } else {
        match pane {
            "microphone" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"),
            "screen" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"),
            _ => None,
        }
    }
}

pub(crate) fn app_support_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(crate::APP_ID)
}
