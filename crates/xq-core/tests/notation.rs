//! Notation tests (§8.4): ICCS / Chinese / Vietnamese / WXF, both ways.

use xq_core::board::Position;
use xq_core::notation::{
    iccs, parse_cn, parse_iccs, parse_vn, parse_wxf, render_cn, render_vn, render_wxf,
};
use xq_core::types::Move;

fn mv(from_file: i32, from_rank: i32, to_file: i32, to_rank: i32) -> Move {
    Move::new(
        (from_rank * 9 + from_file) as u8,
        (to_rank * 9 + to_file) as u8,
    )
}

#[test]
fn iccs_roundtrip() {
    let m = mv(7, 2, 4, 2);
    assert_eq!(iccs(m), "h2e2");
    assert_eq!(parse_iccs("h2e2").unwrap(), m);
}

#[test]
fn chinese_startpos_cases() {
    let pos = Position::startpos();
    let cases = [
        (mv(7, 2, 4, 2), "炮二平五", "P2-5"),
        (mv(7, 0, 6, 2), "马二进三", "M2.3"),
        (mv(1, 0, 2, 2), "马八进七", "M8.7"),
        (mv(8, 0, 8, 1), "车一进一", "X1.1"),
        (mv(6, 0, 4, 2), "相三进五", "T3.5"),
        (mv(2, 3, 2, 4), "兵七进一", "B7.1"),
    ];
    for (m, cn, vn) in cases {
        assert_eq!(render_cn(&pos, m), cn, "CN for {}", iccs(m));
        assert_eq!(render_vn(&pos, m), vn, "VN for {}", iccs(m));
        assert_eq!(parse_cn(&pos, cn).unwrap(), m);
        assert_eq!(parse_vn(&pos, vn).unwrap(), m);
        // Round-trip: render(parse(s)) == s.
        assert_eq!(render_cn(&pos, parse_cn(&pos, cn).unwrap()), cn);
        assert_eq!(render_vn(&pos, parse_vn(&pos, vn).unwrap()), vn);
    }
}

#[test]
fn chinese_black_after_h2e2() {
    let mut pos = Position::startpos();
    let u = pos.make_move(mv(7, 2, 4, 2));
    let _ = u;
    let cases = [
        (mv(7, 9, 6, 7), "马8进7", "M8.7"),
        (mv(1, 7, 4, 7), "炮2平5", "P2-5"),
    ];
    for (m, cn, vn) in cases {
        assert_eq!(render_cn(&pos, m), cn, "CN for {}", iccs(m));
        assert_eq!(render_vn(&pos, m), vn, "VN for {}", iccs(m));
        assert_eq!(parse_cn(&pos, cn).unwrap(), m);
        assert_eq!(parse_vn(&pos, vn).unwrap(), m);
    }
}

#[test]
fn parse_accepts_traditional_and_fullwidth() {
    let pos = Position::startpos();
    assert_eq!(parse_cn(&pos, "車一进一").unwrap(), mv(8, 0, 8, 1));
    assert_eq!(parse_cn(&pos, "馬二進三").unwrap(), mv(7, 0, 6, 2));
    let mut pos2 = Position::startpos();
    let _ = pos2.make_move(mv(7, 2, 4, 2));
    assert_eq!(parse_cn(&pos2, "馬８进７").unwrap(), mv(7, 9, 6, 7));
}

#[test]
fn wxf_shunpao_trap() {
    // WXF from §5.4: 1. C2=5 C8=5 2. H2+3 H8+7 3. R1+1 R9=8
    let pos = Position::startpos();
    assert_eq!(render_wxf(&pos, mv(7, 2, 4, 2)), "C2=5");
    assert_eq!(parse_wxf(&pos, "C2=5").unwrap(), mv(7, 2, 4, 2));
    assert_eq!(render_wxf(&pos, mv(7, 0, 6, 2)), "H2+3");
    assert_eq!(render_wxf(&pos, mv(8, 0, 8, 1)), "R1+1");
}

#[test]
fn front_back_rooks() {
    // Two red rooks on the same file (e): front one advances, back one slides.
    // Black king on d9 (not the e-file) so the back rook may leave the file.
    let pos = Position::from_fen("3k5/9/9/9/9/4R4/9/9/4R4/4K4 w - - 0 1").unwrap();
    // Front rook (e4) advances two: 前车进二.
    assert_eq!(render_cn(&pos, mv(4, 4, 4, 6)), "前车进二");
    assert_eq!(parse_cn(&pos, "前车进二").unwrap(), mv(4, 4, 4, 6));
    // Back rook (e1) slides to file f (Red column 4): 后车平四.
    assert_eq!(render_cn(&pos, mv(4, 1, 5, 1)), "后车平四");
    assert_eq!(parse_cn(&pos, "后车平四").unwrap(), mv(4, 1, 5, 1));
}

#[test]
fn invalid_strings_error() {
    let pos = Position::startpos();
    assert!(parse_cn(&pos, "炮二进十").is_err());
    assert!(parse_cn(&pos, "马二平三").is_err()); // horse cannot move sideways
    assert!(parse_cn(&pos, "车一进九").is_err());
    assert!(parse_vn(&pos, "X1.9").is_err());
}

#[test]
fn roundtrip_all_legal_startpos() {
    // Every legal move renders to a unique CN string that parses back.
    let pos = Position::startpos();
    let mut seen = std::collections::HashSet::new();
    for m in pos.legal_moves() {
        let s = render_cn(&pos, m);
        assert!(seen.insert(s.clone()), "duplicate CN rendering: {s}");
        assert_eq!(parse_cn(&pos, &s).unwrap(), m);
        let v = render_vn(&pos, m);
        assert_eq!(parse_vn(&pos, &v).unwrap(), m);
        let w = render_wxf(&pos, m);
        assert_eq!(parse_wxf(&pos, &w).unwrap(), m);
    }
}

#[test]
fn roundtrip_all_legal_random_positions() {
    // 200 pseudo-random playout positions; CN render must stay injective.
    use xq_core::board::Position as P;
    let mut rng_state: u64 = 0x1234_5678_9ABC_DEF0;
    let mut next_rand = move || {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state
    };
    let mut pos = P::startpos();
    for _ in 0..200 {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            pos = P::startpos();
            continue;
        }
        let mut seen = std::collections::HashSet::new();
        for m in &moves {
            let s = render_cn(&pos, *m);
            assert!(
                seen.insert(s.clone()),
                "duplicate CN {s} in {}",
                pos.to_fen()
            );
            assert_eq!(parse_cn(&pos, &s).unwrap(), *m);
        }
        let pick = moves[(next_rand() % moves.len() as u64) as usize];
        let _ = pos.make_move(pick);
        if next_rand() % 7 == 0 {
            pos = P::startpos();
        }
    }
}
