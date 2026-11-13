# Thiết kế giao diện Iced — Cờ Tướng AI

Nguồn: `~/workspace/user/files/C__t__ng___Thi_t_k__giao_di_n__iced.html`
(bản thiết kế 7 trang, Hieu gửi 2026-11-13 09:32). Đây là bản tham chiếu cho
bản viết lại UI bằng Iced.

> **Nguyên tắc Hieu chốt 2026-11-13 09:34: chỉ follow GIAO DIỆN, BỐ CỤC và
> MÀU SẮC của bản thiết kế — KHÔNG chỉnh sửa tính năng để đáp ứng giao diện
> mẫu.** Tính năng giữ nguyên y như app hiện tại: không thêm tính năng chỉ vì
> bản thiết kế vẽ nó (vd: không thêm cài đặt luồng CPU / bộ nhớ hash / đường
> dẫn Pikafish chỉ vì màn Cài đặt mẫu có), không bớt hay đổi tính năng chỉ vì
> bản thiết kế không vẽ nó. Bản thiết kế là chuẩn về mặt nhìn, không phải
> chuẩn về mặt tính năng.

## 1. Quy chuẩn giao diện (trang 7)

### Màu sắc (theme TỐI — khác theme giấy sáng hiện tại của egui)
| Vai trò | Mã màu |
|---|---|
| Nền cửa sổ | `#17191D` |
| Bề mặt | `#20232A` |
| Bề mặt 2 (nút, hàng chọn) | `#2A2E36` |
| Viền | `#3A3F4A` |
| Chữ | `#ECE7DC` |
| Chữ phụ | `#A9A599` |
| Nhấn (vàng) | `#D4A24C` |
| Cảnh báo / bên Đỏ | `#EA6A5F` |
| Thành công / sẵn sàng | `#5DB08A` |
| Gợi ý của bạn (khai cuộc) | `#2F6FD0` (xanh dương) |
| Gợi ý của đối phương | `#7B3FC4` (tím) |
| Gỗ bàn cờ | `#E2BC82` |
| Đường kẻ bàn | `#4A3320` |
| Nền quân | `#F6EAD0` |
| Quân Đỏ | `#B3261E` |
| Quân Đen | `#2B2B2B` |

### Chữ
- Tiêu đề lớn 36 · Tiêu đề màn hình 20 · Tiêu đề thẻ 16 · Nội dung 15 ·
  Chú thích 13 · Chữ quân (Hán) 30.
- Chữ Hán: nhúng font **Noto Serif SC** (`include_bytes`), `Font::with_name`.

### Khoảng cách và bo góc
- Khoảng cách: 4 / 8 / 12 / 16 / 24 / 32. Bo góc: 6 / 10 / 14.

### Cửa sổ và bàn cờ
- Cửa sổ mặc định 1280 × 800, tối thiểu 1100 × 720.
- Ô lưới (cell) 64 px · Lề bàn 40 px · Bàn cờ 592 × 656 px.
- Bán kính quân 27 px (vòng trong 22).
- Co giãn: `scale = min(w/592, h/656)`.
- Tâm quân: `(40 + x·64, 40 + y·64)`, y = 0 là hàng Đỏ (hàng Đen ở trên).

### Quy tắc tương tác trên bàn cờ
- Chọn quân: viền xanh. Nước hợp lệ: chấm xanh; ăn quân: vòng đỏ.
- Nước vừa đi: tô vàng.
- Màn Chơi và Học dùng chung bố cục và widget bàn cờ.
- Học khai cuộc thêm 3–4 gợi ý: mũi tên xanh dương, số 1–4 trùng với danh
  sách bên phải. Lượt đối phương: mũi tên tím nét đứt.
- Mặc định các mũi tên mờ 38%; rê vào một mục gợi ý thì mũi tên của mục đó
  đậm 100%.

### Ánh xạ sang Iced (theo bản thiết kế)
- Dùng **iced 0.14, bật feature canvas**.
- Nút chính / phụ / nguy hiểm: `button` + style tự định nghĩa.
- Thẻ, panel: `container` với background, border, radius.
- Chọn Tiên/Hậu, mức độ: row các button (hoặc radio).
- Thanh đánh giá: canvas hoặc progress_bar dọc.
- Thanh trượt: `slider`. Công tắc: `toggler`. Danh sách thả xuống: `pick_list`.
- Danh sách nước đi: `scrollable` + `column`.
- Bàn cờ và quân: `canvas::Program`, `Frame`, `Path`, `fill_text`, `Cache`.
- Chuyển màn hình: `enum Screen { Home, Play, Learn, Settings }`.
- Rê vào mục gợi ý → đậm mũi tên: `mouse_area` (`on_enter` / `on_exit`)
  gửi `Message::Hover(Option<usize>)`, canvas vẽ lại.
- Máy tính chạy nền: `Task` / subscription, gửi `Message::EngineMove`.

## 2. Các màn hình (bố cục tham chiếu; chữ trong [ngoặc vuông] là chỗ giữ chỗ, lấy dữ liệu thật)

### 1 · Màn hình chính
- Tiêu đề "Cờ Tướng", phụ "Chơi, học khai cuộc và luyện tập".
- 3 thẻ: **Chơi với máy** ("Đấu với Pikafish ở mức độ bạn chọn. Có thể hoàn
  nước và xin gợi ý." — nút "Bắt đầu ván mới") · **Học khai cuộc** ("Chọn Tiên
  hoặc Hậu, máy làm đối thủ và gợi ý các nước đi theo khai cuộc." — nút "Vào
  học") · **Tự luyện tập** ("Tự đi quân cho cả hai bên trên bàn cờ, không có
  máy đối thủ." — nút "Vào luyện tập").
- Cài đặt · Phiên bản [x.y.z] · Pikafish [phiên bản].

### 2 · Chơi với máy
- Header: Quay lại · "Chơi với máy" · "Mức: Cao thủ" · "Pikafish sẵn sàng" ·
  Cài đặt.
- Thẻ đối thủ: Pikafish · "Cao thủ · độ sâu [N]" · "Chờ bạn".
- Danh sách nước đi: "Ván 1", bảng # / Đỏ (Tiên) / Đen (Hậu), vd "Pháo 2 bình
  5", "Mã 8 tiến 7".
- Thẻ người chơi: Bạn · "Bên Tiên (Đỏ)" · "Đến lượt bạn".
- Nút: Hoàn nước · Gợi ý · Xin hòa · Xin thua · Ván mới.

### 3 · Học khai cuộc: chọn bên và khai cuộc
- "Chọn bên và khai cuộc" · "Máy sẽ là đối thủ và gợi ý các nước đi theo
  khai cuộc."
- Bạn chơi bên: Tiên (Đỏ) / Hậu (Đen).
- Khai cuộc: Trung pháo (Tiên) · Xuyên cung mã (Phòng thủ) · Uyên ương pháo
  (Tiên) — mỗi bài có [Mô tả ngắn].
- Nút "Bắt đầu học".

### 4 · Học khai cuộc: lượt của bạn
- Header: Khai cuộc: Trung pháo · Bạn: Tiên (Đỏ) · "Pikafish sẵn sàng".
- Danh sách nước đi: "Bước 2 / [N]".
- "Gợi ý nước tiếp theo cho bạn": 1–4 mục, vd "Mã 2 tiến 3", "Mã 8 tiến 7",
  "Tốt 7 tiến 1", "Tốt 3 tiến 1", mỗi mục có [điểm].
- Nút: Hoàn nước · Ẩn gợi ý · Đổi khai cuộc · Đặt lại · Dừng và tóm tắt ·
  Chơi tiếp với máy.

### 5 · Học khai cuộc: lượt đối phương
- Như màn 4 nhưng: "Đang chọn nước" · "Bước 1 / [N]" · "Đối phương có thể
  đi" (4 mục) · "Chờ đối phương".

### 6 · Cài đặt
- Máy tính (Pikafish): Mức độ (Dễ / Vừa / Khó / Cao thủ / Tối đa) · Thời gian
  suy nghĩ mỗi nước [N] giây · Số luồng CPU [N] · Bộ nhớ băm (hash) [N] MB ·
  Đường dẫn Pikafish.
- Bàn cờ: Chủ đề bàn (Gỗ sáng) · Hiện nước đi hợp lệ · Đánh dấu nước vừa đi ·
  Hiện thanh đánh giá.
- Chung: Âm thanh · Ngôn ngữ (Tiếng Việt).
- Nút: Khôi phục mặc định · Lưu.

## 3. Tính năng giữ nguyên y như app hiện tại (không theo bản thiết kế)
- Mode Tự luyện tập: chọn độ khó, bắt đầu/FEN/ảnh + soạn thế cờ thủ công, tự
  đi cả Đỏ lẫn Đen, ChessDB chấm + gợi ý top 4, chấm cả nước ngoài gợi ý.
- Section "Điểm ChessDB" ở cả 2 màn (Chơi + Học) — điểm góc Đỏ, tỉ lệ
  thắng/hòa/thua, nước tốt nhất; không mạng thì hiện "không khả dụng".
- Tải ảnh từ thiết bị (rfd) + báo không nhận diện được khi không nhận diện được.
- Âm thanh đi/ăn/chiếu/chiếu hết; halo chiếu nhấp nháy; hiệu ứng ăn quân.
- Localize EN/VI; ký hiệu nước đi Việt/Anh; nút Reset ở cả 3 mode.
- Nguyên tắc vàng: quân phải đặt xuống và animation chạy XONG rồi mới tính
  toán (AI, chấm, gợi ý, ChessDB).
- Ngược lại, KHÔNG thêm tính năng chỉ vì bản thiết kế vẽ nó: màn Cài đặt mẫu
  có mức độ / thời gian suy nghĩ / luồng CPU / hash / đường dẫn Pikafish —
  chỉ lấy bố cục và màu sắc, tính năng nào app chưa có thì không thêm.
