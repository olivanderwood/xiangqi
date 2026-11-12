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
  chiếu hết hiển thị `M3`/`-M2` (`format_advantage` trong `app.rs`,
  `MATE = 30000`).
- Màu ô: xanh lá (`0x2E7D32`) khi điểm > 0 (lợi cho người học), đỏ
  (`0xC0392B`) khi < 0, xám khi bằng 0. Điểm là điểm sau nước đi, tính
  từ phía người đi (lượt người học nên chính là phía người học).
- Hàng gợi ý gồm 2 dòng: dòng 1 là số thứ tự + tên nước + ô điểm,
  dòng 2 là giải thích (chữ nhỏ, xám, thụt vào). Bấm vào hàng thì đi
  nước đó; rê chuột lên hàng thì mũi tên tương ứng sáng lên (đồng bộ
  `hint_hover` như trước). Hàng đang rê có nền xanh nhạt.
- Ô điểm là chip riêng (`advantage_chip_ui` trong `app.rs`, từ đợt
  đồng bộ giao diện 2026-11-11): nền tint cùng màu + viền 1,5 px, số
  đậm 28 px, rộng tối thiểu 110 px, nằm sát phải hàng — đọc được ngay
  từ xa, không còn là nhãn 18 px lẫn với tên nước. Màn Đấu với AI
  cũng có chip này ở đầu panel ("Điểm lợi thế", tính theo phía người
  chơi; chế độ 2 người thì theo phía Đỏ), vì thanh eval chỉ cho thấy
  hình mà không cho số.

## §3. Font to, rõ + bố cục (đợt 2: đồng bộ toàn app, 2026-11-11)

- Hieu phản hồi sau đợt 1: font tiếng Việt "vẫn như vậy", giao diện
  chưa tốt — yêu cầu chữ/nút to, đồng bộ rõ ràng, bố cục đồng nhất,
  ô điểm hiển thị to hơn, rõ số điểm. Nguyên nhân: đợt 1 chỉ tăng cỡ
  chữ (Body 18) nhưng nút vẫn dùng padding mặc định (trông nhỏ), nhiều
  chỗ còn dùng chữ `.small()` 15 px, và mỗi màn tự đặt cỡ riêng nên
  không có cảm giác đồng nhất. Font tiếng Việt thì không thiếu glyph
  (đã kiểm: subset Noto Sans có đủ dấu tiếng Việt) — vấn đề là cỡ và
  bố cục, không phải font.
- Một thang chữ duy nhất trong `install_fonts` (`app.rs`): Heading 32,
  Body 20, Button 20, Small 17, Monospace 18; `button_padding`
  18×12, `item_spacing` 10×8. Mọi màn đều đọc từ thang này, không đặt
  cỡ riêng trừ số thứ tự gợi ý (24), tên nước (22) và số điểm (28).
- Helper dùng chung (`app.rs`): `big_button` (nút tối thiểu 120×46 px,
  dùng cho mọi nút panel/dialog), `section_title` (tiêu đề mục đậm
  24 px, dùng giống nhau mọi màn), `advantage_chip_ui` (ô điểm, §2).
- Panel Đấu với AI và Học chơi cùng rộng 380 px (trước là 340/360),
  thanh eval rộng 16 → 20 px; nút Menu 340×64, thẻ chọn bên 300×72.
  Cỡ chữ quân cờ trên bàn không đổi (tính theo ô cờ).

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
  hướng để học. Sách vẫn đủ 3–4 hướng ở mỗi nút spine vì Pikafish
  phân tích MultiPV=4 — UI lấy hướng từ sách, không còn gợi ý trực
  tiếp từ engine lúc chạy (xem §6).
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
- Áp dụng ở 3 chỗ: `Tutor::suggestions()` (chỉ lấy con sách, điểm đã
  lưu — xem §6), fallback không Tutor trong `refresh_hints`, và lúc
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

## §6. Pikafish LÀ sách — dataset nhất quán, Tutor bỏ engine live (Hieu, 2026-11-11)

Hieu báo: đi theo gợi ý Pikafish thì bị báo "nước này không có trong
sách khai cuộc" — gợi ý và sách mâu thuẫn nhau. Quyết định: **coi
Pikafish là sách khai cuộc**, và phải đảm bảo bài học đồng bộ trong mọi
lần thử — có 1 bộ dữ liệu nhất quán từ trước cho từng thế trận.

### Nguyên nhân mâu thuẫn (đã xác minh trong code)
- `Tutor::suggestions()` cũ chấm các con sách bằng engine **live** rồi
  thêm top-4 nước engine live vào gợi ý (`in_book: false`); nhưng
  `on_user_move()` chỉ chấp nhận con của cây sách → đi theo gợi ý
  Pikafish live thì bị báo ngoài sách.
- Engine live còn dùng giới hạn theo thời gian (gợi ý Pikafish 1500 ms)
  nên điểm không tất định giữa các lần thử — cùng một thế, lần này
  gợi ý nước A, lần sau nước B.

### Thiết kế mới: dataset là sách
- Thêm trường `score_cp: Option<i32>` vào `BookNode` (`schema.rs`,
  `#[serde(default)]` để sách cũ vẫn đọc được): điểm Pikafish của
  nước đó, theo góc nhìn bên đi tại nút cha — cùng quy ước điểm UCI
  MultiPV / `Searcher::score_move` (điểm của nước = `-negamax(con)`).
- `tools/gen-book` ghi điểm cho **mọi** nút:
  - Nút spine/nhánh phụ: điểm của line MultiPV mà nó sinh ra.
  - Nút chuỗi PV nhánh phụ: điểm lan truyền theo PV, đổi dấu luân
    phiên (`-S, +S, -S, ...`) — xấp xỉ vài chục cp so với phân tích
    mới, nhưng tất định; chấp nhận được cho chip hiển thị.
  - Nút prefix viết tay: chấm từng nước bằng `score_move` cùng depth
    12 (150 lần chấm cho 25 bài).
  - `self_check` của tool + `validate_book.rs` đều bắt buộc mọi nút
    phải có `score_cp` (test `every_node_has_score`).
- `Tutor` bỏ hẳn engine live (`tutor.rs` viết lại, `xq-book` bỏ
  dependency `xq-engine`):
  - `suggestions()` chỉ lấy con sách, điểm = `score_cp` đã lưu, sắp
    giảm dần, cắt theo `direction_count` (§5). Không còn trường
    `in_book` trên `BookHint`, không còn `with_engine`/`engine()`.
  - Nước ngoài sách chấm `Grade::Acceptable` + giải thích "Nước này
    không có trong sách khai cuộc." + nước đúng = con `main` của sách.
    Không chấm bằng engine live nữa (đã cân nhắc và loại: eval tĩnh mù
    chiến thuật, sẽ khen nhầm nước treo quân; Builtin depth cố định
    chưa kiểm chứng tính tất định).
  - Đối thủ trả lời vẫn theo `weight` với RNG seed cố định
    `0x1234_5678` (tất định theo đường đi); `summary()` vốn dùng
    `eval::eval` tĩnh (tất định) — giữ nguyên.
- Màn Đấu với AI không đổi: ở đó engine live vẫn là AI đối thủ, gợi ý
  phím H vẫn từ engine live — chỉ màn Học chơi mới dùng dataset.
- Nhãn tag Sách/Máy trên hàng gợi ý đã bỏ (cùng đợt này): mọi gợi ý
  nay đều từ sách, không còn phân biệt.
- Test mới (`tests/tutor.rs`, fixture `mini_book.json` nay có
  `score_cp` cho cả 10 nút): `suggestions_use_stored_scores`
  (gợi ý = điểm đã lưu), `suggested_moves_are_always_in_book` (đi theo
  gợi ý đầu tiên ở mọi lượt người học thì mọi nước đều chấm Book —
  chính là lỗi Hieu báo), `off_book_move_is_acceptable_with_book_best`
  (ngoài sách → Acceptable, best = main của sách, không dùng engine).
  Test cũ dùng MockEngine (`tutor_with_mock_engine_grades`) đã xóa.

### Bài mới: Pháo đầu đối Xuyên cung mã
- **ĐÃ THAY BẰNG §7 (2026-11-11):** bài này bị thay bằng Pháo đầu phá
  Phi tượng, Mã quỳ (`r-paodau-phituong-maquy`) theo lệnh Hieu — xem
  §7. Nội dung dưới đây giữ lại để biết lịch sử: Hieu yêu cầu thêm
  1 thế pháo đầu đối xuyên cung mã. Bài mới
  `r-paodau-xcgma` (Đỏ, cấp 2): "Pháo đầu đối Xuyên cung mã"
  (炮头对穿宫马), prefix 6 nửa nước:
  `炮二平五, 马2进3, 马二进三, 马3退5, 车一平二, 马5进4`.
- Ý tưởng thế: sau 1.炮二平五, Đen không ra Xe mà lên Mã xuyên cung —
  Mã chui vào giữa cung (马3退5 tới (4,8), điểm giữa cung Đen) rồi
  lao ra trung lộ (马5进4 tới (3,6)); Đỏ phát triển Xe và Pháo, tìm
  cách vây bắt Mã.
- Prefix viết tay lần đầu SAI, tool bắt được (bài học như §4: đừng tin
  trí nhớ): `马3进4` không hợp lệ vì chân Mã ở (2,6) bị Tốt Đen chặn;
  `马3进5` cũng sai vì đó là nước lùi về cung, phải ghi `马3退5`.
  Đã kiểm lại bằng probe tạm dùng đúng parser của `xq-core` (probe đã
  xóa sau khi dùng): cả 6 nước của prefix mới đều hợp lệ, FEN sau
  马3退5 cho Mã ở giữa cung (`4n4` ở hàng 8).
- Sách nay **25 bài** (13 Đỏ / 12 Đen); `validate_book.rs` cập nhật
  (`EXPECTED_IDS` + test `twenty_five_lessons`). Font subset cần thêm
  2 chữ mới 穿 (U+7A7F), 宫 (U+5BAB) cho tên bài — `subset-font.sh`
  tự dựng lại từ sách mới.
- Kết quả sinh lại toàn bộ (2026-11-11, depth 12, 40 nửa nước):
  **25 bài, 18.011 nút, 2.192 lá, 3.192 lần phân tích, 330,4 s**.
  Bài r-paodau-xcgma: 775 nút, 100 lá, lệch lớn nhất 110 cp, 0 hướng
  bị bỏ vì lệch > 300 cp — thế sau prefix cân bằng, spine mọc đủ.
  `openings.json` minified 2,6 MB.
- Lưu ý về tính cân bằng: Xuyên cung mã là hệ Đen hơi mạo hiểm (Mã
  chui cung làm chậm triển khai) — nhưng báo cáo trên cho thấy thế
  sau prefix cân bằng, không cần chỉnh prefix thêm.

## §7. Pháo đầu phá Phi tượng, Mã quỳ + biến đối thủ soạn sâu + sửa số mũi tên chồng nhau (Hieu, 2026-11-11)

### 7.1. Thay bài Xuyên cung mã bằng Pháo đầu phá Phi tượng, Mã quỳ

Hieu: "đổi pháo đầu đối xuyên cung mã thành pháo đầu phá phi tượng
mã quỳ, nghiên cứu và soạn lại tài liệu dựa trên pikafish cho thế
trận này, nghiên cứu thật kỹ".

**Nghiên cứu.** Ba nguồn chính:
- Định nghĩa 穿宫马 (Wikipedia, thuật ngữ cờ tướng Trung Quốc):
  "马从九宫的一边跳到九宫的另一边；开局泛指飞象后马由底线穿过
  九宫中路到士角位，又称拐角马" — tức là: **sau khi phi Tượng**,
  Mã từ hàng cuối xuyên qua giữa cung tới vị trí góc Sĩ; còn gọi là
  拐角马 (Mã góc). Vậy hệ chuẩn là Tượng phi TRƯỚC, Mã xuyên cung
  SAU — bài cũ (§6) lên Mã trước, chưa phi Tượng, chưa đúng hệ này.
- Tên gọi tiếng Việt: **"Mã quỳ" là tên thật (biệt danh) của Xuyên
  cung mã trong giới cờ Việt** — licadho.org viết rõ: "Xuyên cung
  mã hay còn gọi là 'Tượng triều xuyên cung mã', 'Mã giáp sĩ' hay
  'Mã quỳ'... trước là lên tượng, sau đó nhảy mã cùng phía tượng
  lên khóa chân tượng lại không cho thoái về."; "63 yếu lĩnh trong
  cờ tướng" (Thăng Long Kỳ Đạo): "29. Mã quỳ có thể phòng thủ, giữ
  sĩ hiệu quả."; evbn.org cũng dùng "Mã quỳ" khi phòng thủ chống
  Pháo đầu. Nói trung thực: chữ 跪马 không có trong danh sách thuật
  ngữ Wikipedia tiếng Trung (穿宫马 ở đó chỉ ghi "又称拐角马"), nên
  "Mã quỳ" có thể là tên gọi của giới cờ Việt — nhưng đó đúng là tên
  Hieu đã viết và giới cờ Việt dùng.
- Cách phá (Ziga, "Pháo đầu phá phi tượng cục"): Đỏ dùng Pháo đầu +
  Xe hoành (横车) ép trung lộ; Đen thủ bằng Sĩ Tượng cục, một Mã giữ
  Tốt đầu, một Mã ở biên. Đỏ phá bằng cách lên Xe, thí quân gài thế.

**Sửa tên (2026-11-11, Hieu xác nhận "ừ mã quỳ").** Bản đầu của
mục này ghi giả định "quỳ = quỷ do nhầm dấu" và dùng tên "Mã quỷ"
— giả định đó SAI, đã sửa toàn bộ thành "Mã quỳ" (chỉ đổi tên:
`name_vi` + `summary_vi` trong `lessons-src.json` và
`openings.json`; giữ nguyên id `r-paodau-phituong-maquy`, prefix và
sách đã sinh, không cần sinh lại, không cần đổi font subset).

**Bài mới.** `r-paodau-phituong-maquy` (Đỏ, cấp 2), thay đúng chỗ
của `r-paodau-xcgma` (sau `r-jingoupao`), tên Hán
"炮头破飞象穿宫马" (tên Hán dùng thuật ngữ chuẩn 穿宫马, tên Việt
giữ cách gọi của Hieu: Mã quỳ). Prefix 6 nửa nước (đã kiểm bằng
probe tạm dùng đúng parser của `xq-core`, probe đã xóa):
`炮二平五, 象3进5, 马二进三, 马2进3, 车一平二, 马3退5`
— Tượng phi giữ trung lộ ngay nước 1, Mã lên rồi lùi vào giữa cung
(马3退5 tới (4,8), FEN sau nước này có `4n4` ở hàng 8), rồi spine
của Pikafish sẽ cho Mã lao ra trung lộ (马5进4/进6). Khác bài cũ ở
thứ tự: bài cũ Mã lên trước (马2进3 ở nước 1), bài mới Tượng phi
trước (象3进5 ở nước 1).
- Kết quả sinh lại (2026-11-11, cùng đợt sinh toàn bộ sách §7.2):
  bài mới **1.849 nút, 348 lá, 13 sub-spine đối thủ**, lệch lớn nhất
  299 cp, 18 hướng bị bỏ vì lệch > 300 cp — thế sau prefix cân
  bằng, spine mọc đủ. Tổng sách vẫn **25 bài** (13 Đỏ / 12 Đen);
  `validate_book.rs` cập nhật (`EXPECTED_IDS` + test
  `twenty_five_lessons`). Font subset: chữ Hán mới duy nhất là
  破 (U+7834, trong tên Hán 炮头破飞象穿宫马) — 飞 đã có từ bài
  r-feixiang, còn chữ 鬼 không xuất hiện trong sách vì "Mã quỳ"
  viết bằng chữ Việt;
  test `fonts_cover_book` xác nhận font phủ đủ 60 chữ Hán trong sách.

### 7.2. Biến của đối thủ soạn sẵn 3-4 biến, sâu hơn

Hieu: "ở mode học khai cuộc, đối với đối thủ cũng phải soạn sẵn từ
3-4 biến mạnh nhất, từ đó mở rộng độ sâu của thế cờ".

**Vấn đề.** Trước đây mọi hướng phụ (kể cả của đối thủ) chỉ là một
chuỗi PV đơn (tối đa 8 nửa nước, mỗi nút 1 con): người học đi lệch
khỏi main là hết biến, đối thủ cũng chỉ có một đường duy nhất.

**Thiết kế mới (`tools/gen-book`).** Trong 16 nửa nước đầu của bài
(giai đoạn khai cuộc, nơi bài học sống), hướng phụ của ĐỐI THỦ mọc
thành **sub-spine có nhánh**: gọi đệ quy `children_at` với ngân
sách 8 nửa nước, mỗi nút trong sub-spine lại giữ 3-4 hướng (hướng
phụ bên trong sub-spine quay về chuỗi PV) — chỉ một tầng đệ quy để
khống chế kích thước. Hướng phụ của NGƯỜI HỌC vẫn là chuỗi PV như
cũ (Hieu chỉ yêu cầu cho đối thủ).
- Tham số mới: `--side-spine-plies` (mặc định 8), `--branch-plies`
  (mặc định 16). Báo cáo mỗi bài nay in thêm số sub-spine đối thủ.
- Kiểm chứng: test mới `opponent_variations_are_deep`
  (`validate_book.rs`) — trong 16 nửa nước đầu, ít nhất một biến
  phụ của đối thủ phải có cây con ≥ 20 nút (chuỗi PV tối đa 8 nút,
  sub-spine ~100 nút). Test xanh trên sách mới.
- Kết quả sinh lại toàn bộ (2026-11-11, depth 12, 40 nửa nước,
  MultiPV=4): **25 bài, 49.407 nút, 9.146 lá, 12.810 lần phân tích,
  1.321,6 s (~22 phút)**; mỗi bài có **10–15 sub-spine đối thủ**.
  `openings.json` minified 7,2 MB (trước đây 2,6 MB — cây to hơn
  ~2,7 lần vì biến đối thủ nay có nhánh thật).
  Lưu ý vận hành: lần chạy đầu (một mạch 25 bài) bị runtime kill
  (SIGTERM) giữa chừng sau 2 bài — sinh lại thành 3 chunk
  (8 + 9 + 8 bài, ~7 phút/chunk) rồi gộp bằng script tạm
  `merge-book-chunks.py` (kiểm tra đúng thứ tự/id theo
  `lessons-src.json`); script và file chunk đã xóa sau khi gộp.
  Số nút mỗi bài hơi khác giữa các lần chạy (Pikafish đa luồng
  không tất định hoàn toàn) — dataset trong file là cố định nên
  runtime vẫn nhất quán như §6.

**Để người học thật sự gặp các biến đó.** `Tutor` chọn nước đối thủ
theo weight với seed cố định, nên một lần học chỉ gặp một biến.
Màn Học chơi nay xoay seed theo lần bắt đầu mới (`LearnState.attempt`,
`seed = 0x1234_5678 + attempt`; lần đầu giữ nguyên seed cũ): mỗi
lần vào bài (chọn bài hoặc bấm "Học lại") đối thủ đổi một biến đã
soạn sẵn. Sách (gợi ý, điểm, chấm) giống hệt mọi lần; chỉ nước đối
thủ trong số các biến đã soạn là đổi. Nút "Thử lại" trong bài vẫn
giữ nguyên vị trí nên lần học hiện tại không bị xáo.

### 7.3. Số thứ tự trên mũi tên chồng chéo nhau

Hieu: "các đánh số đang bị chồng chéo nhau, xem xét update lại UI
để trực quan hơn".

**Nguyên nhân.** Huy hiệu số trước đây đặt ở chân Mã (mũi tên Mã)
hoặc giữa mũi tên (mũi tên thường) — hai mũi tên chung chân Mã hay
chung điểm giữa (mũi tên song song/ngược chiều) thì hai số đè lên
nhau. Đĩa số bán kính 0.22 ô cũng to.

**Sửa (`board_view.rs`, `draw_arrow`).** Huy hiệu nay rải dọc theo
mũi tên theo hạng — hạng 1 ở 30% chiều dài, hạng 2 ở 45%, hạng 3 ở
60%, hạng 4 ở 75% — và quạt ngang khỏi đường mũi tên
`(hạng − 1.5) × 0.28 ô`, hai phía đối xứng. Đĩa nhỏ lại (0.19 ô),
thêm viền trắng để hai đĩa kề nhau vẫn đọc được. Huy hiệu vẫn nằm
trên đường mũi tên của nó nên số vẫn gắn với đúng mũi tên đó.
Chưa kiểm bằng mắt (VM không chạy được app).

### Cổng sau §7

- `cargo fmt` OK, `cargo clippy --workspace --all-targets -D warnings`
  sạch, `cargo test --workspace` **154 pass / 0 fail / 26 ignored**
  (tăng 1 test so với 153: `opponent_variations_are_deep` mới;
  `respects_time` ở `xq-core` có rớt 1 lần khi chạy song song trên
  VM 1,5 CPU — chạy riêng lại xanh, là chập chờn đã biết từ trước,
  không liên quan đợt này).
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy
  được dưới Xvfb — cần Hieu kiểm trên máy thật (đặc biệt huy hiệu số
  trên mũi tên và biến đối thủ khi học lại).

## §8. Đối thủ: người học tự chọn biến + bài học đủ 20 nước (Hieu, 2026-11-11)

Hieu: "trong mode học khai cuộc thì đến lượt đối thủ đánh sẽ hiện ra
các biến đi để người dùng tự chọn. hiện tại các bài khai cuộc chỉ có 8
nước trong khi tôi yêu cầu lên đến 20 nước, hãy cập nhật thêm".

### 8.1. Vì sao bài học chỉ "có 8 nước" dù main line đã đủ 20

Đo trên sách §7 (`openings.json` 49.407 nút): main line của cả 25 bài
đều đủ 40 nửa nước (20 nước), nhưng có 9.146 lá với độ sâu trung bình
chỉ 21,9 nửa nước, nông nhất 10 nửa nước (5 nước). Nguyên nhân: bất
cứ khi nào người học (hoặc đối thủ) rời main line, nhánh phụ chỉ dài
tối đa 8 nửa nước (chain) hoặc 8 nửa nước có nhánh (sub-spine) rồi
hết — bài học kết thúc. Đối thủ lại tự chọn biến theo weight (main
1.0, phụ 0.2×3 → lệch khỏi main ~37% mỗi lượt), nên thực tế gần như
mọi bài học đều kết thúc sau ~8 nước. Hai yêu cầu của Hieu thực ra
là một vấn đề: bài học kết thúc sớm vì nhánh phụ nông.

### 8.2. Đối thủ: người học tự chọn biến

- `Tutor::play_opponent_move(idx)` mới (`xq-book/src/tutor.rs`):
  đi đúng con `children[idx]` của nút hiện tại, trả về
  `TutorEvent::OpponentMoves` như `next_opponent_move`; `None` khi
  không phải lượt đối thủ, ngoài sách, hoặc idx vượt phạm vi.
  `next_opponent_move` (chọn theo weight) giữ lại cho trường hợp chỉ
  có 1 con (tự đi sau 600 ms như cũ).
- Màn Học chơi (`screens/learn.rs`): khi `opponent_due` tới, nếu nút
  hiện tại có ≥ 2 con thì hiện panel "Đối thủ đi biến nào?" thay cho
  phần gợi ý — các biến xếp theo điểm (1 = tốt nhất cho đối thủ),
  mỗi hàng có tên nước + ô điểm lợi thế **theo phía người học** (điểm
  của đối thủ lấy dấu ngược lại, vì `score_cp` lưu theo góc nhìn bên
  đi) + ghi chú; bấm một hàng thì đối thủ đi biến đó
  (`play_opponent_choice`). Các biến cũng hiện thành mũi tên đánh số
  trên bàn cờ (cùng thứ tự với panel, dùng `HintArrow::for_rank`).
  Chỉ có 1 con thì tự đi; 0 con thì engine chơi như cũ (§5.3.4).
  Key i18n mới: `OpponentChooseTitle`, `OpponentChooseMsg`.
- **Seed xoay theo lần học lại (§7) đã bỏ**: người học tự chọn biến
  nên không còn cần RNG chọn biến cho đối thủ nữa — `LearnState`
  bỏ field `attempt`, `make_tutor` quay về seed cố định
  `0x1234_5678`. API `Tutor::new(lesson, seed)` giữ nguyên (test
  `opponent_follows_weights` vẫn dùng).
- Test mới (`xq-book/tests/tutor.rs`): `learner_chooses_opponent_variation`
  (chọn idx 1 → đúng nước nhánh B bất kể seed; idx vượt phạm vi →
  `None`) và `play_opponent_move_rejects_learner_turn`.

### 8.3. Kéo dài mọi nhánh phụ tới hết 40 nửa nước

- gen-book: sau khi mọc cây, `extend_leaves` đi khắp cây; mỗi lá ở
  độ sâu < 40 nửa nước được nối thêm bằng `extend_from` — theo nước
  tốt nhất của Pikafish (MultiPV=1, depth 12) từng đoạn, mỗi đoạn tối
  đa 12 nửa nước (dài PV), phân tích lại khi hết đoạn, tới khi đủ 40
  nửa nước. Điểm lan truyền đổi dấu luân phiên trong mỗi đoạn (cùng
  quy ước `side_chain`, nhưng đổi dấu tính lại từ đầu mỗi đoạn vì
  điểm của đoạn mới tính từ bên đi tại đó); nước của người học ghi
  chú "Hướng theo Pikafish", nước đối thủ không ghi chú.
- Phần nối thêm là **đơn tuyến** (không rẽ nhánh mới): rẽ nhánh đầy
  đủ tới 40 nửa nước là hàm mũ (cây 3 hướng sâu 40 có ~3^20 lá) nên
  không thể; các lựa chọn vẫn nằm ở điểm rẽ nhánh, phần nối thêm là
  nước tốt nhất để học thuộc (Tutor chấm từng nước theo đúng nước
  sách duy nhất đó). Dừng sớm khi: thế cờ đã quyết định (điểm phân
  tích tiếp theo |cp| > 600), hết nước đi, hoặc nước tốt nhất lặp lại
  thế cờ đã có trên đường đi.
- Vì sao không nối tất cả lá lên 40 một cách "rẻ": đo trước khi làm —
  nối toàn bộ lá lên 40 tốn +165.126 nút (tổng 214.533, file ~31 MB),
  trong đó 9.146 lá. Đây là cái giá của "mọi đường đi đều 20 nước"
  và Hieu đã yêu cầu rõ, nên làm trọn (không cắt ở 14/16 nước).
- **Đẩy lên repo được không?** File 30,3 MB làm helper push cũ (mỗi
  lần upload lại TOÀN BỘ file, timeout 300 s/request) không đẩy nổi
  (7,2 MB đã mất ~200 s). Đã sửa 2 chỗ trong helper push:
  `github_push_xiangqi.py`: timeout `api()` 300 → 1800 s; và
  `github_push_xiangqi_update.py`: lấy cây remote (`?recursive=1`),
  tính `git hash-object` cho file local, file nào trùng SHA với cây
  remote thì tái dùng blob cũ (không upload lại) — SHA của blob chỉ
  phụ thuộc nội dung nên tái dùng là an toàn; kiểm tra cây
  (tree == local tree) vẫn bắt lỗi nếu có sai sót. Nhờ đó các lần
  push sau (chỉ đổi code) không phải upload lại file sách 30 MB.
- Sinh lại toàn bộ sách (3 chunk như §7; chunk 3 bị runtime restart
  kill lúc đang làm bài 6/8 — gen-book chỉ ghi file cuối cùng nên 5
  bài đã xong bị mất, phải chạy lại cả chunk từ đầu):
  chunk 1: 8 bài, 70.094 nút, 9.789 phân tích, 821,8 s;
  chunk 2: 9 bài, 79.188 nút, 11.084 phân tích, 1.011,1 s;
  chunk 3: 8 bài, 69.828 nút, 10.040 phân tích, 802,8 s.
  **Tổng: 25 bài, 219.110 nút, 9.328 lá, 30.913 lần phân tích,
  ~2.635,7 s (~44 phút); `openings.json` minified 30.253.769 byte
  (~30,3 MB, trước 7,2 MB).** Mỗi bài 12–15 sub-spine đối thủ.
- Test mới `every_path_reaches_twenty_moves`
  (`validate_book.rs`): mọi lá phải ở `max_plies` (40), hoặc là thế
  cờ được phép dừng sớm — hết nước đi, đã quyết định, hoặc lặp lại
  thế cờ trên đường đi. **"Đã quyết định" đọc từ ghi chú `end`** (phân
  tích mới depth 8 của `finish_leaves`), KHÔNG đọc từ `score_cp` của
  nút: điểm lưu trong nút là xấp xỉ lan truyền từ phân tích cũ, có
  thể lệch rất xa giá trị thật (một lá lưu +45 cp nhưng thực tế +730
  cp, chiếu hết trong 2 nước). Ngưỡng 4,0 tốt trên ghi chú `end`
  (phân tích depth 12 của extension dừng ở >6,0 tốt; ghi chú depth 8
  đọc thấp hơn một chút, và nước tốt nhất dạng xáo quân cũng dừng
  extension trên thế cờ thắng rõ — đã kiểm bằng tay một lá ở
  r-paodau-phituong-maquy, 35 nửa nước: depth 12 cho −583 cp phía
  Đen, PV xáo quân lặp lại thế cờ). Probe tạm đã xóa sau khi dùng.
- Font subset chạy lại: sách chỉ dùng 25 chữ Hán phân biệt (tên quân
  + số + hướng đi, tập cha `cjk_chars.txt` có 70 chữ) nên không thêm
  chữ mới; `fonts_cover_book` xanh.

### Cổng sau §8

- `cargo fmt` OK, `cargo clippy --workspace --all-targets -D warnings`
  sạch, `cargo test --workspace` **157 pass / 0 fail / 26 ignored**
  (tăng 3 test so với 154: 2 test `play_opponent_move` +
  `every_path_reaches_twenty_moves`; test này từng FAIL trên sách cũ
  — đúng thiết kế, vì nó bắt được lá kết thúc sớm).
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không
  chạy được dưới Xvfb — cần Hieu kiểm trên máy thật (panel chọn biến
  đối thủ + mũi tên đánh số của biến đối thủ + bài học dài 20 nước
  khi đi lệch main line).
- Chưa push: chờ lệnh Hieu. Lưu ý lần push này sẽ upload file sách
  30,3 MB (lần đầu sau khi sửa helper, timeout 1800 s) nên sẽ mất
  ~10–15 phút chạy nền.

## §9. Sách mới từ dữ liệu VietCoTuong — 24 bài theo đường chính hay gặp thật (Hieu, 2026-11-11)

### Dữ liệu
- **Scrape VietCoTuong** (`https://explore.vietcotuong.com/badge/openings?lang=en`, chỉ trang HTML công khai, robots cấm `/api/`, không đăng nhập, 0 lần 429/5xx/timeout): **38.804 ván duy nhất / 821 bộ / 52 mã ECCO / 3.309.908 nước**; 7 bộ thiếu 4.538 lượt (phân trang kẹp ở trang 101). Dữ liệu ở `data/vietcotuong/` (155 MB). FEN VietCoTuong dùng `H`=Mã, `E`=Tượng → đổi `H→N, E→B, h→n, e→b`. Giấy phép: không có điều khoản trên site; dữ liệu trích từ sách — không tái phân phối nguyên văn, dùng nội bộ để sinh sách thì được.
- **Tool `tools/build-lessons/`** (Rust): replay games.jsonl qua xq-core → `data/vietcotuong/ecco-trees.json` (**37.860 ván ok, 7 nước sai luật, 937 ván bắt đầu giữa chừng bỏ qua; 42 mã ECCO**, đường chính 12 nửa nước + top 4 nước mỗi nút). Thêm `--chessdb-out`: queryall + queryscore qua curl, delay 400 ms, resume theo FEN. Lỗi đã sửa: ChessDB trả thêm **byte NUL `\0`** cuối response làm parse score fail — strip `\0` ở cả hai action.
- **Phát hiện thiết kế: mã ECCO trùng đường chính** — chỉ 19 đường khác nhau ở 6 nửa nước, **25 đường ở 12 nửa nước** (nhóm lớn nhất 19.897 ván Pháo đầu–Bình phong mã). Hieu chốt 2026-11-11 22:47 "lấy hết" → làm bài theo 25 đường chính khác nhau ở 12 nửa nước, không theo mã ECCO (29/52 bài sẽ trùng nhau).
- **ChessDB eval toàn bộ** (`data/vietcotuong/chessdb-eval.json`): **504 vị trí / 178 FEN duy nhất / 42 mã ECCO, 503 có score, 1 top rỗng**; 308 query mới + 196 dùng lại cache.
- **Repo chasoft** (`chasoft/community-xiangqi-games-database`) **CHÍNH LÀ database của vietcotuong.com**. Đã clone sparse `data/opening` (386 bộ, 359 MB) về `data/chasoft-scan/` và parse bằng `data/parse-dpxq.py` → `data/dpxq-trees.json`: **38.853 file .dpxq, 0 mainline lỗi, 137.524 biến phụ, 317.456 chú giải, 258 mã ECCO, 2.002 file bắt đầu giữa chừng**. Định dạng .dpxq đã giải mã + kiểm chứng (movelist 4 chữ số/nước, rank đếm từ trên: sang ICCS rank = 9 − rank_dhtmlxq; `move_P_B_I` = biến phụ rẽ từ dòng P sau B nước). Chưa kiểm hợp lệ toàn bộ nước .dpxq bằng replay qua xq-core; chưa tích hợp biến phụ .dpxq vào gen-book.

### 25 bài → 24 bài (bỏ r-gongpao)
- `data/gen-lessons-src.py` sinh 25 bài: prefix = đường chính 12 nửa nước, tên theo hệ thống + mã ECCO chính (vd "Pháo đầu đối Bình phong mã (C04)"), learner **red hết**, cấp 1 (≥3.000 ván) / 2 (≥150) / 3 (còn lại). 3 bài prefix chỉ 2 nửa nước (b00/a10/d25). Chỉ 3/25 bài cũ khớp đường chính dữ liệu — bộ mới là bộ khác hẳn. Bài Mã quỳ Hieu dặn trước KHÔNG nằm trong top 25 — Hieu chốt 2026-11-11 22:59 **"ok bỏ"** → bỏ Mã quỳ, bộ sách thuần theo dữ liệu.
- **gen-book** (`--branch-plies 28` = 12 prefix + 16, giữ cửa sổ sub-spine như §7): **25 bài, 220.129 nút, 12.982 lá, 37.027 lần phân tích, 3.287,7 s; `data/openings-new.json` 30,5 MB**; book-check pass; mọi nút có score_cp; max_plies đều 40. Đã swap vào app (`crates/xq-book/data/openings.json` + `lessons-src.json`; bản cũ backup ở `data/openings-old.json` + `data/lessons-src-old.json`).
- **Mojibake đã sửa**: chuỗi `\u00e2\u0080\u0094` (bytes UTF-8 của "—" bị lưu sai từ scrape) thay thành "—" trong 17 tên ở ecco-trees.json + 11 summary_vi trong openings-new.json (chỉ text hiển thị); đã thêm hàm `clean_name()` vào gen-lessons-src.py.
- **r-gongpao bị bỏ** (Hieu, 2026-11-12): bài `r-gongpao` (Cung pháo cục, d25, chỉ 2 ván) **không phải khai cuộc thật** — prefix `炮二平四 卒5进1 炮八平五 卒5进1 炮五进二 卒3进1 炮四进三 卒3进1 炮四平五` là đường Đen đi sai bị Đỏ **chiếu hết ở nửa nước 9**. Cây của bài chỉ có **9 nút / 1 lá / 0 sub-spine**, nên test `opponent_variations_are_deep` fail. Gỡ nguyên lesson khỏi 4 file JSON (`crates/xq-book/data/openings.json`, `data/openings-new.json`, `crates/xq-book/data/lessons-src.json`, `data/lessons-src-new.json`) — các lesson độc lập nên KHÔNG cần chạy lại gen-book 55 phút. Sửa test: `EXPECTED_IDS` 25→24, `twenty_five_lessons` → `twenty_four_lessons` (24 red / 0 black), `book_parses` 25→24. **Sách hiện tại: 24 bài, 220.120 nút, 12.981 lá, mọi nút có score_cp, mọi bài max 40 nửa nước, learner red hết.**
- Smoke test xq-app cũng phải sửa theo tên bài mới: `menu_to_learn_plays_book_move` và `menu_to_learn_select_shows_lessons_for_each_side` từng hardcode tên bài cũ ("Pháo đầu Xe qua hà") → đổi sang "Pháo đầu đối Bình phong mã (C04)"; bỏ assert cứng `black_count == 12` (sách §9 không còn bài black) vì phía Đen nay là trạng thái trống.

### Cổng sau §9

- `cargo fmt` OK, `cargo clippy --workspace --all-targets --release -D warnings` sạch, `cargo test --workspace --release` **pass toàn bộ, 0 fail** (gồm `validate_book` 12 pass: `twenty_four_lessons` + `opponent_variations_are_deep` nay xanh — việc bỏ r-gongpao đã sửa đúng test từng fail), `book-check --book crates/xq-book/data/openings.json` pass (exit 0).
- **Thêm 2 bài Bình phong mã 卒7进1 (Hieu, 2026-11-12)** từ scan 92.931 ván thật (`data/selected-games` + `data/tournaments`): `r-paodau-pingfeng-tot7-c45` (C45/C49, 1.098 ván, 7.455 nút, 471 lá, 19 sub-spine) và `r-paodau-pingfeng-tot7-c23` (C23/C24, 886 ván, 7.118 nút, 442 lá, 19 sub-spine). Sinh bằng gen-book chỉ 2 bài mới (`--src data/lessons-src-c45-c23.json --branch-plies 28`, 254,9 s, 2.596 lần phân tích) rồi gộp vào sách — không chạy lại 24 bài cũ. **Sách hiện tại: 26 bài, 234.693 nút, 13.894 lá, mọi nút có score_cp, mọi bài max 40 nửa nước, learner red hết.** Test cập nhật: `EXPECTED_IDS` 24→26, `twenty_four_lessons` → `twenty_six_lessons`, `book_parses` 24→26. Font subset chạy lại (thêm 4 chữ 两互兑蛇 từ tên Hán của 2 bài mới; `cjk_chars.txt` nay 55 chữ) nên `fonts_cover_book` xanh. Cổng sau khi thêm: fmt OK, clippy sạch, `cargo test --workspace` 143 pass / 0 fail, book-check pass.
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy được dưới Xvfb — cần Hieu kiểm màn Học với sách mới trên máy thật.
- Chưa push: chờ lệnh Hieu.

## §10. ChessDB live scoring — hệ thống chấm điểm riêng, section riêng ở cả 2 chế độ (Hieu, 2026-11-11)

### Thiết kế đã chốt (2026-11-11 23:50)
- **Hệ thống chấm điểm RIÊNG, độc lập** (không thay Pikafish/sách); **sau mỗi nước thì gọi API live** để lấy điểm mới của thế cờ và show lên; **làm riêng 1 section cho điểm đó, cho cả 2 chế độ** Chơi với AI và Học. Hệ quả: section này CẦN mạng (app không còn offline 100% — chỉ phần ChessDB cần mạng, còn lại vẫn offline); khi không có mạng thì hiện "không khả dụng", không crash.
- API: `http://www.chessdb.cn/chessdb.php?action=queryall|queryscore&board=<FEN>` (HTTP, miễn phí, không key; FEN quy ước chuẩn N/B — dùng thẳng `pos.to_fen()`). queryall trả các nước cách nhau bằng `|`, mỗi nước là `key:value` cách nhau bằng dấu phẩy: `move:b2e2,score:1,rank:2,note:! (45-02),winrate:1.87,drawrate:96.40,lossrate:1.73`; thế cờ không có dữ liệu → body là `unknown` / `invalid board` / rỗng. Response có **byte NUL cuối** phải strip. Mã ECCO trong note là hệ số (vd `45-02`), KHÁC hệ chữ+số của VietCoTuong, chưa có ánh xạ.
- **Điểm + tỉ lệ thắng/hòa/thua của ChessDB đều theo góc nhìn ĐỎ** (kiểm chứng trên eval: vị trí Đen đi thì 196 âm / 3 dương) — khớp cách hiện lợi thế của app nên hiện thẳng được. **`queryscore` == `top[0].score` của queryall ở 503/503 vị trí** → client chỉ cần gọi 1 lần queryall (có luôn điểm + top nước + tỉ lệ). Điểm có thể cực trị kiểu 29999 (chiếu hết) — `format_advantage` xử lý được (ngưỡng MATE−500).

### Implement
- **Crate mới `crates/xq-chessdb/`** (đã thêm vào workspace members + dep của xq-app): `Cargo.toml` dùng `ureq = { version = "2", default-features = false}`. `src/lib.rs`: `ChessDbClient::query_all(fen)` (timeout 10 s, chỉ queryall), `ChessDbMove`/`ChessDbEval { fen, score: Option<i32>, top}`/`ChessDbError { Network, HttpStatus, Parse}` (Display tiếng Việt); `parse_queryall` public (strip NUL; unknown/invalid/empty → Ok(vec rỗng) = "không có dữ liệu", không phải lỗi; garbage → Parse error; giữ top 4). **Proxy**: `proxy_from_env()` đọc `http_proxy` rồi `HTTP_PROXY` (ureq KHÔNG tự nhận proxy env; thiếu nó thì live test fail "HTTP version did not start with HTTP/" trên VM này; trên máy Hieu không có env proxy thì nối trực tiếp). 5 unit test parse pass; live test `live_queryall_startpos` (#[ignore]) pass sau khi có proxy (0,79 s).
- **Worker `crates/xq-app/src/chessdb_worker.rs`** (theo mẫu ai_worker.rs): `ChessDbWorker` (thread riêng, mpsc channel, **cache HashMap theo FEN** nên Undo/chuyển màn không gọi lại HTTP), `ChessDbRequest::Evaluate { fen, req_id}`, `ChessDbResponse::Evaluated { req_id, result}`, `response_is_current`, enum `ChessDbView { Idle, Loading, Ready(ChessDbEval), Unavailable(String)}` (không dữ liệu → Unavailable với text ChessDbNoData; lỗi mạng → Unavailable với `err.to_string()`). 2 unit test pass.
- **Tích hợp app** (`app.rs`): fields `chessdb: ChessDbWorker`, `chessdb_view`, `chessdb_req_id: Option<u64>`, `chessdb_last_fen: Option<String>`. Method `poll_chessdb(&mut self, fen, ctx)`: drain rx bằng try_recv, áp response đúng req_id, chỉ gửi request mới khi FEN khác FEN đã hỏi. Method `chessdb_section_ui(&self, ui, pos)`: `section_title` "Điểm ChessDB" + `advantage_chip_ui` (điểm góc Đỏ) + dòng tỉ lệ "Đỏ thắng {n}% • Hòa {m}% • Thua {k}%" (lấy rates của nước tốt nhất) + "Nước tốt nhất: {name}" (parse ICCS bằng `notation::parse_iccs` rồi `render_move`, fallback về ICCS thô nếu parse fail); Loading hiện "Đang lấy điểm ChessDB…".
- **i18n** (đã thêm vào `i18n.rs`): Key `ChessDbTitle` "Điểm ChessDB", `ChessDbLoading` "Đang lấy điểm ChessDB…", `ChessDbNoData` "ChessDB chưa có dữ liệu cho thế cờ này", `ChessDbWinRate` "Đỏ thắng {n}% • Hòa {m}% • Thua {k}%", `ChessDbBestMove` "Nước tốt nhất: {name}".
- **Hai màn đã nối**: `vs_ai.rs` — `show_vs_ai` gọi `poll_chessdb` với `vs_ai_display_pos().to_fen()`, section đặt trong `vs_ai_panel` sau khối điểm lợi thế/chấm, trước "Game info"; `learn.rs` — `show_learn` gọi `poll_chessdb` với `learn.game.pos.to_fen()`, section đặt trong `learn_panel` ngay sau khối chấm của Tutor, trước khối chọn biến đối thủ.

### Nguyên tắc vàng khi kết hợp Cloud Book (ChessDB) và Engine (Pikafish) — Hieu, 2026-11-12
- **Ưu tiên Cloud Book ở khai cuộc** (10–15 nước đầu): biến có mẫu số lớn (nhiều người chơi) và điểm cân bằng trên Cloud Book thường định hướng chiến lược tốt hơn.
- **Tin Engine khi có biến cố chiến thuật**: đòn phối hợp phức tạp, cạm bẫy sát cục, tàn cuộc ít quân — để Engine tính sâu vì Cloud Book đôi khi chứa nước lỗi thời/bẫy chưa cập nhật.
- **Kiểm tra chéo**: nước có tỉ lệ thắng cao trên Cloud Book nhưng Engine chấm thấp → đi thử nước đó và xem biến tiếp theo của Engine để hiểu cạm bẫy ẩn giấu.
- Áp dụng: section ChessDB là hệ thống riêng hiển thị song song với điểm Engine/sách để người dùng đối chiếu; khi 2 nguồn lệch nhau thì áp dụng 3 quy tắc trên. (Chưa implement gợi ý tự động khi 2 nguồn lệch nhau — chỉ ghi làm nguyên tắc thiết kế; nếu Hieu muốn hiện gợi ý trong section thì làm riêng.)

### Cổng sau §10

- `cargo fmt` OK, `cargo clippy --workspace --all-targets --release -D warnings` sạch, `cargo test -p xq-chessdb -p xq-app --release` **31 pass (xq-app) + 5 pass (xq-chessdb), 0 fail, 1 ignored** (live test). 2 smoke test từng fail vì hardcode tên bài cũ — đã sửa theo tên bài mới (§9) và xanh trở lại.
- **Crash 0xc0000409 khi vào bàn, có mạng (Hieu, 2026-11-12) — ĐÃ SỬA (nguyên nhân thật, không phải ureq)**: app đứng rồi tự thoát với `STATUS_STACK_BUFFER_OVERRUN` khi vào bàn chơi ở **cả 2 màn** (Đấu với AI và Học); **ngắt mạng thì hết crash**. Ban đầu nghi `ureq` nên đã bỏ hẳn `ureq` 2.12.1, viết lại client HTTP/1.1 tối giản bằng `std::net::TcpStream` trong cùng crate (`http_get` + `parse_http_response` + `decode_chunked` + `percent_encode` + `base64_encode` cho proxy Basic auth + `parse_proxy_url`; vẫn hỗ trợ proxy qua `http_proxy`/`HTTP_PROXY`; giao diện `query_all`/`parse_queryall` không đổi; thêm 7 unit test HTTP, `xq-chessdb` nay 12 test; live test pass qua proxy VM 1,13 s; push `2c9a3a5`) — **nhưng Hieu báo vẫn crash, ureq không phải thủ phạm**. Khoanh vùng tiếp: tắt hẳn ChessDB (`8247ce6`) thì hết crash → lỗi trong ChessDB; bật lại worker, giữ tắt phần vẽ section (`485279b`) thì hết crash → lỗi ở **phần vẽ section**. **Nguyên nhân thật**: nước tốt nhất của ChessDB có thể thuộc về một thế cờ khác với thế đang hiển thị (xem lại nước cũ ở chế độ Đấu với AI, hoặc đi nước mới trong lúc trả lời đang bay về). Code cũ chỉ kiểm tra nước parse được dạng ICCS rồi gọi `render_move` → `render_cn`/`render_vn`, mà hai hàm này `.expect("moving piece")` nên panic khi ô xuất phát trống. Bản release đặt `panic = "abort"` nên panic thành crash native 0xc0000409. **Sửa thật (`4798dfc`)**: trước khi vẽ, kiểm tra nước có trong `legal_moves()` của thế cờ hiện tại không; nước không hợp lệ (hoặc không parse được) thì hiện nguyên chuỗi ICCS thô, không bao giờ panic. Bật lại section ChessDB. Cổng sau sửa: fmt OK, clippy sạch, `cargo test --workspace` 171 pass / 0 fail. Cần Hieu kiểm lại trên máy thật (có mạng, cả 2 màn, thử xem lại nước cũ / đi nhanh) xem còn crash không và section "Điểm ChessDB" hiện lại đúng không.
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy được dưới Xvfb — cần Hieu kiểm section "Điểm ChessDB" trên máy thật (cả 2 chế độ, cả khi có mạng và khi không có mạng).
- Chưa push: chờ lệnh Hieu.

## §11. Trải nghiệm mượt mà: nước tiếp theo theo sách hiện ngay, hủy tính toán cũ, nút Lùi nước (Hieu, 2026-11-12)

- **Vấn đề**: sau khi người học hạ cờ, giao diện phải phản hồi ngay và đưa ra các nước kế tiếp ngay lập tức theo sách (chưa tối ưu qua API cũng được, đưa trước); điểm số / phương án tối ưu hơn tính sau. Nếu người dùng đi tiếp trong lúc đang tính, phải hủy tính toán / API gọi ở bước trước. Thêm nút lùi nước cho mode Học. (Bài khai cuộc cho bên hậu thủ làm luồng riêng — xem báo cáo nghiên cứu 12 bài Đen, chờ Hieu chốt đợt làm.)
- **Nước tiếp theo hiện ngay**: `Tutor::peek_next()` (mới, chỉ đọc, không đổi trạng thái) trả về nước đáp chính của đối thủ + các nước của người học sau nước đó, từ sách (điểm đã lưu). `LearnState.next_preview` được đặt ngay trong `play_user_move` (khi nước trong sách) và panel Học hiện section "Nước tiếp theo theo sách" ngay trên section ChessDB — tức là nước sách hiện TRƯỚC điểm live. Preview bị xóa khi đối thủ đi (`after_opponent_move`), khi người học đi lại, khi retry / đổi bài. Điểm của nước đối thủ được đổi dấu về phía người học (theo quy ước lưu trữ §6); điểm các nước tiếp theo đã ở phía người học. Test mới `peek_next_returns_reply_and_followups` (kiểm cả tính chỉ-đọc).
- **Hủy tính toán cũ**: ChessDB worker giờ bỏ qua các request cũ đã xếp hàng — mỗi vòng chỉ xử lý request mới nhất trong hàng đợi (`try_recv` để rút hết), nên người dùng đi nhanh liên tiếp không làm worker tụt lại sau; response cũ bay về vẫn bị loại bằng `req_id` như trước. Mode Học: `LearnState::cancel_opponent_ai` gọi `worker.request_stop()` khi retry / lùi nước, hủy nước AI đối thủ (ngoài sách) đang tính. (Mode Đấu với AI đã có hủy khi Hoàn nước từ trước.)
- **Nút Lùi nước (Undo) cho mode Học**: nút "Hoàn nước" ở đầu panel Học (bên phải dòng tiến độ), tắt khi chưa có nước nào. `LearnState::undo(worker)` lùi về trước nước cuối của người học (lùi cả nước đáp của đối thủ nếu đã đi), hủy nước đối thủ đang chờ (`opponent_due`) / AI đang tính / hộp chọn biến, xóa preview + trạng thái hoàn thành, đồng bộ lại Tutor. `Tutor::undo_ply()` (mới) lùi 1 nước trong Tutor: trả lại `node_path` (nước trong sách là tiền tố của danh sách nước — lùi nước trong sách thì pop `node_path`), trả lại grade + `book_moves` khi lùi nước của người học, và lùi qua điểm rời sách thì về lại trong sách (xóa snapshot rời sách). Trường hợp nước AI ngoài sách chỉ có trong game của màn hình (không có trong game của Tutor) thì Tutor chỉ lùi khi nước cuối của nó khớp nước vừa lùi. Test mới: `undo_ply_restores_learner_turn`, `undo_ply_restores_in_book_after_off_book`.
- **Cổng**: fmt OK, clippy sạch, `cargo test --workspace` 174 pass / 0 fail (tăng 3 test Tutor mới). Chưa kiểm bằng mắt trên máy thật.

## §12. Hạ cờ ở Đấu với AI (tiếp tục từ bài học) phải hiện ngay, chấm nước tính sau (Hieu, 2026-11-12)

- **Vấn đề**: đánh theo gợi ý (màn Học) đã mượt, nhưng tự đánh tiếp với AI thì vẫn chờ lâu ở nước hạ cờ. Nguyên nhân: ở chế độ tiếp tục từ bài học (`from_learn`), `VsAiState::play()` chấm nước người chơi **đồng bộ ngay trong khung hình** — gọi `engine.best_move()` rồi `engine.score_move()` trên luồng UI, chặn bàn cờ vài giây (thời gian Pikafish suy nghĩ) trước khi nước kịp hiện. Chế độ Đấu với AI thường không bị vì không có đoạn chấm đồng bộ này.
- **Sửa**: hạ cờ trước, chấm sau. `play()` chỉ áp nước lên bàn (đặt `last_move`/`anim` như cũ) và ghi lại `pending_grade` (engine request cho thế cờ trước nước + nước vừa đi). Khung hình tiếp theo, `show_vs_ai` gửi grade **sau** khi `maybe_trigger_ai` đã gửi nước của AI — nên AI được xếp hàng trước, chấm không bao giờ làm chậm nước trả của AI. Worker mới có request `GradeMove` + response `Graded`: chạy `best_move` + `score_move` trên luồng worker (cùng luật chấm cũ: `grade_from_delta_with`, `explain_tags`), trả grade về thì panel mới hiện điểm chấm. Grade cũ bị hủy khi Undo / đi nước mới (xóa `pending_grade`/`grade_req_id`, response cũ loại bằng `req_id`); Learn `poll` bỏ qua `Graded` (không dùng).
- **Cổng**: fmt OK, clippy sạch, `cargo test --workspace` 175 pass / 0 fail (thêm test worker `grade_move_returns_a_grade`: nước 炮二平五 chấm Best/Book). Chưa kiểm bằng mắt trên máy thật — cần Hieu thử: học xong 1 bài → "Đánh tiếp với AI" → đi nước tự do, quân phải hiện ngay, điểm chấm hiện sau vài giây, AI trả nước không chậm hơn trước.

## §14. Animation hạ cờ phải chạy xong rồi mới tính toán (Hieu, 2026-11-12)

- **Vấn đề**: click/drop đặt quân thì màn hình dừng hình cho đến lúc đối thủ đi quân. Hieu yêu cầu: mọi animation drag-drop phải thực hiện xong rồi mới thực hiện tính toán. Nguyên nhân có 2 lớp: (1) `VsAiState::play()` khóa engine (`grade_limits()`) để dựng `EngineRequest` chấm nước — nếu worker đang chạy phân tích gợi ý (Pikafish 1,5 s) thì luồng UI bị chặn ngay trong khung hình hạ cờ, animation không kịp vẽ; (2) ngay khung hình sau, AI/gợi ý/ChessDB được kích hoạt liền trong khi quân còn đang trượt (150 ms), và `maybe_trigger_ai`/`request_hint` cũng khóa engine nên có thể chặn tiếp.
- **Sửa**: (a) `GradeMove` không mang `EngineRequest` nữa mà mang FEN gốc + danh sách nước trước nước chấm + nước cần chấm; worker tự dựng `EngineRequest` bằng `grade_limits()` của engine nó đang giữ — UI không bao giờ khóa engine để chấm nước. (b) Hoãn mọi tính toán tới khi animation xong: `show_vs_ai` và `show_learn` tính `animating` (animation còn `progress`), trong lúc đó bỏ qua `maybe_trigger_ai`, `maybe_send_grade`, `maybe_auto_hint`, `poll_chessdb`; chỉ `poll` (đọc kết quả đã tính xong) là không hoãn. Nước đối thủ trong sách ở Học đã trễ 600 ms nên vốn đã sau animation. (c) `maybe_trigger_ai`/`request_hint`/`pick_config` dùng `try_lock` (mới thêm `SharedEngine::try_lock`) thay vì `lock`: engine đang bận thì hoãn sang khung hình sau, không đóng băng bàn cờ; `request_hint` trả bool để `maybe_auto_hint` chỉ đánh dấu `hint_for_ply` khi đã gửi thật. (d) `play()` xóa cả `hint_req_id` để gợi ý cũ (cho thế cờ trước nước đi) không hiện nhầm.
- **Cổng**: fmt OK, clippy sạch, `cargo test --workspace` 175 pass / 0 fail. Chưa kiểm bằng mắt trên máy thật — cần Hieu thử: hạ cờ bằng click và bằng kéo-thả ở cả 2 màn, quân phải trượt mượt 150 ms rồi AI/gợi ý/ChessDB mới chạy sau, không còn dừng hình.

## §15. Cải thiện giao diện lớn: localize EN/VI, nút common, âm thanh + hiệu ứng, mode Tự luyện tập (Hieu, 2026-11-12)

- **Localize EN/VI**: `i18n` nay có 2 ngôn ngữ (Việt mặc định / English, chọn trong Cài đặt, lưu lại, áp dụng ngay qua `set_language` dùng static atomic — call site `t(key)` không đổi). Thêm 39+ key Practice. Ký hiệu nước đi đổi từ Trung Quốc sang Việt/Anh (`NotationChoice::{Vietnamese, English}`, `render_en` = WXF `C2=5`); chữ Trung CHỈ còn trên quân cờ trên bàn cờ. Bỏ hiển thị tên bài tiếng Trung (`name_zh`), mục "Nguồn" và "%" ở màn Học/chọn bài (xóa key `SourcesTitle`, `BestScore`). Gợi ý màn Học render tên nước theo ký hiệu đã chọn, dùng `hint_pos` (vị trí lúc sinh gợi ý) để không panic khi thế cờ đã đổi.
- **Nút common + bố cục** (`widgets.rs` mới): `hero_button`, `primary_button`, `secondary_button`, `big_button`, `icon_button`, `big_radio` — style thống nhất (chữ đậm 20 px, bo góc 10 px, hover/pressed rõ), thay hết nút ad-hoc. Menu: Cài đặt + Giới thiệu căn giữa thật (`horizontal_centered`). Chống flexing: panel bên cố định `SIDE_PANEL_WIDTH = 380.0` cả 2 màn, nội dung dài cuộn trong `ScrollArea`. Cài đặt chia section có tiêu đề + separator, slider hiện giá trị. Màn Học chia 6 section; ChessDB ở Học bỏ tỉ lệ thắng/hòa/thua (chỉ chip điểm + nước tốt nhất), Đấu với AI giữ nguyên.
- **Âm thanh + hiệu ứng** (`sound.rs` mới, rodio 0.21, ALSA có sẵn trên VM): `SoundPlayer` tổng hợp tông bằng code (không cần file nhạc), phát nền không chặn UI, im lặng khi không có thiết bị âm thanh. 4 tiếng: đi thường / ăn quân / chiếu / chiếu hết. `SharedSound` (`Arc<Mutex<SoundPlayer>>`) + `sound_for_move` (quyết định từ thế cờ trước/sau: chiếu hết > chiếu > ăn > đi thường); gọi trong `play()` của Đấu với AI, Học và Tự luyện tập; toggle `sound` trong Cài đặt đồng bộ ngay. Hiệu ứng hình ảnh (`board_view.rs`): halo chiếu nhấp nháy (pulse theo thời gian), chiếu hết halo đỏ đậm nhấp nháy mạnh, hiệu ứng ăn quân tại ô vừa ăn, highlight ô đi/đến rõ hơn.
- **Mode Tự luyện tập** (`screens/practice.rs` mới, 1.389 dòng): chọn độ khó (quy định cho Pikafish, cùng thang 0–4), bắt đầu mới / nhập FEN (validate, báo lỗi) / upload ảnh + bộ soạn thế cờ thủ công (nhận diện ảnh tự động KHÔNG làm — cần mô hình thị giác riêng; ảnh hiển thị thật nhờ `egui_extras` + `install_image_loaders`). Người chơi tự đi cả Đỏ lẫn Đen; mỗi nước được ChessDB chấm + gợi ý top 4, fallback Pikafish `Analyze` khi ChessDB không có dữ liệu; chấm nước bằng `GradeMove` async kể cả nước ngoài gợi ý; gợi ý/chấm lại từ thế cờ mới. Nút Reset về thế cờ ban đầu (giữ độ khó). Nối vào app: `Screen::Practice`, nút menu "Tự luyện tập", `suggest_req_id`/`grade_req_id` vào mảng `wanted` (không là response bị drop). Reset có ở cả 3 mode: Đấu với AI ("Ván mới"), Học ("Đi lại"), Tự luyện tập (Reset).
- **Cổng**: fmt OK, clippy sạch, `cargo test --workspace` 178 pass / 0 fail (175 cũ + 3 test sound mới). Chưa kiểm bằng mắt trên máy thật — cần Hieu thử: đổi ngôn ngữ sang English, nghe tiếng đi/ăn/chiếu, xem halo chiếu nhấp nháy, vào mode Tự luyện tập thử FEN và upload ảnh.

## §16. Tải ảnh từ thiết bị + nhận diện báo lỗi + theme cờ tướng (Hieu, 2026-11-12)

- **Tải ảnh từ thiết bị** (`rfd` 0.15, native file dialog): mode Tự luyện tập nay có nút "Tải ảnh từ thiết bị" — bấm là mở hộp thoại chọn file của hệ điều hành (lọc png/jpg/jpeg/bmp/webp), không cần gõ đường dẫn nữa. Ảnh vẫn hiển thị thật qua `egui_extras`.
- **Nhận diện ảnh**: sau khi tải, app thử đọc thế cờ từ ảnh bằng `recognize_board_image` — hiện chưa có mô hình thị giác nên hàm này luôn báo lỗi, và UI hiện rõ "Không nhận diện được ảnh này. Hãy dùng bộ soạn thế cờ bên dưới để dựng lại thế cờ từ ảnh." (không đoán bừa một thế cờ sai). Khi có mô hình nhận diện thật, chỉ cần cài đặt lại hàm này; nếu nhận diện thành công thì `editor_from_position` đổ thế cờ vào bộ soạn để người dùng kiểm tra/sửa trước khi chơi.
- **Cài đặt + Giới thiệu căn đều với các mode**: hai nút nay là `hero_half_button` (164×56, nền đỏ) xếp cạnh nhau trong một hàng căn giữa — hai nút cộng lại đúng bằng bề rộng một nút mode (340 px), nên hàng này cùng bề rộng và cùng tâm với các nút Đấu với AI / Học chơi / Tự luyện tập phía trên.
- **Theme cờ tướng** (`theme.rs` mới): bỏ hoàn toàn màu xanh dương/xám mặc định của egui. Nền giấy gạo (`PAPER` #F5EFE0), chữ mực nâu đậm (`INK`), nút chính đỏ Trung Quốc (`RED` #A61E1E, hover #C02727, active #8B1A1A) với viền vàng (`GOLD` #C9A227), nút phụ viền nâu gỗ, hover vàng nhạt. `apply_theme` cài visuals lúc khởi động (`XiangqiApp::new`); `widgets.rs` dùng chung hằng số theme nên nút và panel không lệch nhau. Trên bàn cờ: màu chọn/nước vừa đi và mũi tên gợi ý đổi từ xanh lá/xanh dương sang vàng (cùng họ với viền vàng); halo chiếu/chiếu hết vẫn đỏ.
- **Cổng**: fmt OK, clippy sạch, `cargo test --workspace` 178 pass / 0 fail. Chưa kiểm bằng mắt trên máy thật — cần Hieu thử: bấm "Tải ảnh từ thiết bị" chọn một ảnh bàn cờ (phải hiện ảnh + báo không nhận diện được + soạn thế cờ tay), xem menu (Cài đặt/Giới thiệu căn đều với các mode), và đánh giá màu đỏ/vàng/giấy gạo mới.

## §13. 12 bài khai cuộc cho bên hậu thủ (Đen) — Hieu chốt làm cả 12 bài (2026-11-12)

- **12 bài, learner black hết** (sách nay 38 bài: 26 Đỏ + 12 Đen, 310.822 nút, 18.537 lá, mọi nút có score_cp, mọi bài max 40 nửa nước; openings.json 40,8 MB):
  - **8 bài đối xứng** (cùng prefix 12 nước và cùng cây sách với bài Đỏ, chỉ đổi learner sang black): b-pingfeng-d42, b-pingfeng-c04, b-pingfeng-tot7-c45, b-pingfeng-tot7-c23 (Đen chỉ thắng 14,1% — bài nhấn phòng thủ), b-pingfeng-c92, b-fangong, b-xianren-feixiang-d29, b-paodau-pao-c14.
  - **4 bài mới** sinh bằng gen-book từ prefix riêng: b-fangong-wuqi-b55 (3.958 nút/199 lá), b-xianren-feixiang-xe9-e25 (4.177/203), b-feixiang-a28 (3.967/199), b-qima-a45 (3.475/180). Tổng 4 bài: 15.577 nút, 781 lá, 2.384 lần phân tích, 178,5 s.
- **Cách sinh**: 4 bài mới chạy gen-book riêng từ file src tạm (`data/lessons-src-black-new.json`, không trong repo) ra `data/openings-black-new.json`. 8 bài đối xứng KHÔNG chạy lại gen-book (prefix giống hệt bài Đỏ nên cây Pikafish giống hệt): copy cây từ bài Đỏ trong openings.json rồi sửa note theo lượt người học mới — duyệt cây theo depth (gốc depth 0; Đỏ đi khi depth chẵn, Đen khi depth lẻ): nút tới lượt Đen đặt note "Nước mở đầu của hệ khai cuộc." nếu depth < 12 (prefix), còn không thì "Hướng theo Pikafish"; nút tới lượt Đỏ đặt note None. Giữ nguyên score_cp, weight, main, trap, end (end verdict tính theo góc Đỏ nên không đổi theo learner). Script `data/merge_black.py` làm việc gộp + kiểm (38 bài, 26 cây Đỏ giống hệt trước/sau).
- **Font**: tên bài Đen dùng 2 chữ Hán mới — 出 (bài E25: 飞相对仙人指路出车) và 跪 (bài A45: 跪马对中炮). `tools/cjk_chars.txt` nay 57 chữ (was 55); chạy lại `tools/subset-font.sh`, kiểm bằng fontTools đủ 57 glyph ở cả 2 font CJK subset.
- **Test**: `EXPECTED_IDS` 26→38, `book_parses` 26→38, `twenty_six_lessons` → `thirty_eight_lessons` (26 đỏ + 12 đen). `cargo test --workspace` 175 pass / 0 fail; book-check pass; fmt OK, clippy sạch.
- Chưa kiểm bằng mắt: cần Hieu mở màn Học xem 12 bài Đen có hiện đủ không, vào 1 bài Đen thử đi nước đầu (đối thủ Đỏ đi trước, người học cầm Đen đi sau) và kiểm nước gợi ý + chấm điểm.

## Cổng sau §1–§5

- `cargo fmt` OK, `cargo clippy --workspace --all-targets -D warnings`
  sạch, `cargo test --workspace` 150 pass / 0 fail / 26 ignored (gồm
  2 test mới cho `direction_count` + 3 test cho ô điểm lợi thế).
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy
  được dưới Xvfb — cần Hieu kiểm trên máy thật.
