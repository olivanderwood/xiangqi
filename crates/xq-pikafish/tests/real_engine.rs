//! Real-Pikafish behaviour (integration spec §9.3). All tests are
//! `#[ignore]`: they need the real binary + NNUE and take minutes.
//!
//! Run: `XQ_PIKAFISH_PATH=$PWD/engine/Pikafish-Linux-x86-64-universal \
//!   cargo test --release -p xq-pikafish -- --ignored`
//!
//! Pinned release: Pikafish 2026-09-06, commit
//! `4c17cee11f888ae1d48a9494f2e2239f019f0a1f` — see
//! `docs/research-pikafish-2026-09-06.md` and
//! `engine/PIKAFISH_VERSION.toml`.
//!
//! Known real-binary facts that shape these tests:
//! - The binary advertises only 11 UCI options (no `UCI_Elo`,
//!   `Skill Level`, `Repetition Rule`, `Sixty Move Rule`,
//!   `Rule60MaxPly` — the wiki documents another build). Difficulty is
//!   therefore set with `go` limits, not options (P6).
//! - `searchmoves` IS supported (verified live during research).

use std::path::PathBuf;
use std::time::{Duration, Instant};
use xq_core::board::Position;
use xq_core::notation::parse_iccs;
use xq_core::types::{Color, Move, PieceKind};
use xq_engine::{BuiltinEngine, Engine, EngineRequest, Level, Score, SearchLimits};
use xq_pikafish::{EngineOptions, LocatedEngine, PikafishEngine};

/// Spawn the real engine from `XQ_PIKAFISH_PATH` (the executable; the
/// NNUE must sit next to it as `pikafish.nnue`).
fn real_engine() -> PikafishEngine {
    let exe = std::env::var("XQ_PIKAFISH_PATH").unwrap_or_else(|_| {
        panic!(
            "đặt XQ_PIKAFISH_PATH tới binary Pikafish thật, vd:\n\
             XQ_PIKAFISH_PATH=$PWD/engine/Pikafish-Linux-x86-64-universal \
             cargo test --release -p xq-pikafish -- --ignored"
        )
    });
    let exe = PathBuf::from(exe);
    let dir = exe
        .parent()
        .expect("binary phải nằm trong một thư mục")
        .to_path_buf();
    let located = LocatedEngine {
        exe,
        engine_dir: dir.clone(),
        nnue: dir.join("pikafish.nnue"),
    };
    let mut options = EngineOptions::new(dir.join("pikafish.nnue"));
    options.threads = 1;
    options.hash_mb = 64;
    let engine = PikafishEngine::spawn(located, options).expect("spawn Pikafish thật");
    assert!(
        engine.name().starts_with("Pikafish"),
        "mong đợi Pikafish, gặp {}",
        engine.name()
    );
    engine
}

fn startpos_req(limits: SearchLimits) -> EngineRequest {
    EngineRequest::new(Position::startpos().to_fen(), Vec::new(), limits)
}

/// Deterministic PRNG (LCG) for building random games.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Play `plies` pseudo-random legal moves from the start position.
/// Returns `(start_fen, moves)`; retries with another seed if the
/// game ends early.
fn random_game(plies: usize) -> (String, Vec<Move>) {
    for seed in 1..=5u64 {
        let mut rng = Rng(seed * 0x9e3779b97f4a7c15);
        let mut pos = Position::startpos();
        let mut moves = Vec::new();
        while moves.len() < plies {
            let legal = pos.legal_moves();
            if legal.is_empty() {
                break;
            }
            let mv = legal[(rng.next() as usize) % legal.len()];
            pos.make_move(mv);
            moves.push(mv);
        }
        if moves.len() == plies {
            return (Position::startpos().to_fen(), moves);
        }
    }
    panic!("không tạo được ván ngẫu nhiên {plies} nửa nước sau 5 seed");
}

/// Material balance (Red minus Black) in centipawn-ish units.
fn material(pos: &Position) -> i32 {
    let value = |k: PieceKind| match k {
        PieceKind::King => 0,
        PieceKind::Advisor => 200,
        PieceKind::Elephant => 200,
        PieceKind::Horse => 270,
        PieceKind::Rook => 500,
        PieceKind::Cannon => 450,
        PieceKind::Pawn => 100,
    };
    let mut total = 0;
    for sq in 0..90u8 {
        if let Some(p) = pos.piece_at(sq) {
            let v = value(p.kind);
            total += match p.color {
                Color::Red => v,
                Color::Black => -v,
            };
        }
    }
    total
}

#[test]
#[ignore]
fn startpos_bestmove_legal() {
    let mut engine = real_engine();
    let line = engine
        .best_move(&startpos_req(SearchLimits::time_ms(500)), &mut |_| {})
        .expect("Pikafish đi được từ startpos");
    assert!(Position::startpos().legal_moves().contains(&line.mv));
}

#[test]
#[ignore]
fn fen_accepted_roundtrip() {
    let mut engine = real_engine();
    let (start_fen, moves) = random_game(50);
    let mut pos = Position::startpos();
    let mut played: Vec<Move> = Vec::new();
    for (i, mv) in moves.iter().enumerate() {
        pos.make_move(*mv);
        played.push(*mv);
        let req = EngineRequest::new(start_fen.clone(), played.clone(), SearchLimits::depth(2));
        let line = engine
            .best_move(&req, &mut |_| {})
            .unwrap_or_else(|e| panic!("FEN ở nửa nước {i} bị từ chối: {e}"));
        assert!(
            pos.legal_moves().contains(&line.mv),
            "nước ở nửa nước {i} phải hợp lệ"
        );
    }
    assert_eq!(played.len(), 50);
}

#[test]
#[ignore]
fn mate_in_one() {
    // Three moves win in one here (b0b9 and b0d0 are checkmates, b0a0
    // stalemates the bare king — in xiangqi a stalemate also loses),
    // so pin the mate score and the terminal result, not one move.
    let mut engine = real_engine();
    let req = EngineRequest::new(
        "3k5/R8/9/9/9/9/9/9/9/1R2K4 w - - 0 1".to_string(),
        Vec::new(),
        SearchLimits::depth(5),
    );
    let line = engine
        .best_move(&req, &mut |_| {})
        .expect("Pikafish tìm nước thắng trong 1");
    assert!(
        matches!(line.score, Score::Mate(1)),
        "mong đợi Mate(1), gặp {:?} với nước {}",
        line.score,
        xq_core::notation::iccs(line.mv)
    );
    let mut pos = Position::from_fen("3k5/R8/9/9/9/9/9/9/9/1R2K4 w - - 0 1").expect("FEN hợp lệ");
    assert!(pos.legal_moves().contains(&line.mv));
    pos.make_move(line.mv);
    assert!(
        pos.legal_moves().is_empty(),
        "sau nước thắng, Đen không còn nước đi"
    );
}

#[test]
#[ignore]
fn takes_hanging_rook() {
    let mut engine = real_engine();
    let req = EngineRequest::new(
        "4k4/9/4r4/9/4P4/9/9/4C4/9/4K4 w - - 0 1".to_string(),
        Vec::new(),
        SearchLimits::depth(5),
    );
    let line = engine
        .best_move(&req, &mut |_| {})
        .expect("Pikafish ăn Xe treo");
    assert_eq!(line.mv, parse_iccs("e2e7").unwrap(), "phải ăn Xe e7");
}

#[test]
#[ignore]
fn multipv_three_distinct() {
    let mut engine = real_engine();
    let lines = engine
        .analyze(&startpos_req(SearchLimits::depth(5)), 3, &mut |_| {})
        .expect("analyze top 3");
    assert_eq!(lines.len(), 3, "phải có đúng 3 dòng");
    let moves: Vec<_> = lines.iter().map(|l| l.mv).collect();
    assert!(moves.windows(2).all(|w| w[0] != w[1]), "3 nước khác nhau");
    for w in lines.windows(2) {
        assert!(w[0].score.to_cp_clamped() >= w[1].score.to_cp_clamped());
    }
}

#[test]
#[ignore]
fn searchmoves_supported() {
    // The research verified `searchmoves` live (`go depth 6 searchmoves
    // a6a5` -> `bestmove a6a5`); this test pins the end-to-end
    // `score_move` path against the real binary.
    let mut engine = real_engine();
    let mv = parse_iccs("h2e2").unwrap();
    let score = engine
        .score_move(&startpos_req(SearchLimits::depth(4)), mv)
        .expect("score_move chạy được với Pikafish thật");
    eprintln!("searchmoves h2e2 -> score {score:?}");
    let cp = score.to_cp_clamped();
    assert!(cp.abs() < 1000, "điểm phải trong khoảng hợp lý, gặp {cp}");
}

#[test]
#[ignore]
fn stop_responsive() {
    let mut engine = real_engine();
    let stop = engine.stop_handle();
    let req = startpos_req(SearchLimits::time_ms(30_000));
    let handle = std::thread::spawn(move || {
        let r = engine.best_move(&req, &mut |_| {});
        (engine, r)
    });
    std::thread::sleep(Duration::from_millis(300));
    let t0 = Instant::now();
    stop.stop();
    let (mut engine, result) = handle.join().expect("worker thread");
    let elapsed = t0.elapsed();
    assert!(
        matches!(result, Err(xq_engine::EngineError::Stopped)),
        "mong đợi Stopped sau stop, gặp {result:?}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "stop phải có hiệu lực nhanh, mất {elapsed:?}"
    );
    // The engine must still work after a stop.
    let line = engine
        .best_move(&startpos_req(SearchLimits::time_ms(300)), &mut |_| {})
        .expect("engine vẫn dùng được sau stop");
    assert!(Position::startpos().legal_moves().contains(&line.mv));
}

#[test]
#[ignore]
fn movetime_respected() {
    let mut engine = real_engine();
    let t0 = Instant::now();
    let line = engine
        .best_move(&startpos_req(SearchLimits::time_ms(500)), &mut |_| {})
        .expect("Pikafish đi được");
    let elapsed = t0.elapsed();
    assert!(Position::startpos().legal_moves().contains(&line.mv));
    assert!(
        elapsed >= Duration::from_millis(300) && elapsed < Duration::from_secs(3),
        "movetime 500 ms phải dùng ~0,5 s, thực tế {elapsed:?}"
    );
}

#[test]
#[ignore]
fn long_game_moves_list() {
    // A 150-ply history must be accepted as `position fen ... moves ...`.
    let mut engine = real_engine();
    let (start_fen, moves) = random_game(150);
    let req = EngineRequest::new(start_fen, moves, SearchLimits::depth(2));
    assert_eq!(req.moves.len(), 150);
    let line = engine
        .best_move(&req, &mut |_| {})
        .expect("Pikafish đi được sau 150 nửa nước");
    let mut pos = Position::startpos();
    for mv in &req.moves {
        pos.make_move(*mv);
    }
    assert!(pos.legal_moves().contains(&line.mv));
}

#[test]
#[ignore]
fn pikafish_beats_builtin() {
    // 10 games, alternating colours, 100 ms/move, cap 100 plies.
    // A win is a checkmate, or a >= 300 material lead at the cap.
    // Pikafish at full strength must win >= 9/10 against the builtin
    // searcher at 100 ms/move.
    let mut wins = 0;
    for game in 0..10 {
        let pikafish_red = game % 2 == 0;
        let mut pikafish = real_engine();
        let mut builtin = BuiltinEngine::with_level(Level::Medium);
        let mut pos = Position::startpos();
        let mut moves: Vec<Move> = Vec::new();
        let mut result: Option<bool> = None; // Some(true) = Pikafish won
        for _ in 0..100 {
            let red_to_move = pos.side() == Color::Red;
            let pikafish_to_move = red_to_move == pikafish_red;
            let req = EngineRequest::new(
                Position::startpos().to_fen(),
                moves.clone(),
                SearchLimits::time_ms(100),
            );
            let line = if pikafish_to_move {
                pikafish.best_move(&req, &mut |_| {})
            } else {
                builtin.best_move(&req, &mut |_| {})
            };
            let Ok(line) = line else {
                // An engine error loses the game for its side.
                result = Some(!pikafish_to_move);
                break;
            };
            if !pos.legal_moves().contains(&line.mv) {
                result = Some(!pikafish_to_move);
                break;
            }
            pos.make_move(line.mv);
            moves.push(line.mv);
            if pos.legal_moves().is_empty() {
                // Side to move is mated (or stalemated); in xiangqi a
                // bare king with no move loses.
                result = Some(pikafish_to_move);
                break;
            }
        }
        let won = match result {
            Some(w) => w,
            None => {
                let diff = material(&pos);
                let pikafish_diff = if pikafish_red { diff } else { -diff };
                pikafish_diff >= 300
            }
        };
        eprintln!(
            "game {game} (Pikafish {}): {}",
            if pikafish_red { "Red" } else { "Black" },
            if won { "thắng" } else { "không thắng" }
        );
        if won {
            wins += 1;
        }
    }
    assert!(wins >= 9, "Pikafish phải thắng >= 9/10, thực tế {wins}/10");
}

#[test]
#[ignore]
fn perpetual_not_self_inflicted() {
    // One game, Pikafish (Red) vs builtin (Black), cap 150 plies. If a
    // position repeats 3 times, fail only when Pikafish made the move
    // that created the third occurrence.
    let mut pikafish = real_engine();
    let mut builtin = BuiltinEngine::with_level(Level::Medium);
    let mut pos = Position::startpos();
    let mut moves: Vec<Move> = Vec::new();
    let mut seen: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    seen.insert(pos.hash(), 1);
    for _ in 0..150 {
        let red_to_move = pos.side() == Color::Red;
        let req = EngineRequest::new(
            Position::startpos().to_fen(),
            moves.clone(),
            SearchLimits::time_ms(100),
        );
        let line = if red_to_move {
            pikafish.best_move(&req, &mut |_| {})
        } else {
            builtin.best_move(&req, &mut |_| {})
        }
        .expect("engine đi được");
        assert!(pos.legal_moves().contains(&line.mv));
        pos.make_move(line.mv);
        moves.push(line.mv);
        if pos.legal_moves().is_empty() {
            return; // mate/stalemate: no perpetual
        }
        let count = seen.entry(pos.hash()).or_insert(0);
        *count += 1;
        if *count >= 3 {
            assert!(
                !red_to_move,
                "Pikafish tự gây lặp lại thế cờ 3 lần ở nửa nước {}",
                moves.len()
            );
            return; // repetition caused by the builtin: not our fault
        }
    }
}
