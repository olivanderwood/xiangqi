# Quyết định lệch đặc tả — tích hợp Pikafish

Đặc tả gốc: `~/workspace/user/files/pikafish-integration.md` (v1.0).
File này ghi mọi chỗ code lệch khỏi đặc tả, kèm lý do và bằng chứng.
Cập nhật theo từng task P1–P9; KHÔNG tự commit/push.

## 1. GradeThresholds đặt ở `xq-core::analysis` (P1/P2)

Đặc tả không chỉ rõ đặt ở đâu. `GradeThresholds {best, good, acceptable,
inaccurate}` thêm vào `xq-core::analysis` với `BUILTIN = {20, 60, 150, 300}`
(giữ nguyên ngưỡng cũ trong `grade_from_delta`) và
`PIKAFISH = {15, 50, 120, 250}`; `grade_from_delta_with(delta, in_book,
thresholds)` mới, `grade_from_delta` cũ delegate sang BUILTIN.

## 2. Một engine chia sẻ, mutex tuần tự hoá (P2)

Đặc tả §0 điều kiện 1 đòi mọi chức năng AI (kể cả chấm nước) chạy bằng
Pikafish khi khả dụng, mà chỉ được MỘT tiến trình Pikafish. Vì vậy dùng
`xq_engine::SharedEngine = Arc<Mutex<Box<dyn Engine>>>` — MỘT engine
chia sẻ giữa worker thread và các đường chấm điểm đồng bộ trên UI thread.
`xq-book::Tutor` nhận `SharedEngine` thay vì `Searcher`.

## 3. `AiRequest` đổi payload sang `EngineRequest` (P2)

Bắt buộc vì Pikafish cần `start_fen` + toàn bộ nước (`position fen ...
moves ...`), không chỉ vị trí hiện tại + lịch sử hash như Searcher cũ.

## 4. Worker không ngắt được search đang chạy từ hàng đợi (P2/P4)

§1.3 mô tả ngắt search đang chạy khi có request mới, nhưng worker thread
bị chặn trong lời gọi engine nên không ngắt được từ hàng đợi. Thay vào đó:
sau mỗi request, rút hết request đang chờ và chỉ giữ request MỚI NHẤT
(`Stop` trong hàng đợi vẫn áp dụng ngay); UI tự loại response cũ theo
`req_id`. Chỉ có cờ stop mới ngắt được search đang chạy.

## 5. Ví dụ §9.4: delta 250 là Inaccurate, không phải Blunder (P2)

Test `tutor_with_mock_engine_grades` dùng Cp(-250) → delta 300 → Blunder
theo ngưỡng PIKAFISH. Ví dụ đặc tả dùng Cp(-200) → delta 250 = đúng biên
Inaccurate (121–250).

## 6. `set_level` chỉ lưu level ở P4, áp option UCI ở P6 (P4)

`PikafishEngine::set_level` hiện chỉ lưu `level`; việc áp option UCI
thật dời sang P6 (cùng bảng mức §4).

## 7. Hash/RAM thật dời sang P7 (P3)

`hash_mb_for_total_ram_gb()` có bảng §3, nhưng việc đọc RAM thật (crate
sysinfo) để dành P7; hiện mặc định 256 MB.

## 8. Test worker-fallback dùng MockEngine, không dùng fake-uci (P4)

`xq-app` là bin crate, không import được từ integration test; đường dẫn
`target/debug` của fake-uci quá mong manh. 4 test fallback của worker
(`worker_falls_back_to_builtin_on_crash`,
`illegal_move_uses_one_off_builtin`, `illegal_move_swaps_after_three`,
`stale_request_dropped`) viết dạng unit test trong `ai_worker.rs` với
MockEngine. Hành vi của fake-uci vẫn được phủ bởi
`xq-pikafish/tests/fake_engine.rs` (11 test).

## 9. fake-uci nhận cấu hình qua tham số dòng lệnh, không qua env (P4)

Ban đầu fake-uci đọc kịch bản/log/state qua biến môi trường
`FAKE_UCI_*`. Khi test chạy song song, engine KHỞI CHẠY LẠI (restart sau
crash/timeout) đọc env tại thời điểm restart — lúc đó một test khác có thể
đã đổi env — nên engine restart chạy sai kịch bản (3 test rớt khi chạy
song song, chạy riêng đều pass). Sửa: fake-uci nhận `--script/--log/--state`
qua argv (env vẫn làm fallback khi chạy tay); `PikafishEngine` lưu
`spawn_args` và dùng lại ở mọi lần restart
(`spawn_with_timeouts_and_args`); đếm dòng log `"uci"` phải so khớp cả dòng
(`"ucinewgame"` chứa chuỗi con `"uci"`).

## 10. Release ghim: Pikafish 2026-09-06 (P5)

Nghiên cứu xác minh live trên binary thật (xem
`docs/research-pikafish-2026-09-06.md`): tag `Pikafish-2026-09-06`,
commit `4c17cee11f888ae1d48a9494f2e2239f019f0a1f`, release có ĐÚNG 1 asset
`Pikafish.2026-09-06.7z` (53.603.424 byte,
sha256 `41952bbf...e5b`), `bench` in `Nodes searched: 2651062` khớp đúng
message commit. Ghim trong `engine/PIKAFISH_VERSION.toml`.

## 11. Chỉ có binary universal, không có avx2/sse41 (P5)

Release 2026-09-06 chỉ ship binary universal cho mỗi nền tảng
(`Pikafish-Linux-x86-64-universal`,
`Pikafish-Windows-x86-64-universal.exe`, `Pikafish-MacOS-universal`).
`locate::exe_name()` sửa theo tên này, bỏ chọn avx2/sse41 bằng
`is_x86_feature_detected!`; `from_dir_or_exe` giữ fallback tên thường
`pikafish`/`pikafish.exe` cho dev. Nếu release sau đổi layout, sửa P8.

## 12. KHÔNG có NNUE tải riêng, KHÔNG có hash chính thức (P5)

`pikafish.nnue` (50.706.378 byte) nằm TRONG archive release, không có link
tải riêng và không có sha256 công bố (đã kiểm release notes, repo
`official-pikafish/Networks`, wiki). `nnue_sha256` trong
`PIKAFISH_VERSION.toml` là giá trị tự tính từ file giải nén
(`7d13d735...900e`) — chỉ dùng kiểm toàn vẹn nội bộ, không phải hash
upstream. `fetch-engine.sh` (P8) phải tải archive + kiểm hash archive,
không tải NNUE riêng.

## 13. Binary thật chỉ có 11 option UCI — KHÔNG có UCI_Elo/Skill Level/Rule60 (P5)

Đây là lệch lớn nhất so với đặc tả. `uci` của binary thật chỉ liệt kê:
Debug Log File, NumaPolicy, Threads, Hash, Clear Hash, Ponder, MultiPV,
Move Overhead, nodestime, UCI_ShowWDL, EvalFile. KHÔNG có `Skill Level`,
`UCI_LimitStrength`, `UCI_Elo`, `Repetition Rule`, `Sixty Move Rule`,
`Rule60MaxPly` (gửi `setoption` trả `No such option: …`; đã đối chiếu
`src/ucioption.cpp` + `src/engine.cpp` ở commit ghim). Wiki
pikafish.com mô tả các option này nhưng đó là build/biến thể khác —
KHÔNG dùng được với release ghim.

Hệ quả cho P6 (ghi trước để P6 làm theo):
- Mức khó KHÔNG đặt được qua `UCI_Elo`/`Skill Level`; phải giới hạn sức
  mạnh bằng giới hạn `go` do app tự đặt (depth/movetime) — và/hoặc
  MultiPV + chọn ngẫu nhiên.
- Bảng mức §4 của đặc tả phải viết lại theo hướng này (Elo chỉ còn là
  nhãn hiển thị, hiệu chỉnh bằng đo thực tế).
- `Rule60MaxPly`/`Repetition Rule`/`Sixty Move Rule` sẽ luôn nằm trong
  `missing_options()` khi nói chuyện với Pikafish thật — đó là hành vi
  đúng theo thiết kế "chỉ gửi option có trong danh sách", không phải lỗi.
- Các option DÙNG ĐƯỢC: EvalFile, Threads, Hash, MultiPV, Ponder.

## 14. `searchmoves` ĐƯỢC hỗ trợ (P5)

Đã xác minh live: `go depth 6 searchmoves a6a5` → `bestmove a6a5 ponder
e2e6`. `score_move` dùng đường `searchmoves` như đặc tả §2.5.

## 15. BUG luật cờ trong `xq-core`: tốt tấn công sai hướng (P5, ngoài phạm vi Pikafish)

`Position::is_attacked` (section 3, `board.rs`) chỉ kiểm tra tốt địch ở
hai ô CHÉO sau mục tiêu — kiểu tốt cờ vua phương Tây. Luật cờ tướng: tốt
tấn công các ô nó ĐI ĐƯỢC — thẳng tiến, cộng ngang sau khi qua sông —
không bao giờ chéo. Hậu quả: `legal_moves()` sinh nước không hợp lệ ở các
thế tốt chiếu thẳng/ngang vua (vd FEN `3a4r/3na4/b3nk3/...` tốt đỏ f6
chiếu vua đen f7, code cũ liệt kê 30 nước, Pikafish chỉ chấp nhận f7f8).

Đã sửa: kiểm tra tốt thẳng trước mục tiêu + tốt ngang sau khi qua sông;
bỏ kiểm tra chéo. `gen_piece_moves` (sinh nước tốt) vốn đã đúng, và
`attacks_square` trong `analysis.rs` cũng đã đúng — chỉ `is_attacked` sai.
Thêm test hồi quy `pawn_attacks_forward_not_diagonal` vào
`crates/xq-core/tests/rules.rs` (FEN trên → `in_check` đúng + chỉ còn
đúng 1 nước f7f8; cộng các trường hợp tiến/ngang/chéo/chưa qua sông).
Toàn bộ cổng vẫn xanh sau sửa (perft không đổi: 44/1920/79666/...).

## 16. `mate_in_one` (§9.3): 3 nước thắng trong 1, không chỉ b0b9 (P5)

Ở FEN `3k5/R8/9/9/9/9/9/9/9/1R2K4 w - - 0 1` có 3 nước thắng trong 1:
b0b9, b0d0 (chiếu hết) và b0a0 (bí — hết nước đi, cờ tướng tính là thua).
Pikafish chọn b0a0 với `score mate 1` (đúng theo luật: bí cũng là thắng).
Test ghim điểm `Mate(1)` + sau nước đi Đen hết nước, không ghim một nước
cụ thể. (Đã kiểm bằng xq-core: b0b9/b0d0 chiếu hết, b0a0 bí.)

## 17. Bestmove sau lệnh stop = `Err(Stopped)`, không phải kết quả (P5)

`wait_bestmove_until` trước đây trả `Ok(bestmove)` cả khi bestmove đến
sau khi người dùng đã yêu cầu stop. Sửa: nếu đã gửi `stop` thì bestmove
đến sau đó trả `Err(EngineError::Stopped)` — worker/UI phân biệt được
"tìm kiếm bị huỷ" với "tìm kiếm xong". Test `stop_responsive` (§9.3) ghim
hành vi này (stop sau 300 ms ở `movetime 30000` → `Stopped` trong < 3 s,
engine vẫn dùng được sau đó).

## 18. Nghiên cứu "xác minh live" lệnh `go` từng bị hiểu sai (P5)

Subagent nghiên cứu ban đầu báo `go movetime/depth` trả bestmove hợp lý
(a6a5, a3a4...) — thực ra đó là các lần trả tức thời 0 node: lệnh `quit`
được gửi ngay sau `go` qua cùng pipe nên engine đọc `quit` và dừng tìm
kiếm ngay (UCI `go` là bất đồng bộ; `bench` mới là đồng bộ nên vẫn chạy
đủ 2,65M node). Engine không hề lỗi; cách kiểm tra sai. Kiểm chứng đúng:
giữ pipe mở (sleep) giữa `go` và `quit` — engine tìm bình thường
(vd mate-in-1: depth 8, 251 node, `score mate 1`).

## 19. Kết quả test 9.3 trên Pikafish thật (P5)

Chạy `cargo test --release -p xq-pikafish -- --ignored` với
`XQ_PIKAFISH_PATH=$PWD/engine/Pikafish-Linux-x86-64-universal`: **11/11
pass** (9 nhanh ~9 s + 2 ván đấu ~32 s). `pikafish_beats_builtin` thắng
**10/10** (yêu cầu ≥ 9/10; mỗi ván cap 100 nửa nước, 100 ms/nước, hoà ở
cap tính theo hơn chất ≥ 300). `perpetual_not_self_inflicted` pass —
không có lần lặp lại thế cờ nào do Pikafish gây ra trong 1 ván cap 150
nửa nước.

## 20. Mức khó: KHÔNG dùng UCI_Elo, dùng giới hạn `go` + chọn nước yếu hơn (P6)

§4 của đặc tả không thực hiện được (xem §13): mức khó được định nghĩa
lại trong `xq-pikafish/src/levels.rs` (`pikafish_level`):

| Mức | depth | movetime | Làm yếu |
|---|---|---|---|
| Dễ | 3 | 500 ms | chọn trong top 3, biên 150 cp, nhiệt độ 50 |
| Trung bình | 6 | 1000 ms | chọn trong top 3, biên 80 cp, nhiệt độ 25 |
| Khó | 12 | 2000 ms | không (đi nước tốt nhất) |
| Cao thủ | không giới hạn | 5000 ms | không; đa dạng §5 trong 24 nửa nước đầu |
| Elo(x) | nội suy tuyến tính giữa các mốc | | nội suy theo |

Làm yếu là cần thiết vì chỉ giới hạn depth KHÔNG làm Pikafish yếu:
đo `measure_level_strength` — ngay cả depth 2 cũng thắng 4/4 trước
Builtin(Medium, 100 ms) ở ~0 ms/nước (depth 4: 1 ms, depth 8: 3 ms,
depth 12: 29 ms). Thang NNUE mạnh tới mức Pikafish nông vẫn đi gần
như đúng. Cơ chế làm yếu dùng chung hàm chọn có trọng số với đa dạng
(`pick_weighted`, `w = exp((score − best) / temperature)`); nhiệt độ
cao = hay chọn nước kém hơn. Engine cũ dùng nhiễu điểm với cùng mục
đích. `Level::Elo(x)` nội suy cả 4 số (depth, movetime, biên, nhiệt độ)
giữa các mốc 1400/1900/2400/2800; dưới 1400 = Dễ, từ 2800 = Cao thủ.

Kiểm chứng thứ bậc (`measure_level_matches`, 4 ván/cặp, cap 40 nửa
nước, hơn chất ≥ 300 tính thắng, Cao thủ chạy 1000 ms thay vì 5000 ms):
Dễ thắng 1/4 trước Trung bình; Trung bình thắng 0/4 trước Khó; Khó
thắng 0/4 trước Cao thủ. Thứ bậc rõ ràng, khoảng cách hợp lý.

Test §9.3 (`tests/levels_diversity.rs`, Pikafish thật, `#[ignore]`):
`levels_ordered` — Cao thủ thắng **10/10** trước Dễ (yêu cầu ≥ 9/10;
10 ván, cap 60 nửa nước, Cao thủ 1000 ms/nước thay vì 5000 ms);
`diversity_varies` — 10 ván seed khác nhau cho **10 chuỗi 12 nửa nước
đầu khác nhau** (yêu cầu ≥ 5); `diversity_off_deterministic` pass
(cùng seed, tắt đa dạng, `go depth 12` → cùng nước).

## 21. Đa dạng nước đi: RNG theo (seed ván, nửa nước) (P6)

§5.4 nói RNG khởi tạo từ seed của ván. Code dùng `GameRng::for_ply
(game_seed, ply)` — RNG được suy ra lại từ (seed, ply) thay vì một
luồng chung chạy xuyên suốt ván — để xem lại ván hoặc hoàn nước rồi đi
lại vẫn cho cùng nước (luồng chung sẽ lệch sau mỗi lần hoàn). Cùng seed
+ cùng thế → cùng nước; khác seed → khác nước (test
`diversity_off_deterministic` ghim `go depth 12` cho cùng nước khi tắt
đa dạng).

Đa dạng chỉ áp dụng khi: mức Cao thủ, engine là Pikafish
(`supports_diversity`, builtin luôn trả false — analyze top 3 ở
Master của builtin tốn 8 s/nước), trong 24 nửa nước đầu, không bên nào
đang bị chiếu, nước tốt nhất không phải `Mate` (trường hợp Mate thì
`pick_weighted` tự trả nước chiếu hết). Cài đặt "Đa dạng nước đi"
(mặc định bật) và "Chế độ thi đấu" (tắt hẳn) thêm vào `Settings`
(serde default để settings cũ vẫn đọc được); P7 thêm nút bật/tắt trên UI.

## 22. `DIVERSITY_MARGIN_CP = 20` giữ nguyên, `GradeThresholds` Pikafish giữ nguyên (P6)

Đo `measure_top3_gaps` (30 thế khai cuộc, analyze 1000 ms, top 3):
gap(1–2) trung vị 32 cp (p25=8, p75=113), ≤20 cp ở 47% số thế, ≤50 cp
ở 53%; gap(1–3) trung vị 77 cp, ≤20 cp ở 20%. Phân phối phân cực: hoặc
nước thứ 2 gần như ngang nước tốt nhất (≤20 cp), hoặc kém hẳn (>50 cp)
— nên biên 20 đã bắt gần hết các lựa chọn thực sự gần, tăng lên 50
chỉ thêm 6% số thế. Giữ biên khởi đầu 20.

Cùng số đo này kiểm chứng ngưỡng chấm Pikafish {15, 50, 120, 250}:
nước tốt thứ 2 thường kém 8–32 cp → "Tốt" (16–50); nước thứ 3
trung vị kém 77 cp → "Chấp nhận được" (51–120). Ngưỡng khớp với dữ
liệu đo nên giữ nguyên, không chỉnh.

## 23. Trait `Engine` thêm 4 method, mặc định = hành vi builtin cũ (P6)

- `limits_for(level)` — giới hạn theo engine đang chạy (Pikafish khác
  builtin ở cùng mức). Mặc định `level.builtin_limits()`.
- `weaken(level) -> Option<Weaken>` — làm yếu bằng chọn nước (Pikafish
  Dễ/Trung bình); mặc định `None` (luôn đi nước tốt nhất).
- `supports_diversity()` — Pikafish `true`, builtin `false`.
- `grade_limits()` / `hint_limits()` — chấm/gợi ý: builtin giữ depth
  6/4 như cũ; Pikafish dùng movetime 1500 ms cho cả hai (theo §6: gợi ý
  `analyze(top_n = 3, movetime = 1500 ms)`, chấm nước "cùng movetime").

Người gọi đổi theo engine đang chạy thay vì luôn `builtin_limits()`:
`vs_ai::maybe_trigger_ai` (nước AI), `vs_ai::play` (chấm ở chế độ tiếp
tục từ Học chơi), `learn.rs` (gợi ý/ghi điểm/nước đối thủ ngoài sách),
`Tutor::grade` + `Tutor::suggestions`. `PikafishEngine::set_level` chỉ
lưu level (không có option UCI để gửi — xem §13); mức có hiệu lực qua
giới hạn của request tiếp theo.

## 24. `go` gửi cả depth và movetime khi cả hai đều đặt (P6)

`go_command` trước đây chỉ gửi một trong hai (movetime thắng depth).
Sửa: gửi cả hai (`go depth 8 movetime 1000`, engine dừng ở giới hạn nào
tới trước) — cần thiết vì bảng mức P6 đặt cả depth cap lẫn movetime cap.
Deadline chờ `bestmove` vẫn theo movetime + slack.

## 25. Cờ stop sống trong `SharedEngine`, không sống trong engine (P7)

Phát hiện khi làm P7: cờ stop vốn sống trong từng engine (flag của
`Searcher` / của `UciSession`), nên sau mỗi lần đổi engine sau lưng
`SharedEngine` (Pikafish → Builtin khi crash ở P4; Builtin → Pikafish
lúc khởi động ở P7) thì handle stop của worker trỏ vào flag đã chết —
bấm Dừng/Hoàn nước không ngắt được search đang chạy. Thậm chí
`BuiltinEngine::set_level` cũng tạo lại `Searcher` (flag mới) nên
handle cũ cũng hỏng.

Sửa: `SharedEngine` đổi từ alias `Arc<Mutex<...>>` thành struct giữ
`stop: Arc<AtomicBool>` chung; mọi engine khi được cài đặt đều nhận
flag này qua trait method mới `Engine::set_stop_flag` (mặc định
không-op cho mock); `Searcher` có thêm `set_stop_flag`,
`BuiltinEngine` giữ flag riêng và áp lại sau mỗi lần rebuild searcher,
`UciSession` có thêm `set_stop_flag`. `swap_to_builtin` áp flag sau
khi đổi engine. Test hồi quy `stop_flag_survives_engine_swap`:
đổi engine rồi stop giữa chừng bằng handle cũ → search dừng trong
< 5 s (thực tế 0,33 s).

## 26. Tắt engine lúc thoát app (P7)

Thêm trait method `Engine::shutdown` (mặc định không-op);
`PikafishEngine` quit session (`quit`, chờ tối đa 2 s rồi kill).
`eframe::App::on_exit` stop search đang chạy trước rồi mới shutdown.
Drop của session vẫn là backstop nếu shutdown không được gọi (vd
`swap_to_builtin` drop engine cũ — chờ tối đa 2 s trong luồng worker,
chấp nhận được vì hiếm).

## 27. Engine manager: pre-warm lúc khởi động, banner một lần (P7)

`crates/xq-app/src/engine_manager.rs`: app khởi động với Builtin ngay
để dùng được tức thì, đồng thời một luồng nền chạy `locate` + spawn
Pikafish (pre-warm, ván đầu không chờ nạp NNUE); thành công thì đổi
engine sau lưng `SharedEngine` (giữ level theo Cài đặt), thất bại thì
giữ Builtin + banner một lần ("Không tìm thấy Pikafish, đang dùng AI
tích hợp" / lý do lỗi). Nút "Thử lại Pikafish" trong Cài đặt chạy lại
cùng luồng với settings hiện tại. Banner hiển thị ở đầu mọi màn hình,
bấm ✕ để tắt; banner giữa ván (worker fallback) cũng đi qua cùng banner
(`take_fallback_notice` đã nối, bỏ `#[allow(dead_code)]`).

## 28. Cài đặt engine (P7)

Thêm vào `Settings` (serde default để settings cũ vẫn đọc được):
Threads 1–8 (mặc định `default_threads()` = clamp(cpus−1, 1, 8)),
Hash 0–1024 MB bước 64 (0 = tự động theo RAM qua crate sysinfo:
<4 GB → 64, <8 GB → 128, còn lại 256), Elo tùy chỉnh 1200–3000 bước
50 + mức "Tùy chỉnh (Elo)" (difficulty 4, cả trong Cài đặt lẫn hộp
thoại bắt đầu), "Đa dạng nước đi" (mặc định bật) + "Chế độ thi đấu"
(đã thêm field ở P6, P7 mới có nút bật/tắt trên UI), đường dẫn
Pikafish tự đặt, ghi trace UCI. Đổi Threads/Hash chỉ có hiệu lực ở lần
cài đặt tiếp theo (khởi động lại app hoặc bấm "Thử lại Pikafish").

## 29. Log engine (P7)

`xq-pikafish/src/log.rs`: sink toàn cục (không sink = không-op, test
không ồn). Mức Warn (fallback, restart, handshake lỗi) luôn ghi; mức
Trace (dòng UCI gửi/nhận, tiền tố →/←) chỉ khi bật trong Cài đặt.
`xq-app/src/engine_log.rs`: file xoay vòng 1 MB × 3 tại
`<eframe storage_dir>/logs/engine.log` (ghi cả timestamp); không tạo
được thư mục thì tắt log thay vì làm hỏng app. Nút "Mở engine.log"
trong màn Giới thiệu mở thư mục logs.

## 30. Màn Giới thiệu (P7)

Thêm: tên engine đang dùng (vd "Pikafish 2026-09-06", lấy từ `id
name`), ghi chú GPLv3 + link mã nguồn
github.com/official-pikafish/Pikafish, ghi chú NNUE chỉ dùng phi
thương mại, nút "Mở thư mục engine" (xdg-open/open/explorer theo nền
tảng) và "Mở engine.log".

## 31. Smoke test 9.4 pass (P7)

`vs_ai_plays_with_fake_pikafish` trong `smoke_tests.rs`: đặt
`XQ_PIKAFISH_PATH` trỏ tới fake-uci (copy vào thư mục tạm kèm
`pikafish.nnue` rỗng), app cài đặt fake engine lúc khởi động (không
banner), mở Đấu với AI, người chơi đi pháo đầu, AI trả lời một nước
hợp lệ. Fake-uci được tìm theo thư mục target của test binary, build
lại bằng `cargo build` nếu thiếu.

## 32. Giới hạn môi trường: không chạy được app dưới Xvfb trên VM này (P7)

VM thiếu `libxkbcommon-x11.so` (chỉ có `libxkbcommon.so.0`) nên cả bản
debug lẫn release đều panic lúc khởi động dưới Xvfb; `apt-get update`
chạy quá chậm (>7 phút chưa xong) nên không cài được gói. Đây là thiếu
thư viện hệ thống, không phải lỗi code — build release vẫn xong, toàn
bộ test (kể cả smoke test headless qua egui_kittest) đều xanh. Kiểm tra
giao diện bằng mắt cần chạy trên máy có đủ thư viện (máy Hieu).

## 33. Đóng gói: KHÔNG dùng CI, build từ mã nguồn bằng script tay (P8)

Hieu không muốn chạy workflow nên §8.2 (job `engine-build` trong CI)
được thay bằng script tay, còn `release.yml` chỉ là tài liệu mô tả quy
trình (file vẫn nằm local, gitignored, không bao giờ chạy trên GitHub):

- `tools/fetch-engine.sh`: tải archive release, kiểm sha256 archive,
  giải nén (7z/7zz/bsdtar/py7zr), kiểm sha256 NNUE, ghi
  `engine/SOURCE.txt`. Đã kiểm end-to-end trên VM (cả hai sha256 đều
  OK). Temp dir dùng `target/.fetch-tmp` thay vì `/tmp` vì `/tmp` trên
  VM chỉ là tmpfs 512 MB (lần đầu chạy rớt lỗi "No space left").
- `tools/build-engine.sh`: clone repo ở commit đã ghim rồi build từ
  mã nguồn (Linux: `make profile-build ARCH=x86-64-universal`;
  Windows MSYS2: thêm `COMP=mingw`; macOS: `make macos-lipo`),
  kiểm `bench` in ra "Nodes searched".
- `engine/PIKAFISH_VERSION.toml` đã có từ P5 (ghi chú thêm: không có
  `nnue_url` riêng vì NNUE nằm trong archive — mẫu §8.1 không áp dụng
  được cho trường này).
- `tools/bundle-macos.sh`: copy `engine/` (binary macOS + nnue + giấy
  phép + SOURCE.txt) vào `Contents/Resources/engine/` TRƯỚC khi
  codesign; ký riêng binary Pikafish trước rồi mới ký cả bundle.
  Thiếu `engine/` thì cảnh báo và đóng gói không có Pikafish (chỉ AI
  tích hợp) thay vì fail.
- `release.yml` (local): thêm job `engine-build` (build từ mã nguồn
  cho Windows + macOS, tạo tarball mã nguồn `pikafish-<version>-src.
  tar.gz` để đính kèm Release theo nghĩa vụ GPL), job windows/macos
  đưa `engine/` vào zip/DMG, job publish đính kèm tarball. YAML đã
  kiểm cú pháp (python yaml). Các ARCH trong §8.2 (avx2, sse41-popcnt,
  apple-silicon) đã lỗi thời với release này: Makefile ở commit ghim
  có `x86-64-universal` + target `macos-lipo` (x86-64 slice +
  Apple-silicon slice), và release chính thức cũng chỉ ship universal.
- Checklist §9.5 thêm vào `docs/release-checklist.md` (10 mục, ghi rõ
  mục nào đã kiểm tự động trên Linux, mục nào cần Hieu kiểm tay).
- Mục tiêu kích thước gói "< 15 MB" của đặc tả gốc đổi thành "< 90
  MB" theo §8.3 — việc sửa chữ trong đặc tả gốc để dành P9.

## 34. Tài liệu: đặc tả gốc, README, CHANGELOG (P9)

- Đặc tả gốc `~/workspace/user/files/a__c_ta__ky__thua__t_Game_Co___Tu_o__ng_AI__Rust.md`
  (file ngoài repo, không vào commit): §4.4 thay bằng bảng mức Pikafish
  (Dễ depth 3 + chọn yếu, Trung bình depth 6, Khó depth 12, Cao thủ không
  giới hạn depth + 5 s/nước), bảng cũ giữ lại làm dự phòng; §4.5 ghi chú
  ngưỡng chấm khi chạy Pikafish là {15, 50, 120, 250} (thang engine cũ
  {20, 60, 150, 300} chỉ dùng khi chạy AI tích hợp); §9 đổi mục tiêu kích
  thước gói "< 15 MB" thành "< 90 MB" (riêng `pikafish.nnue` đã ~50 MB)
  và ghi chú thư mục `engine/` + 2 script `fetch-engine.sh` /
  `build-engine.sh`.
- `README.md`: AI mặc định là Pikafish (UCI, offline), engine cũ làm dự
  phòng; thêm 2 crate mới (`xq-engine`, `xq-pikafish`), cách lấy engine
  bằng `fetch-engine.sh`, mục giấy phép GPLv3 (engine) + NNUE-License
  (chỉ phi thương mại).
- `CHANGELOG.md`: mục [1.0.0] chưa phát hành thêm bullet Pikafish (mức
  khó mới, Elo tùy chỉnh, Cài đặt engine, log, Giới thiệu; Pikafish thắng
  AI cũ 10/10 ván đo trên Linux).
- Kiểm chứng build từ mã nguồn (`tools/build-engine.sh`, checklist §9.5):
  ĐÃ XONG trên Linux 2026-11-11 sau 3 lần sửa script (xem §35–§38):
  `profile-build` universal lỗi vì bench PGO thiếu net trong
  `temp_builds/<arch>/`; universal Linux cần clang+libc++ nên đổi sang
  build 1 arch x86-64-avx2 bằng gcc; sửa `--out` tương đối và copy
  `pikafish.nnue` vào thư mục đích. Kết quả cuối: script exit 0,
  `bench` = 2651062 nodes khớp bản release, thư mục đích tự chạy được.
- Cổng cuối (fmt + clippy -D warnings + cargo test --workspace), chạy
  2026-11-11 sau khi mọi sửa đổi dừng: **fmt OK, clippy sạch, 145 pass
  / 0 fail / 26 ignored**.

## 35. Build từ mã nguồn: `profile-build` lỗi, đổi sang `build` như upstream (P8/P9)

- Lần kiểm chứng đầu (2026-11-11, `./tools/build-engine.sh --out
  /tmp/engine-build-test`): FAIL, exit 2, rớt ở bước PGO "Step 2/4.
  Running benchmark" trong `src/temp_builds/x86-64-avx2` (`./pikafish
  bench` → Error 1).
- Nguyên nhân gốc (đọc mã nguồn ở commit đã ghim, không đoán): bench PGO
  chạy trong `src/temp_builds/<arch>/`; target `.setup` của Makefile chỉ
  symlink `TRACKED_TOP` (= SRCS + HEADERS + Makefile) vào thư mục đó,
  KHÔNG có `pikafish.nnue`; `Network::load` (`nnue/network.cpp`) chỉ tìm
  net ở cwd + thư mục chứa binary — cả hai đều là `temp_builds/<arch>/`
  nên bench báo net không load được và chết. Copy net vào `src/` (bản sửa
  trước) chỉ đủ cho chạy tay từ `src/`, không đủ cho bench PGO. CPU VM có
  đủ AVX2/AVX512 nên không phải lỗi SIGILL.
- Workflow release chính thức của upstream (`.github/workflows/
  pikafish.yml`, dòng 53/120) dùng `make build`, KHÔNG dùng
  `profile-build` (macOS: `make macos-lipo`) — tức binary release chính
  thức cũng không PGO.
- Sửa `tools/build-engine.sh`: Linux + Windows dùng `make build
  ARCH=x86-64-universal` (macOS giữ `macos-lipo`); kiểm `bench` cuối
  script vẫn chạy từ `src/` (có net) nên vẫn đối chiếu được
  `Nodes searched: 2651062`. Binary không PGO chỉ chậm hơn một chút,
  đúng tính năng và khớp cách build của upstream.
- Kiểm chứng lại: đang chạy nền, log đầy đủ ở `/tmp/build-engine-
  verify2.log`; kết quả ghi tiếp tại đây khi xong.

## 36. Build từ mã nguồn: universal trên Linux cần clang+libc++, đổi sang build 1 arch bằng gcc (P8/P9)

- Lần kiểm chứng 2 (2026-11-11, sau sửa §35): 8 arch biên dịch xong
  bằng g++ 13.3.0, rớt ở bước link cuối universal: `g++: error:
  unrecognized command-line option '-stdlib=libc++'` (kèm
  `-rtlib=compiler-rt`, `-fuse-ld=lld`, `-lc++`, `-lc++abi`,
  `-lunwind`). Log đầy đủ ở `target/build-engine-verify2.log`.
- Nguyên nhân gốc (Makefile ở commit ghim, dòng 1391–1404): trên Linux,
  `UNIVERSAL_FINAL_FLAGS` dùng cờ clang/libc++ VÔ ĐIỀU KIỆN (trừ
  `COMP=ndk` → `-static`); không có nhánh gcc. Workflow release của
  upstream (`.github/workflows/pikafish.yml`) build Linux bằng clang 22
  + `libc++-22-dev`, `libc++abi-22-dev`, `libunwind-22-dev`,
  `COMP=clang`, `EXTRACXXFLAGS/EXTRALDFLAGS=-stdlib=libc++`. VM này
  không có clang, apt lại chậm nên không cài để kiểm.
- Sửa `tools/build-engine.sh`: Linux build 1 arch slice bằng gcc thay
  vì universal — `ARCH=x86-64-avx2` nếu CPU có AVX2 (đọc
  `/proc/cpuinfo`), còn không thì `x86-64` (chạy mọi máy x86-64). App
  vẫn tìm được binary `pikafish` thường trong `engine/` (fallback dev
  trong `xq-pikafish/src/locate.rs`). Ai cần đúng binary universal trên
  Linux thì cài clang + libc++ rồi chạy tay: `make build
  ARCH=x86-64-universal COMP=clang` (ghi trong comment đầu script).
  Windows giữ `COMP=mingw`, macOS giữ `macos-lipo` — cả hai chưa kiểm
  được trên OS thật.
- Kiểm chứng lại: đang chạy nền, log ở `target/build-engine-verify3.log`;
  kết quả ghi tiếp tại đây khi xong.

## 37. Build từ mã nguồn: OK — binary 1 arch, bench khớp release (P8/P9)

- Lần kiểm chứng 3 (2026-11-11, sau sửa §36): **thành công**.
  `./tools/build-engine.sh --out target/engine-build-test` exit 0, log
  `target/build-engine-verify3.log`: `built
  target/engine-build-test/pikafish (Pikafish 2026-09-06, commit
  4c17cee11f888ae1d48a9494f2e2239f019f0a1f)`, `bench` in
  `Nodes searched : 2651062` — khớp đúng số của binary release
  (commit message `Bench: 2651062`). Binary là slice `x86-64-avx2`
  duy nhất (CPU VM có AVX2), build bằng g++ 13.3.0, không PGO.
- Lỗi nhỏ tìm thấy trong lần này (không phải lỗi build): `--out` tương
  đối bị hiểu theo thư mục `src/` của clone tạm (script `cd` vào đó
  trước khi dùng `$OUT`) nên thư mục đích rơi vào temp dir rồi bị trap
  xóa — bản build thành công nhưng file không còn. Đã sửa: chuẩn hóa
  `$OUT` thành đường dẫn tuyệt đối theo cwd của người gọi ngay sau khi
  đọc tham số. Đang kiểm chứng lại (log
  `target/build-engine-verify4.log`).
- Thiếu sót còn lại của script (ghi để sửa sau lần kiểm này): `--out`
  staging chỉ copy binary + giấy phép, chưa copy `pikafish.nnue` đã
  kiểm sha256 — thư mục đích chưa chạy độc lập được (mặc định
  `--out engine/` thì không sao vì net đã nằm sẵn đó).

## 38. Build từ mã nguồn: sửa `--out` tương đối + copy net vào thư mục đích (P8/P9)

- Lần kiểm chứng 4 (2026-11-11): **thành công end-to-end** sau sửa
  `--out`. Script exit 0, binary đích nằm đúng
  `target/engine-build-test/pikafish` (sha256
  `beaf57d496bce2cb71129e2be3c0d80002dbb9c5f93b0f01df2634671cf60320`),
  kèm `Copying.txt`, `bench` trong script in `Nodes searched :
  2651062`. Log `target/build-engine-verify4.log`.
- Lưu ý: binary build từ mã nguồn tự xưng `id name Pikafish
  dev-20260906-4c17cee1` (khác binary release `Pikafish 2026-09-06`)
  vì chuỗi version lấy từ git — màn Giới thiệu sẽ hiện tên dev này
  nếu dùng binary tự build; không ảnh hưởng nước đi hay bench.
- Sửa tiếp: copy `pikafish.nnue` (đã kiểm sha256 trong script) vào
  `$OUT` để thư mục đích chạy độc lập được; lần kiểm 4 cho thấy
  `bench` chạy từ thư mục đích (chưa có net) không ra số, chỉ handshake
  `uci` vẫn trả lời. Đang kiểm chứng lại lần 5 (log
  `target/build-engine-verify5.log`), gồm cả `bench` độc lập từ thư
  mục đích.

- Lần kiểm chứng 5 (2026-11-11): **thành công, thư mục đích tự chạy
  được**. Script exit 0; `target/engine-build-test/` có đủ `pikafish`
  + `pikafish.nnue` (50.706.378 byte) + `Copying.txt`; `bench` chạy
  độc lập từ thư mục đích in `Nodes searched : 2651062`. Log
  `target/build-engine-verify5.log`. Build từ mã nguồn coi như đã
  kiểm chứng xong trên Linux (binary 1 arch x86-64-avx2, gcc, không
  PGO, không universal — xem §35–§36).
