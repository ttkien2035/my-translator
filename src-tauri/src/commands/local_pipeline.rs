//! Local (offline) translation pipeline: a Python sidecar (Whisper + LLM on
//! MLX) fed raw 16 kHz s16le audio over stdin, answering JSON lines on stdout.
//!
//! Hot path is `send_audio_to_pipeline` (5×/s): it must never block the
//! caller, so audio goes through a bounded queue to a dedicated stdin writer
//! thread. Start/stop are async so their waits happen off the main thread.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::ipc::{Channel, InvokeBody, Request};

/// stdin queue depth: 200 ms chunks → ~10 s of backlog before chunks are dropped.
const STDIN_QUEUE_CHUNKS: usize = 50;
/// After stdin is closed, how long Python gets to flush and exit before SIGKILL.
const STOP_GRACE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct LocalPipelineState {
    pub process: Mutex<Option<Child>>,
    /// Feeds the stdin writer thread; `None` while no pipeline is running.
    stdin_tx: Mutex<Option<SyncSender<Vec<u8>>>>,
}

/// `~/Library/Application Support/My Translator` — venv, models and logs.
fn app_support_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/My Translator")
}

fn venv_python() -> PathBuf {
    app_support_dir().join("mlx-env/bin/python3")
}

fn system_python() -> &'static str {
    if Path::new("/opt/homebrew/bin/python3").exists() {
        "/opt/homebrew/bin/python3"
    } else {
        "python3"
    }
}

/// Append one line to the pipeline log in app support (lifecycle + errors only;
/// transcript content is never written here — see the stdout reader).
fn log_to_file(msg: &str) {
    use std::fs::OpenOptions;
    let dir = app_support_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("local_pipeline.log"))
        .and_then(|mut f| {
            writeln!(
                f,
                "[{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                msg
            )
        });
    eprintln!("[local-pipeline] {}", msg);
}

/// Properly escaped `{"type":"status","message":…}` line for the frontend.
fn status_json(message: &str) -> String {
    serde_json::json!({ "type": "status", "message": message }).to_string()
}

/// Locate a bundled script: dev tree first, then the app bundle's Resources.
fn find_script(name: &str) -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../scripts")
            .join(name),
        PathBuf::from("scripts").join(name),
        exe_dir.join("../Resources/scripts").join(name),
    ];
    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| format!("{} not found. Ensure scripts/{} is bundled.", name, name))
}

/// Start the local translation pipeline (Python sidecar).
#[tauri::command]
pub async fn start_local_pipeline(
    source_lang: String,
    target_lang: String,
    channel: Channel<String>,
    state: tauri::State<'_, LocalPipelineState>,
) -> Result<(), String> {
    log_to_file(&format!("start: src={} tgt={}", source_lang, target_lang));
    let _ = channel.send(status_json("Stopping old pipeline..."));
    stop_pipeline(&state).await;

    let script_path = find_script("local_pipeline.py")?;

    // Reap an orphan left by a crashed earlier run of THIS install — match the
    // exact script path so other installs (dev vs. bundled) are untouched.
    {
        let pattern = script_path.to_string_lossy().into_owned();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            Command::new("pkill").args(["-f", &pattern]).output()
        })
        .await;
    }

    let _ = channel.send(status_json("Starting Python pipeline..."));
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let venv = venv_python();
    let python = if venv.exists() {
        venv
    } else {
        PathBuf::from(system_python())
    };
    log_to_file(&format!(
        "script={} python={}",
        script_path.display(),
        python.display()
    ));

    let mut child = Command::new(&python)
        .arg(&script_path)
        .arg("--asr-model")
        .arg("whisper")
        .arg("--source-lang")
        .arg(&source_lang)
        .arg("--target-lang")
        .arg(&target_lang)
        .env("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
        .env("HOME", &home)
        .env("TOKENIZERS_PARALLELISM", "false")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let msg = format!("Failed to start pipeline: {}", e);
            log_to_file(&msg);
            msg
        })?;

    let pid = child.id();
    log_to_file(&format!("spawned PID={}", pid));
    let _ = channel.send(status_json(&format!(
        "Python started (PID={}), loading models...",
        pid
    )));

    let stdin = child.stdin.take().ok_or("Failed to get stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to get stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to get stderr")?;

    // stdin writer: the only thread that touches the pipe. Dropping the sender
    // (stop) ends the loop, which closes stdin → Python sees EOF and exits.
    let (tx, rx) = sync_channel::<Vec<u8>>(STDIN_QUEUE_CHUNKS);
    std::thread::spawn(move || {
        let mut stdin = stdin;
        for chunk in rx {
            if let Err(e) = stdin.write_all(&chunk) {
                log_to_file(&format!("stdin write error: {}", e));
                break;
            }
        }
        log_to_file("stdin writer ended");
    });

    // stdout → frontend verbatim (JSON lines). Result lines carry transcript
    // text, so only the other kinds are logged.
    let ch = channel.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines() {
            match line {
                Ok(line) if line.is_empty() => {}
                Ok(line) => {
                    if !line.contains(r#""type":"result""#) && !line.contains(r#""type": "result""#) {
                        log_to_file(&format!("stdout: {}", line));
                    }
                    let _ = ch.send(line);
                }
                Err(e) => {
                    log_to_file(&format!("stdout error: {}", e));
                    break;
                }
            }
        }
        log_to_file("stdout reader ended");
    });

    // stderr (model loading progress, warnings) → log + status event.
    let ch = channel.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr).lines() {
            match line {
                Ok(line) => {
                    log_to_file(&format!("stderr: {}", line));
                    let _ = ch.send(status_json(&line));
                }
                Err(_) => break,
            }
        }
        log_to_file("stderr reader ended");
    });

    *state.stdin_tx.lock().map_err(|e| e.to_string())? = Some(tx);
    *state.process.lock().map_err(|e| e.to_string())? = Some(child);
    Ok(())
}

/// Hot path: raw PCM invoke body → bounded queue. Never blocks; when Python
/// falls behind, the newest chunk is dropped rather than growing memory.
#[tauri::command]
pub fn send_audio_to_pipeline(
    request: Request<'_>,
    state: tauri::State<'_, LocalPipelineState>,
) -> Result<(), String> {
    let pcm = match request.body() {
        InvokeBody::Raw(bytes) => bytes,
        InvokeBody::Json(_) => return Err("expected raw PCM body".into()),
    };
    let guard = state.stdin_tx.lock().map_err(|e| e.to_string())?;
    let Some(tx) = guard.as_ref() else {
        return Err("pipeline not running".into());
    };
    match tx.try_send(pcm.clone()) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => {
            eprintln!("[local-pipeline] stdin queue full — dropping chunk");
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => Err("pipeline stdin closed".into()),
    }
}

/// Stop the local pipeline.
#[tauri::command]
pub async fn stop_local_pipeline(
    state: tauri::State<'_, LocalPipelineState>,
) -> Result<(), String> {
    log_to_file("stop called");
    stop_pipeline(&state).await;
    Ok(())
}

/// Close stdin (EOF lets Python flush its last window), wait up to STOP_GRACE
/// off the main thread, then kill whatever is still running.
async fn stop_pipeline(state: &LocalPipelineState) {
    if let Ok(mut tx) = state.stdin_tx.lock() {
        tx.take();
    }
    let child = match state.process.lock() {
        Ok(mut p) => p.take(),
        Err(_) => None,
    };
    let Some(mut child) = child else {
        return;
    };
    let pid = child.id();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let deadline = Instant::now() + STOP_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    log_to_file(&format!("PID={} exited: {}", pid, status));
                    break;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    log_to_file(&format!("PID={} killed", pid));
                    break;
                }
            }
        }
    })
    .await;
}

/// Check if MLX setup is complete.
#[tauri::command]
pub fn check_mlx_setup() -> Result<String, String> {
    let marker = app_support_dir().join("mlx-env/.setup_complete");
    let python = venv_python();
    if marker.exists() && python.exists() {
        // The marker is JSON written by setup_mlx.py; pass it through as an
        // object when it parses, otherwise as a string (always valid JSON).
        let content = std::fs::read_to_string(&marker).unwrap_or_default();
        let details = serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or(serde_json::Value::String(content));
        Ok(serde_json::json!({
            "ready": true,
            "python": python.to_string_lossy(),
            "details": details,
        })
        .to_string())
    } else {
        Ok(r#"{"ready":false}"#.to_string())
    }
}

/// Run MLX setup (install venv + packages + download models).
#[tauri::command]
pub fn run_mlx_setup(channel: Channel<String>) -> Result<(), String> {
    log_to_file("run_mlx_setup called");
    let script_path = find_script("setup_mlx.py")?;

    // Setup runs with the system python (it creates the venv itself).
    let mut child = Command::new(system_python())
        .arg(&script_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start setup: {}", e))?;
    log_to_file(&format!("setup spawned PID={}", child.id()));

    let stdout = child.stdout.take().ok_or("Failed to get stdout")?;
    let ch = channel.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines() {
            match line {
                Ok(line) if line.is_empty() => {}
                Ok(line) => {
                    log_to_file(&format!("setup stdout: {}", line));
                    let _ = ch.send(line);
                }
                Err(e) => {
                    log_to_file(&format!("setup stdout error: {}", e));
                    break;
                }
            }
        }
    });

    let stderr = child.stderr.take().ok_or("Failed to get stderr")?;
    let ch = channel.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr).lines() {
            match line {
                Ok(line) => {
                    log_to_file(&format!("setup stderr: {}", line));
                    let _ = ch
                        .send(serde_json::json!({ "type": "log", "message": line }).to_string());
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}
