# Cách QA chạy các bài test trên macOS

Mọi tệp tạm để trong một thư mục scratch, không để trong repo.

## Build và chạy app

```bash
npm install
npm run dev            # log ra terminal; app tự mở
```

Settings: `~/Library/Application Support/com.personal.translator/settings.json` (+ `.bak`).
Model: `~/Library/Application Support/My Translator/{audio-models,local-models}`.
Phiên: `~/Library/Application Support/com.personal.translator/transcripts/`.

## Clippy và unit test

```bash
cd src-tauri
cargo clippy --all-targets
cargo test
```

## Test với model thật

Model đã tải trong app dùng được trực tiếp. Tạo wav tiếng Trung 16 kHz mono từ TTS macOS (voice `Tingting` có sẵn):

```bash
S=/path/to/scratch
M="$HOME/Library/Application Support/My Translator/local-models"
say -v Tingting "资产负债表反映企业在某一特定日期的财务状况。这个公式期末考试会考，大家注意一下。" -o "$S/zh.aiff"
afconvert -f WAVE -d LEI16@16000 -c 1 "$S/zh.aiff" "$S/zh.wav"
```

Lưu ý: voice `Eddy` (đầu danh sách `say -v '?'`) không cài sẵn, cho ra wav gần rỗng.

```bash
cd src-tauri
# nhận dạng X-ASR (+ hotword) và bộ đọc bpe.model
MT_TEST_XASR_DIR="$M/x-asr-zh-en-punct-int8" MT_TEST_WAV="$S/zh.wav" cargo test --release --lib local:: -- --ignored --nocapture
# dịch Hy-MT2
MT_TEST_GGUF="$M/Hy-MT2-1.8B-Q6_K.gguf" cargo test --release --lib local::llm -- --ignored --nocapture
# ngắt câu 8–12 s dưới tiếng ồn liên tục (cần một wav dài ≥ 330 s có giọng nói + ồn; ghi âm lớp học thật là tốt nhất)
MT_TEST_XASR_DIR="$M/x-asr-zh-en-punct-int8" MT_TEST_VAD="$HOME/Library/Application Support/My Translator/audio-models/silero_vad.onnx" \
  MT_TEST_LONG_WAV="$S/lecture.wav" cargo test --release --test local_pipeline -- --ignored --nocapture
```

Lọc bớt log ggml/llama: `2>&1 | grep -vE "^ggml_|^llama_|^load|^print_info"`.

## Đo app nhàn rỗi (P8)

```bash
PID=$(pgrep -f "target/debug/my-translator" | head -1)
for i in $(seq 1 8); do ps -o %cpu=,rss= -p $PID; sleep 5; done
ps -M -p $PID | tail -n +2 | wc -l        # số thread
lsof -i -a -p $PID -n                      # kết nối mạng
```

## Chụp màn hình để QA giao diện

`screencapture` từ terminal không có quyền Screen Recording sẽ báo
`could not create image from display`. Kiên chụp tay (⌘⇧4) và dán vào chat.
