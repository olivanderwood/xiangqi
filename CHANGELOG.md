# Changelog

Định dạng theo [Keep a Changelog](https://keepachangelog.com/vi/1.1.0/).
Nội dung release trên GitHub lấy từ đoạn của đúng phiên bản trong file này
(`tools/release-notes.py`).

## [1.0.0] — chưa phát hành

> Chưa phát hành: tag `v1.0.0` chưa tạo. Trước khi tag, đổi `version`
> trong `Cargo.toml` của 3 crate từ `0.1.0` lên `1.0.0` và chạy hết
> checklist trong `docs/release-checklist.md` trên Windows và macOS.

### Thêm mới

- Đấu với AI: 4 mức độ (Dễ, Trung bình, Khó, Cao thủ), alpha-beta +
  PVS + bảng băm + quiescence, gợi ý top 3, hoàn nước, xin hòa/thua,
  xem lại ván, chơi 2 người trên một máy.
- **AI mặc định nay là Pikafish 2026-09-06** (thay cho alpha-beta tự
  viết, giờ làm dự phòng): chạy tiến trình con qua UCI, offline hoàn
  toàn, tự chuyển sang AI tích hợp + banner khi Pikafish lỗi. Mức khó
  mới: Dễ (depth 3 + chọn nước yếu hơn), Trung bình (depth 6), Khó
  (depth 12), Cao thủ (không giới hạn depth, 5 s/nước, đa dạng nước
  đi trong 24 nửa nước đầu), Elo tùy chỉnh (thanh trượt, bước 50).
  Cài đặt có Engine đang dùng, Threads, Hash (tự động theo RAM),
  đường dẫn Pikafish, nút Thử lại Pikafish; log `engine.log` xoay
  vòng; màn Giới thiệu có phiên bản Pikafish + GPLv3 + giấy phép NNUE.
  Pikafish thắng AI cũ 10/10 ván (đo trên Linux).
- Học chơi khai cuộc: 12 bài (6 đi tiên, 6 đi hậu), gia sư chấm từng
  nước theo sách + engine, gợi ý, đi lại/tiếp tục khi ra khỏi sách,
  tổng kết cuối bài, đánh tiếp với AI giữ nguyên thế cờ.
- Luật cờ tướng đầy đủ: perft thế khởi đầu 44 / 1.920 / 79.666 /
  3.290.240 / 133.312.995, chiếu dai thua, lặp 3 lần hòa, giới hạn
  120 nửa nước, không đủ lực chiếu hòa.
- Giao diện egui: bàn cờ vẽ tay, quân chữ Hán phồn thể, lật bàn,
  hiệu ứng trượt quân, mũi tên gợi ý, thanh đánh giá, font Noto
  subset 166 KB (CJK + tiếng Việt).
- Build phát hành qua GitHub Actions: Windows x64 `.zip`, macOS
  universal `.dmg` (xem `.github/workflows/release.yml`).

### Hạn chế đã biết (bản 1.0)

- Chưa ký số: Windows SmartScreen và macOS Gatekeeper hiện cảnh báo
  ở lần mở đầu (xem hướng dẫn trong `README-vi.txt` và trang Release).
- Công tắc âm thanh có trong Cài đặt nhưng chưa phát ra tiếng
  (không có backend âm thanh).
- Bài `b-chuangong` chưa có nhánh phụ cho Đỏ đi khác ở nước 4/6
  (không tìm được nhánh đã xác minh; xem `docs/DECISIONS-m4.md`).
- Hai nguồn Uyên ương pháo (NetEase) và Xuyên cung mã (Zhihu) chỉ
  đối chiếu được qua nguồn phụ, mức xác minh trung bình.
- Không có chơi online, chơi 2 người qua mạng, engine ngoài
  (UCCI/Pikafish), xử lý đầy đủ luật bắt dai theo AXF.

## [0.1.0] — 2026-11-10

- Khung workspace 3 crate + luật cờ (M0–M1).
