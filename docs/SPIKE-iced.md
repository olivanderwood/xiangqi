# Spike Iced 0.14 — kết quả (2026-11-13)

## Kết luận
**Iced 0.14 đáp ứng được đúng bản thiết kế** (`docs/DESIGN-iced.md`: theme tối,
hình học bàn cờ, font Hán nhúng, mũi tên gợi ý + hover) **và giữ nguyên được
toàn bộ tính năng hiện có**. Prototype build pass, clippy sạch.

Prototype: `~/workspace/iced-spike` (riêng, không sửa repo), `src/main.rs`
(~950 dòng), `iced 0.14` + feature `canvas`, `tokio`.

## Bằng chứng
- `cargo build` dev lần đầu 246 s, sửa code build lại 13–19 s.
- `cargo build --release` lần đầu 567 s (9′26″), build lại 29 s. Binary
  release **23 MB** (mục tiêu gói <90 MB). Debug binary 363 MB, `target/` 2,8 GB.
  Lock 388 package (egui closure 419 — tương đương).
- `cargo fmt --check` OK, `cargo clippy --all-targets` 0 warning / 0 error.
- Chạy thử trên VM chỉ dừng ở `iced_winit: Create event loop: ... DISPLAY is
  not set` — binary chạy tới bước tạo cửa sổ rồi mới lỗi vì VM không có màn
  hình. Kiểm bằng mắt phải làm trên máy Hieu (Windows/Mac).
- **Font Hán nhúng đã chứng minh bằng thực nghiệm**: subset
  `NotoSerifCJK-Bold.ttc` (27 MB) lấy đúng 18 ký tự cần
  (`俥傌相仕帥炮兵車馬象士將砲卒楚河漢界`) → 10.312 byte, đủ 18/18 ký tự,
  family `"Noto Serif CJK SC"`, Bold. Nhúng bằng `include_bytes!` +
  `font::load` + `Font::with_name`. Lưu ý: tên đúng là `"Noto Serif CJK SC"`
  (không phải `"Noto Serif SC"` như thiết kế viết tắt), và **bắt buộc subset**
  — embed nguyên TTC 27 MB thì binary phình thêm 27 MB.
- Prototype có đủ: theme tối, bàn cờ đúng hình học (`Geom`, cell 64, lề 40,
  592×656, quân r=27 + vòng trong r=22), kéo-thả quân, chọn quân viền xanh,
  chấm xanh / vòng đỏ / tô vàng, 4 mũi tên gợi ý xanh + 1 mũi tên tím nét đứt,
  hover làm đậm mũi tên (`mouse_area` → `Message::Hover(Option<usize>)`,
  alpha 0.38/1.0), panel điểm + `progress_bar` + danh sách nước cuộn +
  `toggler` + `pick_list` + `slider` + ô nhập FEN, animation trượt 150 ms,
  halo chiếu nhấp nháy.

## Chỗ khó / lưu ý (đã kiểm chứng trong source 0.14)
1. `time::every` cần feature `tokio` (hoặc `smol`) — default executor không có.
   Phải bật `features = ["canvas", "tokio"]`.
2. **Không có modal dialog sẵn** (không như `egui::Window`): dialog Cài đặt /
   kết quả phải tự dựng bằng `stack` + render có điều kiện theo state.
3. **rfd không gắn modal được vào cửa sổ Iced** (Iced không expose raw window
   handle ra public API). Dùng `rfd::AsyncFileDialog` qua `Task` (dialog hiện
   rời, không có parent) — đủ dùng cho tải ảnh.
4. Font Hán bắt buộc subset (27 MB → 10 KB).
5. **Iced 0.14 là bản thử nghiệm cuối trước 1.0** — lên 1.0 sẽ phá API một lần.
   Pin `0.14`, chỉ nâng cấp khi có lý do.
6. Không có accessibility trong 0.14 stable; IME tiếng Việt dựa vào winit
   (chưa kiểm chứng trên Windows — cần Hieu gõ thử).

## Theo đính chính của Hieu: chỉ lấy giao diện, không đổi tính năng
Những thứ bản thiết kế vẽ mà app KHÔNG có → **sẽ KHÔNG làm**:
- "Thời gian suy nghĩ mỗi nước [N] giây" (app dùng `time_ms(1500)` cố định).
- "Chủ đề bàn (Gỗ sáng)" (app chỉ có một kiểu bàn cố định).
- "Chơi tiếp với máy" (không có trong `learn.rs`).
- Mức 5 tên "Tối đa" → app giữ tên **"Tùy chỉnh"** (custom Elo).
- Nút "Khôi phục mặc định · Lưu" (app lưu tự động, không có nút này).

Các mục Cài đặt mẫu còn lại app đã có: mức độ, luồng CPU, bộ nhớ hash, đường
dẫn Pikafish, âm thanh, ngôn ngữ — chỉ lấy bố cục/màu sắc, giữ nguyên tính năng.

## Ước lượng viết lại (map `xq-app` → Iced)
- `main.rs` → boot `application`, `font::load`, window 1280×800 (min
  1100×720), `Theme::custom`.
- `app.rs` (870) → `App` (State) + `update` + `view` + `subscription`;
  `Screen` enum (Home/Play/Learn/LearnSelect/Practice/Summary); dialog Cài
  đặt/Giới thiệu thành overlay `stack`.
- `board_view.rs` (1.016) → `BoardProgram` + `Geom` + `Cache` cho lớp bàn tĩnh.
- `widgets.rs` (201) → style nút/card chung; panel cố định 380 px giữ nguyên.
- `theme.rs` (74) → palette tối theo thiết kế.
- `i18n.rs` (563), `sound.rs` (243), `engine_manager.rs`, `engine_log.rs`,
  `settings.rs` → giữ nguyên gần như toàn bộ.
- `screens/menu.rs` → `Screen::Home`; `vs_ai.rs` (1.070) → `Screen::Play`;
  `learn.rs` (1.399) + `learn_select.rs` → `Screen::Learn`; `practice.rs`
  (1.440) → `Screen::Practice`; `summary.rs` → màn/dialog tóm tắt.
- `ai_worker.rs` (915) + `chessdb_worker.rs` (147) → giữ nguyên thread +
  mpsc, thêm cầu nối nhỏ (~50 dòng) sang iced `Subscription`/`Task` (push thay
  vì egui poll mỗi frame).
- `smoke_tests.rs` (373) → kiểm headless của iced (`iced_tester` đi kèm 0.14).
- Khác biệt kiến trúc lớn nhất: (1) immediate-mode → Elm retained (mọi state
  trong `App`, dialog thành state tường minh); (2) state kéo-thả riêng trong
  `Program::State`, giao tiếp qua `Message`; (3) worker poll → push qua
  subscription; (4) không có modal `egui::Window`.
- **Ước lượng: ~1 tuần làm việc tập trung** cho bản viết lại đầy đủ + cổng +
  kiểm bằng mắt trên Windows/Mac. Phần khó nhất (canvas, drag, mũi tên+hover,
  font, theme) đã chứng minh qua spike; phần tốn thời gian nhất là 3 màn chơi
  + worker + kiểm bằng mắt.
- **Nguyên tắc animation-xong-mới-tính**: trong Elm còn dễ giữ hơn egui —
  `update()` chỉ kích hoạt AI/chấm/gợi ý/ChessDB khi `anim.is_none()`;
  subscription tick chỉ chạy khi có animation/halo. Ghi thành bất biến trong
  code bản viết lại.
- Settings persistence: chuyển từ `eframe::Storage` sang file JSON (thư mục
  home) — thay đổi nhỏ.

## Rủi ro chính
1. Breaking change khi lên Iced 1.0 — pin `0.14`.
2. Build release lần đầu ~9′26″ trên VM (wgpu nặng); binary 23 MB, trong mục
   tiêu <90 MB.
3. Không chạy GUI được trên VM — kiểm bằng mắt trên Windows/Mac của Hieu.
4. Modal/file-picker không native như đã nêu.
