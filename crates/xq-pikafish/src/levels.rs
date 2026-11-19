//! Difficulty levels for Pikafish (integration spec §4, rewritten).
//!
//! The real binary (2026-09-06) has NO `UCI_LimitStrength`, `UCI_Elo`
//! or `Skill Level` option (see `docs/research-pikafish-2026-09-06.md`
//! §4 and `docs/DECISIONS-pikafish.md` §13), so strength is limited
//! the only way the protocol allows: with the `go` limits themselves,
//! plus — for the low levels — by picking among the top lines instead
//! of always the best one (the old builtin weakened itself with score
//! noise; same purpose, different mechanism).
//!
//! Why weakening is needed at all: depth caps alone do NOT make
//! Pikafish weak. Measured (`tests/calibration.rs`,
//! `measure_level_strength`, 2026-11-11): even depth 2 wins 4/4 against
//! the builtin at Medium/100 ms, at ~0 ms/move. The NNUE evaluation is
//! so strong that a shallow Pikafish still plays near-perfect moves.
//! So Easy/Medium pick a worse line often (wide margin + flat
//! temperature), Hard plays its best move at depth 12, Master plays at
//! full strength with the §5 diversity pick in the opening.
//!
//! That was still not enough (Hieu, 2026-11-19: Easy plays like Hard).
//! Measured again (`measure_top3_gaps`, `measure_level_matches`,
//! 2026-11-19, 30 opening positions, 4 games per pair): the top-3
//! lines cluster tightly — median gap best→2nd is 29 cp, best→3rd is
//! 66 cp — so a pick among them costs only ~10–30 cp a move and never
//! hangs a piece; and Easy beat Medium 1/4 head-to-head. A level only
//! *feels* weaker when it sometimes makes a real mistake, so the low
//! levels now also blunder: a random legal move on a fraction of turns
//! (`Weaken::blunder_rate`), and the table grew to 8 UI levels (7 Elo
//! anchors + Master) so the steps between levels are visible.
//!
//! Calibration (`tests/calibration.rs`, 2026-11-11, startpos, 1 thread):
//! depth 4 ≈ 0–2 ms/move, depth 8 ≈ 0–15 ms, depth 12 ≈ 13–41 ms,
//! depth 16 ≈ 123–479 ms; Master `movetime 5000` is respected exactly
//! (4999 ms).

use xq_engine::{Level, SearchLimits, Weaken};

/// Full per-level configuration for one Pikafish move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PikafishLevel {
    /// `go` limits (depth cap + movetime safety valve).
    pub limits: SearchLimits,
    /// Pick among the top lines instead of the best one (`None` = best).
    pub weaken: Option<Weaken>,
}

/// Limits for one Pikafish move at a level.
#[must_use]
pub fn pikafish_limits(level: Level) -> SearchLimits {
    pikafish_level(level).limits
}

/// Full configuration for one Pikafish move at a level.
#[must_use]
pub fn pikafish_level(level: Level) -> PikafishLevel {
    match level {
        Level::Easy => anchor_level(ANCHORS[1]),
        Level::Medium => anchor_level(ANCHORS[2]),
        Level::Hard => anchor_level(ANCHORS[4]),
        Level::Master => PikafishLevel {
            limits: SearchLimits {
                max_depth: None,
                max_time_ms: Some(5000),
                max_nodes: None,
            },
            weaken: None,
        },
        Level::Elo(x) => elo_level(x),
    }
}

/// Anchors: (elo, depth, movetime_ms, margin_cp, temperature,
/// blunder_rate). The UI's 8 levels are fixed points on this table:
/// 0 → 1000 (Mới chơi), 1 → 1400 (Dễ), 2 → 1900 (Trung bình),
/// 3 → 2200 (Khá), 4 → 2400 (Mạnh), 5 → 2600 (Chuyên gia),
/// 6 → Master, 7 → custom Elo. The 2800 anchor is only the
/// interpolation endpoint — `Elo(x >= 2800)` plays as Master.
const ANCHORS: [(u16, u8, u64, i32, f64, f64); 7] = [
    (1000, 2, 500, 300, 100.0, 0.30),
    (1400, 3, 500, 200, 60.0, 0.15),
    (1900, 6, 1000, 100, 30.0, 0.06),
    (2200, 9, 1500, 60, 20.0, 0.02),
    (2400, 12, 2000, 0, 10.0, 0.00),
    (2600, 16, 3000, 0, 10.0, 0.00),
    (2800, 20, 5000, 0, 10.0, 0.00),
];

/// One anchor row as a full level config.
fn anchor_level(a: (u16, u8, u64, i32, f64, f64)) -> PikafishLevel {
    let (_, depth, movetime, margin, temperature, blunder_rate) = a;
    PikafishLevel {
        limits: SearchLimits {
            max_depth: Some(depth),
            max_time_ms: Some(movetime),
            max_nodes: None,
        },
        weaken: if margin > 0 {
            Some(Weaken {
                margin_cp: margin,
                temperature,
                blunder_rate,
            })
        } else {
            None
        },
    }
}

/// `Elo(x)` → (depth, movetime, weaken) by linear interpolation
/// between the table's anchors. Below 1000 plays as the lowest
/// anchor, at/above 2800 plays as Master.
fn elo_level(x: u16) -> PikafishLevel {
    if x <= ANCHORS[0].0 {
        return anchor_level(ANCHORS[0]);
    }
    if x >= ANCHORS[6].0 {
        return pikafish_level(Level::Master);
    }
    for w in ANCHORS.windows(2) {
        let (x0, d0, t0, m0, temp0, b0) = w[0];
        let (x1, d1, t1, m1, temp1, b1) = w[1];
        if x >= x0 && x <= x1 {
            let f = f64::from(x - x0) / f64::from(x1 - x0);
            let lerp = |a: f64, b: f64| a + f * (b - a);
            let depth = lerp(f64::from(d0), f64::from(d1)).round() as u8;
            let movetime = lerp(t0 as f64, t1 as f64).round() as u64;
            let margin = lerp(f64::from(m0), f64::from(m1)).round() as i32;
            let temperature = lerp(temp0, temp1);
            let blunder_rate = lerp(b0, b1).clamp(0.0, 1.0);
            return PikafishLevel {
                limits: SearchLimits {
                    max_depth: Some(depth.clamp(2, 20)),
                    max_time_ms: Some(movetime.clamp(500, 5000)),
                    max_nodes: None,
                },
                weaken: if margin > 0 {
                    Some(Weaken {
                        margin_cp: margin,
                        temperature,
                        blunder_rate,
                    })
                } else {
                    None
                },
            };
        }
    }
    pikafish_level(Level::Medium)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_anchors() {
        let easy = pikafish_level(Level::Easy);
        assert_eq!(easy.limits.max_depth, Some(3));
        assert_eq!(easy.limits.max_time_ms, Some(500));
        let w = easy.weaken.expect("Easy weakens");
        assert_eq!(w.margin_cp, 200);
        assert_eq!(w.temperature, 60.0);
        assert_eq!(w.blunder_rate, 0.15);
        let medium = pikafish_level(Level::Medium);
        assert_eq!(medium.limits.max_depth, Some(6));
        let w = medium.weaken.expect("Medium weakens");
        assert_eq!(w.margin_cp, 100);
        assert_eq!(w.temperature, 30.0);
        assert_eq!(w.blunder_rate, 0.06);
        let hard = pikafish_level(Level::Hard);
        assert_eq!(hard.limits.max_depth, Some(12));
        assert!(hard.weaken.is_none());
        let master = pikafish_level(Level::Master);
        assert_eq!(master.limits.max_depth, None);
        assert_eq!(master.limits.max_time_ms, Some(5000));
        assert!(master.weaken.is_none());
        // New lowest anchor: genuinely weak (depth 2 + 30% blunders).
        let beginner = pikafish_level(Level::Elo(1000));
        assert_eq!(beginner.limits.max_depth, Some(2));
        let w = beginner.weaken.expect("Beginner weakens");
        assert_eq!(w.margin_cp, 300);
        assert_eq!(w.blunder_rate, 0.30);
        // New middle anchors.
        let fairly_strong = pikafish_level(Level::Elo(2200));
        assert_eq!(fairly_strong.limits.max_depth, Some(9));
        assert!(fairly_strong.weaken.is_some());
        let expert = pikafish_level(Level::Elo(2600));
        assert_eq!(expert.limits.max_depth, Some(16));
        assert!(expert.weaken.is_none());
    }

    #[test]
    fn elo_interpolation() {
        // Anchors map exactly.
        assert_eq!(
            pikafish_level(Level::Elo(1400)),
            pikafish_level(Level::Easy)
        );
        assert_eq!(
            pikafish_level(Level::Elo(1900)),
            pikafish_level(Level::Medium)
        );
        assert_eq!(
            pikafish_level(Level::Elo(2400)),
            pikafish_level(Level::Hard)
        );
        // Between anchors: depth and time rise, weakening falls.
        let d1650 = pikafish_level(Level::Elo(1650));
        let d2150 = pikafish_level(Level::Elo(2150));
        assert_eq!(d1650.limits.max_depth, Some(5));
        assert_eq!(d2150.limits.max_depth, Some(9));
        assert!(d1650.limits.max_time_ms < d2150.limits.max_time_ms);
        assert!(d1650.weaken.unwrap().margin_cp > d2150.weaken.unwrap().margin_cp);
        assert!(d1650.weaken.unwrap().blunder_rate > d2150.weaken.unwrap().blunder_rate);
        // Clamps: below the table plays as the lowest anchor, above
        // as Master.
        let lowest = pikafish_level(Level::Elo(1000));
        assert_eq!(pikafish_level(Level::Elo(800)), lowest);
        assert_eq!(lowest.limits.max_depth, Some(2));
        assert_eq!(
            pikafish_level(Level::Elo(3200)),
            pikafish_level(Level::Master)
        );
    }
}
