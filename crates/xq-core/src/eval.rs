//! Static evaluation (§4.3, Appendix A).
//!
//! `eval(pos)` returns the score from the side-to-move's point of view,
//! in the same units as the material table (unadvanced pawn = 30).
//!
//! Components (all coefficients in [`EvalParams`]):
//! material (Cannon/Horse interpolated by game phase), pawn PST from
//! Appendix A, formula PSTs for Horse/Cannon/Rook/Advisor/Elephant/King,
//! king safety, rook mobility, tempo.

use crate::board::Position;
use crate::eval_params::EvalParams;
use crate::types::{file_of, rank_of, Color, PieceKind};

/// Mate score. A mate in `n` plies scores `MATE - n`.
pub const MATE: i32 = 30000;
/// Perpetual-check score used inside search (§4.2 item 10).
pub const PERPETUAL: i32 = 9000;

/// Middlegame material value of one piece (Appendix A).
#[must_use]
pub fn piece_value_mg(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::King => 0,
        PieceKind::Advisor => 120,
        PieceKind::Elephant => 120,
        PieceKind::Horse => 270,
        PieceKind::Rook => 600,
        PieceKind::Cannon => 285,
        PieceKind::Pawn => 30,
    }
}

/// Pawn PST for Red, indexed `[rank][file]` (Appendix A; ranks 0–2 are 0
/// because a Red pawn can never be there).
const PAWN_PST: [[i32; 9]; 10] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0],          // rank 0
    [0, 0, 0, 0, 0, 0, 0, 0, 0],          // rank 1
    [0, 0, 0, 0, 0, 0, 0, 0, 0],          // rank 2
    [0, 0, 0, 0, 0, 0, 0, 0, 0],          // rank 3
    [0, 0, -3, 0, 5, 0, -3, 0, 0],        // rank 4
    [20, 25, 30, 40, 45, 40, 30, 25, 20], // rank 5
    [25, 35, 45, 55, 60, 55, 45, 35, 25], // rank 6
    [30, 45, 55, 70, 75, 70, 55, 45, 30], // rank 7
    [30, 45, 60, 75, 80, 75, 60, 45, 30], // rank 8
    [5, 10, 15, 20, 25, 20, 15, 10, 5],   // rank 9
];

/// Horse PST rank component (Appendix A), indexed by Red-perspective rank.
const HORSE_RANK: [i32; 10] = [0, 2, 6, 10, 14, 16, 18, 14, 8, 0];

/// Positional bonus for one piece, in Red's perspective (`f` = file 0..8,
/// `r` = rank 0..9 as seen by Red). Black pieces call this with the
/// vertically flipped square (`r' = 9 - r`).
fn pst(kind: PieceKind, f: i32, r: i32) -> i32 {
    match kind {
        PieceKind::King => {
            if f == 4 && r == 0 {
                0
            } else if r == 0 {
                -8
            } else if r == 1 {
                -15
            } else if r == 2 {
                -25
            } else {
                0
            }
        }
        PieceKind::Advisor => {
            if f == 4 && r == 1 {
                5
            } else {
                0
            }
        }
        PieceKind::Elephant => {
            if f == 4 && r == 2 {
                5
            } else if r == 2 && (f == 0 || f == 8) {
                -2
            } else {
                0
            }
        }
        PieceKind::Horse => {
            let c = f.min(8 - f);
            let mut v = 4 * c + HORSE_RANK[r as usize];
            if f == 0 || f == 8 {
                v -= 15;
            }
            v
        }
        PieceKind::Rook => {
            let mut v = 0;
            if (f == 3 || f == 5) && r >= 5 {
                v += 6;
            }
            if r == 6 {
                v += 8;
            }
            v
        }
        PieceKind::Cannon => {
            let mut v = 0;
            if f == 4 && r <= 2 {
                v += 8;
            }
            if r == 2 {
                v += 4;
            }
            if r == 7 {
                v += 6;
            }
            v
        }
        PieceKind::Pawn => PAWN_PST[r as usize][f as usize],
    }
}

/// Count squares a rook on `sq` can move to (empty squares + first enemy
/// piece on each ray). Used for rook mobility.
fn rook_mobility(pos: &Position, sq: u8, color: Color) -> i32 {
    let f = file_of(sq);
    let r = rank_of(sq);
    let mut n = 0i32;
    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let mut nf = f + df;
        let mut nr = r + dr;
        while (0..9).contains(&nf) && (0..10).contains(&nr) {
            let s = (nr * 9 + nf) as u8;
            match pos.piece_at(s) {
                None => n += 1,
                Some(p) => {
                    if p.color != color {
                        n += 1;
                    }
                    break;
                }
            }
            nf += df;
            nr += dr;
        }
    }
    n
}

/// Evaluate `pos` from the side-to-move's point of view.
#[must_use]
pub fn eval(pos: &Position) -> i32 {
    eval_with_params(pos, &EvalParams::default())
}

/// Evaluate with explicit parameters.
#[must_use]
pub fn eval_with_params(pos: &Position, params: &EvalParams) -> i32 {
    // Game phase (§4.3 item 3): 4 per rook + 2 per horse/cannon, max 24.
    let rooks = pos.count_kind(PieceKind::Rook, None);
    let horses = pos.count_kind(PieceKind::Horse, None);
    let cannons = pos.count_kind(PieceKind::Cannon, None);
    let phase = (4 * rooks + 2 * (horses + cannons)).min(24) as i32;
    // Interpolated material values.
    let interp = |mg: i32, eg: i32| (mg * phase + eg * (24 - phase)) / 24;
    let rook_v = interp(params.rook_mg, params.rook_eg);
    let cannon_v = interp(params.cannon_mg, params.cannon_eg);
    let horse_v = interp(params.horse_mg, params.horse_eg);

    let mut score = [0i32; 2]; // [Red, Black] totals
    let mut guards = [0i32; 2]; // advisors + elephants per colour
    let mut attackers = [0i32; 2]; // R/H/C + crossed pawns per colour
    for s in 0..90u8 {
        let Some(p) = pos.piece_at(s) else {
            continue;
        };
        let ci = p.color.index();
        let f = file_of(s);
        let r = rank_of(s);
        // Red-perspective coordinates for the PST.
        let (pf, pr) = match p.color {
            Color::Red => (f, r),
            Color::Black => (f, 9 - r),
        };
        let mat = match p.kind {
            PieceKind::King => 0,
            PieceKind::Advisor => {
                guards[ci] += 1;
                params.advisor
            }
            PieceKind::Elephant => {
                guards[ci] += 1;
                params.elephant
            }
            PieceKind::Horse => {
                attackers[ci] += 1;
                horse_v
            }
            PieceKind::Rook => {
                attackers[ci] += 1;
                rook_v
            }
            PieceKind::Cannon => {
                attackers[ci] += 1;
                cannon_v
            }
            PieceKind::Pawn => {
                let crossed = match p.color {
                    Color::Red => r >= 5,
                    Color::Black => r <= 4,
                };
                if crossed {
                    attackers[ci] += 1;
                }
                params.pawn
            }
        };
        score[ci] += mat + pst(p.kind, pf, pr);
        if p.kind == PieceKind::Rook {
            score[ci] += params.rook_mobility * rook_mobility(pos, s, p.color);
        }
    }

    // King safety (§4.3 item 4).
    for c in [Color::Red, Color::Black] {
        let ci = c.index();
        let enemy = c.opposite();
        let ei = enemy.index();
        let missing = (4 - guards[ci]).max(0);
        if missing > 0 && attackers[ei] >= 2 {
            score[ci] -= params.missing_guard_penalty * missing;
        }
        // Head cannon: an enemy cannon in the king's file with no piece
        // between it and the king gives +30 to the cannon's owner. The
        // spec's wording ("cộng 30 cho Pháo địch ... cho bên có Pháo đó")
        // is implemented as a bonus to the side owning such a cannon.
        let king = pos.king_sq(c);
        let kf = file_of(king);
        let kr = rank_of(king);
        for s in 0..90u8 {
            if let Some(p) = pos.piece_at(s) {
                if p.kind == PieceKind::Cannon && p.color == enemy && file_of(s) == kf {
                    let cr = rank_of(s);
                    let (lo, hi) = if cr < kr { (cr, kr) } else { (kr, cr) };
                    let mut blocked = false;
                    for r in lo + 1..hi {
                        if pos.piece_at((r * 9 + kf) as u8).is_some() {
                            blocked = true;
                            break;
                        }
                    }
                    if !blocked {
                        score[ei] += params.head_cannon_bonus;
                    }
                }
            }
        }
    }

    let red_minus_black = score[0] - score[1];
    let signed = match pos.side() {
        Color::Red => red_minus_black,
        Color::Black => -red_minus_black,
    };
    signed + params.tempo
}
