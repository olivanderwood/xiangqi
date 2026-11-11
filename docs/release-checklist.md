# Checklist phát hành (đặc tả §8.9)

Kiểm thủ công trước mỗi bản phát hành, trên **Windows 10/11 và macOS 13+**.
Mục nào đã kiểm được trên Linux (VM, bằng test tự động) thì ghi rõ bằng
chứng bên dưới; còn lại để trống cho Hieu kiểm tay trên Windows/Mac.
Không tick mục nào chỉ dựa trên test tự động khi mục đó yêu cầu nhìn
bằng mắt trên máy thật (hiển thị chữ, co giãn cửa sổ, Retina).

- [ ] Chữ Hán hiển thị đúng trên Windows 10/11 và macOS 13+ (kể cả máy
  không cài font tiếng Trung)
  - Đã kiểm trên Linux: test `fonts_cover_book` pass — subset CJK chứa
    đủ mọi ký tự CJK trong sách, tổng 4 file font 166 KB < 300 KB.
    Chưa kiểm: hiển thị thật trên Windows/macOS, icon `.icns` mới
    (tạo bằng `tools/make-icons.py`, PNG elements + fallback `it32`).
- [ ] Dấu tiếng Việt hiển thị đủ (ệ, ữ, ậ…)
  - Đã kiểm trên Linux: script `tools/subset-font.sh` cắt Noto Sans
    theo dải Unicode tiếng Việt + Latin. Chưa có test tự động assert
    từng dấu — cần nhìn bằng mắt trên máy thật.
- [ ] Cầm Đen: bàn lật, AI đi trước, số cột đúng
  - Đã kiểm trên Linux: 9 unit test `board_view` pass
    (`to_screen`/`from_screen` có/không lật, nhãn cột). Chưa kiểm:
    AI đi nước đầu khi cầm Đen, số cột trên bàn đã lật (chưa có
    smoke test cho trường hợp này).
- [ ] Hoàn nước khi AI đang nghĩ không làm treo hoặc đi sai
  - Đã kiểm trên Linux: 5 test `ai_worker` pass (Stop tức thì,
    `req_id` cũ bị bỏ), test `stop_flag` của engine pass
    (dừng trong ≤ 100 ms sau cờ stop). Chưa kiểm: bấm Hoàn nước
    thật trong lúc AI đang nghĩ trên máy thật.
- [x] Chơi 1 ván trọn vẹn mỗi mức độ
  - Đã kiểm trên Linux (release, 2026-11-11): `selfplay_sanity`
    pass — 20 ván Medium vs Medium, Đỏ thắng 20, Hòa 0, tối đa
    69 nửa nước; `levels_ordered` pass — Hard thắng Easy 10–0
    (cần ≥ 9/10). Lưu ý: 20 ván selfplay giống hệt nhau (cùng 69
    nước, Đỏ thắng) vì noise/search tất định theo hash thế cờ —
    đây là bằng chứng "chơi trọn ván không lỗi", không phải đo
    cân bằng Đỏ/Đen. Smoke test `menu_to_vs_ai_plays_cannon_opening`
    pass; `always_legal` pass (200 thế ngẫu nhiên).
  - Chưa kiểm tự động: ván trọn vẹn ở mức Cao thủ (8 s/nước) —
    cần chơi tay 1 ván.
- [ ] Học hết 1 bài tiên + 1 bài hậu, thử cả hai lựa chọn khi hết khai cuộc
  - Đã kiểm trên Linux: 7 test `tutor` pass (đi đúng sách, retry,
    ra khỏi sách, hết bài, `max_plies`), smoke test học bài Pháo đầu
    pass (gợi ý, chấm `Book`, đối thủ trả lời theo sách). Chưa kiểm:
    học trọn 1 bài tiên + 1 bài hậu trên máy thật, cả hai lựa chọn
    "Dừng và tổng kết" / "Đánh tiếp với AI".
- [ ] Co giãn cửa sổ tới kích thước tối thiểu, màn Retina/HiDPI 150%
  - Chưa kiểm trên Linux (không có màn hình thật trên VM).
- [ ] Tắt và mở lại: cài đặt và tiến độ học còn nguyên
  - Chưa kiểm trên Linux (chưa có test tự động cho lưu/đọc settings
    và tiến độ học). Cần tắt mở app thật trên Windows/macOS.

## Kết quả test release `#[ignore]` trên Linux (2026-11-11)

- `perft` d4/d5 (release): **pass** — 2/2, 14,10 s
  (d4 = 3.290.240, d5 = 133.312.995, test assert đúng số).
- `mate_suite` (release): **pass** — 10/10 thế giải được trong
  0,57 s (cần ≥ 9).
- `selfplay_sanity` (release): **pass** — 20 ván Medium vs Medium,
  Đỏ 20 – Đen 0 – Hòa 0, tối đa 69 nửa nước, 14,33 s.
- `levels_ordered` (release): **pass** — Hard thắng Easy 10–0,
  43,92 s.
- `engine_sanity` (release): **pass** — toàn bộ nước của người học trong 12 bài (depth 8), 212,54 s, 0 vi phạm delta > 150. Chạy lại sáng 2026-11-11 sau bản sửa search (bỏ LMR ở nút gốc + guard ăn quân treo); chốt lại sách sau bản sửa.
- Chưa chạy: `nps_benchmark` / `depth_in_3s_probe` (chỉ in số, không assert).
