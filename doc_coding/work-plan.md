# Kế hoạch việc — chốt ngày 2026-09-25

Đã thống nhất giữa kỹ sư trưởng (WSL) và QA (Mac) sau QA vòng 1–2.
HEAD hiện tại: `f7d0769` trên cả `main` và `feature/lecture-assistant`.
Chi tiết findings: xem `qa-reports/2026-09-25-round1-2.md`.

Thực hiện theo thứ tự: **Commit A** trước, **Commit B** sau. Mỗi commit push
lên `feature/lecture-assistant` rồi fast-forward `main` để QA pull về test.

---

## Commit A — nền + test seam (không đổi giao diện)

| # | Việc | Chi tiết đã chốt |
|---|---|---|
| F1 | Lọc segment rác trong `src-tauri/src/local/pipeline.rs` | Bỏ segment có RMS < −45 dBFS **hoặc** text sau khi lược dấu câu ≤ 1 ký tự. Không đưa text trùng với segment liền trước vào LLM. Lý do: SenseVoice trả `"没。"` cho 1 s im lặng. |
| F2 | Trạng thái + warm-up Metal trong `src-tauri/src/local/llm.rs` / `pipeline.rs` | Lần nạp LLM đầu tiên trên macOS ggml biên dịch thư viện `fa` mất ~13,5 s. Thêm status `"Đang khởi tạo Metal (lần đầu ~15 s)…"` khi nạp, và warm-up một prompt 1 token ngay sau khi load để lần dịch đầu không gánh chi phí này. |
| F3 | Test đọc `MT_TEST_WAV` | Trong `src-tauri/src/local/asr.rs`, test `transcribes_chinese_sample` đọc wav từ env `MT_TEST_WAV`, fallback `$MT_TEST_SENSEVOICE_DIR/test_wavs/zh.wav`. Không commit wav vào repo; ghi lại cách tạo wav bằng `say` + `afconvert` (xem `qa-howto-macos.md`). |
| S1 | `MT_SETTINGS_DIR` | `settings_path()` trong `src-tauri/src/settings.rs` đọc env `MT_SETTINGS_DIR`; nếu có thì dùng thư mục đó thay cho `~/Library/Application Support/com.personal.translator`. Để QA test P3 trong thư mục tạm, không đụng settings thật. |
| S2 | Seam cho pipeline | `pipeline::start_with_sink(cfg, Box<dyn Fn(LocalEvent) + Send>)` vì `tauri::ipc::Channel` không tạo được trong test. `start()` hiện tại gọi qua seam này. Thêm cách thay LLM bằng stub (ví dụ trait hoặc `cfg(test)` hook) ngủ 5 s để test backlog (P5). |
| S3 | Seam cho mic DSP | `mic_pipeline::MicProcessor` public, có hàm xử lý buffer f32 vào → s16le ra không phụ thuộc cpal, để test P6. |

Sau Commit A, QA chạy P1–P7 (bảng bên dưới).

### Trạng thái Commit A — đã làm (kỹ sư trưởng, 2026-09-25)

Đủ F1, F2, F3, S1, S2, S3. Chỗ khác hoặc thêm so với bảng trên:

- **F1+ pre-roll (mới, phát hiện khi tự chạy P1-dạng):** VAD cắt mất phụ âm đầu câu ngay sau im lặng (`开放时间` → `放时间`). Pipeline giữ vòng đệm 12 s và ghép 300 ms trước mỗi segment. Kiểm với `zh.wav`, im lặng đầu 0/300/500 ms → cả ba ra `开放时间…`. P1 nên kiểm cả chữ đầu câu.
- **S1+ lỗi `.bak`:** `save()` trước đây luôn chép file chính sang `.bak`, nên khi file chính hỏng thì `.bak` tốt bị đè ngay lần lưu sau. Nay chỉ sao lưu nếu file chính parse được. Đây đúng là tình huống P3.
- **S2:** sink có kiểu `Box<dyn Fn(LocalEvent) + Send + Sync>` (phải `Sync` vì hai thread dùng chung). LLM stub đi qua `pipeline::start_with_translator(cfg, sink, factory)`, với `factory: Box<dyn FnOnce() -> Result<Box<dyn Translator>, String> + Send>` chạy trên thread `local-llm`. Thêm `Session::finish()`: đóng audio **không** cancel, flush câu cuối và dịch hết hàng đợi rồi `Closed`. Dùng cho P1/P5 (“câu cuối luôn được dịch”). `drop(Session)` vẫn là dừng ngay.
- **Đường vào cho test tích hợp:** `my_translator_lib::test_api` re-export `MicProcessor`, `MicOptions`, `start_with_sink`, `start_with_translator`, `Session`, `SessionConfig`, `LocalEvent` (có `Debug`), `Translator`, `TranslatorFactory`, `TranslateRequest`, `UTTERANCE_QUEUE_MAX`, `Settings`. Đặt test ở `src-tauri/tests/*.rs`.
- **S3/P6:** `MicProcessor::process` giữ lại tối đa ~1 chunk resample + 511 mẫu (cửa sổ 32 ms). Tự đo: sine 48 kHz stereo 10 s → 159 744/160 000 mẫu (lệch 0,16 %).
- **F2:** trên macOS/aarch64, status `loading` là "Đang khởi tạo Metal (lần đầu ~15 s)…". `Llm::warm_up()` decode 1 token sau khi load; bản CPU bỏ qua warm-up. Nếu LLM load lỗi, pipeline set cancel để ASR dừng.
- Unit test mới (không cần model): `rms_levels`, `filter_rejects_junk_and_repeats`, `history_ring_ranges`, `queue_drops_oldest_and_keeps_newest`.
- Chưa kiểm trên macOS: warm-up Metal (có thật sự bỏ được 13,5 s khỏi câu đầu không). QA đo giúp: thời gian từ `ready` → `Result` đầu tiên, lần chạy đầu sau khi xoá cache Metal.

---

## Commit B — macOS look (làm theo thứ tự U3 → U1 → U2 → U5 → U4 → U6)

Nhận xét gốc của Kiên: *"giao diện không ra một cái app trên macOS lắm"*.

| # | Việc | Chi tiết đã chốt |
|---|---|---|
| U3 | Font hệ thống | `src/styles/main.css`: `font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif`. Bỏ `<link>` Google Fonts trong `src/index.html` và bỏ `fonts.googleapis.com`/`fonts.gstatic.com` khỏi CSP trong `src-tauri/tauri.conf.json`. Lý do: Inter trông "web", và offline (lớp học) font nhấp nháy. |
| U1 | Traffic lights macOS | `tauri.conf.json` cửa sổ `main`: `decorations: true`, `titleBarStyle: "Overlay"`, `hiddenTitle: true`. Giữ `data-tauri-drag-region`. Bỏ nút `×` tự vẽ (`#btn-close`, `.close-btn`). Chừa ~78 px bên trái control-bar cho 3 nút. Windows sẽ có thanh tiêu đề gốc, chấp nhận. |
| U2 | Cửa sổ chính, không phải overlay | `alwaysOnTop: false` mặc định; kích thước mặc định 1000×680 (chế độ "mở rộng" hiện có thành mặc định). Giữ nút 📌 (`⌘P`) và chế độ compact overlay `⤢` cho lúc chiếu slide. |
| U5 | Modal chọn engine chỉ hiện lần đầu | Lưu cờ `engine_picker_done` trong settings; nếu đã có `translation_mode` do người dùng chọn thì không hiện `#engine-picker` khi mở app. Hiện tại modal đè lên app mỗi lần mở, kể cả khi toolbar phía sau đã là "Local ● Ready". |
| U4 | Việt hoá modal | Tiêu đề, 3 thẻ, hint, đơn vị giá ("~0,12 $/giờ"). Thứ tự thẻ: **Soniox → Local → Qwen**; **ẩn OpenAI** khỏi modal (vẫn chọn được trong Cài đặt). |
| U6 | Thẩm mỹ (làm cuối, có thể revert riêng) | Bo góc card 10–12 px; toolbar cao 38 px; nút primary theo accent hệ thống (#0A84FF); nền qua `windowEffects` macOS (`underWindowBackground`, cần `transparent: true`) **có fallback** giữ nền tối nếu effect không khả dụng. Chưa làm light mode (đợi Kiên xác nhận). |

Tiêu chí QA cho Commit B (QA kiểm bằng screenshot trên Mac):
- Traffic lights hiển thị đúng và kéo được cửa sổ bằng toolbar.
- Không còn chữ tiếng Anh trên màn hình chính.
- Mở app lần 2 không hiện modal chọn engine.
- Font là SF; mở app khi offline không đổi font.

---

## Bài kiểm thử QA sẽ chạy (tiêu chí pass/fail)

| Ưu tiên | Kiểm thử | Pass khi | Cần |
|---|---|---|---|
| P1 | Local end-to-end: đẩy wav 8,7 s (+1 s im lặng cuối) vào pipeline theo chunk 200 ms | ≥ 2 `Result`; `Result` đầu ≤ 4 s sau khi câu 1 kết thúc; 0 `Error`; không có result ≤ 1 ký tự; `Closed` ≤ 1 s sau drop `Session` | S2 |
| P2 | Rò rỉ bộ nhớ: 20 chu kỳ start → feed 3 s → drop `Session`; đo RSS trước/sau | RSS sau − trước < 50 MB; không còn thread `local-asr`/`local-llm`/`mic-dsp` | S2 |
| P3 | settings.rs: `settings.json` rỗng/hỏng khi có `.bak` hợp lệ → load | Key API và `profiles` giữ nguyên từ `.bak`; save tiếp theo tạo lại file đúng, `.bak` cập nhật | S1 |
| P4 | Downloader: (a) sai SHA → (b) chặn `huggingface.co` | (a) lỗi "SHA-256 mismatch", không có file đích, `.part` bị xoá; (b) tự chuyển `hf-mirror.com`, tiến độ phát ≤ 256 KiB/lần | — |
| P5 | Backlog Local: LLM stub ngủ 5 s, đẩy 10 câu liên tiếp | Có event `backlog_skipped`; hàng đợi ≤ 4; câu cuối luôn được dịch | S2 |
| P6 | Mic DSP: sine 1 kHz 48 kHz stereo 10 s qua `MicProcessor` (AGC off) | Mẫu ra = vào/3 ± 0,5 %; không alias > −60 dB; HPF loại DC (0,5 → < 0,01); AGC on: −40 dBFS nâng ≤ +18 dB, im lặng không nâng | S3 |
| P7 | Session export: mark ⭐/📝 + notes → `_toMarkdown`, `export_session_srt` | Mục `## Đánh dấu`, `## Ghi chú` đúng; SRT cue 1..n, timestamp tăng dần | — |
| P8 | Nhàn rỗi 60 s | CPU < 1 %, không thread DSP/LLM, không kết nối mạng ngoài updater | **PASS** (đã chạy) |

Các bước cần người thao tác (Kiên làm, QA đọc log): mic cpal + Soniox, khử ồn + VAD,
Apple Voice Processing, Soniox reset 3 phút, Local trong app, ghi chú/đánh dấu.
