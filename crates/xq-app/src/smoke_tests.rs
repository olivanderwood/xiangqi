//! Smoke tests (§8.9) with `egui_kittest` + font coverage (§7.5).
//!
//! These run the real `XiangqiApp` headless: menu navigation, starting a
//! Vs-AI game, clicking board squares, and opening the lesson picker.

use crate::app::XiangqiApp;
use crate::settings::NotationChoice;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

fn new_harness() -> Harness<'static, XiangqiApp> {
    Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .with_max_steps(200)
        .build_eframe(|cc| XiangqiApp::new(cc))
}

/// Simulate a mouse click at a screen position.
fn click_at(harness: &mut Harness<'static, XiangqiApp>, pos: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();
    harness.run_ok();
}

/// Click a board square by its square index, using the geometry the app
/// actually drew last frame.
fn click_square(harness: &mut Harness<'static, XiangqiApp>, sq: u8) {
    let (rect, geom) = harness.state().vs_ai.last_board.expect("board was drawn");
    let pos = rect.min + geom.to_screen(sq).to_vec2();
    click_at(harness, pos);
}

fn wait_for_ai(harness: &mut Harness<'static, XiangqiApp>, min_moves: usize) {
    for _ in 0..40 {
        if harness.state().vs_ai.game.moves.len() >= min_moves {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        harness.run_steps(2);
    }
}

#[test]
fn menu_to_vs_ai_plays_cannon_opening() {
    let mut harness = new_harness();
    // Chinese notation, so the move list shows 炮二平五 as in §8.9.
    harness.state_mut().settings.notation = NotationChoice::Chinese;
    harness.run_ok();

    harness.get_by_label("Đấu với AI").click();
    harness.run_ok();
    harness.get_by_label("Bắt đầu").click();
    harness.run_ok();

    assert!(harness.state().vs_ai.started);
    assert!(harness.state().vs_ai.last_board.is_some());

    // h2 = sq 25, e2 = sq 22 (rank index 2 in ICCS).
    click_square(&mut harness, 25);
    harness.run_ok();
    assert_eq!(harness.state().vs_ai.board.selected, Some(25));
    click_square(&mut harness, 22);
    harness.run_ok();

    let moves = harness.state().vs_ai.game.moves.clone();
    assert!(!moves.is_empty(), "human move should be played");
    assert_eq!(moves[0], xq_core::types::Move::new(25, 22));
    // Rendered with the position before the move.
    let rendered = {
        let app = harness.state();
        let pos = xq_core::board::Position::startpos();
        app.render_move(&pos, moves[0])
    };
    assert_eq!(rendered, "炮二平五");
    // The move list widget shows the same string.
    assert!(
        harness.query_by_label_contains("炮二平五").is_some(),
        "move list should contain 炮二平五"
    );
    // The AI (Black) answers afterwards.
    wait_for_ai(&mut harness, 2);
    assert!(
        harness.state().vs_ai.game.moves.len() >= 2,
        "AI should reply to the opening"
    );
}

/// Click a board square on the Learn screen, using the geometry the app
/// actually drew last frame.
fn click_learn_square(harness: &mut Harness<'static, XiangqiApp>, sq: u8) {
    let (rect, geom) = harness
        .state()
        .learn
        .last_board
        .expect("learn board was drawn");
    let pos = rect.min + geom.to_screen(sq).to_vec2();
    click_at(harness, pos);
}

#[test]
fn menu_to_learn_plays_book_move() {
    let mut harness = new_harness();
    harness.get_by_label("Học chơi khai cuộc").click();
    harness.run_ok();
    harness.get_by_label("Đi tiên (cầm Đỏ)").click();
    harness.run_ok();
    harness.get_by_label_contains("Pháo đầu Xe qua hà").click();
    harness.run_ok();
    harness.get_by_label("Bắt đầu").click();
    harness.run_ok();

    assert!(harness.state().learn.started);
    assert!(
        !harness.state().learn.hints.is_empty(),
        "learner's first turn should show book hints"
    );
    assert!(
        harness
            .state()
            .learn
            .hints
            .iter()
            .any(|h| h.mv_cn == "炮二平五" && h.in_book),
        "first hint should be the book move 炮二平五"
    );

    // Play 炮二平五 on the board (h2 = sq 25 -> e2 = sq 22).
    click_learn_square(&mut harness, 25);
    harness.run_ok();
    click_learn_square(&mut harness, 22);
    harness.run_ok();
    // The learner's move is played; the opponent may already have replied
    // inside run_ok() because simulated time passes the 600 ms delay.
    let moves = harness.state().learn.game.moves.clone();
    assert!(!moves.is_empty(), "learner move should be played");
    assert_eq!(moves[0], xq_core::types::Move::new(25, 22));
    assert_eq!(
        harness
            .state()
            .learn
            .last_grade
            .as_ref()
            .map(|(g, _, _)| *g),
        Some(xq_core::analysis::Grade::Book),
        "playing the book move should grade Book"
    );
    // The opponent (Black) replies from the book after its 600 ms delay
    // (simulated time advances 0.25 s per harness step).
    harness.run_steps(6);
    assert!(
        harness.state().learn.game.moves.len() >= 2,
        "opponent should reply from the book"
    );
}

#[test]
fn menu_to_learn_select_shows_lessons_for_each_side() {
    let mut harness = new_harness();
    harness.get_by_label("Học chơi khai cuộc").click();
    harness.run_ok();

    harness.get_by_label("Đi tiên (cầm Đỏ)").click();
    harness.run_ok();
    let red_count = harness.state().book.by_learner(xq_core::Color::Red).len();
    if red_count > 0 {
        assert!(
            harness
                .query_by_label_contains("Pháo đầu Xe qua hà đối Bình phong mã")
                .is_some(),
            "red lesson list should show the sample lesson"
        );
    }
    harness.get_by_label("Đi hậu (cầm Đen)").click();
    harness.run_ok();
    let black_count = harness.state().book.by_learner(xq_core::Color::Black).len();
    if black_count == 0 {
        // The book currently ships only the 1 sample lesson (M4 not done);
        // the screen must say so instead of showing an empty list.
        assert!(
            harness
                .query_by_label_contains("Chưa có bài học cho bên này.")
                .is_some(),
            "empty side should show the empty-state text"
        );
    } else {
        assert_eq!(black_count, 6, "full book has 6 black lessons (§5.1)");
    }
}

/// Font subset covers every CJK char in the book (§7.5, test 8.7.9).
#[test]
fn fonts_cover_book() {
    let book_text = include_str!("../../xq-book/data/openings.json");
    let chars: std::collections::BTreeSet<char> = book_text
        .chars()
        .filter(|c| {
            ('\u{4e00}'..='\u{9fff}').contains(c)
                || ('\u{3400}'..='\u{4dbf}').contains(c)
                || ('\u{f900}'..='\u{faff}').contains(c)
        })
        .collect();
    assert!(!chars.is_empty());
    // And the committed char list contains them all (rebuilt by the script).
    let listed = include_str!("../../../tools/cjk_chars.txt");
    for c in &chars {
        assert!(listed.contains(*c), "cjk_chars.txt is missing {c}");
    }
    for (name, bytes) in [
        (
            "NotoSerifCJKSC-Bold-subset.ttf",
            include_bytes!("../assets/fonts/NotoSerifCJKSC-Bold-subset.ttf").as_slice(),
        ),
        (
            "NotoSerifCJKSC-Regular-subset.ttf",
            include_bytes!("../assets/fonts/NotoSerifCJKSC-Regular-subset.ttf").as_slice(),
        ),
    ] {
        assert!(
            bytes.len() < 300 * 1024,
            "{name} should be < 300 KB, got {} bytes",
            bytes.len()
        );
        let face = ttf_parser::Face::parse(bytes, 0).unwrap_or_else(|_| panic!("{name} parses"));
        for c in &chars {
            assert!(
                face.glyph_index(*c).is_some(),
                "{name} is missing glyph for {c}"
            );
        }
    }
}
