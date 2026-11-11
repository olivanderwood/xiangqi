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
        Level::Easy => PikafishLevel {
            limits: SearchLimits {
                max_depth: Some(3),
                max_time_ms: Some(500),
                max_nodes: None,
            },
            weaken: Some(Weaken {
                margin_cp: 150,
                temperature: 50.0,
            }),
        },
        Level::Medium => PikafishLevel {
            limits: SearchLimits {
                max_depth: Some(6),
                max_time_ms: Some(1000),
                max_nodes: None,
            },
            weaken: Some(Weaken {
                margin_cp: 80,
                temperature: 25.0,
            }),
        },
        Level::Hard => PikafishLevel {
            limits: SearchLimits {
                max_depth: Some(12),
                max_time_ms: Some(2000),
                max_nodes: None,
            },
            weaken: None,
        },
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

/// `Elo(x)` → (depth, movetime, weaken) by linear interpolation
/// between the table's anchors. Below 1400 clamps to Easy, at/above
/// 2800 plays as Master.
fn elo_level(x: u16) -> PikafishLevel {
    // Anchors: (elo, depth, movetime_ms, margin_cp, temperature).
    const ANCHORS: [(u16, u8, u64, i32, f64); 4] = [
        (1400, 3, 500, 150, 50.0),
        (1900, 6, 1000, 80, 25.0),
        (2400, 12, 2000, 0, 10.0),
        (2800, 20, 5000, 0, 10.0),
    ];
    if x <= ANCHORS[0].0 {
        return pikafish_level(Level::Easy);
    }
    if x >= ANCHORS[3].0 {
        return pikafish_level(Level::Master);
    }
    for w in ANCHORS.windows(2) {
        let (x0, d0, t0, m0, temp0) = w[0];
        let (x1, d1, t1, m1, temp1) = w[1];
        if x >= x0 && x <= x1 {
            let f = f64::from(x - x0) / f64::from(x1 - x0);
            let lerp = |a: f64, b: f64| a + f * (b - a);
            let depth = lerp(f64::from(d0), f64::from(d1)).round() as u8;
            let movetime = lerp(t0 as f64, t1 as f64).round() as u64;
            let margin = lerp(f64::from(m0), f64::from(m1)).round() as i32;
            let temperature = lerp(temp0, temp1);
            return PikafishLevel {
                limits: SearchLimits {
                    max_depth: Some(depth.clamp(3, 20)),
                    max_time_ms: Some(movetime.clamp(500, 5000)),
                    max_nodes: None,
                },
                weaken: if margin > 0 {
                    Some(Weaken {
                        margin_cp: margin,
                        temperature,
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
        assert!(easy.weaken.is_some());
        let medium = pikafish_level(Level::Medium);
        assert_eq!(medium.limits.max_depth, Some(6));
        assert!(medium.weaken.is_some());
        let hard = pikafish_level(Level::Hard);
        assert_eq!(hard.limits.max_depth, Some(12));
        assert!(hard.weaken.is_none());
        let master = pikafish_level(Level::Master);
        assert_eq!(master.limits.max_depth, None);
        assert_eq!(master.limits.max_time_ms, Some(5000));
        assert!(master.weaken.is_none());
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
        // Clamps.
        assert_eq!(
            pikafish_level(Level::Elo(1000)),
            pikafish_level(Level::Easy)
        );
        assert_eq!(
            pikafish_level(Level::Elo(3200)),
            pikafish_level(Level::Master)
        );
    }
}
