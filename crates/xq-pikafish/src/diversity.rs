//! Diverse move picking at Master level (integration spec §5).
//!
//! When diversity is on, the app asks the engine for its top 3 lines
//! and picks one at random, weighted towards the best line. The pick is
//! a pure function of (lines, margin, seed, ply), so a replayed game
//! with the same seed reproduces the same picks — including after an
//! undo, because the RNG is re-derived per ply instead of advancing a
//! shared stream.

use xq_core::types::Move;
use xq_engine::{Line, Score};

/// Candidates within this many centipawns of the best line are eligible
/// for a diverse pick. Starting value from the spec; validated by
/// measurement in `tests/calibration.rs` (`measure_top3_gaps`,
/// 2026-11-11): at 30 opening positions the 2nd line is within 20 cp
/// in 47% of positions and within 50 cp in 53% — the distribution is
/// polarised, so 20 already captures nearly all genuinely close
/// alternatives without ever reaching for a clearly worse move.
pub const DIVERSITY_MARGIN_CP: i32 = 20;

/// Opening plies (half-moves) in which diversity applies.
pub const DIVERSITY_PLIES: usize = 24;

/// Deterministic RNG (SplitMix64). One stream per (game seed, ply).
#[derive(Clone, Copy, Debug)]
pub struct GameRng(u64);

impl GameRng {
    /// RNG for one ply of one game: the game seed mixed with the ply
    /// number, so picks do not depend on how many picks happened before.
    #[must_use]
    pub fn for_ply(game_seed: u64, ply: usize) -> Self {
        Self(
            game_seed
                .wrapping_add(ply as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15),
        )
    }

    /// Next uniform `f64` in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Top 53 bits → [0, 1).
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Pick a move from `lines` (best first) with weights
/// `w = exp((score − best) / temperature)`, restricted to lines within
/// `margin_cp` of the best. Returns `None` when `lines` is empty.
/// Lines whose score is `Mate` are never diluted: a mating best line
/// is always picked on its own (the caller skips diversity entirely
/// when the best line mates).
#[must_use]
pub fn pick_weighted(
    lines: &[Line],
    margin_cp: i32,
    temperature: f64,
    rng: &mut GameRng,
) -> Option<Move> {
    if lines.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Line> = lines.iter().collect();
    sorted.sort_by_key(|l| std::cmp::Reverse(l.score.to_cp_clamped()));
    let best = sorted[0].score.to_cp_clamped();
    if matches!(sorted[0].score, Score::Mate(_)) {
        return Some(sorted[0].mv);
    }
    let mut candidates: Vec<(&Line, f64)> = Vec::new();
    let mut total = 0.0;
    for line in sorted.iter().copied() {
        let cp = line.score.to_cp_clamped();
        if best - cp > margin_cp {
            break;
        }
        let w = ((f64::from(cp - best)) / temperature).exp();
        candidates.push((line, w));
        total += w;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].0.mv);
    }
    let mut pick = rng.next_f64() * total;
    for (line, w) in candidates {
        pick -= w;
        if pick <= 0.0 {
            return Some(line.mv);
        }
    }
    Some(sorted[0].mv)
}

/// The §5 diversity pick: [`pick_weighted`] with the spec's margin and
/// temperature (10).
#[must_use]
pub fn pick_diverse(lines: &[Line], margin_cp: i32, rng: &mut GameRng) -> Option<Move> {
    pick_weighted(lines, margin_cp, 10.0, rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::notation::parse_iccs;
    use xq_engine::Score;

    fn line(mv: &str, cp: i32) -> Line {
        Line {
            mv: parse_iccs(mv).unwrap(),
            score: Score::Cp(cp),
            depth: 10,
            pv: Vec::new(),
        }
    }

    #[test]
    fn deterministic_for_same_seed_and_ply() {
        let lines = vec![line("h2e2", 50), line("h0g2", 40), line("b2b3", 10)];
        let a = pick_diverse(&lines, 20, &mut GameRng::for_ply(42, 3));
        let b = pick_diverse(&lines, 20, &mut GameRng::for_ply(42, 3));
        assert_eq!(a, b);
        let c = pick_diverse(&lines, 20, &mut GameRng::for_ply(43, 3));
        // A different seed may pick differently; it must still be a candidate.
        assert!(c.is_some());
    }

    #[test]
    fn margin_excludes_far_lines() {
        // Only the best line is within a 5 cp margin.
        let lines = vec![line("h2e2", 50), line("h0g2", 40), line("b2b3", 10)];
        let picked = pick_diverse(&lines, 5, &mut GameRng::for_ply(1, 0));
        assert_eq!(picked, Some(parse_iccs("h2e2").unwrap()));
    }

    #[test]
    fn mate_best_line_is_always_picked() {
        let lines = vec![
            Line {
                mv: parse_iccs("b0b9").unwrap(),
                score: Score::Mate(1),
                depth: 10,
                pv: Vec::new(),
            },
            line("h2e2", 500),
        ];
        let picked = pick_diverse(&lines, 20, &mut GameRng::for_ply(7, 0));
        assert_eq!(picked, Some(parse_iccs("b0b9").unwrap()));
    }

    #[test]
    fn empty_lines_pick_nothing() {
        assert_eq!(pick_diverse(&[], 20, &mut GameRng::for_ply(1, 0)), None);
    }

    #[test]
    fn weighted_pick_favours_the_best() {
        // Gap 1 cp: weights 1.0 vs 0.90 — the best line wins most draws,
        // but the second line is reachable; over 200 seeds both appear.
        let lines = vec![line("h2e2", 50), line("h0g2", 49)];
        let mut best_count = 0;
        let mut second_count = 0;
        for seed in 0..200u64 {
            match pick_diverse(&lines, 20, &mut GameRng::for_ply(seed, 0)) {
                Some(mv) if mv == parse_iccs("h2e2").unwrap() => best_count += 1,
                Some(mv) if mv == parse_iccs("h0g2").unwrap() => second_count += 1,
                other => panic!("unexpected pick {other:?}"),
            }
        }
        assert!(
            best_count > second_count,
            "best {best_count} vs 2nd {second_count}"
        );
        assert!(
            second_count > 20,
            "2nd line must be reachable, got {second_count}"
        );
    }
}
