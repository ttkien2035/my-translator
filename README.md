# My Translator — Lecture Edition

Ứng dụng dịch giọng nói **theo thời gian thực** trên macOS/Windows, được tuỳ biến cho việc **nghe giảng bằng tiếng Trung và ghi chú bằng tiếng Việt** (ngành tài chính – kinh tế). Fork từ [phuc-nt/my-translator](https://github.com/phuc-nt/my-translator) (MIT), giữ nguyên kiến trúc Tauri + Rust và bổ sung: hồ sơ môn học với từ điển thuật ngữ, khung ghi chú có phím tắt, xử lý micro cho lớp học, và engine offline **thuần Rust** (không Python).

> *Real-time speech translation desktop app (Tauri 2 + Rust). Tuned for Chinese finance lectures → Vietnamese: course glossaries fed to the STT engine, note-taking hotkeys, a classroom microphone chain, and a pure-Rust offline engine (SenseVoice + Qwen2.5 via llama.cpp).*

---

## Mục lục

1. [Tính năng](#tính-năng)
2. [Cài đặt cho người dùng (macOS)](#cài-đặt-cho-người-dùng-macos)
3. [Thiết lập lần đầu](#thiết-lập-lần-đầu)
4. [Dùng trên lớp — quy trình gợi ý](#dùng-trên-lớp--quy-trình-gợi-ý)
5. [Phím tắt](#phím-tắt)
6. [Build từ mã nguồn](#build-từ-mã-nguồn)
7. [Đóng gói & phát hành cho người dùng phổ thông](#đóng-gói--phát-hành-cho-người-dùng-phổ-thông)
8. [Dữ liệu nằm ở đâu](#dữ-liệu-nằm-ở-đâu)
9. [Xử lý sự cố](#xử-lý-sự-cố)
10. [Kiến trúc & công nghệ](#kiến-trúc--công-nghệ)

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
- **Thư viện phiên**: mọi buổi được lưu (Markdown + JSON), tìm kiếm, đổi tên, xuất SRT/TXT.

### Micro & âm thanh

- Chuỗi xử lý micro chạy ở thread riêng, không chặn callback âm thanh: resample chống aliasing → lọc thông cao 80 Hz → **khử ồn GTCRN** (trên máy) → **AGC** (chỉ tăng khi có tiếng nói) → **VAD Silero** (tuỳ chọn: chỉ gửi khi có tiếng nói, tiết kiệm phí STT).
- **Apple Voice Processing** (macOS): khử vang – khử ồn – AGC của hệ thống, gần như không tốn CPU.
- Nguồn: micro, âm thanh hệ thống (ScreenCaptureKit / WASAPI), hoặc **trộn cả hai**.
- **Bám kịp thời gian thực**: khi mạng nghẽn hoặc máy chậm, app bỏ phần âm thanh cũ thay vì để bản dịch trễ dần (có báo "⏩ đã bỏ qua X s").

### Đọc & nghe

- **TTS** đọc bản dịch (Edge miễn phí, Microsoft, Google, ElevenLabs, TikTok, hoặc **Piper offline** với giọng Việt) và **chế độ Đọc** cho văn bản dán vào.
- Giao diện nổi luôn trên cùng, chế độ compact tự ẩn, chế độ hai cột nguồn | dịch, cỡ chữ tới 140 px.

### Kỹ thuật (điểm khác biệt so với bản gốc)

- Audio qua IPC dạng nhị phân, hàng đợi có giới hạn, timeout kết nối, dừng thu tức thì, model giải phóng khi dừng phiên — không rò rỉ, không chạy ngầm khi nhàn rỗi.
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

3. Mở app lần đầu:
   - Nếu bản phát hành **đã ký và notarize**, app mở bình thường.
   - Nếu macOS báo *"không thể mở vì không xác minh được nhà phát triển"* (bản build chưa ký): vào **Cài đặt hệ thống › Quyền riêng tư & Bảo mật**, kéo xuống dưới, bấm **Vẫn mở** cạnh tên app; hoặc chuột phải vào app → **Mở** → **Mở**. Chỉ cần làm một lần. (Cách khác cho người quen Terminal: `xattr -d com.apple.quarantine /Applications/MyTranslator.app`.)

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
- Hết buổi bấm **Dừng**; buổi học nằm trong **Thư viện** với bản dịch, câu gốc, các câu đã đánh dấu và ghi chú của bạn — xuất Markdown để đưa vào Notion/Apple Notes.
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
# Kiểm thử engine Local với model thật (tải SenseVoice int8 + GGUF Qwen về một thư mục tạm):
MT_TEST_SENSEVOICE_DIR=/path/sensevoice MT_TEST_GGUF=/path/qwen2.5-3b-instruct-q4_k_m.gguf \
  cargo test --lib local:: -- --include-ignored --nocapture
```

### Build bản phát hành (chưa ký)

```bash
npm run build
# macOS:   src-tauri/target/release/bundle/dmg/MyTranslator_<ver>_aarch64.dmg
# Windows: src-tauri/target/release/bundle/nsis/MyTranslator_<ver>_x64-setup.exe
```

Build cho Mac Intel từ máy Apple Silicon:

```bash
rustup target add x86_64-apple-darwin
npm run tauri build -- --target x86_64-apple-darwin
# → src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/
```

---

## Đóng gói & phát hành cho người dùng phổ thông

Có ba mức, từ đơn giản đến "mở là chạy":

### Mức 1 — DMG chưa ký (miễn phí)

`npm run build` → gửi file `.dmg`. Người nhận phải làm bước **Vẫn mở** một lần (xem *Cài đặt cho người dùng*). Phù hợp cho bạn bè, lớp học.

### Mức 2 — Ký & notarize (Apple Developer Program, $99/năm)

Người nhận mở app không gặp cảnh báo nào.

1. Tạo chứng chỉ **Developer ID Application** trong Xcode (Settings › Accounts › Manage Certificates) hoặc tại developer.apple.com, cài vào Keychain.
2. Tạo **app-specific password** tại [appleid.apple.com](https://appleid.apple.com) (Sign-In and Security › App-Specific Passwords).
3. Điền vào `.env` (đã gitignore):
   ```
   APPLE_ID=you@example.com
   APPLE_TEAM_ID=XXXXXXXXXX
   APPLE_PASSWORD=xxxx-xxxx-xxxx-xxxx
   APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (XXXXXXXXXX)"
   ```
4. Chạy `./scripts/build-notarized.sh` — Tauri sẽ ký, gửi notarize và staple. DMG kết quả nằm ở `src-tauri/target/release/bundle/dmg/`.

### Mức 3 — Phát hành tự động trên GitHub + tự cập nhật

Workflow `.github/workflows/release.yml` build cả ba bản (macOS Apple Silicon, macOS Intel, Windows), ký/notarize macOS, tạo `latest.json` cho **auto-update** và mở một *draft release* mỗi khi bạn đẩy tag:

```bash
# 1. Cập nhật version ở 3 chỗ: package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json
# 2. Thêm mục "## vX.Y.Z - YYYY-MM-DD" vào docs/project-changelog.md (workflow dùng làm release notes)
git tag vX.Y.Z && git push origin vX.Y.Z
```

Secrets cần khai báo trong repo (*Settings › Secrets and variables › Actions*):

| Secret | Nội dung |
|---|---|
| `APPLE_CERTIFICATE` | file `.p12` của chứng chỉ Developer ID, mã hoá base64 |
| `APPLE_CERTIFICATE_PASSWORD` | mật khẩu file `.p12` |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | như mức 2 |
| `TAURI_SIGNING_PRIVATE_KEY` | khoá ký updater (xem dưới) |

**Khoá updater (bắt buộc cho tự cập nhật):** bản fork này *không* dùng khoá của repo gốc. Tạo cặp khoá mới:

```bash
npm run tauri signer generate -- -w ~/.tauri/my-translator.key
```

Dán **khoá công khai** in ra vào `plugins.updater.pubkey` trong `src-tauri/tauri.conf.json`, và nội dung **khoá riêng** vào secret `TAURI_SIGNING_PRIVATE_KEY`. Endpoint updater đã trỏ về `github.com/ttkien2035/my-translator`. Chừng nào chưa thay `pubkey`, app không tự cập nhật (an toàn — không bao giờ bị bản khác đè lên).

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
| macOS không cho mở app | Bản chưa ký — xem bước 3 mục *Cài đặt cho người dùng* |

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

## Ghi công & giấy phép

Dựa trên [My Translator](https://github.com/phuc-nt/my-translator) của Nguyễn Trọng Phúc — MIT License. Phần tuỳ biến trong fork này cũng theo MIT. Model: [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (FunAudioLLM), [Qwen2.5](https://huggingface.co/Qwen) (Alibaba), [Silero VAD](https://github.com/snakers4/silero-vad), GTCRN, [Piper](https://github.com/rhasspy/piper) — theo giấy phép riêng của từng model.
