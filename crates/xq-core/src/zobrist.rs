//! Zobrist hashing: deterministic keys (fixed seed, SplitMix64).

use crate::board::Position;
use crate::types::{Color, PieceKind};

struct Keys {
    /// `table[color][kind][square]`.
    table: [[[u64; 90]; 7]; 2],
    side: u64,
}

static KEYS: std::sync::OnceLock<Keys> = std::sync::OnceLock::new();

fn kind_index(k: PieceKind) -> usize {
    match k {
        PieceKind::King => 0,
        PieceKind::Advisor => 1,
        PieceKind::Elephant => 2,
        PieceKind::Horse => 3,
        PieceKind::Rook => 4,
        PieceKind::Cannon => 5,
        PieceKind::Pawn => 6,
    }
}

fn keys() -> &'static Keys {
    KEYS.get_or_init(|| {
        // SplitMix64 with a fixed seed — deterministic across runs/platforms.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let mut table = [[[0u64; 90]; 7]; 2];
        for color_table in table.iter_mut() {
            for kind_table in color_table.iter_mut() {
                for slot in kind_table.iter_mut() {
                    *slot = next();
                }
            }
        }
        Keys {
            table,
            side: next(),
        }
    })
}

#[must_use]
pub fn piece_key(color: Color, kind: PieceKind, square: u8) -> u64 {
    keys().table[color.index()][kind_index(kind)][square as usize]
}

#[must_use]
pub fn side_key() -> u64 {
    keys().side
}

/// Full recomputation of a position's hash (used by property tests).
#[must_use]
pub fn compute(pos: &Position) -> u64 {
    let mut h: u64 = 0;
    for s in 0..90u8 {
        if let Some(p) = pos.piece_at(s) {
            h ^= piece_key(p.color, p.kind, s);
        }
    }
    if pos.side() == Color::Black {
        h ^= side_key();
    }
    h
}
