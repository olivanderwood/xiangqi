//! Rule tests (§8.2): each test checks the *set* of moves (ICCS).

use std::collections::BTreeSet;
use xq_core::board::Position;
use xq_core::game::{Game, GameResult, Reason};
use xq_core::notation::iccs;
use xq_core::types::{file_of, rank_of, Move};

fn moves_of(pos: &Position, from: u8) -> BTreeSet<String> {
    pos.legal_moves()
        .into_iter()
        .filter(|m| m.from == from)
        .map(iccs)
        .collect()
}

fn sq(file: i32, rank: i32) -> u8 {
    (rank * 9 + file) as u8
}

#[test]
fn horse_leg_blocked() {
    let pos = Position::startpos();
    // Horse b0: b0d1 is blocked by the elephant on c0 (leg).
    assert_eq!(
        moves_of(&pos, sq(1, 0)),
        ["b0a2", "b0c2"].into_iter().map(String::from).collect()
    );
}

#[test]
fn elephant_eye_and_river() {
    let pos = Position::from_fen("3k5/9/9/9/9/2B6/9/9/9/4K4 w - - 0 1").unwrap();
    assert_eq!(
        moves_of(&pos, sq(2, 4)),
        ["c4a2", "c4e2"].into_iter().map(String::from).collect()
    );
}

#[test]
fn elephant_start() {
    let pos = Position::startpos();
    assert_eq!(
        moves_of(&pos, sq(2, 0)),
        ["c0a2", "c0e2"].into_iter().map(String::from).collect()
    );
}

#[test]
fn cannon_needs_screen() {
    let pos = Position::from_fen("5k3/9/4r4/9/4p4/9/9/4C4/9/3K5 w - - 0 1").unwrap();
    let moves = moves_of(&pos, sq(4, 2));
    assert!(
        moves.contains("e2e7"),
        "cannon must capture the rook over the pawn screen: {moves:?}"
    );
    assert!(
        !moves.contains("e2e5"),
        "cannon cannot capture adjacent piece without a screen"
    );
    assert!(
        !moves.contains("e2e6"),
        "cannon cannot jump over the screen without capturing"
    );
    assert!(!moves.contains("e2e8"));
}

#[test]
fn flying_general_pins_rook() {
    let pos = Position::from_fen("4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1").unwrap();
    let moves = moves_of(&pos, sq(4, 1));
    let expected: BTreeSet<String> = (2..=8).map(|r| format!("e1e{r}")).collect();
    assert_eq!(moves, expected);
}

#[test]
fn pawn_before_river() {
    let pos = Position::startpos();
    assert_eq!(
        moves_of(&pos, sq(0, 3)),
        ["a3a4"].into_iter().map(String::from).collect()
    );
}

#[test]
fn pawn_after_river() {
    let pos = Position::from_fen("3k5/9/9/9/4P4/9/9/9/9/4K4 w - - 0 1").unwrap();
    assert_eq!(
        moves_of(&pos, sq(4, 5)),
        ["e5e6", "e5d5", "e5f5"]
            .into_iter()
            .map(String::from)
            .collect()
    );
}

#[test]
fn advisor_palace() {
    let pos = Position::startpos();
    assert_eq!(
        moves_of(&pos, sq(3, 0)),
        ["d0e1"].into_iter().map(String::from).collect()
    );
}

#[test]
fn king_palace() {
    let pos = Position::startpos();
    assert_eq!(
        moves_of(&pos, sq(4, 0)),
        ["e0e1"].into_iter().map(String::from).collect()
    );
}

#[test]
fn checkmate_two_rooks() {
    let pos = Position::from_fen("R2k5/R8/9/9/9/9/9/9/9/4K4 b - - 0 1").unwrap();
    assert!(pos.legal_moves().is_empty());
    assert!(pos.in_check(xq_core::Color::Black));
    let game = Game::new(pos);
    assert_eq!(game.result(), GameResult::RedWins(Reason::Checkmate));
}

#[test]
fn stalemate_loses() {
    // NOTE (see docs/DECISIONS.md): the FEN printed in the spec §8.2 for
    // this test is wrong — in `3k5/9/3P5/9/9/9/9/9/4R4/5K3` the black king
    // can walk to c9 and d8, so it is not stalemate. This corrected
    // position is a genuine stalemate: king d9 is not in check, but c9 is
    // covered by the pawn on b8, e9/e8 by the rook on e1, c8 by the pawn
    // on d7 and d8 by the pawn on c7.
    let pos = Position::from_fen("3k5/1P7/2PP5/9/9/9/9/9/4R4/4K4 b - - 0 1").unwrap();
    assert!(pos.legal_moves().is_empty());
    assert!(!pos.in_check(xq_core::Color::Black));
    let game = Game::new(pos);
    assert_eq!(game.result(), GameResult::RedWins(Reason::Stalemate));
}

#[test]
fn fen_roundtrip() {
    let pos = Position::startpos();
    assert_eq!(Position::from_fen(&pos.to_fen()).unwrap(), pos);
}

#[test]
fn fen_errors() {
    assert!(Position::from_fen("9/9/9/9/9/9/9/9/9/9 w - - 0 1").is_err()); // no kings
    assert!(Position::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1").is_ok());
    assert!(Position::from_fen("4k4/9/9/9/9/9/9/9/9/2K6 w - - 0 1").is_err()); // king outside palace
    assert!(Position::from_fen(
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR x - - 0 1"
    )
    .is_err());
    assert!(Position::from_fen("4k4/9/9/9/9/9/9/9/P8/4K4 w - - 0 1").is_err()); // red pawn behind start rank
}

#[test]
fn move_struct_helpers() {
    let m = Move::new(sq(7, 2), sq(4, 2));
    assert_eq!(file_of(m.from), 7);
    assert_eq!(rank_of(m.from), 2);
}
