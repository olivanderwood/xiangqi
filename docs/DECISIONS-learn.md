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
  Phi tượng, Mã quỷ (`r-paodau-phituong-maquy`) theo lệnh Hieu — xem
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

## §7. Pháo đầu phá Phi tượng, Mã quỷ + biến đối thủ soạn sâu + sửa số mũi tên chồng nhau (Hieu, 2026-11-11)

### 7.1. Thay bài Xuyên cung mã bằng Pháo đầu phá Phi tượng, Mã quỷ

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
- Tên gọi tiếng Việt: các bài giảng trên Facebook dùng cụm
  "PHI TƯỢNG MÃ QUỶ XUYÊN CUNG MÃ" (Pháo đầu tam binh phá trận Phi
  Tượng Mã Quỷ) — "Mã Quỷ" là con Mã lén qua cung như bóng ma.
- Cách phá (Ziga, "Pháo đầu phá phi tượng cục"): Đỏ dùng Pháo đầu +
  Xe hoành (横车) ép trung lộ; Đen thủ bằng Sĩ Tượng cục, một Mã giữ
  Tốt đầu, một Mã ở biên. Đỏ phá bằng cách lên Xe, thí quân gài thế.

**Giả định về "mã quỳ" — cần Hieu xác nhận.** Hieu viết "mã quỳ",
nhưng tra cứu không thấy thuật ngữ 跪马 (Mã quỳ) trong khai cuộc;
thuật ngữ có thật trong các bài giảng tiếng Việt là **"Mã Quỷ"**
(鬼, bóng ma) đi cùng Xuyên Cung Mã — "quỳ" và "quỷ" cùng vần,
khác dấu, rất dễ nhầm khi gõ/nghe. Em dùng **"Mã quỷ"** cho bài
mới; nếu anh thật sự muốn "quỳ" theo nghĩa khác thì bảo em sửa lại.

**Bài mới.** `r-paodau-phituong-maquy` (Đỏ, cấp 2), thay đúng chỗ
của `r-paodau-xcgma` (sau `r-jingoupao`), tên Hán
"炮头破飞象穿宫马" (tên Hán dùng thuật ngữ chuẩn 穿宫马, tên Việt
giữ cách gọi của Hieu: Mã quỷ). Prefix 6 nửa nước (đã kiểm bằng
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
  r-feixiang, còn 鬼 không cần vì "Mã quỷ" viết bằng chữ Việt;
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

## Cổng sau §1–§5

- `cargo fmt` OK, `cargo clippy --workspace --all-targets -D warnings`
  sạch, `cargo test --workspace` 150 pass / 0 fail / 26 ignored (gồm
  2 test mới cho `direction_count` + 3 test cho ô điểm lợi thế).
- Chưa kiểm bằng mắt: VM thiếu `libxkbcommon-x11.so` nên app không chạy
  được dưới Xvfb — cần Hieu kiểm trên máy thật.
