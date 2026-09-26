# My Translator — Lecture Edition

**English** · [Tiếng Việt](README.vi.md)

A **real-time** speech translation app for macOS and Windows, tuned for **listening to lectures in Chinese and taking notes in Vietnamese** (finance and economics). Forked from [phuc-nt/my-translator](https://github.com/phuc-nt/my-translator) (MIT). It keeps the Tauri + Rust architecture and adds course profiles with term glossaries, a notes pane with hotkeys, a classroom microphone chain, and a **pure-Rust** offline engine (no Python).

> The app's interface is in Vietnamese. Menu names below are given as they appear in the app, with a translation in parentheses.

---

## Contents

1. [Features](#features)
2. [Install (macOS)](#install-macos)
3. [First-time setup](#first-time-setup)
4. [In class — suggested workflow](#in-class--suggested-workflow)
5. [Keyboard shortcuts](#keyboard-shortcuts)
6. [Build from source](#build-from-source)
7. [Free packaging for friends](#free-packaging-for-friends)
8. [Where your data lives](#where-your-data-lives)
9. [Troubleshooting](#troubleshooting)
10. [Architecture](#architecture)
11. [Repository layout](#repository-layout)

---

## Features

### Four translation engines, switchable from the toolbar

| Engine | Runs | Latency | Cost | Notes |
|---|---|---|---|---|
| ☁️ **Soniox** (recommended) | cloud | ~2 s | ~$0.12/hour | 70+ source languages; uses the course profile's **glossary** and context |
| 🌏 **Qwen LiveTranslate** | cloud (Alibaba) | ~4 s | free (preview) | reachable from mainland China without a VPN; text only |
| ⚡ **OpenAI Realtime** | cloud | ~2 s | ~$4/hour | translated voice output; needs a VPN in China |
| 🖥️ **Local** (offline) | on device, pure Rust | ~2–3 s after each sentence | free | SenseVoice (speech recognition) + Tencent Hy-MT2-1.8B (dedicated translation model, Metal on Apple Silicon); the course glossary goes into the prompt |

Each engine's model name can be changed in **Cài đặt › Model** (Settings › Model), including a custom GGUF for Local. The same screen holds a **helper LLM** slot (DeepSeek / Qwen DashScope / Zhipu GLM / OpenAI / any OpenAI-compatible API) for the upcoming academic re-translation and summary features.

### For the classroom

- **Course profiles.** Each subject has its own domain, recognition terms, background context and a **source → target glossary**. A built-in list of **278 Chinese–English–Vietnamese finance terms** loads with one click. It covers financial statements, ratios, corporate finance, investments, derivatives, banking and monetary policy, macroeconomics, econometrics, tax and governance, and common classroom phrases. Switching profile **mid-lecture** takes effect immediately.
- **Notes pane** under the transcript (`⌘⇧N`), saved with the session and exported to Markdown.
  - `⌘⇧C` copies the latest sentence into the notes, with its time and source text.
  - `⌘⇧1/2/3` mark the latest sentence ⭐ important / ❓ unclear / 📝 on the exam. Marks show in the transcript and are collected under *Đánh dấu* (Marks) on export.
- **Automatic 📝** when the lecturer says *会考 / 考点 / 期末考 / 必考…* ("this will be on the exam").
- **The live transcript scrolls back through the whole lecture** (up to ~1 500 sentences on screen). Scrolling up to reread doesn't get pulled back down; at the bottom it follows new text.
- **Library study view.** Every session saves itself (Markdown + JSON, every 15 s and on stop/quit). Open one to:
  - reread every sentence (translation, source, time);
  - **add ⭐ ❓ 📝 marks after class**;
  - **keep writing notes** (auto-saved);
  - filter by mark and search within the session, then click a result to jump to it in context.

  You can also search across sessions, rename them, copy as Markdown, and export SRT/TXT.

### Microphone & audio

- **Microphone chain.** It runs on its own thread and never blocks the audio callback. Steps: anti-aliased resampling → 80 Hz high-pass → **GTCRN noise suppression** (on device) → **AGC** (gain up only during speech) → optional **Silero VAD** (sends audio only while someone speaks, which saves STT cost).
- **Apple Voice Processing** (macOS): the system's own echo cancellation, noise suppression and AGC, at almost no CPU cost.
- Sources: microphone, system audio (ScreenCaptureKit / WASAPI), or **both mixed**.
- **Stays in real time.** When the network or the machine falls behind, the app skips old audio instead of letting the translation drift further behind, and tells you ("⏩ đã bỏ qua X s", skipped X s).

### Reading & listening

- **Text-to-speech** reads translations aloud. Providers: free Edge, Microsoft, Google, ElevenLabs, TikTok, or **offline Piper** with Vietnamese voices; see the [TTS guide](docs/tts_guide.md).
- A **Read mode** (Đọc) reads pasted text aloud.

### macOS-style interface

- **Light (default) or dark theme**, or follow the system: **Cài đặt › Hiển thị › Giao diện** (Settings › Display › Appearance), or the ⋯ menu for a quick switch. Every text colour meets WCAG AA contrast (≥ 4.5:1) in both themes.
- A normal window that doesn't cover other apps, native traffic lights and system font (SF, with PingFang for Chinese). Also: opaque surfaces, native scrollbars and cursor, and *Reduce motion* is respected.
- **⤢ Floating panel**: shrinks to a small always-on-top window to use next to slides. **📌 Pin** (`⌘P`) keeps the normal window on top.
- Two-column source | translation view, a compact mode that hides the toolbar, transcript font 18 px by default (up to 140 px).

### Engineering (what differs from the original)

- **No leaks, nothing running while idle.** Audio crosses IPC as binary with bounded queues and connection timeouts. Capture stops instantly, and models are freed when a session stops.
- **Light on resources** (only the Local model may be heavy):
  - no blur or transparency, no animations running during a lecture, no web fonts;
  - the transcript renders incrementally: each sentence costs the same however long the lecture runs, and all tokens in one frame are drawn once.
- **Nothing is downloaded at install.** Models download only when you press **Tải model** (Download models): noise suppression + VAD ~1.2 MB, Local ~1.6 GB, all SHA-256 verified.
- `settings.json` is written atomically with a `.bak` copy; API keys are never logged.

---

## Install (macOS)

**Requirements:** macOS 13 or later. Apple silicon (M1–M4) works best; Intel works too (Local is slower).

1. Open **[Releases](https://github.com/ttkien2035/my-translator/releases/latest)** and download the right file:

   | Your machine | File |
   |---|---|
   | Mac with Apple silicon (M1/M2/M3/M4) | `MyTranslator_<version>_aarch64.dmg` |
   | Intel Mac | `MyTranslator_<version>_x64.dmg` |
   | Windows 10/11 | `MyTranslator_<version>_x64-setup.exe` |

   To check your chip:  → **About This Mac** → *Chip*.

2. Open the `.dmg`, drag **My Translator** into **Applications**, then eject it.

3. First launch. Releases are **free**, not signed with an Apple Developer ID, so macOS blocks the first launch:
   1. Open the app → macOS says it can't be opened → click **Done**.
   2. **System Settings › Privacy & Security** → scroll down → **Open Anyway** next to the app → enter your password.
   3. You only do this once. Later versions update from inside the app (**Cài đặt › Giới thiệu › Kiểm tra bản mới**, Settings › About › Check for updates).

   (From Terminal: `xattr -dr com.apple.quarantine /Applications/MyTranslator.app`.)

4. Grant permissions when asked: **Microphone** (required for lectures) and **Screen & System Audio Recording** (only to translate audio playing on the Mac, e.g. Zoom or videos). macOS may ask you to reopen the app after granting them.

No Python, Homebrew or anything else to install.

---

## First-time setup

1. **Cài đặt › Engine dịch** (Settings › Translation engine): paste a **Soniox** API key. Create one at [console.soniox.com](https://console.soniox.com); $10 covers ~80 hours. Source **Chinese** → target **Vietnamese** is the default.
2. **Hồ sơ môn học** (Course profiles, same screen):
   1. Press **+** to add a profile for your subject, e.g. *Corporate Finance*.
   2. Press **📚 Nạp từ điển tài chính Trung–Việt** (load the finance glossary).
   3. Add your lecturer's own terms if needed, then **save**.
3. **Cài đặt › Micro** (Settings › Microphone): press **Tải model** (1.2 MB) to enable noise suppression. On a MacBook, also try **Apple Voice Processing**.
   - Keep noise suppression on; turn VAD on for lectures with long pauses.
   - If recognition gets *worse*, turn noise suppression off (STT copes well with noise on its own).
4. (Optional) **Cài đặt › Model › Local › Tải model** (1.6 GB, once) for offline translation without network or VPN.
5. On the Live bar, choose the **🎤 Micro** source and your course profile, then press **▶ Bắt đầu** (Start).

---

## In class — suggested workflow

- Sit near the lecturer or use an external/lapel mic. A MacBook mic 5–10 m away loses a lot of accuracy.
- Open notes with `⌘⇧N`, then mark as you listen:
  - `⌘⇧1` an important point;
  - `⌘⇧2` something unclear;
  - `⌘⇧3` "on the exam" (or let the app catch it);
  - `⌘⇧C` to keep a sentence verbatim.
- Switch subject from the profile menu on the Live bar (shown once you have 2+ profiles).
- After class press **Dừng** (Stop). The session is in **📚 Thư viện** (Library); open it to study:
  - filter 📝 for exam material, and ❓ for questions to ask;
  - add marks and keep writing notes;
  - **Chép** (Copy) gives Markdown to paste into Notion or Apple Notes.
- Ask the lecturer before recording.

---

## Keyboard shortcuts

| Keys | Action |
|---|---|
| `⌘↩` | Start / Stop |
| `⌘1` / `⌘2` / `⌘3` | Source: system audio / microphone / both |
| `⌘⇧N` | Toggle the notes pane |
| `⌘⇧C` | Copy the latest sentence into notes (with time) |
| `⌘⇧1` / `⌘⇧2` / `⌘⇧3` | Mark the latest sentence ⭐ / ❓ / 📝 |
| `⌘T` | Toggle reading translations aloud |
| `⌘,` | Settings |
| `⌘P` · `⌘D` · `⌘M` | Pin window · Compact · Minimize |
| `?` | Shortcut sheet |

(On Windows use `Ctrl` instead of `⌘`.)

---

## Build from source

### Requirements

| | macOS | Windows | Linux (development/testing only) |
|---|---|---|---|
| Toolchain | Xcode Command Line Tools (`xcode-select --install`) | Visual Studio Build Tools (C++), WebView2 | `build-essential clang cmake pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libssl-dev` |
| Rust | [rustup](https://rustup.rs) — stable | rustup stable | rustup stable |
| Node.js | 20+ | 20+ | 20+ |
| CMake | `brew install cmake` | cmake.org | apt |

CMake and clang are needed for llama.cpp (the Local engine). It compiles **once** on the first build (5–10 minutes) and is cached after that.

### Run a development build

```bash
git clone https://github.com/ttkien2035/my-translator.git
cd my-translator
npm install
cp .env.example .env          # optional; empty is fine
npm run dev                   # opens the app with DevTools; UI hot-reloads
```

With `APP_IDENTIFIER=com.personal.translator.dev` in `.env`, `npm run dev` gets its own Microphone/Screen Recording permissions and leaves the installed app alone.

### Checks & tests

```bash
cd src-tauri
cargo check && cargo clippy --all-targets      # must be warning-free
cargo test                                     # tests that need no models
# Local engine against real models (the app's installed model folder works):
MT_TEST_SENSEVOICE_DIR=/path/sensevoice MT_TEST_GGUF=/path/Hy-MT2-1.8B-Q6_K.gguf \
  MT_TEST_WAV=/path/zh.wav cargo test --lib local:: -- --include-ignored --nocapture
```

Test environment variables:

| Variable | Purpose |
|---|---|
| `MT_TEST_SENSEVOICE_DIR` | folder with `model.int8.onnx` + `tokens.txt` |
| `MT_TEST_WAV` | 16 kHz mono s16le wav for the SenseVoice test (default `$MT_TEST_SENSEVOICE_DIR/test_wavs/zh.wav`). On macOS: `say -v Tingting "…" -o zh.aiff && afconvert -f WAVE -d LEI16@16000 -c 1 zh.aiff zh.wav` |
| `MT_TEST_GGUF` | GGUF file for the LLM test |
| `MT_SETTINGS_DIR` | the app/tests read and write `settings.json` here instead of the real location, to test corrupt settings/`.bak` without touching yours |

Integration tests (`src-tauri/tests/*.rs`) use the seams in `my_translator_lib::test_api`:

- `MicProcessor`: the microphone DSP without cpal;
- `start_with_sink` / `start_with_translator`: a Local session with a closure event sink and a stub translator;
- `Session::finish`: graceful end that translates what's queued;
- `Settings`.

### Release build

```bash
npm run build:local
# macOS:   src-tauri/target/release/bundle/dmg/MyTranslator_<ver>_aarch64.dmg
# Windows: src-tauri/target/release/bundle/nsis/MyTranslator_<ver>_x64-setup.exe
```

`build:local` ad-hoc signs on macOS. If the auto-updater's private key isn't available, it skips the update files; the app itself is unaffected. No Apple account is needed.

Intel Mac build from an Apple silicon Mac:

```bash
rustup target add x86_64-apple-darwin
npm run build:local -- --target x86_64-apple-darwin
# → src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/
```

---

## Free packaging for friends

No Apple Developer membership ($99/year) is needed. macOS builds are **ad-hoc signed**; friends click **Open Anyway** once (see *Install*, step 3).

### Option 1 — build a DMG yourself

On a Mac: `npm run build:local`, then share the `.dmg` from `src-tauri/target/release/bundle/dmg/`.

### Option 2 — publish on GitHub (with auto-update)

GitHub Actions (free for public repos) builds all three versions: Apple silicon Mac, Intel Mac and Windows. It then creates a **draft Release** with the downloads and the `latest.json` the auto-updater reads.

**One-time setup:**

1. GitHub disables Actions on forks by default: open the **Actions** tab → *I understand my workflows, go ahead and enable them*.
2. Create the auto-updater key (free, unrelated to Apple). This repo's public key is already in `src-tauri/tauri.conf.json`; only make a new pair for a different fork:
   ```bash
   npm run tauri signer generate -- -w ~/.tauri/my-translator-updater.key --ci
   # paste the .pub file's content into plugins.updater.pubkey in src-tauri/tauri.conf.json
   ```
3. Open **Settings › Secrets and variables › Actions › New repository secret**. Name it `TAURI_SIGNING_PRIVATE_KEY`; the value is the private key file's content (`cat ~/.tauri/my-translator-updater.key`). Never commit this file.

**Each release:**

```bash
# 1. Bump the version in 3 places: package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json
# 2. Add a "## vX.Y.Z - YYYY-MM-DD" section to docs/project-changelog.md — it becomes the Release page text
git tag vX.Y.Z && git push origin vX.Y.Z
# 3. Wait for Actions (~20–30 min), open Releases, check the draft, press Publish
```

To build without releasing: **Actions › Build & Release › Run workflow** (downloads are attached to the run).

Installed copies see the new version under **Cài đặt › Giới thiệu › Kiểm tra bản mới**. The updater only accepts builds signed with this repo's key.

---

## Where your data lives

| What | macOS | Windows |
|---|---|---|
| Settings (API keys, course profiles, glossaries) | `~/Library/Application Support/com.personal.translator/settings.json` (+ `.bak`) | `%APPDATA%\com.personal.translator\settings.json` |
| Sessions (Markdown + JSON) | `~/Library/Application Support/com.personal.translator/transcripts/` | `%APPDATA%\com.personal.translator\transcripts\` |
| Noise/VAD models, Local models, Piper voices | `~/Library/Application Support/My Translator/{audio-models,local-models,…}` | `%APPDATA%\My Translator\…` |

Everything stays on your machine. Only the cloud engine you choose receives audio; there is no intermediate server.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| Nothing is heard / status never changes | Check the **Microphone** permission in System Settings; pick the 🎤 source on the Live bar |
| Soniox error 401/402 | Wrong key or no credit — check console.soniox.com |
| Qwen `WebSocket error` right after Start | The DashScope key must be created in the **Singapore** region (international endpoint) |
| "⏩ Mạng chậm" (slow network) toasts keep appearing | Weak Wi-Fi: switch to Qwen (no VPN needed) or Local (offline) |
| Local says models are needed | Cài đặt › Model › Local › **Tải model**; needs ~1.6 GB free |
| Local leaves a few Chinese characters untranslated | A limit of the 3B model; point **custom GGUF** at a 7B model if you have ≥16 GB RAM |
| Noise suppression makes recognition worse | Turn it off, or try Apple Voice Processing instead |
| The first build takes very long | llama.cpp is compiling; first time only. Needs `cmake` + `clang` |
| macOS won't open the app / says it is "damaged" | Free build without a Developer ID — **Privacy & Security › Open Anyway**, or `xattr -dr com.apple.quarantine /Applications/MyTranslator.app` |

In a dev build (`npm run dev`), DevTools shows the `[Soniox]`, `[Mic]` and `[Local]` logs.

---

## Architecture

```
                  ┌ Soniox (WebSocket from the UI; profile glossary + context)
Mic / system ─► Rust capture ─► DSP thread (resample · HPF · GTCRN · AGC · VAD) ─► binary IPC ─┼ Qwen LiveTranslate (Rust WS)
                                                                                                ├ OpenAI Realtime (Rust WS)
                                                                                                └ Local: Silero VAD → SenseVoice → Hy-MT2 (llama.cpp)
                                                                                                                      │
                                                         Live view · Notes · Library (Markdown + JSON)  ◄─────────────┘
```

- **Tauri 2** (Rust backend; HTML/JS UI with no framework and no bundler)
- **cpal** / **ScreenCaptureKit** / **WASAPI** capture; **coreaudio-rs** for Apple Voice Processing; **rubato** resampling
- **sherpa-onnx**: Silero VAD, GTCRN noise suppression, SenseVoice recognition, Piper TTS
- **llama-cpp-2** (llama.cpp): Hy-MT2-1.8B GGUF (any instruct GGUF as a custom model), Metal on Apple silicon
- **reqwest / tokio-tungstenite** for the cloud engines

Rust is checked with `cargo clippy --all-targets` (zero warnings) and with tests against real models on Linux/CI. The macOS-only parts (ScreenCaptureKit, Voice Processing, Metal) are verified on a MacBook.

---

## Repository layout

```
src/                     UI (plain HTML/CSS/JS, no bundler)
  js/app.js              orchestration: sessions, engines, settings, shortcuts
  js/ui.js               live transcript (incremental rendering)
  js/study-view.js       Library › study view
  js/session-store.js    session persistence (JSON + Markdown)
  js/glossary/           Chinese–English–Vietnamese finance glossary
src-tauri/               Rust backend (Tauri 2)
  src/audio/             capture: cpal, ScreenCaptureKit, WASAPI, Apple Voice Processing, mic DSP
  src/local/             offline engine: VAD → SenseVoice → Hy-MT2 (llama.cpp)
  src/commands/          Tauri commands: cloud engines, TTS, sessions, model downloads
docs/project-changelog.md  change history (CI uses it as release notes)
docs/tts_guide*.md         text-to-speech guide (English / Vietnamese)
doc_coding/                plans & QA reports between the dev session (WSL) and the QA session (macOS)
scripts/tauri-with-env.mjs dev/build wrapper (.env, ad-hoc signing, skips update files without the key)
.github/workflows/         build + release on GitHub Actions
```

---

## Credits & license

Lecture Edition by **ttkien2035**. Based on [My Translator](https://github.com/phuc-nt/my-translator) by Nguyễn Trọng Phúc — MIT License. The changes in this fork are MIT as well. Models: [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (FunAudioLLM), [Hy-MT2](https://huggingface.co/tencent/Hy-MT2-1.8B) (Tencent, Apache-2.0), [Silero VAD](https://github.com/snakers4/silero-vad), GTCRN, [Piper](https://github.com/rhasspy/piper) — each under its own license.
