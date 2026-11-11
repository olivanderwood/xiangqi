# Quyết định & điểm lệch — M3 + M5 (giao diện)

Tài liệu này ghi các quyết định của agent Giao diện (M3: T3.1–T3.5, M5: T5.2–T5.3).
Không sửa `docs/DECISIONS.md` của coordinator. Ngày: 2026-11-10.

## 1. Trạng thái khi bắt đầu

- M0+M1 xong; khung `xq-app` đã có (`BoardGeometry`, `AiWorker`, `Settings`,
  `i18n` skeleton, placeholder các màn).
- Trong lúc làm, hai agent song song đã hoàn thành phần việc của họ và thay
  đổi bối cảnh:
  - **M2 (AI)**: `Searcher` thật đã thay placeholder depth-1
    (alpha-beta, PVS, TT, quiescence, MultiPV `analyze`, `score_move`,
    `MoveTag` + SEE). Màn hình dùng đúng API này, không sửa `xq-core`.
  - **M4 (sách)**: `openings.json` đủ 12 bài (6 đỏ, 6 đen) thay vì 1 bài mẫu.
  - **T5.1 (Tutor)**: `Tutor` thật đã thay placeholder trong lúc làm
    (`on_user_move` chấm + snapshot `retry`, `next_opponent_move` chọn
    nhánh theo `weight`, `hints_for_learner_turn`, `summary`,
    `is_complete`). Màn Học đã được nối lại theo Tutor thật (mục 5).

## 2. Font (T3.1)

- `tools/subset-font.sh` + `tools/cjk_chars.txt` đã commit và chạy được.
  Script tự sinh lại `cjk_chars.txt` từ `openings.json` mỗi lần chạy, rồi
  trích face SC (index 2 trong `.ttc`, thứ tự JP/KR/**SC**/TC/HK) ra `.ttf`
  tạm bằng fontTools và subset bằng `python3 -m fontTools.subset`
  (`pyftsubset` không có sẵn dưới dạng lệnh riêng trên VM này, nhưng cùng
  một công cụ fontTools).
- Kết quả (chạy lại sau khi sách đủ 12 bài, 61 ký tự CJK):
  - `NotoSerifCJKSC-Bold-subset.ttf` 17.540 byte, `-Regular-subset.ttf` 17.704 byte
  - `NotoSans-Regular-subset.ttf` 65.704 byte, `-Bold-subset.ttf` 65.512 byte
    (subset dải Unicode tiếng Việt + Latin)
  - Tổng 4 file ≈ 166 KB < 300 KB. `OFL.txt` kèm trong `assets/fonts/`
    (bản OFL 1.1 chuẩn; lấy từ gói font hệ thống).
- Nạp bằng `include_bytes!`; family `Proportional` = Noto Sans rồi CJK
  fallback; family riêng `cjk` / `cjk-bold` cho chữ quân, chữ sông, số cột.
- Icon: `assets/icon.png` (512×512, vẽ bằng PIL: nền gỗ + lưới + quân 帥 đỏ).
  File `.ico` / `.icns` **chưa làm** — để agent M6 (T6.1) tạo từ PNG này.

## 3. Chế độ 2 người chơi (T3.2)

- Đặc tả §6.2 không nêu, nhưng T3.2 yêu cầu "chơi 2 người trên một máy
  được". Thêm lựa chọn **"2 người chơi"** vào hộp thoại bắt đầu ván
  (cùng nhóm radio Bên). Ở chế độ này: không có AI, Hoàn nước lùi 1 nửa
  nước, Xin hòa ẩn (chưa có đối thủ AI để hỏi), Xin thua áp cho bên đang đi.
- Lựa chọn này được nhớ vào `settings.human_color = "two-player"`.

## 4. `ai_worker` (T3.3) — sửa thiết kế Stop

- Khung cũ: `AiWorker::stop` là một `Arc` riêng, còn `Stop` gửi qua kênh
  sẽ xếp hàng **sau** lệnh search đang chạy nên không thể ngắt kịp.
- Đã sửa: worker gửi lại đúng `stop_handle()` của `Searcher` ngay khi khởi
  động; `AiWorker::stop` **là** cờ đó. `request_stop()` đặt cờ trực tiếp
  (hiệu lực ngay cả khi đang search) rồi mới gửi thêm `AiRequest::Stop`.
- Giao diện giữ `req_id` đang chờ và bỏ mọi response khác
  (`response_is_current`); response mồ côi được lọc ở `App::update`
  (chỉ giữ response mà một màn nào đó còn đang chờ) và mỗi màn chỉ lấy
  response của mình ra khỏi bộ đệm chung `pending_ai`, để chuyển màn giữa
  lúc AI đang nghĩ không làm mất nước đi.
- Test `stop_returns_quickly_after_a_search_starts` chỉ kẹp được hợp đồng
  "đặt cờ là tức thì + worker vẫn trả lời" vì search placeholder cũ trả
  ngay; kiểm chứng độ trễ Stop ≤ 100 ms với search sâu thật thuộc về test
  `stop_flag` của M2 (§8.6).

## 5. Màn Học nối với Tutor thật (T5.2–T5.3)

- Tutor là source of truth: `on_user_move` (chấm, kể cả quy tắc mở rộng
  của T5.1: nước sách đi vào nhánh toàn lá xấu cho người học chỉ được
  `Acceptable` thay vì `Book`), `next_opponent_move` (chọn nhánh theo
  `weight`), `hints_for_learner_turn` (gợi ý sách + tối đa 2 dòng engine),
  `summary` / `is_complete` (điểm cuối tính theo góc nhìn người học).
  Sau mỗi lần gọi Tutor, màn hình đồng bộ `node_path / grades /
  book_moves / out_of_book / plies_after_book` từ Tutor
  (`sync_from_tutor`).
- Fallback nội bộ chỉ chạy khi không có Tutor hoặc Tutor trả `None`:
  chấm bằng engine tại chỗ, chọn nhánh sách theo `weight`, đối thủ ngoài
  sách do AI worker đánh (Trung bình). Các fallback này không được dùng
  trong luồng chính hiện tại.
- `Tutor<'a>` mượn `Lesson`: màn hình giữ `Box<Lesson>` và tạo Tutor từ
  con trỏ tới pointee của Box (địa chỉ ổn định, Tutor khai báo trước
  `lesson_box` nên bị drop trước, và luôn được clear trước khi thay Box).
  Đây là điểm duy nhất dùng `unsafe` trong crate.
- Hints của Tutor chạy engine đồng bộ (depth 4) **một lần mỗi lượt người
  học** (khóa bởi `hint_ply`), không chạy mỗi frame.
- Màn Tổng kết lưu "nước đúng" từng dòng: nước sách = chính nước đã đi;
  nước ngoài sách = nước tốt nhất Tutor/engine ghi lại lúc chấm
  (`graded_best`, kèm thế cờ trước nước để render đúng ký hiệu).
- Tiến độ hiển thị "Nước x / khoảng y" với y = số nửa nước đã đi + độ sâu
  còn lại của nhánh `main` (§6.4.5), không dùng `max_plies` như
  `Tutor::progress_text`.

## 6. Các lệch / chưa làm theo đặc tả

1. **Âm thanh (T3.5):** công tắc âm thanh có trong Cài đặt và được lưu,
   nhưng **chưa phát ra tiếng** — không có backend âm thanh trong phạm vi
   (không thêm crate audio). Ghi rõ trong màn Giới thiệu.
2. **Điểm Xin hòa:** AI đồng ý khi điểm engine (góc nhìn AI) ≤ −50, tính
   bằng `eval` tĩnh tại thời điểm bấm, không phải điểm search gần nhất
   (search cũ có thể đã lỗi thời sau nước người chơi).
3. **`ai generate` / CLI:** không thuộc M3, không đụng.
4. **Chấm nhãn trong "Đánh tiếp với AI":** dùng search depth Trung bình
   tại chỗ (đồng bộ, nhanh) thay vì worker bất đồng bộ, để nhãn hiện ngay
   sau nước đi.
5. **Mũi tên gợi ý Mã:** gấp khúc qua chân mã (§7.3) — chân mã tính theo
   hình học nước Mã, không theo trạng thái bàn cờ.
6. **Chữ "Chiếu!":** hiện trên bàn (trên đầu Tướng bị chiếu) trong 1 giây
   đầu + luôn hiện trong thanh trạng thái khi đang bị chiếu; quầng đỏ
   nhấp nháy trong lúc bị chiếu.

## 7. Kiểm chứng

- `cargo fmt --check`, `cargo clippy -p xq-app --all-targets -- -D warnings`:
  sạch (chỉ chạy cho `xq-app`; clippy workspace còn lỗi ở crate khác, xem mục 8).
- `cargo test -p xq-app`: **19 passed, 0 failed** (< 5 s debug), gồm:
  - 9 unit test `board_view` (to/from_screen có/không lật, nhãn cột, chân mã,
    nước Pháo, thanh đánh giá, animation),
  - 5 test `ai_worker` (req_id tăng, bỏ response cũ, Stop tức thì, worker
    trả nước hợp lệ),
  - 3 smoke test `egui_kittest`: Menu → Đấu với AI → bấm h2 rồi e2,
    danh sách có "炮二平五" và AI trả lời; Menu → Học chơi → Đi tiên/hậu
    (đủ 6 bài mỗi bên sau khi sách đủ 12 bài); Menu → Học chơi → mở bài
    Pháo đầu → gợi ý có 炮二平五, đi nước sách được chấm `Book`, đối thủ
    trả lời theo sách sau 600 ms,
  - `fonts_cover_book`: subset chứa đủ glyph cho mọi ký tự CJK trong sách.
- Không còn `todo!()` / `unimplemented!()` trong `xq-app`; chuỗi tiếng Việt
  chỉ nằm trong `i18n.rs` (đã quét, kể cả tiêu đề cửa sổ).

## 8. Blocker / việc của crate khác (không sửa — ngoài phạm vi)

- Lúc làm việc, `xq-core` và `xq-book` đang được agent khác sửa song song
  nên có lúc không biên dịch được (lỗi `E0382` ở `search.rs`, lỗi borrow
  `E0502` ở `tutor.rs`); cuối đợt cả hai đã biên dịch lại được.
- `cargo test --workspace` còn **2 test fail ở `xq-core` (M2, T2.5)**:
  `tags_hanging` ("hanging tag missing: []") và `tags_threat`
  ("spurious threat tag at startpos: [Threat(Pawn), CentralCannon]").
  Ảnh hưởng tới M3/M5: lời giải thích gợi ý (`MoveTag`) có thể thiếu cảnh
  báo treo quân hoặc thừa cảnh báo đe dọa ở thế khởi đầu. Không sửa vì
  thuộc `xq-core`.
- `cargo clippy --workspace --all-targets -- -D warnings` còn lỗi ở
  `xq-book/tests/tutor.rs` (doc comment, `clippy::doc-lazy-continuation`).
  Riêng `cargo clippy -p xq-app` sạch.
