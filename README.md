# Cờ Tướng AI

Game cờ tướng desktop viết bằng Rust: hai chế độ **Đấu với AI** và **Học chơi khai cuộc**, build Windows + macOS qua GitHub Actions.

AI mặc định là **Pikafish** (engine cờ tướng mã nguồn mở mạnh nhất hiện nay, chạy tiến trình con qua UCI, offline hoàn toàn). Engine alpha-beta tự viết vẫn còn, làm dự phòng khi Pikafish không chạy được (tự chuyển, có banner báo).

## Chạy

```sh
cargo run --release -p xq-app   # chơi ngay (tự dùng Pikafish nếu có)
cargo test --workspace           # chạy test
./tools/fetch-engine.sh          # tải Pikafish (binary + NNUE) vào engine/
```

Yêu cầu: Rust stable (xem `rust-toolchain.toml`). Windows cần Visual Studio Build Tools (workload "Desktop development with C++"); macOS cần `xcode-select --install`. `fetch-engine.sh` cần `curl` + trình giải nén 7z (`7z`, `7zz`, `bsdtar` hoặc `python3 -m py7zr`).

## Cấu trúc

- `crates/xq-core` — luật cờ + AI alpha-beta dự phòng (không phụ thuộc giao diện)
- `crates/xq-engine` — ranh giới engine chung (trait `Engine`, mức độ, chấm điểm)
- `crates/xq-pikafish` — client UCI cho Pikafish (tiến trình con, handshake, khôi phục lỗi)
- `crates/xq-book` — sách khai cuộc (`data/openings.json`) + Tutor
- `crates/xq-app` — giao diện eframe/egui
- `tools/book-check` — CLI kiểm tra sách khai cuộc bằng engine
- `engine/` — binary Pikafish + `pikafish.nnue` + giấy phép (không commit vào git; lấy bằng `tools/fetch-engine.sh`, ghim phiên bản ở `engine/PIKAFISH_VERSION.toml`)

## Tiến độ

Xem `docs/DECISIONS.md` — ghi tiến độ từng milestone và mọi điểm lệch so với đặc tả. Tích hợp Pikafish: xem `docs/DECISIONS-pikafish.md` và `docs/research-pikafish-2026-09-06.md`.

## Giấy phép

Code: MIT. Font Noto: SIL Open Font License 1.1 (xem `crates/xq-app/assets/fonts/OFL.txt`). Engine Pikafish: GPLv3 (`engine/Copying.txt`, mã nguồn tại github.com/official-pikafish/Pikafish). Mạng nơ-ron `pikafish.nnue`: NNUE-License, chỉ dùng phi thương mại — app phát hành miễn phí, phi thương mại.
