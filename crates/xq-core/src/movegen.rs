//! Move-generation helpers.
//!
//! The generators themselves live on [`crate::board::Position`]
//! (`pseudo_legal_moves`, `captures`, `legal_moves`); this module holds
//! the perft counter used by the tests (§8.1).

use crate::board::Position;

/// Count leaf nodes to depth `d` (perft). Uses make/unmake in place.
pub fn perft(pos: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let mut work = pos.clone();
    perft_rec(&mut work, depth)
}

fn perft_rec(work: &mut Position, depth: u32) -> u64 {
    let mut moves = Vec::new();
    work.legal_moves_into(&mut moves);
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0u64;
    for m in moves {
        let u = work.make_move(m);
        nodes += perft_rec(work, depth - 1);
        work.unmake_move(m, u);
    }
    nodes
}

/// Perft divide: node counts per root move (for debugging, §8.1).
pub fn perft_divide(pos: &Position, depth: u32) -> Vec<(crate::types::Move, u64)> {
    let mut out = Vec::new();
    let mut work = pos.clone();
    let mut moves = Vec::new();
    work.legal_moves_into(&mut moves);
    for m in moves {
        let u = work.make_move(m);
        out.push((m, perft_rec(&mut work, depth - 1)));
        work.unmake_move(m, u);
    }
    out
}
