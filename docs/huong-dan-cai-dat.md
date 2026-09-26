# MeowLaoshi 猫老师

*Hướng dẫn cài đặt và sử dụng · bản 1.0 · cập nhật 26-09-2026*

MeowLaoshi (猫老师, “thầy mèo”) nghe giảng viên nói tiếng Trung và hiện bản dịch tiếng Việt ngay trên máy Mac, kèm ghi chú theo từng câu. Cài và thiết lập mất khoảng 10 phút, chỉ làm một lần.

**Cần có:** máy Mac chạy macOS 13 (Ventura) trở lên, mật khẩu đăng nhập máy, và API key Soniox do tác giả cung cấp nếu dịch qua mạng (xem mục 4.1).

---

## 1. Chọn đúng file

Xem loại chip: bấm biểu tượng quả táo ở góc trên bên trái màn hình → **Giới thiệu về máy Mac này** → dòng **Chip**.

| Máy của bạn | File cần mở |
|---|---|
| Chip Apple (M1, M2, M3, M4, M5) | `MeowLaoshi_1.0.0_aarch64.dmg` |
| Chip Intel | `MeowLaoshi_1.0.0_x64.dmg` |

## 2. Cài đặt và mở lần đầu

App phát hành miễn phí, chưa ký bằng tài khoản Apple Developer, nên macOS chặn ở lần mở đầu. Đây là bình thường; bước 3 chỉ làm một lần.

1. Nháy đúp file `.dmg`, kéo biểu tượng **MeowLaoshi** vào thư mục **Applications**.
2. Bấm nút ⏏ cạnh “MeowLaoshi” ở thanh bên Finder để tháo đĩa. File `.dmg` có thể xoá sau bước này.
3. Mở app lần đầu:
    1. Mở **Applications** → nháy đúp **MeowLaoshi**. macOS báo không mở được → bấm **Xong** (không bấm “Chuyển vào Thùng rác”).
    2. Mở **Cài đặt hệ thống** → **Quyền riêng tư & Bảo mật** → kéo xuống mục **Bảo mật**.
    3. Bấm **Vẫn mở** cạnh dòng “MeowLaoshi bị chặn…” → nhập mật khẩu máy → **Mở**.
4. Khi app hỏi quyền **Micrô**, bấm **Cho phép**. Không có quyền này thì app không nghe được giảng viên.
5. Quyền **Ghi màn hình & âm thanh hệ thống** chỉ cần khi muốn dịch âm thanh phát từ máy (Zoom, video). Bật xong, macOS có thể yêu cầu mở lại app.

> Quen dùng Terminal thì thay bước 3 bằng lệnh:
> `xattr -dr com.apple.quarantine /Applications/MeowLaoshi.app`

## 3. Chọn cách dịch

Lần đầu mở, app hỏi **Chọn cách dịch**. Nên chọn **Soniox** để dùng trên lớp và tải thêm **Local** làm dự phòng khi mất mạng. Đo trên cùng một bài giảng tài chính thật, Soniox sai khoảng 3 % số chữ, Local khoảng 11 %.

| Cách dịch | Cần gì | Khi nào dùng |
|---|---|---|
| **Soniox** (khuyên dùng) | Internet, API key Soniox do tác giả cung cấp; ở Trung Quốc đại lục có thể cần VPN | Mọi buổi học có mạng; chính xác nhất, chữ hiện gần như tức thì |
| **Local** (offline) | Tải model 1,6 GB một lần | Mất mạng, không có VPN; bản dịch hiện 1–2 giây sau mỗi câu |
| Qwen LiveTranslate | Internet, API key DashScope của Alibaba | Thử nghiệm; không dùng được từ điển môn học |

Sau này đổi cách dịch ngay trên thanh công cụ hoặc trong **Cài đặt › Model**.

## 4. Thiết lập một lần

Mở **Cài đặt** bằng nút ⚙ trên thanh công cụ (hoặc `⌘,`). Làm xong thì bấm **Lưu & đóng** ở cuối trang.

### 4.1. API key Soniox

Bạn không cần tự tạo tài khoản Soniox.

1. Liên hệ tác giả (người gửi bạn file này) để được cấp API key Soniox.
2. Trong app: **Cài đặt › Engine dịch** → dán key vào ô **Soniox API key**.
3. Ngôn ngữ nguồn **Chinese**, đích **Vietnamese** (đã là mặc định).

Key được cấp riêng cho bạn: không chia sẻ cho người khác.

### 4.2. Hồ sơ môn học và từ điển

Từ điển giúp app nhận đúng và dịch đúng thuật ngữ chuyên ngành. Đo trên bài giảng tài chính, số thuật ngữ được dịch đúng cách tăng từ 41 lên 88 trên 133.

1. **Cài đặt › Engine dịch** → mục **Hồ sơ môn học** → bấm **+**, đặt tên theo môn (ví dụ “Tài chính doanh nghiệp”).
2. Bấm **📚 Nạp từ điển tài chính Trung–Việt**: nạp khoảng 480 thuật ngữ tài chính, kinh tế, kế toán và tên các học viện của ĐH Tài chính Kinh tế Trung ương.
3. Giảng viên hay dùng từ riêng thì thêm vào danh sách thuật ngữ (tiếng Trung → tiếng Việt).

Mỗi môn một hồ sơ. Có từ hai hồ sơ trở lên, thanh công cụ hiện ô chọn hồ sơ; đổi giữa giờ cũng được.

### 4.3. Tải model (cần Internet, một lần)

- **Cài đặt › Micro** → **Tải model** (khoảng 1 MB). Bắt buộc nếu muốn dùng Local.
- **Cài đặt › Model › Local** → **Tải model (1,6 GB)**. Chỉ cần nếu muốn dịch offline; nên tải khi có Wi-Fi ổn định.

### 4.4. Micro

Giữ mặc định trong **Cài đặt › Micro**: **bật** “Lọc tiếng ù / rung”, **tắt** “Khử tiếng ồn nền” và “Tự động tăng âm (AGC)”. Đo trên bài giảng thật, hai mục này làm nhận dạng sai nhiều hơn trong phòng có tiếng vang và tiếng sinh viên.

Micro MacBook đặt cách giảng viên 5–10 m sẽ giảm độ chính xác rõ rệt. Ngồi gần bục giảng hoặc dùng micro rời (loại kẹp áo không dây) tốt hơn nhiều.

## 5. Dùng trên lớp

1. Trên thanh công cụ chọn nguồn **🎤 Mic** và hồ sơ môn hôm nay.
2. Bấm **▶ Bắt đầu** (hoặc `⌘↩`). Câu tiếng Trung và bản dịch hiện dần theo lời giảng viên.
3. Ghi chú và đánh dấu trong lúc nghe bằng các phím tắt dưới đây.
4. Hết buổi bấm **Dừng**. Buổi học tự lưu vào **📚 Thư viện**.

| Phím | Tác dụng |
|---|---|
| `⌘↩` | Bắt đầu / Dừng |
| `⌘⇧N` | Mở / đóng khung ghi chú |
| `⌘⇧C` | Chép câu vừa dịch vào ghi chú, kèm giờ |
| `⌘⇧1` | Đánh dấu ⭐ ý quan trọng |
| `⌘⇧2` | Đánh dấu ❓ chưa hiểu, cần hỏi lại |
| `⌘⇧3` | Đánh dấu 📝 sẽ thi (app cũng tự đánh dấu khi giảng viên nói 会考 / 考点) |
| `⌘,` | Mở Cài đặt |
| `?` | Xem toàn bộ phím tắt |

**Ôn bài:** mở **📚 Thư viện** → chọn buổi. Lọc 📝 để xem phần sẽ thi, lọc ❓ để chuẩn bị câu hỏi, viết tiếp ghi chú. Bấm **Chép** để lấy nội dung dạng Markdown, dán vào Notion hoặc Apple Notes.

Nên xin phép giảng viên trước khi ghi âm trong lớp.

## 6. Khi gặp lỗi

| Hiện tượng | Cách xử lý |
|---|---|
| macOS báo “MeowLaoshi bị hỏng và không thể mở” | App không hỏng; đây là cảnh báo với app chưa ký. Làm lại bước 3 ở mục 2, hoặc chạy lệnh `xattr` ở cuối mục 2 |
| Bấm Bắt đầu mà không hiện chữ | **Cài đặt hệ thống › Quyền riêng tư & Bảo mật › Micrô** → bật MeowLaoshi rồi mở lại app; kiểm tra nguồn đang là **🎤 Mic** |
| Soniox báo lỗi 401 hoặc 402 | Key bị dán thiếu hoặc không còn dùng được: dán lại key, nếu vẫn lỗi thì liên hệ tác giả |
| Soniox không kết nối, hoặc báo “Mạng chậm” liên tục | Bật VPN hoặc đổi Wi-Fi; không có mạng thì chuyển sang **Local** |
| Local báo “cần tải model” | Làm mục 4.3: tải cả model trong **Micro** và trong **Model › Local** |
| Bản dịch bỏ qua vài câu | Khi máy hoặc mạng chậm, app bỏ câu cũ để theo kịp giảng viên. Bình thường; ngồi gần hơn hoặc dùng Soniox sẽ ít gặp |
| Nhận dạng sai nhiều | Xem mục 4.4 (tắt Khử ồn và AGC), ngồi gần giảng viên, chọn đúng hồ sơ môn |

**Cập nhật app:** **Cài đặt › Giới thiệu › Kiểm tra bản mới**. App tự tải và cài bản mới, không phải làm lại bước vượt cảnh báo.

**Dữ liệu của bạn:** API key, hồ sơ, từ điển và các buổi học đều nằm trên máy bạn. Âm thanh chỉ gửi tới Soniox khi bạn dùng Soniox.

Vẫn kẹt thì nhắn cho người gửi bạn file này, kèm ảnh chụp màn hình lỗi.
