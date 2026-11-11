//! Property tests (§8.3) with `proptest`.

use proptest::prelude::*;
use xq_core::board::Position;
use xq_core::eval;
use xq_core::types::{Color, PieceKind};
use xq_core::zobrist;

fn playout(seed: u64, max_plies: usize) -> Vec<Position> {
    let mut rng = seed;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut pos = Position::startpos();
    let mut out = vec![pos.clone()];
    for _ in 0..max_plies {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let m = moves[(next() % moves.len() as u64) as usize];
        let _ = pos.make_move(m);
        out.push(pos.clone());
    }
    out
}

proptest! {
    #[test]
    fn make_unmake_roundtrip(seed in 0u64..u64::MAX) {
        let mut rng = seed;
        let mut next = move || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng };
        let mut pos = Position::startpos();
        for _ in 0..200 {
            let moves = pos.legal_moves();
            if moves.is_empty() { break; }
            let m = moves[(next() % moves.len() as u64) as usize];
            let before = pos.clone();
            let u = pos.make_move(m);
            // Hash must stay incremental.
            prop_assert_eq!(pos.hash(), zobrist::compute(&pos));
            pos.unmake_move(m, u);
            prop_assert_eq!(&pos, &before);
        }
    }

    #[test]
    fn fen_roundtrip_property(seed in 0u64..u64::MAX) {
        for pos in playout(seed, 40) {
            let fen = pos.to_fen();
            let back = Position::from_fen(&fen).unwrap();
            prop_assert_eq!(back, pos);
        }
    }

    #[test]
    fn legal_never_leaves_king_attacked(seed in 0u64..u64::MAX) {
        for pos in playout(seed, 40) {
            let side = pos.side();
            for m in pos.legal_moves() {
                let mut p = pos.clone();
                p.make_move(m);
                prop_assert!(!p.in_check(side), "legal move leaves king in check: {}", p.to_fen());
            }
            // Every pseudo-legal move that was filtered out leaves the king attacked.
            let mut pseudo = Vec::new();
            pos.pseudo_legal_moves(&mut pseudo);
            let legal: std::collections::HashSet<_> = pos.legal_moves().into_iter().collect();
            for m in pseudo {
                if !legal.contains(&m) {
                    let mut p = pos.clone();
                    p.make_move(m);
                    prop_assert!(p.in_check(side), "filtered move does not leave king in check");
                }
            }
        }
    }

    #[test]
    fn gives_check_matches(seed in 0u64..u64::MAX) {
        for pos in playout(seed, 30) {
            for m in pos.legal_moves() {
                let mut p = pos.clone();
                p.make_move(m);
                prop_assert_eq!(pos.gives_check(m), p.in_check(p.side()));
            }
        }
    }
}

/// Mirror a position: swap colours, flip vertically, swap side to move.
fn mirror_vertical(pos: &Position) -> Position {
    let mut rows: Vec<String> = Vec::new();
    for rank in (0..10).rev() {
        let mut row = String::new();
        let mut empty = 0;
        for file in 0..9 {
            let src = ((9 - rank) * 9 + file) as u8;
            match pos.piece_at(src) {
                None => empty += 1,
                Some(p) => {
                    if empty > 0 {
                        row.push_str(&empty.to_string());
                        empty = 0;
                    }
                    let swapped = xq_core::Piece::new(p.color.opposite(), p.kind);
                    row.push(swapped.fen_char());
                }
            }
        }
        if empty > 0 {
            row.push_str(&empty.to_string());
        }
        rows.push(row);
    }
    let side = match pos.side() {
        Color::Red => "b",
        Color::Black => "w",
    };
    Position::from_fen(&format!("{} {side} - - 0 1", rows.join("/"))).unwrap()
}

fn mirror_horizontal(pos: &Position) -> Position {
    let mut rows: Vec<String> = Vec::new();
    for rank in (0..10).rev() {
        let mut row = String::new();
        let mut empty = 0;
        for file in (0..9).rev() {
            let src = (rank * 9 + file) as u8;
            match pos.piece_at(src) {
                None => empty += 1,
                Some(p) => {
                    if empty > 0 {
                        row.push_str(&empty.to_string());
                        empty = 0;
                    }
                    row.push(p.fen_char());
                }
            }
        }
        if empty > 0 {
            row.push_str(&empty.to_string());
        }
        rows.push(row);
    }
    let side = match pos.side() {
        Color::Red => "w",
        Color::Black => "b",
    };
    Position::from_fen(&format!("{} {side} - - 0 1", rows.join("/"))).unwrap()
}

#[test]
fn eval_mirror_symmetry() {
    for pos in playout(0xC0FFEE, 30) {
        assert_eq!(eval::eval(&pos), eval::eval(&mirror_vertical(&pos)));
        assert_eq!(eval::eval(&pos), eval::eval(&mirror_horizontal(&pos)));
    }
}

#[test]
fn eval_startpos_range() {
    let e = eval::eval(&Position::startpos());
    assert!((0..=20).contains(&e), "startpos eval {e} not in [0, 20]");
    // Material sanity: a lone extra rook is worth ~600.
    let pos = Position::from_fen("4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1").unwrap();
    let _ = pos.count_kind(PieceKind::Rook, Some(Color::Red));
    assert!(eval::eval(&pos) > 500);
}
