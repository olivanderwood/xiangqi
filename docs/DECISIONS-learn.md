# DECISIONS — Học khai cuộc v2: sách từ Pikafish + gợi ý đánh số

Đợt làm lại màn Học chơi theo yêu cầu của Hieu (2026-11-11): các bài học
dựa vào Pikafish hết, mở rộng danh sách thế khai cuộc, 3 hướng đi đánh số
1–3 thay vì tô màu, ô điểm lợi thế xanh/đỏ, giao diện to rõ hơn.

## §1. Gợi ý đánh số thay vì tô màu (lệch §6.4.2/§7.3)

Đặc tả gốc: mũi tên gợi ý tô màu theo thứ hạng (số 1 xanh, số 2–3 xanh
dương). Hieu yêu cầu bỏ tô màu, đánh số 1–3, 1 là tối ưu nhất.

- `HintArrow` nay có thêm trường `rank`; các mũi tên dùng **một màu
  chung** (xanh dương `0x2F6FC8`, alpha 170), thứ hạng thể hiện bằng **huy
  hiệu số** vẽ trên mũi tên: hình tròn tối + số trắng đậm, đặt ở chỗ gãy
  (nước Mã) hoặc giữa mũi tên. Hover vẫn sáng xanh lá như trước — đó là
  phản hồi tương tác, không phải mã hoá thứ hạng.
- Thay đổi áp dụng cả màn Đấu với AI (cùng dùng `HintArrow::for_rank`),
  nên gợi ý phím H ở đó cũng có số 1–3 (màn Đấu với AI vẫn top 3,
  chỉ màn Học chơi mới lên 4 hướng, xem §5).

## §2. Ô điểm lợi thế cho từng nước

- Mỗi hàng gợi ý có ô điểm riêng, định dạng theo tốt: `+0.8`, `-0.3`,
  chiếu hết hiển thị `M3`/`-M2` (`format_advantage` trong
  `screens/learn.rs`, `MATE = 30000`).
- Màu ô: xanh lá (`0x2E7D32`) khi điểm > 0 (lợi cho người học), đỏ
  (`0xC0392B`) khi < 0, xám khi bằng 0. Điểm là điểm sau nước đi, tính
  từ phía người đi (lượt người học nên chính là phía người học).
- Hàng gợi ý gồm 2 dòng: dòng 1 là `1. 炮二平五  +0.8  Sách`, dòng 2 là
  giải thích (chữ nhỏ, xám, thụt vào). Bấm vào hàng thì đi nước đó;
  rê chuột lên hàng thì mũi tên tương ứng sáng lên (đồng bộ
  `hint_hover` như trước). Hàng đang rê có nền xanh nhạt.

## §3. Font to, rõ + bố cục

- Cỡ chữ toàn app tăng trong `install_fonts` (`app.rs`): Heading 28,
  Body/Button 18, Small 15, Monospace 16 (mặc định egui là 22/14/11/12);
  khoảng cách giữa các mục 8×6. Cỡ chữ quân cờ trên bàn không đổi (tính
  theo ô cờ).
- Panel Học chơi rộng 340 → 360 px để đủ chỗ cho số + tên nước + ô điểm.

## §4. Sách khai cuộc sinh từ Pikafish — ĐÃ LÀM (24 bài, sâu 40 nửa nước)

Hieu chốt 2026-11-11 14:19: 24 bài, sâu 20 nước (40 nửa nước), 3 hoặc 4
hướng nếu tốt ngang nhau.

- Không tải sách ngoài: tìm trên mạng không thấy bộ sách khai cuộc Cờ
  Tướng nào tải về được dạng mở (XQF/PGN phần lớn nằm trong app trả phí
  hoặc cần tài khoản). Sách được sinh từ Pikafish bằng tool mới
  `tools/gen-book` (`cargo run --release -p gen-book`).
- Định nghĩa 24 bài ở `crates/xq-book/data/lessons-src.json`: 12 bài cũ
  giữ nguyên id/tên/cấp độ/tóm tắt, prefix = 6 nửa nước đầu của đường
  chính cũ (định danh hệ khai cuộc); 12 bài mới (Thuận pháo, Liệt pháo,
  Khởi mã, Tuần hà pháo, Sĩ giác pháo, Kim câu pháo bên Đỏ; Đơn đề mã +
  5 bài "đối phó" bên Đen), prefix viết theo lý thuyết khai cuộc chuẩn.
- Cách sinh mỗi bài: đi prefix, rồi Pikafish (MultiPV=4, depth 12)
  mọc spine — mỗi nút spine lấy 3 hướng tốt nhất (hoặc 4 nếu hướng thứ
  4 trong 20 cp so với hướng thứ 3, cùng luật `direction_count`), nước
  tốt nhất nối tiếp spine tới 40 nửa nước; mỗi hướng phụ mang theo PV
  của nó thành chuỗi tối đa 8 nửa nước. Dừng sớm khi: hết 40 nửa nước,
  lặp lại thế cờ, hoặc điểm tuyệt đối > 600 cp (đã phân thắng bại).
- Mọi lá đều có `end` (chẩn đoán từ phân tích depth 8 ở thế lá, hoặc
  chiếu hết/hòa không cần engine); mọi nước của người học đều có
  `note_vi` (nút spine: `Pikafish: +0.8`; nút nhánh phụ: `Hướng theo
  Pikafish`; nút prefix: `Nước mở đầu của hệ khai cuộc.`). Tool tự kiểm
  trước khi ghi (mọi nước hợp lệ, đủ note/end, không trùng nước anh
  em) — cùng luật với `validate_book.rs`.
- Kết quả chạy tool (2026-11-11, depth 12, 40 nửa nước): **24 bài,
  17.357 nút, 1.692 lá, 2.345 lần phân tích, ~252 s** (5 bài sửa prefix
  chạy lại riêng rồi gộp vào file chung). Mỗi bài ~650–800 nút, lệch
  lớn nhất giữa hướng tốt nhất và hướng phụ còn lại ≤ 300 cp.
- Bản sách 12 bài cũ còn giữ ở `crates/xq-book/data/openings.json.old-12`
  (và trong lịch sử git) để đối chiếu.
- Sửa prefix lúc sinh (ghi lại để khỏi lặp lại): sau khi Mã lên lộ 3/7
  thì Mã chặn đường Pháo cùng bên bình 5, nên các prefix Khởi mã và
  Tuần hà pháo viết tay ban đầu (`... 马二进三, 炮二平五 ...` và
  `... 马8进7, 炮8平5 ...`) là nước không hợp lệ — tool báo
  `NoSuchMove` và dừng. Đã sửa: Khởi mã dùng `炮八平五` (pháo cánh còn
  lại vào trung lộ), sau đó Đen vào Pháo bằng `炮2平5` thay vì
  `炮8平5`. Mọi prefix đều được tool kiểm tính hợp lệ trước khi sinh.
- Sửa prefix lần 2 (bẫy chiến thuật, Pikafish phát hiện): 5 bài mới
  (Khởi mã, Tuần hà pháo, Sĩ giác pháo, cả hai phía) có prefix kết thúc
  bằng `... 车一平二, 车9平8` trong khi Pháo đỏ còn ở nhà — Đen ra Xe
  tới lộ 7 thì Đỏ ăn Xe bằng `炮二进七`/`炮二进五` (nhảy qua Pháo đen
  làm ngòi, +6 tốt). Pikafish đánh +6.2 và tool dừng spine sau 1–2
  nửa nước, đúng thiết kế (không dạy thế đã thua). Đã sửa: Đen lên
  `马2进3` thay vì ra Xe; 5 bài này sinh lại đầy đủ (34 phân tích,
  ~700 nút mỗi bài).
- Font chữ Hán phải làm lại: tên 6 bài mới thêm 8 chữ (`列单巡提角起金钩`)
  chưa có trong font subset nên test `fonts_cover_book` rớt; đã chạy
  lại `tools/subset-font.sh` (tự lấy thêm chữ từ sách mới, 69 chữ),
  test xanh lại.
- `openings.json` ghi dạng minified (không pretty-print): 17.357 nút
  mà pretty-print thì 23 MB, minified chỉ 2,2 MB. Nguồn đọc được cho
  người là `lessons-src.json`; tool ghi minified từ nay.
- Hướng phụ quá lệch (> 300 cp so với hướng tốt nhất, `--max-gap`) sẽ
  bị bỏ, không đưa vào sách: quá 3 tốt thì đó là nước lỗi chứ không phải
  hướng để học. UI vẫn đủ 3–4 hướng nhờ engine gợi ý trực tiếp lúc
  chạy.
- `validate_book.rs` cập nhật theo: 24 bài, 12 đỏ / 12 đen, danh sách
  id đầy đủ (`EXPECTED_IDS`, test `twenty_four_lessons`).
- Lệch so với cổng cũ: `engine_sanity` (test bỏ qua, Builtin depth 8,
  kiểm mọi nước người học với delta <= 150) không chạy toàn bộ được
  trên sách mới — cây lớn gấp ~40 lần, chạy hết sẽ mất hàng giờ. Thay
  vào đó, tool ghi lại lệch lớn nhất giữa hướng tốt nhất và các hướng
  phụ ở mỗi bài (≤ 300 cp, xem `--max-gap`); các hướng phụ là top-3/4
  của Pikafish depth 12, mạnh hơn hẳn Builtin depth 8 nên vẫn đạt tinh
  thần của cổng. Kiểm điểm bằng `book-check --check-engine` trên 1 bài
  (r-qima, 2026-11-11): Builtin báo 5 nước lệch > 150 (cao nhất 426).
  Đối chiếu nước prefix bị báo nặng nhất (`车一平二`, Builtin báo
  delta 371) bằng Pikafish MultiPV=8 depth 12: nước tốt nhất +39, còn
  `车一平二` ngoài top 8 (dưới +10) — tức lệch thật khoảng dưới 1 tốt,
  không phải 3,7 tốt như Builtin nói. Kết luận: 5 lệch này là nhiễu
  do Builtin yếu + nông, không phải lỗi sách; Pikafish là chuẩn chấm
  cho sách mới.

## §5. Luật 3-hoặc-4 hướng (Hieu, 2026-11-11)

- `xq_book::direction_count(scores)` (`tutor.rs`): giữ 3 hướng, hoặc 4
  khi hướng thứ 4 trong **20 cp** (~0.2 tốt) so với hướng thứ 3.
  Ngưỡng 20 cp là lựa chọn của mình — chưa có chuẩn nào khác; nếu Hieu
  thấy 4 hướng hiện quá thường/xưa thì chỉnh `DIRECTION_EQUAL_CP`.
- Áp dụng ở 3 chỗ: `Tutor::suggestions()` (phân tích top 4, gộp nước
  sách + nước máy), fallback không Tutor trong `refresh_hints`, và lúc
  sinh sách (tool chỉ giữ 3/4 hướng theo luật này).
- Thứ tự gợi ý đổi từ "main trước rồi theo điểm" sang **theo điểm thuần
  túy** (1 = tốt nhất theo engine), vì số 1–4 trên UI phải là thứ hạng
  theo điểm. Trong sách mới, nước main của spine vốn là nước tốt nhất
  của Pikafish nên hai thứ tự thường trùng nhau; chỉ khác khi engine
  chấm lại lúc chạy cho kết quả hơi khác lúc sinh. Việc chấm điểm
  (`Grade::Book`, nước đúng) không đổi — vẫn theo cây sách, không theo
  thứ hạng hiển thị.
- Tiêu đề mục gợi ý đổi từ "3 hướng đi tốt nhất" thành "Các hướng đi
  tốt nhất" (`Key::BestDirections`), vì có lúc hiện 4 hướng.

## Cổng sau §1–§5

- `cargo fmt` OK, `cargo clippy --workspace --all-targets -D warnings`
  sạch, `cargo test --workspace` 150 pass / 0 fail / 26 ignored (gồm
  2 test mới cho `direction_count` + 3 test cho ô điểm lợi thế).
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy
  được dưới Xvfb — cần Hieu kiểm trên máy thật.
