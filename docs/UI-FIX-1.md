# Sửa UI Iced theo feedback — UI-FIX-1 (2026-11-13)

Hieu chạy thử bản Iced (`ca48e8d`) trên Windows và đối chiếu với bản thiết
kế, phản hồi 4 điểm. Đợt sửa này làm đủ cả 4, không đổi tính năng, không
commit/push. Chuẩn thiết kế: `docs/DESIGN-iced.md` + file mẫu 7 trang
(`~/workspace/user/files/C__t__ng___Thi_t_k__giao_di_n__iced.html`).
Nguyên tắc giữ nguyên: chỉ follow giao diện/bố cục/màu sắc của mẫu, KHÔNG
đổi tính năng để đáp ứng mẫu.

## 1. Mũi tên Mã bẻ qua chân Mã (đường chỉ như bản đầu, hiệu ứng giữ như bản mới)

- **Vấn đề:** bản Iced vẽ mọi mũi tên gợi ý thẳng từ ô đi tới ô đến; bản
  egui bẻ mũi tên Mã qua ô chân Mã (`board_view.rs:115` `horse_leg` +
  `draw_arrow` bẻ polyline, §7.3).
- **Sửa (`crates/xq-iced/src/board.rs`):**
  - Port y nguyên `horse_leg(from, to) -> Option<Square>` từ egui (ô cờ
    0..89; nước Mã (1,2) → chân `(f0, r0+dr.signum())`, (2,1) → chân
    `(f0+df.signum(), r0)`; còn lại `None`).
  - `arrow_points(g, from, to, is_horse) -> Vec<Point>` (hàm thuần, test
    được): mũi tên thường trả `[start, end]` thẳng (rút ngắn cả 2 đầu
    `PIECE_RADIUS + 4` như cũ); mũi tên Mã trả `[ô đi, ô chân, end]`, chỉ
    rút ngắn đoạn cuối theo hướng chân→đích (kiểu egui), đoạn đầu bắt đầu
    đúng tâm ô đi.
  - `draw_arrow(..., is_horse: bool)`: vẽ polyline từng đoạn (nét đứt áp
    cho cả 2 đoạn nếu có); đầu mũi tên V giữ nguyên kiểu Iced, hướng theo
    đoạn cuối; badge tròn `11*scale` đặt ở 50% độ dài polyline (với mũi
    tên thẳng trùng đúng điểm giữa cũ).
  - Xác định nước Mã lúc vẽ, không thêm field vào `struct Arrow`, không
    sửa screens: `BoardProgram::draw` tra `position.piece_at(arrow.from)`
    có phải Mã (`PieceKind::Horse`) không.
  - Giữ nguyên y hệt: màu xanh `#2F6FD0` / tím `#7B3FC4`, mờ 38% / hover
    đậm 100%, nét đứt cho đối phương, đầu V, badge số 1–4. Các quân khác
    vẫn đường thẳng.
- **Test mới (4):** `horse_leg_bends_correctly` (mirror 4 case egui + 3
  case phụ), `arrow_points_horse_bends_through_leg`, `arrow_points_non_horse_stays_straight`,
  `bent_arrow_badge_lies_on_the_polyline`. Lưu ý: với nước Mã ở scale 1,
  đoạn đầu dài 64 px trên tổng ~123,5 px nên điểm 50% rơi ngay trước chân
  Mã, trên đoạn ô đi→chân — vẫn nằm trên đường bẻ.

## 2. Icon + font Inter + style chữ

### 2.1 Inventory icon (11 icon UI, Lucide-style, viewBox `0 0 24 24`, stroke-width 2)

| # | Icon | Màn / vị trí |
|---|---|---|
| 1 | `maximize` | Trang 1: icon thẻ "Chơi với máy" (36 px); trang 4–5: nút "Chơi tiếp với máy" |
| 2 | `plus` | Trang 1: nút "Bắt đầu ván mới"; trang 2: nút "Ván mới" |
| 3 | `book_open` | Trang 1: icon thẻ "Học khai cuộc" (36 px) + nút "Vào học"; trang 3: nút "Bắt đầu học"; trang 4–5: nút "Đổi khai cuộc" |
| 4 | `target` | Trang 1: icon thẻ "Tự luyện tập" (36 px) + nút "Vào luyện tập" |
| 5 | `sliders_horizontal` | Trang 1: footer "Cài đặt"; trang 2/4/5: header "Cài đặt" |
| 6 | `arrow_left` | Trang 2–6: nút "Quay lại" ở header |
| 7 | `undo` (undo-2) | Trang 2/4/5: nút "Hoàn nước"; trang 4–5: nút "Đặt lại" |
| 8 | `lightbulb` | Trang 2: nút "Gợi ý"; trang 4–5: nút "Ẩn gợi ý" |
| 9 | `flag` | Trang 2: nút "Xin thua" (nút "Xin hòa" **không có icon**) |
| 10 | `chevron_down` | Trang 6: dropdown "Chủ đề bàn" và "Ngôn ngữ" (18 px) |
| 11 | `check` | Trang 6: nút "Lưu" (nút "Khôi phục mặc định" **không có icon**) |

Bỏ qua: SVG bàn cờ 592×656 ở trang 2/4/5 (không phải icon UI; bàn cờ vẽ
bằng canvas); trang 7 không có icon; các nút không icon ("Xin hòa",
"Dừng và tóm tắt", "Khôi phục mặc định"), thẻ bài khai cuộc, mục gợi ý
1–4. Các nút "Lật bàn" mẫu cũng không vẽ icon.

- **Trích:** 11 file `.svg` trong `crates/xq-iced/assets/icons/`, giữ
  nguyên viewBox, `fill="none"`, stroke-width 2, round caps/joins.
  Mẫu dùng `stroke="currentColor"` nhưng iced `Svg` không kế thừa
  `currentColor` (usvg resolve thành đen trên theme tối) nên đã nướng sẵn
  `stroke="#ECE7DC"` (màu `TEXT`); vẫn đổi màu được lúc dùng qua
  `icon_tinted` / `.style(svg::Style { color })`.
- **API (`crates/xq-iced/src/icons.rs`):** `pub enum Icon { Maximize,
  Plus, BookOpen, Target, SlidersHorizontal, ArrowLeft, Undo,
  Lightbulb, Flag, Check }` (xem §6 về `ChevronDown`), `icon(name) ->
  Svg` (mặc định 20×20), `icon_tinted(name, color) -> Svg`,
  `ICON_SIZE = 20.0`, `ICON_SIZE_LARGE = 36.0`, `ICON_SIZE_SMALL = 18.0`.
  Cần feature `svg` cho iced (đã thêm vào `Cargo.toml`).
- **Đã gắn vào màn hình:** Home (Maximize/BookOpen/Target 36 px trên 3
  thẻ; Plus/BookOpen/Target trên 3 nút; SlidersHorizontal ở Cài đặt);
  Play (ArrowLeft, SlidersHorizontal, Plus "Ván mới", Undo "Hoàn nước",
  Lightbulb "Gợi ý", Flag "Xin thua"); Learn-select (ArrowLeft, BookOpen
  "Bắt đầu học", Check 18 px xanh cho bài đã hoàn thành — thay ký tự text
  "✓" cũ); Learn (ArrowLeft, SlidersHorizontal, Undo "Hoàn nước" +
  "Đặt lại", Lightbulb "Ẩn gợi ý", BookOpen "Đổi khai cuộc", Maximize
  "Chơi tiếp với máy"); Practice (ArrowLeft, SlidersHorizontal, Undo,
  Plus "Reset" + "Start"); Summary (BookOpen "Học lại"). Icon đặt trước
  chữ trong nút, gap 8 px; icon trên nút chính vàng dùng `icon_tinted`
  màu nền tối để nhìn rõ.
- **Chưa gắn:** `ChevronDown` — không có dropdown/`pick_list` nào trong
  app (Settings dùng radio; không có bộ chọn chủ đề bàn) nên không có chỗ
  đặt hợp lệ (xem §6). "Xin hòa", "Dừng và tóm tắt", "Lật bàn" — mẫu
  không vẽ icon, giữ nguyên nút chữ.

### 2.2 Font Inter (weights 400/500/600/700)

- Nguồn: release chính thức **rsms/inter v4.1** (`Inter-4.1.zip`, OFL),
  lấy 4 static TTF trong `extras/ttf/`. Cách convert woff2 trong file mẫu
  bỏ qua: các `@font-face` trong mẫu chỉ trỏ URL `fonts.gstatic.com`
  (file subset theo unicode-range, hàng trăm mảnh), không trích được file
  hoàn chỉnh.
- File trong `crates/xq-iced/assets/`: `Inter-Regular.ttf` 411.640 B
  (400), `Inter-Medium.ttf` 417.300 B (500), `Inter-SemiBold.ttf`
  419.744 B (600), `Inter-Bold.ttf` 420.428 B (700). Tổng ~1,6 MB.
  woff2 KHÔNG nạp được bằng iced `font::load` nên file cuối phải là TTF.
- Nạp lúc boot: `App::new` batch `theme::load_cjk_font()` +
  `theme::load_inter_fonts()` (`app.rs:243`); helper `inter_font()` (400),
  `inter_font_medium()` (500), `inter_font_semibold()` (600),
  `inter_font_bold()` (700) trong `theme.rs`, theo pattern `cjk_font()`.
- Áp dụng: `main.rs` đặt `.default_font(theme::inter_font())` (Regular
  400) — API `iced::application(...).default_font(Font)` có sẵn trong
  iced 0.14. Weight theo từng phần tử: tiêu đề Bold/Semibold (Home 36
  Bold, tiêu đề màn 20 Semibold, tiêu đề thẻ/section 16 Semibold), nút
  Semibold 15, chip Medium 13, hàng nước/gợi ý Medium 15, số điểm lợi
  thế Bold 28, nội dung Regular. Cỡ chữ đúng DESIGN-iced.md (36/20/16/15/
  13, chữ quân 30 — chữ quân vẫn dùng font Hán subset, không đổi).

### 2.3 Bố cục từng màn đã sửa (đối chiếu mẫu)

- **Home** (viết lại): đĩa quân 帥/將 80 px viền đôi, chữ CJK 44 px; tiêu
  đề 36 Bold, subtitle dim; 3 thẻ một hàng (Fill, cao 264, padding 24,
  radius 14, gap 24), icon vàng 36 px, tiêu đề thẻ Semibold 16, nút
  full-width 48 px; footer: Cài đặt + Giới thiệu (mỗi nút một hàng,
  44 px) + dòng version.
- **Play**: header 64 px (surface, viền dưới 1 px, Back/Settings ghost,
  tiêu đề 20 Semibold flex, chip level + chip engine kèm chấm xanh); thẻ
  đối thủ/người chơi 44 px avatar, tên Semibold 16, subtitle 13 Medium,
  chip trạng thái, viền vàng khi tới lượt; danh sách nước + gợi ý bọc
  trong card; nút thành lưới 2 cột × 40 px + "Ván mới" primary full-width
  44 px; hàng nước/gợi ý Medium 15, padding 4, bo 6 khi chọn.
- **Learn-select** (viết lại): header 64 px; card giữa 880 px, padding
  32, radius 14; tiêu đề 20 Semibold + subtitle; segmented Tiên/Hậu
  320 px pill, nút 40 px, nút chọn nền vàng; danh sách bài 380×320 cuộn,
  hàng bài có sao + Check xanh khi hoàn thành; panel giới thiệu có preview
  board 300×330; "Bắt đầu học" 200×48 góc dưới-phải. Card bọc trong
  scrollable dọc (card cao ~770 px khi có bài được chọn, vượt cửa sổ
  800 px nếu căn giữa cố định).
- **Learn** (lượt bạn + đối phương): header như Play (chip: tên bài, bên
  chơi, tên engine); panel có thẻ đối thủ/người chơi; danh sách nước
  trong card, có thanh tiến độ vàng 6 px + "Bước n / [N]"; hàng gợi ý/
  đối phương/xem trước 15 Medium; lưới nút 2 cột (Hoàn nước, Đổi khai
  cuộc, Lật bàn, Về menu) + "Dừng và tóm tắt" primary full-width 44 px;
  `complete_card`/`off_book_card` dùng card radius 14, padding 16.
- **Practice**: thêm header 64 px (Back → "Đổi thế cờ", Settings) — trước
  đây màn này không có header; content padding 16/24, khoảng cách
  board→panel 24, eval→board 16; Reset primary + Plus, Hoàn nước + Undo;
  nút Start + Plus.
- **Settings**: tiêu đề Semibold, spacing body 8→12, card padding dùng
  token 16. Không thêm ChevronDown (không có dropdown nào) và không thêm
  nút Lưu/Check (lưu tự động).
- **Summary**: padding 12→24, spacing 8→16, tiêu đề + tên bài Semibold,
  hàng bảng Medium 15, nút "Học lại" primary + icon BookOpen.
- **ChessDB panel**: bọc trong card riêng (radius 14, padding 16) trong
  cả 3 màn có panel, để tách thành section rõ ràng.
- Token dùng xuyên suốt: `SPACE_4/8/12/16/24/32`, `RADIUS_6/10/14`,
  `card_style()` mới trong `widgets.rs` (radius 14 — `theme::
  card_container_style` là radius 10, `theme.rs` không được sửa ở phase
  bố cục nên style mới đặt ở `widgets.rs`).

## 3. Responsive + lưới + padding

- Mỗi màn có bàn cờ (Play/Learn/Practice) lưu `window: Size` (mặc định
  1280×800), subscribe `iced::window::resize_events()` →
  `Msg::WindowResized`, và tính kích thước bàn bằng helper chung
  `board_size_for_window(window)` trong `widgets.rs` (trừ header 64 +
  divider 1 + padding 16×2, eval bar 14, gap 16, panel 380, gap 24;
  `scale = min(w_avail/592, h_avail/656)`).
- **Lỗi cũ đã sửa:** cả 3 màn trước đây hardcode biên 592×656 khi đổi
  điểm drop thành ô cờ, nên khi cửa sổ thu nhỏ là drop sai ô. Nay
  DragEnded dùng đúng kích thước đã tính.
- Tính toán tay: ở 1100×720 board = 562×623 (scale 0,9497), board +
  chrome = đúng 720 cao / ≤1100 rộng; ở 1280×800 board = 634×703 (scale
  1,0716). Lưới rõ: board | panel 380 px, section tách bằng card có
  nền/viền/bo góc; padding từng section dùng đúng `SPACE_*`, bo góc
  `RADIUS_*`.
- Kiểm chứng bằng test (không GUI): 4 test mới — `widgets.rs` assert kích
  thước board ở 1280×800 và 1100×720 + tỉ lệ + vừa khung; `play.rs`
  assert map drop ở kích thước thiết kế, ở board thu nhỏ, và resize được
  nhớ. (Trong lúc làm, test map-drop thu nhỏ fail 1 lần vì test nhân điểm
  thiết kế với scale mà bỏ qua offset căn giữa dọc `by` ≈ 0,12 px — đã sửa
  test thành round-trip qua `Geom::square_point`, không sửa code
  production.)

## 4. Giữ nguyên tính năng

Toàn bộ tính năng và nguyên tắc animation-xong-mới-tính giữ nguyên
(không thêm/bớt tính năng theo mẫu). App egui `xq-app` và core crates
không sửa. Không commit/push.

## 5. Bằng chứng cổng (chạy đủ sau lần sửa cuối, 2026-11-13)

| Kiểm tra | Kết quả |
|---|---|
| `cargo fmt --all -- --check` | OK |
| `cargo clippy --workspace --all-targets --release -- -D warnings` | Sạch, exit 0 (0 error, 0 warning) |
| `cargo test --workspace --no-fail-fast` | **264 pass / 0 fail** (178 cũ + 86 `xq-iced`: 77 gốc + 4 mũi tên Mã + 1 icon + 4 responsive) |
| `cargo test -p xq-iced` | 86 pass / 0 fail |
| `cargo build --release -p xq-iced` | OK (8′03″) |
| Binary release | `target/release/xiangqi-iced`, **65.194.240 byte = 62,2 MB < 90 MB** (tăng từ 60 MB do thêm font Inter ~1,6 MB + feature `svg`/resvg) |
| `git rev-parse HEAD` | `b213323` (không đổi — chưa commit/push gì) |

Lưu ý: cổng test chạy thẳng với `--no-fail-fast` (264/0, không có test
flaky nào trong đợt này). Ở đợt viết lại trước, lần kiểm đầu không có
`--no-fail-fast` từng ra 135/1 vì `cargo test` dừng sớm sau 1 lỗi flaky
không tái hiện — nên đợt này chạy đủ với `--no-fail-fast` ngay từ đầu.

## 6. Quyết định / lệch (ghi rõ)

1. **`Icon::ChevronDown` bị xóa khỏi enum** (tích hợp): app không có
   dropdown/`pick_list` nào (Settings dùng radio; không có bộ chọn chủ đề
   bàn) nên variant này không bao giờ được khởi tạo → dead-code warning,
   làm rớt cổng clippy `-D warnings`. File `assets/icons/chevron_down.svg`
   vẫn giữ nguyên (đã trích từ mẫu); thêm lại variant khi nào app có
   dropdown. Ghi chú lý do trong doc của enum `Icon`.
2. **`Icon::all()` thành `#[cfg(test)]`** (tích hợp): chỉ test dùng nó
   (`all_icons_are_distinct_valid_svgs`); ở build thường nó là dead code.
   Test đổi assertion 11 → 10 icon (sau khi bỏ `ChevronDown`).
3. **Nạp font lúc boot nằm ở `app.rs:243`, không phải `main.rs`**: boot
   task thực tế nằm trong `App::new` ở `app.rs` (`main.rs` chỉ có
   `iced::application(...)`, không có chỗ chêm Task). Sửa đúng 1 dòng
   trong `app.rs` để batch 2 lần nạp font.
4. **Lệch mẫu còn tồn tại (theo DESIGN-iced.md, không theo pixel mẫu
   thô):** h1 Home mẫu 52 px nhưng DESIGN-iced.md ghi 36; tiêu đề thẻ mẫu
   22 px vs DESIGN 16 — theo DESIGN. Eval bar bo góc 7 không áp được
   (container ngoài không clip con). Header Learn không có chấm "engine
   sẵn sàng" (màn Learn không có flag engine-ready). "Ván 1" ở danh sách
   nước Play bỏ qua (không có key/data số ván). Chip header Learn không có
   tiền tố "Khai cuộc: / Bạn: " và chip engine Play hiện tên engine thay
   vì "Pikafish sẵn sàng" (không có key i18n — `i18n.rs` ngoài phạm vi,
   không bịa key mới).
5. **Tiêu đề Home vẫn "Cờ Tướng AI"** (key `AppTitle`), không phải "Cờ
   Tướng" như mẫu vẽ — Hieu chưa yêu cầu đổi; nếu muốn đúng y mẫu thì đổi
   key này.
6. **Sửa lỗi clippy do doc comment (tích hợp, 10 lỗi):** doc trong
   `widgets.rs`/`learn_select.rs` có dòng bắt đầu bằng `+` (markdown hiểu
   là bullet list → các dòng sau thành lazy continuation không thụt lề),
   1 doc comment mồ côi sau hàm đã xóa (đổi `///` → `//`), 1
   `#[must_use]` thừa trên `segment_button_style` (trả `impl Fn` đã là
   must_use). Chỉ sửa comment/attribute, không đổi code.
7. **Va chạm khi 2 agent sửa song song:** `cargo fmt -p xq-iced` của
   agent B từng reformat `board.rs` đang làm dở của agent A; agent A làm
   lại và kiểm chứng lại; agent B `git checkout` hoàn lại rồi xác nhận file
   còn đủ code. Bài học: khi nhiều agent cùng sửa 1 crate thì fmt từng
   file, không fmt cả package giữa chừng.

## 7. Việc còn lại + cần Hieu kiểm bằng mắt (Windows)

### Việc còn lại (code)
- Phím tắt chưa port (Ctrl+Z/H/F/N/Esc, mũi tên review) — từ đợt trước,
  không thuộc đợt này.
- Mũi tên Mã: đã bẻ qua chân (đợt này) — còn lại vẫn vẽ thẳng như egui.
- Version app vẫn `0.1.0`, chưa tag `v1.0.0`, chưa ký số/notarize.
- Chưa chạy thử GUI thật: VM không có màn hình/GPU.

### Kiểm bằng mắt (máy Windows của Hieu)
1. Mũi tên Mã có bẻ qua ô chân Mã không (cả mũi tên gợi ý xanh và biến
   đối phương tím nét đứt)? Các quân khác vẫn thẳng?
2. Icon mới có hiện đúng không (thẻ Home 36 px, nút 20 px, Check xanh
   18 px ở bài đã hoàn thành)? Icon trên nút vàng có nhìn rõ không?
3. Font Inter có đúng không (chữ UI Inter, chữ Hán vẫn Noto Serif SC)?
   Weights có phân biệt được không (tiêu đề đậm, nội dung thường)?
4. Layout responsive: thu nhỏ cửa sổ xuống 1100×720 rồi kéo-thả quân —
   có drop đúng ô không? Board + panel có vừa khung, không tràn?
5. Padding từng section có tách bạch, không dính nhau? Card bo góc 14?
6. Màn Learn-select: card cuộn khi có bài được chọn (preview board)?
7. Các màn còn lại (Play/Learn/Practice/Settings/Summary) bố cục có giống
   mẫu không?

## §8. Đổi tên + rà soát responsive (Hieu 2026-11-13 15:23, follow-up)

- **Tên app đổi thành "Cờ Tướng"** (EN "Xiangqi"): `Key::AppTitle`
  trong `i18n.rs`; tiêu đề Home 52 px theo mẫu vẽ thực tế
  (`FONT_TITLE_LARGE` 36 → 52; chỉ Home dùng hằng này);
  tiêu đề cửa sổ Windows cũng theo tên mới
  (`main.rs` `.title(|_: &App| i18n::t(Key::AppTitle).to_string())`).
  Chỉ đổi bản Iced; bản egui giữ nguyên. `DESIGN-iced.md` đã ghi chú
  ngoại lệ 52 px.
- **Responsive đã có sẵn trong UI-FIX-1** (không cần sửa thêm): cả 3 màn
  Đấu/Học/Tự luyện tính cỡ bàn bằng `board_size_for_window(window)`
  và đặt canvas `Length::Fixed(board_size)` — co giãn cả lên lẫn xuống
  theo cửa sổ, panel giữ 380 px, drop dùng đúng cỡ bàn đã tính.
  Bản Hieu chạy thử (`ca48e8d`) chưa có phần này nên anh chưa thấy.
- Cổng đợt đổi tên: fmt OK, clippy sạch (`-D warnings`, release),
  `cargo test -p xq-iced` **86/0**, binary release
  65.194.752 byte = 62,2 MB < 90 MB.

## §9. UI-FIX-2 — responsive lại + menu căn giữa + Tiên/Hậu rõ (Hieu 2026-11-13 16:28, feedback sau khi chạy bản 2e1e9a8)

Ba việc, làm cả ba. Không đổi tính năng, logic game, engine, sách, egui.

### 1. Responsive lại cho màn hình chơi game, cả 3 mode
- **Vấn đề cũ:** 3 màn dùng `Length::Fixed(PANEL_WIDTH)` (380 px) cho
  side panel và `board_size_for_window` trừ cố định 380 px; content row
  không Fill nên trên cửa sổ rộng bàn bị giới hạn bởi chiều cao và thừa
  khoảng trống lớn bên tay phải.
- **Cách sửa:** bàn giữ tỉ lệ 592:656, scale theo cửa sổ (công thức
  `board_size_for_window` không đổi — phần trừ 380 px chính là thứ bảo
  đảm panel Fill luôn ≥ 380 px); content row `width(Length::Fill)`;
  side panel `Length::Fill` ở cả 3 màn (play/learn/practice), nội dung
  panel giãn/căn giữa theo panel (nút action Practice nay giãn đều như
  lưới 2 cột ở Play/Learn). Thêm helper `panel_width_for_window`
  (chỉ dùng trong test) + test mới
  `fill_panel_is_at_least_380_and_leaves_no_gap`: ở 1100×720 / 1280×800 /
  1920×1080 panel ≥ 380 px và bàn + chrome + panel = đúng chiều rộng
  cửa sổ (panel ~436 / ~544 / ~931 px).
- **Không regress bug drop sai ô (UI-FIX-1):** mọi chỗ đổi điểm drop
  thành ô vẫn dùng đúng `board_size_for_window(self.window)`
  (play.rs:1282, learn.rs:1148, practice.rs:941).
- **Lưu ý:** ở 1920×1080 panel rộng tới ~931 px — hệ quả đúng theo yêu
  cầu "fill hết chiều ngang còn lại", nhưng nhìn có thể thấy quá giãn
  (nhất là danh sách nước đi). Nếu Hieu thấy quá rộng thì quyết sau:
  có đặt trần (max-width) cho panel hay không — hiện KHÔNG làm.

### 2. Menu Tự luyện tập + setting mới vào trận (Đấu với AI) ra giữa, style giống Học khai cuộc
- **Play `start_dialog_overlay`:** card 340 px → **640 px**, style
  `card_style` giống Learn-select (surface, bo góc 14, padding 24);
  2 section "Bên chơi" (Đỏ / Đen / Ngẫu nhiên / 2 người) và "Độ khó"
  (5 mức 0–4) nằm **2 cột cạnh nhau**; mọi nút cao 48 px, chữ 15 px
  Semibold; message giữ nguyên (`StartDialogSide`,
  `StartDialogDifficulty`, `StartGame`, `CancelStart`).
- **Practice `view_setup`:** cột full-width căn trái → **card căn giữa
  880 px** (surface, bo góc 14, padding 24), bọc trong `scrollable`
  (nguồn "Từ ảnh" kèm board editor 400×440 sẽ cuộn thay vì tràn);
  nút Bắt đầu / Quay lại menu full-width, cao 48 px. Giữ nguyên 5 radio
  độ khó + slider Elo tùy chỉnh (mức 4), 3 nguồn Start / FEN / ảnh
  (`view_setup_fen`, `view_setup_image`, `view_editor` không động vào).

### 3. Hai nút Đi tiên / Đi hậu rõ ràng hơn (Học khai cuộc)
- Bỏ pill segmented 320 px cũ (nút 40 px, chữ 14 px). Thay bằng nhãn
  "Bạn chơi bên" (key i18n mới `LearnSideLabel`: VI "Bạn chơi bên" /
  EN "You play as", 16 px Semibold) + **2 nút lớn cạnh nhau, cao 60 px,
  chữ 18 px Semibold + glyph quân 24 px** (帥 Đỏ / 將 Đen), lấp đầy
  chiều rộng card, cách nhau 12 px, bo góc 10.
- Mã màu rõ ngay khi chưa chọn: Đỏ = chữ/viền đỏ `#EA6A5F` trên
  surface-2; Đen = nền tối hơn + chữ sáng + viền xám. Đã chọn: **gold
  fill `#D4A24C` + chữ tối** (hover `#E0B25C`, pressed `#B88A3A`).
  Giữ nguyên `Msg::SelectSide` và hành vi. Hàm `segment_button_style`
  trong `widgets.rs` thành dead code nên đã xóa.

### Cổng (chạy đủ sau lần sửa cuối — xem báo cáo cuối đợt)
- `cargo fmt --all` + check, `cargo clippy -p xq-iced --all-targets
  --release -- -D warnings`, `cargo test --workspace --no-fail-fast`,
  `cargo build --release -p xq-iced` (binary < 90 MB).

### Cần Hieu kiểm bằng mắt (Windows)
1. **Responsive, cả 3 màn, cả 3 cỡ 1100×720 / 1280×800 / 1920×1080:**
   không còn khoảng trống bên phải (mép panel sát mép cửa sổ, chỉ còn
   padding 24 px); panel ≥ 380 px, nội dung không bị bóp ở 1100×720.
   Ở 1920×1080: panel ~931 px có quá giãn không (có muốn đặt trần
   max-width không)?
2. **Kéo-thả vẫn đúng ô** ở mỗi cỡ cửa sổ (đặc biệt góc bàn khi thu nhỏ
   1100×720).
3. **Đấu với AI → menu mới vào trận:** card căn giữa, "Bên chơi" và
   "Độ khó" 2 cột cạnh nhau, nút cao đều 48 px, chữ rõ; card có vừa
   ở 1100×720 không?
4. **Tự luyện tập → menu setup:** card 880 px căn giữa, không lệch trái
   như trước; nguồn "Từ ảnh" cuộn dọc mượt, không cắt nội dung ở cửa sổ
   cao 720 px; 3 radio nguồn + slider Elo nằm gọn trong card.
5. **Học khai cuộc:** nhãn "Bạn chơi bên" rõ; 2 nút 60 px (帥 Đi tiên /
   將 Đi hậu) phân biệt được ngay khi chưa chọn; bấm chọn thì nút đó
   nền vàng gold + chữ tối, không nhầm với chưa chọn; đổi sang EN thì
   nhãn/nút dịch đúng, glyph vẫn hiện.
6. Bo góc/padding 2 menu mới có đồng bộ với màn Học khai cuộc không;
   mọi lựa chọn cũ (4 bên, 5 độ khó, 3 nguồn) còn đủ và bấm được không?
