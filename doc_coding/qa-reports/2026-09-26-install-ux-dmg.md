# QA — cài đặt như người dùng không rành kỹ thuật (DMG) — 2026-09-26

Người test: Kiên, đóng vai người dùng phổ thông, cài bản release tải bằng Safari.
Tài liệu theo: `docs/huong-dan-cai-dat.md` (và PDF), mục **2. Cài đặt và mở lần đầu**.
Máy: MacBook Air **M5**, macOS 27.

## Kiên gặp gì

Sau khi kéo biểu tượng **MeowLaoshi** vào **Applications** trong cửa sổ DMG, Kiên
**không tìm thấy bước 2** của hướng dẫn:

> 2. Bấm nút ⏏ cạnh “MeowLaoshi” ở thanh bên Finder để tháo đĩa.

Cửa sổ DMG chỉ có hai biểu tượng trên nền trắng, **không có thanh bên, không có
nút ⏏, không có chữ hướng dẫn**. Người dùng đang nhìn vào cửa sổ đó nên không
biết "thanh bên Finder" ở đâu, và bị kẹt ngay bước thứ hai.

QA kiểm trên máy: app đã được chép vào `/Applications/MeowLaoshi.app` (có cờ
`com.apple.quarantine` từ Safari, đúng như dự kiến), và DMG đã tự tháo. Vậy việc
cài thực ra đã xong; chỉ là hướng dẫn làm người dùng tưởng còn thiếu một bước.

## Findings

| # | Mức | Vấn đề | Đề xuất |
|---|---|---|---|
| I1 | **high** (chặn người dùng phổ thông) | Bước 2 chỉ vào giao diện không có trong cửa sổ người dùng đang nhìn. Tháo đĩa cũng **không bắt buộc**: DMG tự tháo khi khởi động lại, app đã chạy được từ Applications. | Bỏ bước 2 khỏi luồng chính. Thay bằng một dòng: "Xong thì đóng cửa sổ này." Nếu muốn giữ, ghi là *không bắt buộc* và chỉ chỗ dễ thấy: biểu tượng ổ đĩa **MeowLaoshi** trên màn hình nền → bấm chuột phải → **Đẩy "MeowLaoshi"**. |
| I2 | medium | Cửa sổ DMG không nói bước tiếp theo là gì, nên người dùng dễ mở app ngay trong DMG thay vì từ Applications. | Thêm ảnh nền DMG có mũi tên và 2–3 dòng tiếng Việt, ví dụ: "① Kéo MeowLaoshi vào Applications. ② Mở Applications › MeowLaoshi. ③ Bị chặn? Cài đặt hệ thống › Quyền riêng tư & Bảo mật › Vẫn mở." Tauri 2 hỗ trợ `bundle.macOS.dmg.background`, `windowSize`, `appPosition`, `applicationFolderPosition` trong `tauri.conf.json`. |
| I3 | medium | Hướng dẫn chỉ có chữ. Người dùng phổ thông cần ảnh ở hai chỗ khó: cửa sổ DMG, và nút **Vẫn mở** trong Quyền riêng tư & Bảo mật. | Thêm 2 ảnh chụp vào `huong-dan-cai-dat.md` và PDF. QA có thể chụp trên Mac nếu cần. |
| I4 | low | README và bảng chọn file ghi "Apple silicon (M1/M2/M3/M4)". Kiên dùng **M5** nên người dùng máy mới có thể nghi không đúng bản. | Ghi "Apple silicon (M1 trở lên)" / "Apple silicon (M1 or later)" ở README, README.vi và hướng dẫn. |

## Tiêu chí đóng

- Một người chưa từng cài app ngoài App Store làm theo hướng dẫn từ đầu đến lúc app mở được mà không phải hỏi ai.
- Trong luồng chính không còn bước nào chỉ vào thứ không nhìn thấy trên màn hình lúc đó.
- Cửa sổ DMG tự nói bước tiếp theo.

## Bước tiếp theo của Kiên (đang test dở)

Chuyển sang bước 3 của hướng dẫn: mở **Applications › MeowLaoshi** → macOS chặn →
**Cài đặt hệ thống › Quyền riêng tư & Bảo mật** → **Vẫn mở**. QA sẽ ghi tiếp kết
quả bước này vào báo cáo.
