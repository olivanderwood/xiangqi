//! P6 behaviour tests (integration spec §9.3, rows `levels_ordered`
//! and `diversity_*`). All `#[ignore]`: they need the real Pikafish
//! binary and take minutes.
//!
//! Run: `XQ_PIKAFISH_PATH=$PWD/engine/Pikafish-Linux-x86-64-universal \
//!   cargo test --release -p xq-pikafish --test levels_diversity -- --ignored`
//!
//! Test-time scale-down (documented, does not change what is tested):
//! Master runs at 1000 ms/move instead of 5000 ms (still uncapped
//! depth, so still full strength) and diversity games use 500 ms/move
//! (the pick depends on the score gaps, not on the movetime).

use std::path::PathBuf;
use xq_core::board::Position;
use xq_core::types::{Color, Move, PieceKind};
use xq_engine::{Engine, EngineRequest, Level, SearchLimits};
use xq_pikafish::{EngineOptions, LocatedEngine, PikafishEngine};

fn real_engine() -> PikafishEngine {
    let exe =
        std::env::var("XQ_PIKAFISH_PATH").expect("đặt XQ_PIKAFISH_PATH tới binary Pikafish thật");
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
    PikafishEngine::spawn(located, options).expect("spawn Pikafish thật")
}

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

/// One move at a configured level, exactly as the app plays it
/// (`VsAiState`): best move, or analyze top 3 + weighted pick when the
/// level weakens. `master_ms` overrides Master's movetime for tests.
fn level_move(
    engine: &mut PikafishEngine,
    level: Level,
    req: &EngineRequest,
    seed: u64,
) -> Result<xq_engine::Line, xq_engine::EngineError> {
    let cfg = xq_pikafish::pikafish_level(level);
    let Some(w) = cfg.weaken else {
        return engine.best_move(req, &mut |_| {});
    };
    let lines = engine.analyze(req, 3, &mut |_| {})?;
    let mut rng = xq_pikafish::GameRng::for_ply(seed, req.moves.len());
    match xq_pikafish::pick_weighted(&lines, w.margin_cp, w.temperature, &mut rng) {
        Some(mv) => Ok(lines
            .into_iter()
            .find(|l| l.mv == mv)
            .expect("nước chọn phải nằm trong các dòng")),
        None => lines
            .into_iter()
            .next()
            .ok_or_else(|| xq_engine::EngineError::Protocol("analyze không trả dòng nào".into())),
    }
}

/// Play one game between two configured levels. Returns `Some(true)`
/// when `first` (the level playing Red in even games) wins, `Some(false)`
/// when it loses, `None` on a draw at the ply cap.
fn play_game(
    first: Level,
    second: Level,
    first_red: bool,
    seed_base: u64,
    cap_plies: usize,
    master_ms: u64,
) -> Option<bool> {
    let mut engine_first = real_engine();
    let mut engine_second = real_engine();
    let mut pos = Position::startpos();
    let mut moves: Vec<Move> = Vec::new();
    for _ in 0..cap_plies {
        let red_to_move = pos.side() == Color::Red;
        let first_to_move = red_to_move == first_red;
        let (level, engine, seed) = if first_to_move {
            (first, &mut engine_first, seed_base)
        } else {
            (second, &mut engine_second, seed_base + 0x1000)
        };
        let mut limits = xq_pikafish::pikafish_limits(level);
        if level == Level::Master {
            limits.max_time_ms = Some(master_ms);
        }
        let req = EngineRequest::new(Position::startpos().to_fen(), moves.clone(), limits);
        let line = level_move(engine, level, &req, seed);
        let Ok(line) = line else {
            return Some(!first_to_move);
        };
        if !pos.legal_moves().contains(&line.mv) {
            return Some(!first_to_move);
        }
        pos.make_move(line.mv);
        moves.push(line.mv);
        if pos.legal_moves().is_empty() {
            return Some(first_to_move);
        }
    }
    // Cap reached: adjudicate by material (>= 300 wins).
    let diff = material(&pos);
    let first_diff = if first_red { diff } else { -diff };
    if first_diff >= 300 {
        Some(true)
    } else if first_diff <= -300 {
        Some(false)
    } else {
        None
    }
}

#[test]
#[ignore]
fn levels_ordered() {
    // 10 games Master vs Easy, fixed seed → Master wins >= 9.
    let mut master_wins = 0;
    for game in 0..10 {
        let master_red = game % 2 == 0;
        let result = play_game(
            Level::Master,
            Level::Easy,
            master_red,
            0x5eed + game as u64,
            60,
            1000,
        );
        if result == Some(true) {
            master_wins += 1;
        }
        eprintln!(
            "ván {game} (Cao thủ {}): {:?}",
            if master_red { "Đỏ" } else { "Đen" },
            result
        );
    }
    assert!(
        master_wins >= 9,
        "Cao thủ phải thắng >= 9/10 ván trước Dễ, thực tế {master_wins}/10"
    );
}

#[test]
#[ignore]
fn diversity_varies() {
    // 10 self-play Master games, different seeds → >= 5 distinct
    // 12-ply opening sequences.
    let mut sequences: Vec<Vec<Move>> = Vec::new();
    for game in 0..10 {
        let mut engine = real_engine();
        let mut pos = Position::startpos();
        let mut moves: Vec<Move> = Vec::new();
        for _ in 0..12 {
            let mut limits = xq_pikafish::pikafish_limits(Level::Master);
            limits.max_time_ms = Some(500);
            let req = EngineRequest::new(Position::startpos().to_fen(), moves.clone(), limits);
            let lines = engine.analyze(&req, 3, &mut |_| {}).expect("analyze được");
            let mut rng = xq_pikafish::GameRng::for_ply(
                0xbeef_u64.wrapping_add(game as u64 * 0x9e3779b97f4a7c15),
                moves.len(),
            );
            let mv = xq_pikafish::pick_diverse(&lines, xq_pikafish::DIVERSITY_MARGIN_CP, &mut rng)
                .or_else(|| lines.first().map(|l| l.mv))
                .expect("có nước đi");
            assert!(pos.legal_moves().contains(&mv));
            pos.make_move(mv);
            moves.push(mv);
            if pos.legal_moves().is_empty() {
                break;
            }
        }
        sequences.push(moves);
    }
    sequences.sort();
    sequences.dedup();
    eprintln!("số chuỗi mở đầu khác nhau: {}", sequences.len());
    assert!(
        sequences.len() >= 5,
        "10 ván seed khác nhau phải cho >= 5 chuỗi 12 nửa nước đầu khác nhau, thực tế {}",
        sequences.len()
    );
}

#[test]
#[ignore]
fn diversity_off_deterministic() {
    // Diversity off, same seed, `go depth 12` → same move. The pick is
    // not involved (diversity off means best move); determinism comes
    // from the depth-limited search itself (single thread).
    let mut first = None;
    for _ in 0..2 {
        let mut engine = real_engine();
        let req = EngineRequest::new(
            Position::startpos().to_fen(),
            Vec::new(),
            SearchLimits::depth(12),
        );
        let line = engine.best_move(&req, &mut |_| {}).expect("đi được");
        match first {
            None => first = Some(line.mv),
            Some(mv) => assert_eq!(
                mv, line.mv,
                "cùng seed, tắt đa dạng, go depth 12 phải cho cùng nước"
            ),
        }
    }
}
