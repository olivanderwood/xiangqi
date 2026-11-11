//! Perft (§8.1): start position counts must be 44 / 1920 / 79666.

use xq_core::board::Position;
use xq_core::movegen::{perft, perft_divide};

#[test]
fn perft_startpos_d1() {
    assert_eq!(perft(&Position::startpos(), 1), 44);
}

#[test]
fn perft_startpos_d2() {
    assert_eq!(perft(&Position::startpos(), 2), 1920);
}

#[test]
fn perft_startpos_d3() {
    assert_eq!(perft(&Position::startpos(), 3), 79666);
}

#[test]
#[ignore]
fn perft_startpos_d4() {
    assert_eq!(perft(&Position::startpos(), 4), 3_290_240);
}

#[test]
#[ignore]
fn perft_startpos_d5() {
    assert_eq!(perft(&Position::startpos(), 5), 133_312_995);
}

#[test]
fn perft_divide_sums_to_total() {
    let pos = Position::startpos();
    let div = perft_divide(&pos, 2);
    assert_eq!(div.len(), 44);
    let sum: u64 = div.iter().map(|(_, n)| n).sum();
    assert_eq!(sum, 1920);
}
