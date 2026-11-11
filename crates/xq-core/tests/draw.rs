//! End-of-game and repetition tests (§8.5).

use xq_core::board::Position;
use xq_core::game::{Game, GameResult, Reason};
use xq_core::types::Move;

fn mv(ff: i32, fr: i32, tf: i32, tr: i32) -> Move {
    Move::new((fr * 9 + ff) as u8, (tr * 9 + tf) as u8)
}

#[test]
fn threefold_repetition_draw() {
    let mut game = Game::new(Position::startpos());
    // Two horses shuttle out and back, twice.
    let seq = [
        mv(7, 0, 6, 2), // h0g2
        mv(7, 9, 6, 7), // h9g7
        mv(6, 2, 7, 0), // g2h0
        mv(6, 7, 7, 9), // g7h9
    ];
    for _ in 0..2 {
        for &m in &seq {
            game.play(m).unwrap();
        }
    }
    assert_eq!(game.result(), GameResult::Draw(Reason::Repetition));
}

#[test]
fn perpetual_check_loses() {
    // Red rook checks along the e-file / d-file while the black king
    // shuttles e9 <-> d9. FEN: rook a2, black king e9, red king f0.
    //   1. Re2+ Kd9 2. Rd2+ Ke9 3. Re2+ Kd9 4. Rd2+ Ke9 5. Re2+
    // The position after the 5th Re2+ is the 3rd occurrence of its hash,
    // and Red checked on every own move of the cycle, so Red loses.
    let mut game = Game::new(Position::from_fen("4k4/9/9/9/9/9/9/R8/9/5K3 w - - 0 1").unwrap());
    let seq = [
        mv(0, 2, 4, 2), // Ra2-e2+
        mv(4, 9, 3, 9), // Ke9-d9
        mv(4, 2, 3, 2), // Re2-d2+
        mv(3, 9, 4, 9), // Kd9-e9
        mv(3, 2, 4, 2), // Rd2-e2+
        mv(4, 9, 3, 9), // Ke9-d9
        mv(4, 2, 3, 2), // Re2-d2+
        mv(3, 9, 4, 9), // Kd9-e9
        mv(3, 2, 4, 2), // Rd2-e2+ (3rd occurrence)
    ];
    for (i, &m) in seq.iter().enumerate() {
        game.play(m).unwrap();
        if i < seq.len() - 1 {
            assert_eq!(game.result(), GameResult::Ongoing, "ended early at ply {i}");
        }
    }
    assert_eq!(
        game.result(),
        GameResult::BlackWins(Reason::PerpetualCheck),
        "red checked perpetually, so red loses"
    );
}

#[test]
fn move_limit_draw() {
    // Kings inside their palaces (red e0, black f9); rooks walk disjoint
    // perimeter cycles — red 16 squares (files 0-4, ranks 0-4), black 14
    // squares (files 5-8, ranks 5-9) — so no rook ever lines up with the
    // enemy king, no position repeats (lcm(16, 14) = 112 pairs > 60),
    // and after 120 plies without a capture the game is a draw.
    let mut game = Game::new(Position::from_fen("5k3/9/9/9/5r3/9/9/9/9/R3K4 w - - 0 1").unwrap());
    let red: Vec<(i32, i32)> = vec![
        (0, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (3, 1),
        (4, 1),
        (4, 2),
        (4, 3),
        (4, 4),
        (3, 4),
        (2, 4),
        (1, 4),
        (0, 4),
        (0, 3),
        (0, 2),
        (0, 1),
    ];
    let black: Vec<(i32, i32)> = vec![
        (5, 5),
        (6, 5),
        (7, 5),
        (8, 5),
        (8, 6),
        (8, 7),
        (8, 8),
        (8, 9),
        (7, 9),
        (6, 9),
        (6, 8),
        (5, 8),
        (5, 7),
        (5, 6),
    ];
    assert_eq!(red[0], (0, 0));
    assert_eq!(black[0], (5, 5));
    let mut ri = 0usize;
    let mut bi = 0usize;
    for ply in 0..120 {
        let m = if ply % 2 == 0 {
            let (ff, fr) = red[ri];
            ri = (ri + 1) % red.len();
            let (tf, tr) = red[ri];
            mv(ff, fr, tf, tr)
        } else {
            let (ff, fr) = black[bi];
            bi = (bi + 1) % black.len();
            let (tf, tr) = black[bi];
            mv(ff, fr, tf, tr)
        };
        game.play(m)
            .unwrap_or_else(|_| panic!("ply {ply}: move {m:?} illegal in {}", game.pos.to_fen()));
        if ply < 119 {
            assert_eq!(
                game.result(),
                GameResult::Ongoing,
                "game ended early at ply {ply}: {:?}",
                game.result()
            );
        }
    }
    assert_eq!(game.result(), GameResult::Draw(Reason::MoveLimit));
}

#[test]
fn insufficient_material() {
    let game = Game::new(Position::from_fen("3k5/4a4/9/9/9/9/9/9/4A4/5K3 w - - 0 1").unwrap());
    assert_eq!(
        game.result(),
        GameResult::Draw(Reason::InsufficientMaterial)
    );
}

#[test]
fn undo_restores_result() {
    let game = Game::new(Position::from_fen("R2k5/R8/9/9/9/9/9/9/9/4K4 b - - 0 1").unwrap());
    assert_eq!(game.result(), GameResult::RedWins(Reason::Checkmate));
    // Undo is not possible (no moves played); instead play a mate then undo.
    let mut game2 = Game::new(Position::from_fen("3k5/R8/9/9/9/9/9/9/9/1R2K4 w - - 0 1").unwrap());
    game2.play(mv(1, 0, 1, 9)).unwrap(); // Rb0-b9 mate
    assert_eq!(game2.result(), GameResult::RedWins(Reason::Checkmate));
    game2.undo();
    assert_eq!(game2.result(), GameResult::Ongoing);
}
