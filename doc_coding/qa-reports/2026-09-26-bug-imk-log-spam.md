# Bug QA — log macOS "getApplicationProperty: called with invalid property" — 2026-09-26

Người báo: Kiên (chạy app trên Mac), QA (Claude trên Mac) ghi lại và phân tích.
Commit lúc báo: `6a11b9e` trên `main`.
Mức: **low / cần xác minh** — chưa thấy lỗi chức năng đi kèm, nhưng log spam liên tục.

## Log gốc (Kiên dán, rút gọn)

```
2026-09-26 13:28:08.586 my-translator[36928:369717] ERROR: getApplicationProperty: called with invalid property
2026-09-26 13:28:08.588 my-translator[36928:369717] error messaging the mach port for IMKCFRunLoopWakeUpReliable
2026-09-26 13:28:08.686 my-translator[36928:369717] ERROR: getApplicationProperty: called with invalid property
2026-09-26 13:28:08.944 my-translator[36928:369717] ERROR: getApplicationProperty: called with invalid property
... (lặp lại ~20 lần trong 2,5 s, cách nhau 50–450 ms)
2026-09-26 13:28:11.110 my-translator[36928:369717] ERROR: getApplicationProperty: called with invalid property
```

Tất cả trên cùng thread `369717` (main thread AppKit), process `my-translator`
(binary dev `target/debug/my-translator`, chạy bằng `npm run dev`).

## Môi trường

- macOS 27, Apple M5.
- Bộ gõ đang chọn: **Apple Vietnamese — Simple Telex** (`com.apple.inputmethod.VietnameseIM` / `VietnameseSimpleTelex`). Bộ gõ khác đã bật: ABC, Character Palette. Không có bộ gõ bên thứ ba (EVKey/OpenKey/Unikey).
- Cửa sổ: `decorations: true`, `titleBarStyle: "Overlay"`, `hiddenTitle: true`, `macOSPrivateApi: true`, `transparent: false`.

## Phân tích của QA (giả thuyết, chưa xác minh)

1. `IMKCFRunLoopWakeUpReliable` là thông điệp của **InputMethodKit**. Nhịp log 50–450 ms khớp với nhịp gõ phím, nên nhiều khả năng lỗi xuất hiện khi Kiên **gõ tiếng Việt Telex vào một ô nhập liệu trong WebView** (ô ghi chú `⌘⇧N`, ô glossary, ô tìm kiếm…).
2. `getApplicationProperty: called with invalid property` là bộ gõ hỏi thuộc tính của ứng dụng (thường đọc từ `Info.plist` / bundle). Binary dev `target/debug/my-translator` **không nằm trong `.app` bundle**, nên có thể bộ gõ không lấy được thuộc tính → log lỗi. Nếu đúng, bản release `.app` sẽ không có log này.
3. Có thể liên quan `macOSPrivateApi` hoặc `titleBarStyle: Overlay` (mới đổi ở Commit B) nếu log chỉ xuất hiện sau commit đó.

## Việc cần kỹ sư trưởng làm

1. Xác định **khi nào** log xuất hiện: nhờ Kiên thử (a) gõ Telex vào ô ghi chú, (b) gõ với bộ gõ ABC, (c) không gõ gì, chỉ bấm Bắt đầu/Dừng.
2. So sánh bản dev với bản release `.app` (`npm run build:dev` → mở `.app`). Nếu bản `.app` sạch log → đây chỉ là nhiễu của binary dev, ghi vào Troubleshooting trong README và đóng bug.
3. **Quan trọng hơn log**: kiểm tra gõ tiếng Việt Telex trong WebView có **lỗi chức năng** không — mất dấu, lặp ký tự, con trỏ nhảy, chữ gạch chân (marked text) không commit. Đây là lỗi hay gặp với WKWebView + bộ gõ tiếng Việt, và ảnh hưởng trực tiếp tới ô ghi chú khi học trên lớp.
4. Nếu có lỗi chức năng: kiểm tra xử lý `compositionstart/compositionend` / `isComposing` trong JS (các phím tắt `⌘⇧…` và handler `keydown` không được chặn phím khi `event.isComposing === true`).

## Tiêu chí đóng bug

- Gõ "Tiếng Việt có dấu đầy đủ" bằng Telex vào ô ghi chú: chữ ra đúng, không lặp, không mất dấu.
- Bản `.app` release không in log `getApplicationProperty` khi gõ; hoặc nếu chỉ bản dev in, đã ghi chú trong README Troubleshooting.
