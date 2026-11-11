# DECISIONS — M4: Nội dung sách khai cuộc + Tutor (T5.1)

Ghi chú lệch so với đặc tả, quyết định nội dung, mức xác minh nguồn và các
vấn đề phát hiện khi làm M4. File này chỉ thuộc về M4; không sửa
`docs/DECISIONS.md` của coordinator.

## 1. Nội dung sách: 12 bài (406 nút)

| Bài | Nhánh (độ dài nửa nước) | Nguồn đã đối chiếu |
|---|---|---|
| `r-zhongpao-pingfeng` | 2 (15, 21) | Sohu, Bilibili |
| `r-shunpao-hengche` | 1 (25) | xiangqi.com, Paper Generals, *教你学象棋* |
| `r-vs-yuanyang` | 4 (18, 23, 27, 30) | NetEase, xiangqiqipu, *教你学象棋*, Wikibooks |
| `r-feixiang` | 3 (12, 12, 17) | xqarena, Wikibooks, *教你学象棋* |
| `r-xianren` | 2 (9, 8) | Wikibooks, xqarena |
| `r-guogong` | 2 (12, 8) | xqarena, Wikibooks |
| `b-pingfeng` | 2 (15, 21) | Sohu, Bilibili |
| `b-yuanyang` | 4 (18, 23, 27, 30), weight 1:2:1:2 | NetEase, xiangqiqipu, *教你学象棋*, Wikibooks |
| `b-chuangong` | 2 (12, 4) | Zhihu, xiangqi.com (glossary) |
| `b-fangong` | 2 (12, 12) | xqarena, Wikipedia 反宫马 |
| `b-shunpao` | 2 (24, 8) | xqarena, *教你学象棋* |
| `b-zudipao` | 2 (9, 16) | Wikibooks, xqarena, Scribd *中局* |

Mọi bài đều có ≥2 nhánh (trừ `r-shunpao-hengche` chỉ có 1 nhánh bẫy duy
nhất — đúng theo đặc tả §5.4) và ≥8 nửa nước; mọi nước của người học đều
có `note_vi` tự viết, ≤160 ký tự.

Ghi chú: sách được sinh bằng script Python chạy một lần (`/tmp/gen_book.py`,
đã mất khi `/tmp` bị xóa giữa chừng); file
`crates/xq-book/data/openings.json` là bản chính từ đây về sau — sửa sách
thì sửa trực tiếp file JSON rồi chạy lại `tests/validate_book.rs`.

## 2. Mức xác minh nguồn

- **Xác minh trực tiếp (mở được trang):** Sohu (Bình phong mã), Bilibili
  (ngũ cửu pháo), xiangqi.com + Paper Generals + *教你学象棋* (bẫy Quất
  trung bí — hai nguồn độc lập khớp toàn bộ 25 nửa nước), xqarena (Phi
  tượng, Quá cung pháo, Phản cung mã, Thuận pháo, Tiên nhân chỉ lộ),
  Wikibooks (khai cuộc), xiangqiqipu (Uyên ương pháo, biến thể khác),
  Wikipedia (Phản cung mã), Scribd *中局* (Tốt để pháo, qua snippet).
- **Không mở trực tiếp được:** NetEase (Uyên ương pháo) — trang lỗi khi
  fetch; Zhihu (Xuyên cung mã) — trang chặn truy cập tự động (đặc tả đã
  cảnh báo). Với hai nguồn này, chuỗi nước dùng theo đặc tả §5.4 và đối
  chiếu chéo với các nguồn phụ nêu ở bảng trên (đặc biệt với Uyên ương
  pháo: định thức chung và 4 hướng Giáp/Ất/Bính/Đinh khớp với
  xiangqiqipu, Wikibooks và *教你学象棋*). Mức xác minh của hai bài này
  là **trung bình**, không phải **cao** như các bài còn lại.
- Các ô "…" trong nguồn Uyên ương pháo (nước Đen không ghi lại) đã được
  điền bằng các nguồn phụ trên; không còn ô "…" nào trong sách.

## 3. Nước đã cắt / không đưa vào

- **`r-xianren`, nhánh C (24 nửa nước, mirror của "挺兵局" trong
  *教你学象棋*): CẮT.** Bản mirror được tính với quy ước cột sai (xem
  mục 4), tạo thành một biến lai không đúng với sách gốc, và không đối
  chiếu lại được với văn bản gốc trong lúc làm. Bài còn 2 nhánh (9 và 8
  nửa nước), vẫn đủ yêu cầu T4.4 (≥2 nhánh × ≥8 nửa nước). Nguồn
  *教你学象棋* đã gỡ khỏi `sources` của bài này.
- **`b-chuangong`: không thêm được nhánh phụ** cho Đỏ đi khác ở nước 4
  hoặc 6 như đặc tả yêu cầu ("bản 1.0 cần thêm ít nhất 1 nhánh"). Đã tra
  cứu web (Zhihu, xiangqi.com glossary, định nghĩa xuyên cung mã) nhưng
  không tìm được nhánh chắc chắn đã xác minh; theo quy tắc "không bịa",
  chỉ giữ biến chính 12 nửa nước + biến đối xứng 4 nửa nước, end
  verdict equal. Cần coordinator quyết: giữ như vậy hay giao tra cứu
  sâu thêm từ sách *Quất trung bí* / Lưu Điện Trung.
- **`r-shunpao-hengche`: không cắt.** Đặc tả dặn nếu không lấy được tiếp
  sau `4. R1=6 R8+6` thì cắt tại đó; thực tế lấy được tiếp diễn đầy đủ
  (tới chiếu hết song Pháo, 25 nửa nước) từ Paper Generals và
  *教你学象棋*, khớp nhau.

## 4. Quy ước ký hiệu phát hiện khi kiểm chứng (đúng theo `xq-core`)

- Đỏ: số cột = 9 − file; Đen: số cột = file + 1 (tính từ phía người
  cầm quân đó). Hậu tố tiến/lùi của Đen dùng **chữ số Ả Rập**
  (`车8进6`, `炮2进7`), của Đỏ dùng chữ Hán. Sĩ Đỏ = `仕`, Sĩ Đen = `士`.
- Ba lỗi đánh máy theo quy ước này đã sửa trước khi test pass:
  `车8进六` → `车8进6`, `炮2进七` → `炮2进7`,
  `士六进五` (nước của Đỏ) → `仕六进五` (nằm trong nhánh đã cắt ở mục 3).

## 5. Lệch / mở rộng so với đặc tả (Tutor, T5.1)

- **Nhánh xấu vẫn chấm `Acceptable`, không chấm `Book`:** mở rộng quy tắc
  3 của §5.3. Nước đi vào nhánh mà mọi lá dưới nó đều có verdict bất lợi
  cho người học (trường hợp nhánh Giáp của `r-vs-yuanyang`, lá
  `black_better` với learner red) được chấm `Grade::Acceptable` kèm giải
  thích, thay vì `Book`. Mục đích: không khen một nước đi đúng sách
  nhưng dẫn vào thế xấu. Áp dụng chung cho mọi bài, không riêng Giáp.
- **API giữ nguyên, thêm 2 phương thức:** `suggestions() ->
  Vec<BookHint>` (nút con, main đầu, + tối đa 2 nước engine không trong
  sách) và `hints_for_learner_turn() -> TutorEvent` (phát `YourTurn` để
  UI gọi khi tới lượt người học). Không đổi chữ ký các phương thức cũ.
- **Luồng hoàn thành:** `on_user_move` trả `OpeningComplete` khi nước
  của người học kết thúc bài; `next_opponent_move` luôn trả
  `OpponentMoves` (kể cả khi nước đó tới lá) — UI kiểm tra `is_complete()`
  sau đó và gọi `summary()`. `is_complete()` đúng khi: tới lá có `end`,
  hoặc đủ `max_plies`, hoặc ra khỏi sách thêm 8 nửa nước, hoặc ván cờ
  đã kết thúc.
- **Độ sâu engine của Tutor (đặc tả không ghi):** chấm nước ngoài sách
  depth 6, chấm gợi ý depth 4 — chọn để UI phản hồi nhanh; `book-check`
  và `engine_sanity` vẫn dùng depth 8 theo đặc tả.
- **`retry()`:** chỉ hoàn tác được về trước nước ngoài sách **đầu tiên**
  (snapshot chụp một lần); đi sai lần 2 sau khi đã retry thì snapshot
  mới được chụp lại từ điểm đó.
- **Searcher lazy:** `Tutor` chỉ tạo `Searcher` (TT 16 MB) khi lần đầu
  cần chấm/gợi ý, vì test weights tạo 1000 `Tutor`.

## 6. Font subset (T4.1 test 9)

Test `fonts_cover_book` kiểm tra thật (không skip) vì font đã có. Subset
ban đầu do agent giao diện tạo từ sách mẫu cũ nên thiếu 20 ký tự
(`人仙你反学宫局底指教棋横直破穿路顺飞鸯鸳`, chủ yếu từ `name_zh` và
tiêu đề nguồn); agent giao diện đã chạy lại `tools/subset-font.sh`
(lúc 16:41 ngày 2026-11-10) sau khi sách mới được ghi, và test hiện pass.
Nếu sách đổi thêm ký tự CJK mới thì chạy lại script đó trước khi test.

## 7. BLOCKER: engine `xq-core` (search) cho điểm sai — ảnh hưởng T4.2, T5.1, `engine_sanity`

**Hiện tượng (đo được, có thể tái hiện):** tại vị trí sau
`1. 炮二平五 马2进3 2. 马八进七 车1平2`
(FEN `1rbakabnr/9/1cn4c1/p1p1p1p1p/9/9/P1P1P1P1P/1CN1C4/9/R1BAKABNR w - - 4 1`,
static eval +20 cho Đỏ, tức thế cân bằng):

- `Searcher::search` depth 4: nước tốt nhất `车九平八`, score **+377**.
- `Searcher::analyze` depth 4, top 3: `车九平八` / `炮五进四` /
  `车九进二`, **cả ba đều đúng +377**.
- `Searcher::search` depth 8: tốt nhất `炮五平四`, score **+367**;
  difficulty Master cũng +371. Không phải do noise (±20) hay random pick.
- `Searcher::score_move` cùng nước `车九平八`, cùng depth 4: **−594**;
  depth 8: −2. Nghĩa là `search` và `score_move` chênh nhau **971**
  trên cùng một nước, cùng một depth, cùng một Searcher mới tạo (không
  phải do TT dùng chung — đã kiểm bằng Searcher mới cho mỗi lần gọi).
- Kiểm chứng độc lập: sau `车九平八`, search từ phía Đen cho Đen +599,
  trong khi `score_move` và static eval đều nói thế cờ chỉ khoảng cân
  bằng. (Lưu ý: `车九平八` quả thật là nước dở — Xe ở b0 bị Pháo Đen
  `炮2进7` ăn nhờ Pháo Đỏ ở b2 làm ngòi — nhưng mức độ thì `score_move`
  mới phản ánh đúng, còn `search` vừa chọn chính nước dở đó làm "tốt
  nhất" vừa thổi phồng điểm.)

**Kết luận:** đường search gốc của engine (`search_root`/`run_id`,
dùng chung cho `search` và `analyze`) đang trả điểm thổi phồng/sai
trong trạng thái `xq-core` hiện tại (file `search.rs` đang được agent
engine sửa dở — có lúc còn không biên dịch được, lỗi E0382 lúc ~16:35
ngày 2026-11-10); chỉ có `score_move` (negamax trực tiếp) là đáng tin.
Đây là lỗi của `xq-core` (T2), **ngoài phạm vi M4, không sửa ở đây**.

**Ảnh hưởng tới M4 (đã đo, không bịa):**

- `engine_sanity` (§8.7 mục 10, depth 8) trên sách thật: **PASS** — 202 nước
  người học, 0 vi phạm (xem mục 8). Lỗi thổi phồng điểm ở trên chỉ tái
  hiện ở vị trí probe (fixture nhánh B cũ, đã sửa) và **không** ảnh
  hưởng tới sách thật — nhưng vì `search` đã chứng minh là có thể sai
  tới ~970 điểm ở một vị trí cụ thể, kết quả `engine_sanity` nên được
  đọc cùng với lưu ý này, và agent engine vẫn nên xem lại
  `search_root`/`run_id` (đặc biệt là hiện tượng nhiều nước gốc khác
  nhau cùng trả đúng một điểm số 377 ở depth 4).
- `tools/book-check --check-engine`: dùng đúng công thức đặc tả
  (`search` best − `score_move`) và cũng cho exit 0 trên sách thật
  (xem mục 8).
- Tutor chấm nước ngoài sách (depth 6, cùng công thức) có thể chấm gắt
  hơn thực tế ở những vị trí kích hoạt lỗi engine; nước `main` làm `best`
  thì không bị ảnh hưởng. Test Tutor không phụ thuộc vào độ chính xác của
  engine nên vẫn pass.

## 8. Kết quả `engine_sanity` (release, depth 8, `#[ignore]`) và `book-check`

- `cargo test --release -p xq-book --test validate_book engine_sanity
  -- --ignored`: **PASS** — đã chấm **202 nước** của người học (depth 8),
  **0 vi phạm** (delta > 150, không tính trap). Chỉ 2 nước có delta > 60:
  `r-feixiang` nước `车九平八` delta=72 và `b-fangong` nước `卒7进1`
  delta=80 (đều "Chấp nhận được"). Thời gian chạy 187 giây (chủ yếu là
  search depth 8).
- `cargo run --release -p book-check -- --check-engine --depth 8`:
  **exit 0, 0 vi phạm**; số nước đã chấm 202, miễn trap 2, delta lớn
  nhất 76–81 tùy lần chạy (`b-fangong` `卒7进1` — `engine_sanity` đo
  80, hai lần chạy `book-check` đo 81 rồi 76; dao động nhỏ giữa các
  lần chạy, đều xa ngưỡng 150). Thời gian chạy ~3,5 phút.
- Số nước trap được miễn: 2 (`r-shunpao-hengche`: `车九进一`,
  `前车进一`).
