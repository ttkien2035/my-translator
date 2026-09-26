# My Translator — Lecture Edition

[English](README.md) · **Tiếng Việt**

Ứng dụng dịch giọng nói **theo thời gian thực** trên macOS/Windows, được tuỳ biến cho việc **nghe giảng bằng tiếng Trung và ghi chú bằng tiếng Việt** (ngành tài chính – kinh tế). Fork từ [phuc-nt/my-translator](https://github.com/phuc-nt/my-translator) (MIT), giữ nguyên kiến trúc Tauri + Rust và bổ sung: hồ sơ môn học với từ điển thuật ngữ, khung ghi chú có phím tắt, xử lý micro cho lớp học, và engine offline **thuần Rust** (không Python).

---

## Mục lục

1. [Tính năng](#tính-năng)
2. [Cài đặt cho người dùng (macOS)](#cài-đặt-cho-người-dùng-macos)
3. [Thiết lập lần đầu](#thiết-lập-lần-đầu)
4. [Dùng trên lớp — quy trình gợi ý](#dùng-trên-lớp--quy-trình-gợi-ý)
5. [Phím tắt](#phím-tắt)
6. [Build từ mã nguồn](#build-từ-mã-nguồn)
7. [Đóng gói miễn phí cho bạn bè](#đóng-gói-miễn-phí-cho-bạn-bè)
8. [Dữ liệu nằm ở đâu](#dữ-liệu-nằm-ở-đâu)
9. [Xử lý sự cố](#xử-lý-sự-cố)
10. [Kiến trúc & công nghệ](#kiến-trúc--công-nghệ)
11. [Cấu trúc repo & tài liệu](#cấu-trúc-repo--tài-liệu)

---

## Tính năng

### Bốn engine dịch, chuyển đổi ngay trên thanh công cụ

| Engine | Chạy ở đâu | Độ trễ | Chi phí | Ghi chú |
|---|---|---|---|---|
| ☁️ **Soniox** (khuyên dùng) | cloud | ~2 s | ~$0.12/giờ | 70+ ngôn ngữ nguồn; nhận **từ điển thuật ngữ** và ngữ cảnh của hồ sơ môn học |
| 🌏 **Qwen LiveTranslate** | cloud (Alibaba) | ~4 s | miễn phí (preview) | vào được từ Trung Quốc không cần VPN; chỉ văn bản |
| ⚡ **OpenAI Realtime** | cloud | ~2 s | ~$4/giờ | có giọng nói dịch; cần VPN ở Trung Quốc |
| 🖥️ **Local** (offline) | trên máy, thuần Rust | ~2–3 s sau khi hết câu | miễn phí | SenseVoice (nhận dạng) + Qwen2.5-3B (dịch, Metal trên Apple Silicon); từ điển môn học đưa vào prompt |

Tên model của từng engine chỉnh được trong **Cài đặt › Model** (kể cả GGUF tuỳ chỉnh cho Local). Cùng chỗ đó có ô **LLM hỗ trợ** (DeepSeek / Qwen DashScope / Zhipu GLM / OpenAI / bất kỳ API chuẩn OpenAI) dành cho các tính năng dịch lại học thuật và tóm tắt sắp tới.

### Dành cho lớp học

- **Hồ sơ môn học** — mỗi môn một bộ: lĩnh vực, từ nhận dạng, ngữ cảnh nền và **từ điển thuật ngữ nguồn → đích**. Có sẵn **278 thuật ngữ tài chính Trung–Anh–Việt** (báo cáo tài chính, chỉ số, tài chính doanh nghiệp, đầu tư, phái sinh, ngân hàng – tiền tệ, vĩ mô, kinh tế lượng, thuế – quản trị, câu thường gặp trên lớp) nạp bằng một nút bấm. Đổi hồ sơ **giữa giờ** cũng áp dụng ngay.
- **Khung ghi chú** ngay dưới bản dịch (`⌘⇧N`), tự lưu cùng phiên và xuất ra Markdown. `⌘⇧C` chép câu vừa dịch (kèm giờ và câu gốc). `⌘⇧1/2/3` đánh dấu câu vừa dịch ⭐ quan trọng / ❓ chưa hiểu / 📝 sẽ thi — dấu hiện ngay trên bản dịch và được gom thành mục *Đánh dấu* khi xuất.
- **Tự đánh dấu 📝** khi giảng viên nói *会考 / 考点 / 期末考 / 必考…* ("phần này sẽ thi").
- **Bản dịch trực tiếp cuộn lại được cả buổi** (tới ~1 500 câu trên màn hình). Cuộn lên đọc lại thì không bị kéo xuống; ở cuối thì tự theo chữ mới.
- **Thư viện ôn bài**: mọi buổi tự lưu (Markdown + JSON, 15 giây/lần và khi dừng/thoát). Mở một buổi để:
  - đọc lại từng câu (bản dịch, câu gốc, giờ);
  - **bấm đánh ⭐ ❓ 📝 sau giờ học**;
  - **viết tiếp ghi chú** (tự lưu);
  - lọc theo dấu và tìm trong buổi; bấm một câu trong kết quả để nhảy tới đúng chỗ.

  Ngoài ra: tìm giữa các buổi, đổi tên, chép Markdown, xuất SRT/TXT.

### Micro & âm thanh

- Chuỗi xử lý micro chạy ở thread riêng, không chặn callback âm thanh: resample chống aliasing → lọc thông cao 80 Hz → **khử ồn GTCRN** (trên máy) → **AGC** (chỉ tăng khi có tiếng nói) → **VAD Silero** (tuỳ chọn: chỉ gửi khi có tiếng nói, tiết kiệm phí STT).
- **Apple Voice Processing** (macOS): khử vang – khử ồn – AGC của hệ thống, gần như không tốn CPU.
- Nguồn: micro, âm thanh hệ thống (ScreenCaptureKit / WASAPI), hoặc **trộn cả hai**.
- **Bám kịp thời gian thực**: khi mạng nghẽn hoặc máy chậm, app bỏ phần âm thanh cũ thay vì để bản dịch trễ dần (có báo "⏩ đã bỏ qua X s").

### Đọc & nghe

- **TTS** đọc bản dịch (Edge miễn phí, Microsoft, Google, ElevenLabs, TikTok, hoặc **Piper offline** với giọng Việt; xem [hướng dẫn TTS](docs/tts_guide_vi.md)) và **chế độ Đọc** cho văn bản dán vào.

### Giao diện kiểu macOS

- Cửa sổ bình thường (không đè lên app khác), traffic lights gốc, font hệ thống (SF / PingFang cho chữ Hán), nền đặc tối, thanh cuộn và con trỏ gốc, tôn trọng *Reduce motion*.
- **⤢ Cửa sổ nổi**: thu nhỏ thành khung luôn nằm trên để dùng cạnh slide. **📌 Ghim** (`⌘P`) giữ cửa sổ thường nằm trên.
- Chế độ hai cột nguồn | dịch, compact tự ẩn thanh công cụ, cỡ chữ tới 140 px, màn hình chính toàn tiếng Việt.

### Kỹ thuật (điểm khác biệt so với bản gốc)

- Audio qua IPC dạng nhị phân, hàng đợi có giới hạn, timeout kết nối, dừng thu tức thì, model giải phóng khi dừng phiên — không rò rỉ, không chạy ngầm khi nhàn rỗi.
- Nhẹ tài nguyên (chỉ model Local được phép nặng):
  - không hiệu ứng mờ/trong suốt, không animation chạy suốt buổi, không web font;
  - bản dịch vẽ tăng dần: mỗi câu tốn chi phí cố định dù buổi dài bao lâu, nhiều token trong một khung hình gộp thành một lần vẽ.
- Không tải gì khi cài app; model chỉ tải khi bạn bấm **Tải model** (khử ồn + VAD ~1,2 MB; Local ~2,3 GB), có kiểm tra SHA-256.
- `settings.json` ghi atomic + bản sao `.bak`; khoá API không bao giờ ghi ra log.

---

## Cài đặt cho người dùng (macOS)

**Yêu cầu:** macOS 13 trở lên. Chip Apple (M1–M4) chạy tốt nhất; Intel dùng được (Local chậm hơn).

1. Vào **[Releases](https://github.com/ttkien2035/my-translator/releases/latest)** và tải đúng file:

   | Máy của bạn | File |
   |---|---|
   | Mac chip Apple (M1/M2/M3/M4) | `MyTranslator_<phiên bản>_aarch64.dmg` |
   | Mac Intel | `MyTranslator_<phiên bản>_x64.dmg` |
   | Windows 10/11 | `MyTranslator_<phiên bản>_x64-setup.exe` |

   Xem chip:  → **Giới thiệu về máy Mac này** → dòng *Chip*.

2. Mở file `.dmg`, kéo **My Translator** vào **Applications**, rồi eject.

3. Mở app lần đầu. App phát hành **miễn phí**, không ký bằng Apple Developer ID, nên macOS chặn lần mở đầu:
   1. Mở app → macOS báo *không mở được* → bấm **Xong**.
   2. **Cài đặt hệ thống › Quyền riêng tư & Bảo mật** → kéo xuống → bấm **Vẫn mở** cạnh tên app → nhập mật khẩu máy.
   3. Chỉ làm một lần. Các bản sau cập nhật ngay trong app (**Cài đặt › Giới thiệu › Kiểm tra bản mới**).

   (Người quen Terminal: `xattr -dr com.apple.quarantine /Applications/MyTranslator.app`.)

4. Cấp quyền khi được hỏi: **Micro** (bắt buộc để nghe giảng) và **Screen & System Audio Recording** (chỉ cần nếu dịch âm thanh từ máy — Zoom, video). Sau khi bật quyền, macOS có thể yêu cầu mở lại app.

Không cần cài Python, Homebrew hay bất cứ thứ gì khác.

---

## Thiết lập lần đầu

1. **Cài đặt › Engine dịch**: dán API key **Soniox** (tạo tại [console.soniox.com](https://console.soniox.com); nạp $10 dùng được ~80 giờ). Chọn ngôn ngữ nguồn **Chinese** → đích **Vietnamese** (đã là mặc định).
2. **Hồ sơ môn học** (cùng màn hình): bấm **+** tạo hồ sơ cho môn (ví dụ *Tài chính doanh nghiệp*), bấm **📚 Nạp từ điển tài chính Trung–Việt**, thêm thuật ngữ riêng của giảng viên nếu có, **Lưu**.
3. **Cài đặt › Micro**: bấm **Tải model** (1,2 MB) để bật khử ồn; trên MacBook thử thêm **Apple Voice Processing**. Gợi ý: bật khử ồn; bật VAD khi lớp có nhiều khoảng lặng; nếu nhận dạng *kém đi* thì tắt khử ồn (STT vốn chịu ồn tốt).
4. (Tuỳ chọn) **Cài đặt › Model › Local › Tải model** (2,3 GB, tải một lần) để dịch offline khi không có mạng/VPN.
5. Trên thanh Live chọn nguồn **🎤 Mic**, chọn hồ sơ môn, bấm **▶ Bắt đầu**.

---

## Dùng trên lớp — quy trình gợi ý

- Ngồi gần giảng viên hoặc dùng micro rời/kẹp áo; micro MacBook cách 5–10 m sẽ giảm độ chính xác rõ rệt.
- Bật ghi chú `⌘⇧N`. Nghe đến ý quan trọng: `⌘⇧1`; chưa hiểu: `⌘⇧2`; giảng viên báo sẽ thi: `⌘⇧3` (hoặc để app tự bắt). Muốn giữ nguyên câu: `⌘⇧C`.
- Đổi môn ngay trên thanh Live bằng ô hồ sơ (hiện khi có ≥ 2 hồ sơ).
- Hết buổi bấm **Dừng**. Buổi học nằm trong **📚 Thư viện**: mở ra để ôn — lọc 📝 xem phần *sẽ thi*, lọc ❓ để hỏi lại giảng viên, đánh dấu thêm và viết tiếp ghi chú. Bấm **Chép** để lấy Markdown dán vào Notion/Apple Notes.
- Nên xin phép giảng viên trước khi ghi âm.

---

## Phím tắt

| Phím | Tác dụng |
|---|---|
| `⌘↩` | Bắt đầu / Dừng |
| `⌘1` / `⌘2` / `⌘3` | Nguồn: âm thanh hệ thống / micro / cả hai |
| `⌘⇧N` | Mở/đóng khung ghi chú |
| `⌘⇧C` | Chép câu vừa dịch vào ghi chú (kèm giờ) |
| `⌘⇧1` / `⌘⇧2` / `⌘⇧3` | Đánh dấu câu vừa dịch ⭐ / ❓ / 📝 |
| `⌘T` | Bật/tắt đọc bản dịch (TTS) |
| `⌘,` | Cài đặt |
| `⌘P` · `⌘D` · `⌘M` | Ghim cửa sổ · Compact · Thu nhỏ |
| `?` | Bảng phím tắt |

(Trên Windows dùng `Ctrl` thay `⌘`.)

---

## Build từ mã nguồn

### Yêu cầu

| | macOS | Windows | Linux (chỉ để phát triển/kiểm thử) |
|---|---|---|---|
| Toolchain | Xcode Command Line Tools (`xcode-select --install`) | Visual Studio Build Tools (C++), WebView2 | `build-essential clang cmake pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libssl-dev` |
| Rust | [rustup](https://rustup.rs) — stable | rustup stable | rustup stable |
| Node.js | 20+ | 20+ | 20+ |
| CMake | `brew install cmake` | cmake.org | apt |

CMake và clang cần cho llama.cpp (engine Local) — biên dịch **một lần** ở lần build đầu (5–10 phút), sau đó có cache.

### Chạy bản phát triển

```bash
git clone https://github.com/ttkien2035/my-translator.git
cd my-translator
npm install
cp .env.example .env          # tuỳ chọn; để trống cũng được
npm run dev                   # mở app kèm DevTools; hot-reload giao diện
```

`npm run dev` có thể dùng `APP_IDENTIFIER=com.personal.translator.dev` trong `.env` để bản dev có quyền Micro/Screen Recording riêng, không đụng bản cài chính thức.

### Kiểm tra & kiểm thử

```bash
cd src-tauri
cargo check && cargo clippy --all-targets      # phải sạch cảnh báo
cargo test                                     # test không cần model
# Kiểm thử engine Local với model thật (thư mục model đã cài trong app dùng được):
MT_TEST_SENSEVOICE_DIR=/path/sensevoice MT_TEST_GGUF=/path/qwen2.5-3b-instruct-q4_k_m.gguf \
  MT_TEST_WAV=/path/zh.wav cargo test --lib local:: -- --include-ignored --nocapture
```

Biến môi trường cho kiểm thử:

| Biến | Tác dụng |
|---|---|
| `MT_TEST_SENSEVOICE_DIR` | thư mục chứa `model.int8.onnx` + `tokens.txt` |
| `MT_TEST_WAV` | wav 16 kHz mono s16le cho test SenseVoice (mặc định `$MT_TEST_SENSEVOICE_DIR/test_wavs/zh.wav`). Trên macOS: `say -v Tingting "…" -o zh.aiff && afconvert -f WAVE -d LEI16@16000 -c 1 zh.aiff zh.wav` |
| `MT_TEST_GGUF` | file GGUF cho test LLM |
| `MT_SETTINGS_DIR` | app/test đọc-ghi `settings.json` trong thư mục này thay vì thư mục thật — thử settings hỏng/`.bak` mà không đụng cài đặt của bạn |

Test tích hợp (`src-tauri/tests/*.rs`) dùng các seam trong `my_translator_lib::test_api`: `MicProcessor` (DSP micro không cần cpal), `start_with_sink` / `start_with_translator` (phiên Local với closure nhận event và translator giả lập), `Session::finish` (kết thúc êm, dịch nốt hàng đợi), `Settings`.

### Build bản phát hành

```bash
npm run build:local
# macOS:   src-tauri/target/release/bundle/dmg/MyTranslator_<ver>_aarch64.dmg
# Windows: src-tauri/target/release/bundle/nsis/MyTranslator_<ver>_x64-setup.exe
```

`build:local` tự ký ad-hoc trên macOS và, nếu máy không có khoá riêng của bộ tự cập nhật, bỏ qua các file cập nhật (app vẫn chạy bình thường). Không cần tài khoản Apple nào.

Build cho Mac Intel từ máy Apple Silicon:

```bash
rustup target add x86_64-apple-darwin
npm run build:local -- --target x86_64-apple-darwin
# → src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/
```

---

## Đóng gói miễn phí cho bạn bè

Không cần Apple Developer ($99/năm). Bản macOS được **ký ad-hoc**; bạn bè bấm **Vẫn mở** một lần (mục *Cài đặt cho người dùng*, bước 3).

### Cách 1 — Tự build một file DMG

Trên Mac: `npm run build:local` → gửi file `.dmg` trong `src-tauri/target/release/bundle/dmg/`.

### Cách 2 — Phát hành trên GitHub (có tự cập nhật)

GitHub Actions (miễn phí với repo công khai) build cả ba bản — Mac chip Apple, Mac Intel, Windows — rồi tạo một **bản nháp Release** có sẵn file tải và `latest.json` cho bộ tự cập nhật.

**Chuẩn bị một lần:**

1. Repo fork trên GitHub tắt Actions mặc định: vào tab **Actions** → *I understand my workflows, go ahead and enable them*.
2. Tạo khoá cho bộ tự cập nhật (miễn phí, không liên quan Apple). Khoá công khai của repo này đã có trong `src-tauri/tauri.conf.json`; tạo cặp mới chỉ khi làm fork khác:
   ```bash
   npm run tauri signer generate -- -w ~/.tauri/my-translator-updater.key --ci
   # dán nội dung file .pub vào plugins.updater.pubkey trong src-tauri/tauri.conf.json
   ```
3. **Settings › Secrets and variables › Actions › New repository secret**: tên `TAURI_SIGNING_PRIVATE_KEY`, giá trị là nội dung file khoá riêng (`cat ~/.tauri/my-translator-updater.key`). Không bao giờ commit file này.

**Mỗi lần phát hành:**

```bash
# 1. Đổi version ở 3 chỗ: package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json
# 2. Thêm mục "## vX.Y.Z - YYYY-MM-DD" vào docs/project-changelog.md — dùng làm nội dung trang Release
git tag vX.Y.Z && git push origin vX.Y.Z
# 3. Đợi Actions chạy xong (~20–30 phút), mở Releases, kiểm tra bản nháp, bấm Publish
```

Muốn build thử mà không tạo release: **Actions › Build & Release › Run workflow** (file tải kèm theo lượt chạy).

Máy đã cài sẽ thấy bản mới trong **Cài đặt › Giới thiệu › Kiểm tra bản mới**. Bộ tự cập nhật chỉ nhận bản ký bằng đúng khoá của repo này.

---

## Dữ liệu nằm ở đâu

| Nội dung | macOS | Windows |
|---|---|---|
| Cài đặt (API key, hồ sơ môn, từ điển) | `~/Library/Application Support/com.personal.translator/settings.json` (+ `.bak`) | `%APPDATA%\com.personal.translator\settings.json` |
| Buổi học (Markdown + JSON) | `~/Library/Application Support/com.personal.translator/transcripts/` | `%APPDATA%\com.personal.translator\transcripts\` |
| Model khử ồn/VAD, model Local, giọng Piper | `~/Library/Application Support/My Translator/{audio-models,local-models,…}` | `%APPDATA%\My Translator\…` |

Tất cả ở trên máy bạn. Chỉ engine cloud bạn chọn nhận âm thanh; không có máy chủ trung gian.

---

## Xử lý sự cố

| Hiện tượng | Cách xử lý |
|---|---|
| Không nghe được gì / trạng thái không đổi | Kiểm tra quyền **Micro** trong Cài đặt hệ thống; chọn đúng nguồn 🎤 trên thanh Live |
| Soniox báo lỗi 401/402 | Sai key hoặc hết tiền — kiểm tra tại console.soniox.com |
| Qwen báo `WebSocket error` ngay khi Start | Key DashScope phải tạo ở region **Singapore** (endpoint quốc tế) |
| Toast "⏩ Mạng chậm" liên tục | Wi-Fi yếu: chuyển sang Qwen (không cần VPN) hoặc Local (offline) |
| Local: "cần tải model" | Cài đặt › Model › Local › **Tải model**; cần ~2,3 GB trống |
| Local dịch sót vài chữ Hán | Giới hạn của model 3B; trỏ **GGUF tuỳ chỉnh** tới bản 7B nếu máy đủ RAM (≥16 GB) |
| Khử ồn làm nhận dạng kém hơn | Tắt "Khử tiếng ồn nền" (hoặc thử Apple Voice Processing thay thế) |
| Build lần đầu rất lâu | llama.cpp đang được biên dịch; chỉ lần đầu. Cần `cmake` + `clang` |
| macOS không cho mở app / báo "bị hỏng" | Bản miễn phí chưa ký Developer ID — **Quyền riêng tư & Bảo mật › Vẫn mở**, hoặc `xattr -dr com.apple.quarantine /Applications/MyTranslator.app` |

Bạn có thể mở DevTools trong bản dev (`npm run dev`) để xem log `[Soniox]`, `[Mic]`, `[Local]`.

---

## Kiến trúc & công nghệ

```
                 ┌ Soniox (WebSocket từ giao diện; từ điển + ngữ cảnh hồ sơ môn)
Micro / hệ thống ─► Rust capture ─► DSP thread (resample · HPF · GTCRN · AGC · VAD) ─► IPC nhị phân ─┼ Qwen LiveTranslate (Rust WS)
                                                                                                    ├ OpenAI Realtime (Rust WS)
                                                                                                    └ Local: Silero VAD → SenseVoice → Qwen2.5 (llama.cpp)
                                                                                                                          │
                                                             Overlay · Ghi chú · Thư viện (Markdown + JSON)  ◄────────────┘
```

- **Tauri 2** (Rust backend, giao diện HTML/JS không framework, không bundler)
- **cpal** / **ScreenCaptureKit** / **WASAPI** thu âm; **coreaudio-rs** cho Apple Voice Processing; **rubato** resample
- **sherpa-onnx**: Silero VAD, GTCRN khử ồn, SenseVoice nhận dạng, Piper TTS
- **llama-cpp-2** (llama.cpp): Qwen2.5-3B-Instruct GGUF, Metal trên Apple Silicon
- **reqwest / tokio-tungstenite** cho các engine cloud

Mã Rust được kiểm bằng `cargo clippy --all-targets` (0 cảnh báo) và test chạy model thật trên Linux/CI; các phần chỉ có trên macOS (ScreenCaptureKit, Voice Processing, Metal) kiểm trên MacBook.

---

## Cấu trúc repo & tài liệu

```
src/                     giao diện (HTML/CSS/JS thuần, không bundler)
  js/app.js              điều phối chính: phiên, engine, cài đặt, phím tắt
  js/ui.js               bản dịch trực tiếp (vẽ tăng dần)
  js/study-view.js       Thư viện › ôn bài
  js/session-store.js    lưu phiên (JSON + Markdown)
  js/glossary/           từ điển tài chính Trung–Anh–Việt
src-tauri/               backend Rust (Tauri 2)
  src/audio/             thu âm: cpal, ScreenCaptureKit, WASAPI, Apple Voice Processing, DSP micro
  src/local/             engine offline: VAD → SenseVoice → Qwen2.5 (llama.cpp)
  src/commands/          lệnh Tauri: engine cloud, TTS, phiên, tải model
docs/project-changelog.md  lịch sử thay đổi (CI dùng làm release notes)
docs/tts_guide*.md         hướng dẫn giọng đọc
doc_coding/                kế hoạch & báo cáo QA giữa phiên phát triển (WSL) và phiên QA (macOS)
scripts/tauri-with-env.mjs chạy dev/build (.env, ký ad-hoc, tắt file cập nhật khi thiếu khoá)
.github/workflows/         build + phát hành trên GitHub Actions
```

---

## Ghi công & giấy phép

Lecture Edition do **ttkien2035** phát triển. Dựa trên [My Translator](https://github.com/phuc-nt/my-translator) của Nguyễn Trọng Phúc — MIT License. Phần tuỳ biến trong fork này cũng theo MIT. Model: [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (FunAudioLLM), [Qwen2.5](https://huggingface.co/Qwen) (Alibaba), [Silero VAD](https://github.com/snakers4/silero-vad), GTCRN, [Piper](https://github.com/rhasspy/piper) — theo giấy phép riêng của từng model.
