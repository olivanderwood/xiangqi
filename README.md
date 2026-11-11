# Cờ Tướng AI

Game cờ tướng desktop viết bằng Rust: hai chế độ **Đấu với AI** và **Học chơi khai cuộc**, build Windows + macOS qua GitHub Actions.

## Chạy

```sh
cargo run --release -p xq-app   # chơi ngay
cargo test --workspace           # chạy test
```

Yêu cầu: Rust stable (xem `rust-toolchain.toml`). Windows cần Visual Studio Build Tools (workload "Desktop development with C++"); macOS cần `xcode-select --install`.

## Cấu trúc

- `crates/xq-core` — luật cờ + AI (không phụ thuộc giao diện)
- `crates/xq-book` — sách khai cuộc (`data/openings.json`) + Tutor
- `crates/xq-app` — giao diện eframe/egui
- `tools/book-check` — CLI kiểm tra sách khai cuộc bằng engine

## Tiến độ

Xem `docs/DECISIONS.md` — ghi tiến độ từng milestone và mọi điểm lệch so với đặc tả.

## Giấy phép

Code: MIT. Font Noto: SIL Open Font License 1.1 (xem `crates/xq-app/assets/fonts/OFL.txt`).
