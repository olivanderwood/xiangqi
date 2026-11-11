//! Tunable evaluation parameters (§4.3). All coefficients live here so
//! tuning (T2.6) never touches evaluation logic.

/// Material values, middlegame / endgame (Appendix A).
#[derive(Clone, Copy, Debug)]
pub struct EvalParams {
    pub rook_mg: i32,
    pub rook_eg: i32,
    pub cannon_mg: i32,
    pub cannon_eg: i32,
    pub horse_mg: i32,
    pub horse_eg: i32,
    pub advisor: i32,
    pub elephant: i32,
    pub pawn: i32,
    /// King-safety: penalty per missing advisor/elephant while the enemy
    /// still has >= 2 attackers.
    pub missing_guard_penalty: i32,
    /// Bonus for an enemy cannon standing in the king's file with no
    /// piece between (no "head cannon" for the other side).
    pub head_cannon_bonus: i32,
    /// Rook mobility: points per square a rook can reach.
    pub rook_mobility: i32,
    /// Tempo bonus for the side to move.
    pub tempo: i32,
}

impl Default for EvalParams {
    fn default() -> Self {
        Self {
            rook_mg: 600,
            rook_eg: 600,
            cannon_mg: 285,
            cannon_eg: 250,
            horse_mg: 270,
            horse_eg: 300,
            advisor: 120,
            elephant: 120,
            pawn: 30,
            missing_guard_penalty: 15,
            head_cannon_bonus: 30,
            rook_mobility: 2,
            tempo: 10,
        }
    }
}

impl EvalParams {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
