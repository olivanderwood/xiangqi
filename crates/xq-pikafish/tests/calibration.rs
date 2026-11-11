//! Calibration measurements for P6 (integration spec §4/§5/§6).
//!
//! All tests are `#[ignore]`: they need the real Pikafish binary and
//! take minutes. They PRINT measurement tables (via `eprintln!`, run
//! with `--nocapture`) instead of asserting tight bounds; the tables go
//! into `docs/DECISIONS-pikafish.md` and decide the level table,
//! `DIVERSITY_MARGIN_CP` and the `GradeThresholds`.
//!
//! Run: `XQ_PIKAFISH_PATH=$PWD/engine/Pikafish-Linux-x86-64-universal \
//!   cargo test --release -p xq-pikafish --test calibration -- --ignored --nocapture`

use std::path::PathBuf;
use std::time::{Duration, Instant};
use xq_core::board::Position;
use xq_core::types::{Color, Move, PieceKind};
use xq_engine::{BuiltinEngine, Engine, EngineRequest, Level, SearchLimits};
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

/// 30 opening positions: 5 seeded random games, snapshots at plies
/// 4, 6, 8, 10, 12, 14.
fn opening_positions() -> Vec<(String, Vec<Move>)> {
    let mut out = Vec::new();
    for game in 0..5 {
        let mut rng = Rng(0x1234_5678_9abc_def0u64.wrapping_add(game * 0x9e3779b97f4a7c15));
        let mut pos = Position::startpos();
        let mut moves: Vec<Move> = Vec::new();
        for ply in 1..=14 {
            let legal = pos.legal_moves();
            if legal.is_empty() {
                break;
            }
            let mv = legal[(rng.next() as usize) % legal.len()];
            pos.make_move(mv);
            moves.push(mv);
            if [4, 6, 8, 10, 12, 14].contains(&ply) {
                out.push((Position::startpos().to_fen(), moves.clone()));
            }
        }
    }
    assert_eq!(out.len(), 30, "phải có đúng 30 thế");
    out
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

/// §5/§6 calibration: top-3 score gaps at 30 opening positions.
#[test]
#[ignore]
fn measure_top3_gaps() {
    let mut engine = real_engine();
    let positions = opening_positions();
    let mut gap12: Vec<i32> = Vec::new();
    let mut gap13: Vec<i32> = Vec::new();
    let mut mates = 0;
    eprintln!("ply | best | 2nd | 3rd | gap12 | gap13");
    for (i, (fen, moves)) in positions.iter().enumerate() {
        let req = EngineRequest::new(fen.clone(), moves.clone(), SearchLimits::time_ms(1000));
        let lines = engine
            .analyze(&req, 3, &mut |_| {})
            .unwrap_or_else(|e| panic!("analyze thế {i} lỗi: {e}"));
        if lines.is_empty() {
            continue;
        }
        let cps: Vec<i32> = lines.iter().map(|l| l.score.to_cp_clamped()).collect();
        if matches!(lines[0].score, xq_engine::Score::Mate(_)) {
            mates += 1;
        }
        let g12 = cps.first().copied().unwrap_or(0) - cps.get(1).copied().unwrap_or(0);
        let g13 = cps.first().copied().unwrap_or(0) - cps.get(2).copied().unwrap_or(0);
        gap12.push(g12);
        gap13.push(g13);
        eprintln!(
            "{:>3} | {:>5} | {:>5} | {:>5} | {:>5} | {:>5}",
            moves.len(),
            cps[0],
            cps.get(1).copied().unwrap_or(0),
            cps.get(2).copied().unwrap_or(0),
            g12,
            g13
        );
    }
    gap12.sort_unstable();
    gap13.sort_unstable();
    let median = |v: &[i32]| v[v.len() / 2];
    let frac = |v: &[i32], t: i32| v.iter().filter(|&&g| g <= t).count() as f64 / v.len() as f64;
    eprintln!("--- tóm tắt (n={}, mate-best={}) ---", gap12.len(), mates);
    eprintln!(
        "gap12: trung vị={} p25={} p75={} | <=20: {:.0}% <=30: {:.0}% <=50: {:.0}%",
        median(&gap12),
        gap12[gap12.len() / 4],
        gap12[gap12.len() * 3 / 4],
        frac(&gap12, 20) * 100.0,
        frac(&gap12, 30) * 100.0,
        frac(&gap12, 50) * 100.0
    );
    eprintln!(
        "gap13: trung vị={} p25={} p75={} | <=20: {:.0}% <=30: {:.0}% <=50: {:.0}%",
        median(&gap13),
        gap13[gap13.len() / 4],
        gap13[gap13.len() * 3 / 4],
        frac(&gap13, 20) * 100.0,
        frac(&gap13, 30) * 100.0,
        frac(&gap13, 50) * 100.0
    );
}

/// §4 calibration: Pikafish at depth caps vs the builtin (100 ms/move).
/// 4 games per depth, alternating colours, cap 60 plies, material
/// adjudication (>= 300 wins). Also prints Pikafish's mean time/move.
#[test]
#[ignore]
fn measure_level_strength() {
    for depth in [2u8, 3, 4, 8, 12] {
        let mut wins = 0;
        let mut pikafish_ms = 0u128;
        let mut pikafish_moves = 0u32;
        for game in 0..4 {
            let pikafish_red = game % 2 == 0;
            let mut pikafish = real_engine();
            let mut builtin = BuiltinEngine::with_level(Level::Medium);
            let mut pos = Position::startpos();
            let mut moves: Vec<Move> = Vec::new();
            let mut result: Option<bool> = None;
            for _ in 0..60 {
                let red_to_move = pos.side() == Color::Red;
                let pikafish_to_move = red_to_move == pikafish_red;
                let req = if pikafish_to_move {
                    EngineRequest::new(
                        Position::startpos().to_fen(),
                        moves.clone(),
                        SearchLimits {
                            max_depth: Some(depth),
                            max_time_ms: Some(5000),
                            max_nodes: None,
                        },
                    )
                } else {
                    EngineRequest::new(
                        Position::startpos().to_fen(),
                        moves.clone(),
                        SearchLimits::time_ms(100),
                    )
                };
                let t0 = Instant::now();
                let line = if pikafish_to_move {
                    pikafish.best_move(&req, &mut |_| {})
                } else {
                    builtin.best_move(&req, &mut |_| {})
                };
                if pikafish_to_move {
                    pikafish_ms += t0.elapsed().as_millis();
                    pikafish_moves += 1;
                }
                let Ok(line) = line else {
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
            if won {
                wins += 1;
            }
        }
        eprintln!(
            "depth {depth}: thắng {wins}/4 vs Builtin(Medium, 100ms) | trung bình {} ms/nước",
            if pikafish_moves > 0 {
                pikafish_ms / u128::from(pikafish_moves)
            } else {
                0
            }
        );
    }
}

/// Time per move at depth caps (startpos, 5 samples each).
#[test]
#[ignore]
fn measure_depth_timing() {
    let mut engine = real_engine();
    for depth in [4u8, 8, 12, 16] {
        let mut times = Vec::new();
        for _ in 0..5 {
            let t0 = Instant::now();
            let req = EngineRequest::new(
                Position::startpos().to_fen(),
                Vec::new(),
                SearchLimits {
                    max_depth: Some(depth),
                    max_time_ms: Some(30_000),
                    max_nodes: None,
                },
            );
            let line = engine.best_move(&req, &mut |_| {}).expect("đi được");
            assert!(Position::startpos().legal_moves().contains(&line.mv));
            times.push(t0.elapsed().as_millis());
        }
        times.sort_unstable();
        eprintln!("depth {depth}: ms/nước (5 mẫu, đã sắp) = {times:?}");
    }
    // Master (no depth cap) with the 5 s movetime.
    let t0 = Instant::now();
    let req = EngineRequest::new(
        Position::startpos().to_fen(),
        Vec::new(),
        SearchLimits::time_ms(5000),
    );
    engine.best_move(&req, &mut |_| {}).expect("đi được");
    eprintln!(
        "master movetime 5000: thực tế {} ms",
        t0.elapsed().as_millis()
    );
    let _ = Duration::from_secs(0);
}

/// One move at a configured level: best move, or analyze top 3 +
/// weighted pick when the level weakens (same logic as the app's
/// `VsAiState::pick_from_lines`).
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

/// §4 validation: configured levels against each other. 4 games per
/// pair, alternating colours, cap 40 plies, material adjudication
/// (>= 300 wins). Master runs at 1000 ms/move here (test scale-down
/// from 5000 ms; still uncapped depth, so still full strength).
#[test]
#[ignore]
fn measure_level_matches() {
    let pairs = [
        (Level::Easy, Level::Medium),
        (Level::Medium, Level::Hard),
        (Level::Hard, Level::Master),
    ];
    for (weak, strong) in pairs {
        let mut weak_wins = 0;
        for game in 0..4 {
            let weak_red = game % 2 == 0;
            let mut engine_weak = real_engine();
            let mut engine_strong = real_engine();
            let mut pos = Position::startpos();
            let mut moves: Vec<Move> = Vec::new();
            let mut result: Option<bool> = None; // Some(true) = weak won
            for _ in 0..40 {
                let red_to_move = pos.side() == Color::Red;
                let weak_to_move = red_to_move == weak_red;
                let (level, seed) = if weak_to_move {
                    (weak, 0x1000 + game as u64)
                } else {
                    (strong, 0x2000 + game as u64)
                };
                let mut req = EngineRequest::new(
                    Position::startpos().to_fen(),
                    moves.clone(),
                    xq_pikafish::pikafish_limits(level),
                );
                if level == Level::Master {
                    req.limits.max_time_ms = Some(1000);
                }
                let line = if weak_to_move {
                    level_move(&mut engine_weak, level, &req, seed)
                } else {
                    level_move(&mut engine_strong, level, &req, seed)
                };
                let Ok(line) = line else {
                    result = Some(!weak_to_move);
                    break;
                };
                if !pos.legal_moves().contains(&line.mv) {
                    result = Some(!weak_to_move);
                    break;
                }
                pos.make_move(line.mv);
                moves.push(line.mv);
                if pos.legal_moves().is_empty() {
                    result = Some(weak_to_move);
                    break;
                }
            }
            let weak_won = match result {
                Some(w) => w,
                None => {
                    let diff = material(&pos);
                    let weak_diff = if weak_red { diff } else { -diff };
                    weak_diff >= 300
                }
            };
            if weak_won {
                weak_wins += 1;
            }
        }
        eprintln!("{weak:?} vs {strong:?}: yếu thắng {weak_wins}/4");
    }
}
