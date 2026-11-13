# Viết lại UI Cờ Tướng sang Iced 0.14 — Báo cáo hoàn thành

Ngày: 2026-11-13. Trạng thái: **xong, qua cổng đầy đủ**. Chưa commit, chưa
push — xem §8.

## 1. Phạm vi

- Crate mới `crates/xq-iced`, binary `xiangqi-iced` (Iced 0.14, features
  `canvas`, `tokio`, `advanced`, `image`), thêm vào workspace members.
  Tổng **11.999 dòng** (kể cả test).
- App egui `crates/xq-app` **không sửa** (vẫn build + chạy bình thường).
  Core crates (`xq-core`, `xq-book`, `xq-engine`, `xq-pikafish`,
  `xq-chessdb`) **không sửa logic**.
- Tài liệu chuẩn: `docs/DESIGN-iced.md` (chỉ follow giao diện / bố cục /
  màu sắc, không đổi tính năng) và `docs/SPIKE-iced.md` (kết quả spike).
- Font Hán: subset `NotoSerifSC-Bold-subset.ttf` (10.312 byte, 18 ký tự),
  nhúng bằng `include_bytes!` + `font::load` +
  `Font::with_name("Noto Serif CJK SC")`. Không embed TTC 27 MB.

### File (dòng, kể cả test)

| File | Dòng | Nội dung |
|---|---|---|
| `src/main.rs` | 42 | Boot `iced::application`, cửa sổ 1280×800 (min 1100×720) |
| `src/app.rs` | 598 | App shell, `Screen`, `Shared`, điều phối + install receiver |
| `src/board.rs` | 907 | `Geom`, `BoardProgram`, `BoardState`/`Anim`, `Arrow`, render bàn |
| `src/theme.rs` | 193 | Palette tối DESIGN-iced.md, cỡ chữ, style nút, hình học bàn |
| `src/settings.rs` | 285 | Settings JSON `<config_dir>/xiangqi/settings.json`, lưu tự động |
| `src/i18n.rs` | 709 | Bảng `Key` VI/EN + `grade_label()` |
| `src/widgets.rs` | 205 | Nút/card/chip/eval bar dùng chung, panel 380 px |
| `src/helpers.rs` | 124 | `format_advantage`, `render_move`, `position_at` |
| `src/bridge.rs` | 199 | `from_mpsc` — nối `std::mpsc::Receiver` vào `Subscription` |
| `src/chessdb_panel.rs` | 220 | Section "Điểm ChessDB" dùng chung (Play + Learn + Practice) |
| `src/chessdb_worker.rs` | 135 | Worker ChessDB (cache theo FEN, bỏ request cũ) |
| `src/ai_worker.rs` | 916 | Worker AI (copy từ egui, xem §6) |
| `src/engine_manager.rs` | 119 | `spawn_install`, `locate`, `open_folder` |
| `src/engine_log.rs` | 116 | Log xoay `logs/engine.log` 1 MB × 3 (đã nối dây, §4) |
| `src/sound.rs` | 243 | Rodio: tiếng đi/ăn/chiếu/chiếu hết tổng hợp bằng code |
| `src/screens/home.rs` | 113 | Menu chính |
| `src/screens/play.rs` | 1.892 | Đấu với AI |
| `src/screens/learn.rs` | 1.821 | Học khai cuộc (Tutor = sách) |
| `src/screens/learn_select.rs` | 310 | Chọn bên + khai cuộc (37 bài) |
| `src/screens/summary.rs` | 347 | Tóm tắt bài học |
| `src/screens/practice.rs` | 1.842 | Tự luyện tập |
| `src/screens/settings_dialog.rs` | 605 | Dialog Cài đặt + Giới thiệu (stack overlay) |
| `src/screens/mod.rs` | 26 | Khai báo module màn hình |
| `src/assets/NotoSerifSC-Bold-subset.ttf` | 10.312 B | Font Hán subset |

## 2. Trạng thái từng màn (đều xong)

- **Home:** tiêu đề, 3 thẻ mode, Cài đặt + Giới thiệu xếp dọc (mỗi nút một
  hàng căn giữa), dòng version `Phiên bản 0.1.0 · Pikafish 2026-09-06`.
- **Đấu với AI:** dialog bắt đầu ván (bên Đỏ/Đen/Ngẫu nhiên/2 người, 5 mức,
  mức 4 "Tùy chỉnh (Elo)"); Pikafish pre-warm qua `spawn_install`, fallback
  Builtin có banner đỏ (không crash); worker AI bridge `"ai"` + lọc stale
  bằng `req_id`; diversity/weaken Master (margin 20 cp, 24 ply đầu);
  `grade_limits`/`hint_limits` = `time_ms(1500)`, builtin depth 6/4; danh
  sách nước (click xem lại + "Về cuối ván"), eval bar, chip điểm, ngưỡng
  {15, 50, 120, 250}; section ChessDB (`show_win_rate=true`); Hoàn nước
  (về trước nước người chơi, 2 ply) / Gợi ý (top 1, mũi tên xanh + chip
  điểm, hover đậm, click đi nước) / Xin hòa (AI chấp nhận khi điểm của nó
  ≤ −50 cp) / Xin thua / Ván mới / Lật bàn (tự lật khi người chơi là Đen).
- **Học khai cuộc:** chọn Tiên/Hậu, 37 bài (25 Đỏ + 12 Đen) với ★ level +
  ✓ hoàn thành, mini board sau 3 nước main-branch; Tutor = sách (gợi ý chỉ
  từ `Tutor::suggestions()` + `score_cp` lưu sẵn, không engine live; ngoài
  sách Grade::Acceptable + "Nước này không có trong sách khai cuộc."); gợi ý
  1–4 + mũi tên xanh `#2F6FD0`, hover đậm 100% (mặc định 38%), click hàng
  gợi ý đi luôn; lượt đối phương: ≥2 biến → panel "Đối thủ đi biến nào?"
  (mũi tên tím nét đứt có số 1–4), 1 biến tự đi sau 600 ms, đối thủ theo
  weight seed `0x1234_5678`, ngoài sách → engine Medium qua `AiWorker`
  riêng (bridge `"ai-learn"`); section ChessDB (`show_win_rate=false`); Hoàn
  nước / Ẩn gợi ý / Đổi khai cuộc / Đi lại / Dừng và tóm tắt / Lật bàn;
  dialog hoàn thành bài (có nút **"Chơi tiếp với máy"** — xem §6.1); dialog
  ngoài sách; màn Tóm tắt (thống kê đúng/sai lầm/thiếu chính xác, điểm
  engine cuối, bảng chấm theo từng nước, bấm dòng xem thế cờ bằng mini
  board, Học lại / Bài khác / Về menu).
- **Tự luyện tập:** chọn độ khó 0–4 (slider Elo 1200–3000 bước 50 khi mức
  4); 3 nguồn thế cờ: ban đầu / FEN (validate, báo lỗi) / ảnh (rfd
  `AsyncFileDialog`, không nhận diện được thì báo rõ, không đoán bừa);
  soạn thế cờ thủ công (palette 2 hàng Đỏ/Đen + tẩy, Tướng trùng màu tự thay
  thế, kiểm 2 Tướng đối mặt); tự đi cả 2 bên (click + kéo-thả); ChessDB top
  4 + chấm cả nước ngoài gợi ý (fallback Pikafish top 4 khi ChessDB không
  có dữ liệu); Reset / Hoàn nước / Đổi thế cờ / Về menu / Lật bàn.
- **Cài đặt (dialog overlay):** ngôn ngữ VI/EN, âm thanh, animation, ký
  hiệu nước Việt/Anh, mức độ 0–4 + slider Elo, engine đang dùng, threads
  1–8, hash 0–1024 bước 64 (0 = tự động), diversity / tournament /
  trace-log, đường dẫn Pikafish, nút **"Thử lại Pikafish"** (disable khi
  đang cài; thay đổi threads/hash/đường dẫn có hiệu lực sau khi bấm nút
  hoặc khởi động lại app). Mọi thay đổi áp dụng ngay + lưu tự động.
  **Không nút Lưu, không nút Khôi phục mặc định.**
- **Giới thiệu (dialog overlay):** tên app + phiên bản 0.1.0, Engine
  Pikafish 2026-09-06, số bài sách lấy thật từ book (37), credit font Noto
  Serif CJK SC, ghi chú âm thanh + ChessDB, nút "Mở thư mục engine" (chỉ
  hiện khi `locate()` tìm được) và "Mở engine.log" (ẩn khi log chưa khởi
  tạo được).
- **Overlay:** backdrop tối 60% + thẻ căn giữa, click backdrop đóng dialog
  (click vào vùng trống của thẻ cũng đóng vì container không chặn sự kiện
  chuột — đã ghi trong doc của dialog).

## 3. Bất biến bắt buộc (đã thực thi)

**Quân phải đặt xuống và animation chạy XONG rồi mới tính toán** (AI,
chấm, gợi ý, ChessDB). Trong Elm, `update()` chỉ kích hoạt công việc khi
`anim.is_none()`; mỗi màn giữ `pending_*` + `finish_anim()`/`settle()`
để hoãn việc tới khi quân hạ cờ xong (trượt 150 ms). Ghi thành bất biến
trong doc của `board.rs` và từng màn.

## 4. Engine: install + log (đã nối dây đầy đủ)

- Quyền sở hữu install thuộc về `App`: 1 receiver duy nhất
  (`install_rx`), pre-warm lúc khởi động + nút retry trong dialog Cài đặt
  cùng dùng nó (guard chống 2 install đồng thời). `App::subscription`
  nối bridge `"engine-install"` khi đang cài; outcome được forward **vô
  điều kiện** sang Play (`Msg::EngineInstalled`/`EngineUnavailable`) —
  pre-warm thường xong lúc user còn ở Home nên chỉ forward khi đang ở Play
  sẽ làm mất event (lệch có chủ ý so với egui poll-mỗi-frame, kết quả
  tương đương).
- `EngineLog` nối vào `Shared::new`: khởi tạo từ thư mục cha của
  `settings.json` (`<config_dir>/xiangqi/logs/engine.log` — khác egui đặt
  cạnh eframe storage dir, chỉ khác thư mục), `set_trace_enabled` theo
  setting, đăng ký sink qua `xq_pikafish::set_log_sink`. Toggle trace-log
  trong dialog sync ngay (toggle là writer duy nhất, tương đương egui sync
  mỗi frame).

## 5. Bằng chứng cổng (chạy đủ sau lần sửa cuối, 2026-11-13)

| Kiểm tra | Kết quả |
|---|---|
| `cargo fmt --all -- --check` | OK |
| `cargo clippy --workspace --all-targets --release -- -D warnings` | Sạch, exit 0 |
| `cargo test --workspace` | **255 pass / 0 fail** (178 test cũ + 77 test mới `xq-iced`) |
| `cargo test -p xq-iced` | 77 pass / 0 fail |
| `cargo check -p xq-app` | Pass (egui nguyên vẹn) |
| `cargo build --release -p xq-iced` | OK (3′36″) |
| Binary release | `target/release/xiangqi-iced`, **61.909.504 byte = 60 MB < 90 MB** |
| `git rev-parse HEAD` | `e224a40` (không đổi — chưa commit/push gì) |
| `git status --short` | `M Cargo.lock`, `M Cargo.toml`, `?? crates/xq-iced/`, `?? data/` (1,3 GB, không bao giờ commit), `?? docs/DESIGN-iced.md`, `?? docs/SPIKE-iced.md` |

Lưu ý: 1 suite test chạy 391 s trong lần chạy workspace (test
`xq-pikafish`/`xq-book` dùng engine thật) — không phải lỗi, chỉ chậm trên
VM 1,5 CPU. Test `xq-iced` riêng chạy ~54 s.

## 6. Quyết định / lệch so với brief và egui (ghi rõ, không lặng lẽ bỏ)

1. **Nút "Chơi tiếp với máy" đã KHÔI PHỤC** dù brief mục 5 ghi "không
   có" — vì app egui ĐÃ CÓ tính năng này (`ContinueWithAi` →
   `start_from_learn` → VsAi), nên yêu cầu giữ nguyên đầy đủ tính năng
   (§4) thắng tiền đề sai trong mục 5. Cần Hieu xác nhận.
2. **Lật bàn đã khôi phục** ở cả 3 màn chơi (brief không cấm; egui có).
   Trong lúc làm phát hiện **bug tọa độ `board.rs`**: `Geom::from_bounds`
   cộng `bounds.x`/`bounds.y` tuyệt đối trong khi canvas `Frame` dùng tọa
   độ widget-local — bàn vẽ lệch gấp đôi offset và click/drop trúng ô sai
   khi canvas không ở gốc (0,0). Đã sửa: chỉ căn giữa theo
   width/height; có test round-trip 90 ô cho cả 2 hướng lật.
3. **Phím tắt chưa port** (Ctrl+Z/H/F/N/Esc, mũi tên review) — task không
   yêu cầu; liệt kê ở §7.
4. **Mũi tên Mã vẽ thẳng**, không bẻ qua chân Mã như egui.
5. Tiêu đề Home dùng `Key::AppTitle` "Cờ Tướng AI" thay vì "Cờ Tướng"
   trong DESIGN (P0b).
6. Editor trong Practice lật độc lập với bàn chơi (egui dùng chung 1 cờ
   `flipped`).
7. Mũi tên biến đối thủ: tím nét đứt **có số 1–4** (egui: xanh dương có
   số; DESIGN + task: tím nét đứt).
8. Practice dùng `AiWorker` qua `std::mem::replace` với dummy rx để lấy
   receiver thật (field `rx` private, không destructure được như
   `ChessDbWorker`). Đề xuất thêm outlet chính thức trong `ai_worker.rs`
   sau.
9. **Dead-code đã dọn thật** (không giấu bằng allow): xóa
   `square_at_point`, `anim_progress`, `ArrowColor::Gold`,
   `ChessDbWorker::request` (+ field `next_req_id`),
   `widgets::move_list`, `SummaryData.start`, `Language::name()`,
   `AiWorker::drain_responses` + `request_stop`. Còn 6 `#[allow(dead_code)]`
   hẹp có ghi chú lý do: `AiResponse` + `req_id()` (field/test), `Key`
   enum (key P4 chưa dùng hết), `fallback_notice`/`take_fallback_notice`
   (màn hình phát hiện fallback qua tên engine; kênh notice được test bao
   phủ), `home::State::update` (hợp đồng màn hình; App chặn hết variant
   của Home).
10. Practice: section ChessDB dùng `show_win_rate=true` và 1 evaluation
    chung cho cả section + top-4 (egui Practice không render section
    ChessDB riêng — chỉ dùng nội bộ cho top-4 + điểm).
11. Iced 0.14 là bản cuối trước 1.0 (sẽ phá API một lần); đã pin 0.14.
12. Không có accessibility trong Iced 0.14 stable; gõ tiếng Việt qua IME
    (winit) chưa kiểm chứng trên Windows — mục kiểm bằng mắt §7.
13. `engine.log` nằm ở `<config_dir>/xiangqi/logs/` (cạnh
    `settings.json`), khác egui (cạnh eframe storage dir) — chức năng
    giống hệt, chỉ khác thư mục.

## 7. Việc còn lại + cần Hieu kiểm bằng mắt (Windows/Mac)

### Việc còn lại (code)
- Phím tắt chưa port: Ctrl+Z (hoàn nước), Ctrl+H (gợi ý), Ctrl+F (lật
  bàn), Ctrl+N (ván mới), Esc (đóng dialog / về menu), phím mũi tên
  (xem lại nước). Nếu Hieu muốn thì làm đợt riêng.
- Mũi tên Mã bẻ qua chân Mã (như egui) thay vì vẽ thẳng.
- Test suite ghi lại `~/.config/xiangqi/settings.json` thật qua `save()`
  ở `play.rs` / `learn.rs` (test load config thật rồi save lại; nội dung
  đã kiểm tra vẫn nguyên vẹn) — nên chuyển test sang thư mục config tạm.
- Thêm outlet chính thức cho `AiWorker.rx` thay cho `mem::replace`
  dummy rx ở Practice.
- Version app vẫn `0.1.0`, chưa tag `v1.0.0`, chưa ký số/notarize (theo
  quyết định cũ, chờ Hieu).
- Chưa chạy thử GUI thật: VM không có màn hình/GPU — mọi kiểm giao diện
  dưới đây phải làm trên máy Hieu.

### Kiểm bằng mắt (máy Windows/Mac của Hieu)
1. Chữ Hán trên quân cờ (30 px) có hiện đúng không (font subset)?
2. Theme tối, bố cục, panel 380 px có chống flexing khi nội dung đổi?
3. Mũi tên gợi ý xanh dương, hover đậm 100%; mũi tên biến đối thủ tím nét
   đứt có số 1–4; click hàng gợi ý đi luôn?
4. Kéo-thả quân: quân trượt mượt 150 ms rồi AI/gợi ý/ChessDB mới chạy
   sau (không dừng hình khi hạ cờ)?
5. Âm thanh đi/ăn/chiếu/chiếu hết; halo chiếu nhấp nháy, chiếu hết halo
   đỏ đậm?
6. Đổi ngôn ngữ VI ↔ EN trong Cài đặt: toàn app đổi ngay?
7. Dialog Cài đặt/Giới thiệu: overlay, backdrop tối, click backdrop đóng?
   Nút "Thử lại Pikafish" sau khi đổi threads/hash/đường dẫn?
8. Học: panel "Đối thủ đi biến nào?", bài học 37 bài (C64 mới, 2 bài 0
   ván đã biến mất), màn Tóm tắt?
9. Tự luyện tập: FEN, tải ảnh bàn cờ (báo không nhận diện được), soạn thế
   cờ thủ công?
10. Gõ tiếng Việt bằng IME trong ô FEN / đường dẫn Pikafish?
11. Section "Điểm ChessDB" ở cả 2 màn Chơi + Học (cần mạng)?
12. Cửa sổ 1280×800, thu nhỏ tối thiểu 1100×720, nút Lật bàn ở cả 3 mode?

## 8. Chạy thử (chưa commit/push — cần lệnh của Hieu mới push)

```bat
:: Command Prompt trên Windows (trong thư mục repo đã pull):
tools\fetch-engine.bat
cargo build --release -p xq-iced
target\release\xiangqi-iced.exe
```

```sh
# macOS / Linux:
./tools/fetch-engine.sh
cargo build --release -p xq-iced
./target/release/xiangqi-iced
```

Lưu ý: toàn bộ công việc đang ở trạng thái **uncommitted** (`Cargo.toml` +
`Cargo.lock` sửa 1 dòng members, `crates/xq-iced/` untracked, HEAD vẫn
`e224a40`). Muốn Hieu chạy thử trên máy thật thì cần commit + push trước
— chờ Hieu duyệt rồi mới làm. App egui cũ vẫn chạy như cũ:
`cargo run --release -p xq-app`.
