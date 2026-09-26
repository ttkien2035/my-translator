# doc_coding — phối hợp giữa hai Claude session

Hai máy, hai Claude Code session, một repo:

| Máy | Vai trò | Đường dẫn repo |
|---|---|---|
| WSL2 (Ubuntu 24.04, `ttkien`) | **Kỹ sư trưởng**: thiết kế, viết code, commit | `/mnt/d/working/projects/my-translator` |
| MacBook (Apple Silicon) | **QA/QC**: build, chạy, kiểm thử trên macOS, feedback | `~/Documents/projects/my-translator` |

Máy WSL chỉ `cargo check`/`clippy` được cho Linux. Mọi mã macOS-only
(ScreenCaptureKit, `mic_vpio.rs`, Metal) chỉ build và chạy được trên Mac.

## Quy trình

1. Kỹ sư trưởng commit trên `feature/lecture-assistant`, fast-forward `main`, push cả hai.
2. QA pull `main`, build, chạy các bài kiểm thử, ghi kết quả vào `qa-reports/`.
3. Kỹ sư trưởng đọc báo cáo, cập nhật kế hoạch trong `work-plan.md`, làm vòng tiếp.
4. Hai bên không sửa cùng một file trong cùng một vòng để tránh xung đột.

## Tệp

- `work-plan.md` — kế hoạch việc đã chốt giữa hai bên (Commit A, Commit B) và tiêu chí pass/fail. **Kỹ sư trưởng đọc tệp này trước khi làm.**
- `qa-reports/2026-09-25-round1-2.md` — báo cáo QA vòng 1 và 2 trên macOS.
- `qa-reports/2026-09-26-bug-imk-log-spam.md` — bug: log `getApplicationProperty: called with invalid property` khi gõ Telex trên macOS.
- `qa-howto-macos.md` — cách QA đã chạy các bài test trên Mac (lệnh cụ thể), để lặp lại.
