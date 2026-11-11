//! Tutor behaviour (§8.8) on `tests/fixtures/mini_book.json`.
//!
//! Fixture (`r-mini`, learner red, `max_plies` 6, two branches):
//! - A (main): 1. 炮二平五 马8进7 2. 马二进三 马2进3 3. 车一平二 车9平8
//! - B: 1. 炮二平五 马2进3 (weight 3) 2. 马八进七 车1平2 3. 马二进三
//!   The AI choice after 1. 炮二平五 is 马8进7 (weight 1) vs 马2进3 (weight 3).

use xq_book::schema::Learner;
use xq_book::{Book, Tutor, TutorEvent};
use xq_core::analysis::Grade;
use xq_core::notation::{parse_cn, render_cn};
use xq_core::types::Move;

fn load_mini() -> Book {
    let text = include_str!("fixtures/mini_book.json");
    Book::load_str(text).expect("mini_book.json parses")
}

/// Parse a Chinese-notation move at the tutor's current position.
fn mv(t: &Tutor, s: &str) -> Move {
    parse_cn(&t.game.pos, s).unwrap_or_else(|e| panic!("{s}: {e:?}"))
}

/// Play one learner move; panic unless it grades as expected.
fn play_learner(t: &mut Tutor, s: &str) -> TutorEvent {
    let m = mv(t, s);
    t.on_user_move(m)
}

/// Play the AI reply; panic unless the AI moves.
fn play_ai(t: &mut Tutor) -> Move {
    match t.next_opponent_move() {
        Some(TutorEvent::OpponentMoves { mv, .. }) => mv,
        other => panic!("mong đợi OpponentMoves, gặp {}", event_name(&other)),
    }
}

fn event_name(e: &Option<TutorEvent>) -> &'static str {
    match e {
        None => "None",
        Some(TutorEvent::YourTurn { .. }) => "YourTurn",
        Some(TutorEvent::OpponentMoves { .. }) => "OpponentMoves",
        Some(TutorEvent::Graded { .. }) => "Graded",
        Some(TutorEvent::OpeningComplete { .. }) => "OpeningComplete",
    }
}

fn graded_book(e: TutorEvent, s: &str) {
    match e {
        TutorEvent::Graded {
            grade: Grade::Book, ..
        } => {}
        TutorEvent::Graded { grade, .. } => {
            panic!("nước {s} mong đợi Grade::Book, gặp {grade:?}")
        }
        _ => panic!("nước {s} mong đợi Graded, gặp sự kiện khác"),
    }
}

/// Build a tutor that has played 1. 炮二平五 and whose AI replied 马8进7
/// (branch A), by trying seeds until the weighted pick lands on A.
fn tutor_branch_a<'a>(lesson: &'a xq_book::schema::Lesson) -> Tutor<'a> {
    for seed in 1..=500u64 {
        let mut t = Tutor::new(lesson, seed);
        let e = play_learner(&mut t, "炮二平五");
        assert!(matches!(
            e,
            TutorEvent::Graded {
                grade: Grade::Book,
                ..
            }
        ));
        let pos = t.game.pos.clone();
        let ai = play_ai(&mut t);
        if render_cn(&pos, ai) == "马8进7" {
            return t;
        }
    }
    panic!("không tìm được seed để AI đi nhánh A");
}

#[test]
fn book_move_graded_book() {
    let book = load_mini();
    let lesson = book.by_id("r-mini").unwrap();
    let mut t = Tutor::new(lesson, 1);
    let e = play_learner(&mut t, "炮二平五");
    graded_book(e, "炮二平五");
    assert_eq!(t.book_moves, 1);
    assert!(!t.out_of_book);
}

#[test]
fn opponent_follows_weights() {
    let book = load_mini();
    let lesson = book.by_id("r-mini").unwrap();
    let mut count_a = 0usize;
    let mut count_b = 0usize;
    const N: usize = 1000;
    for i in 0..N {
        let seed = (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut t = Tutor::new(lesson, seed);
        let e = play_learner(&mut t, "炮二平五");
        graded_book(e, "炮二平五");
        let pos_before = t.game.pos.clone();
        let m = play_ai(&mut t);
        match render_cn(&pos_before, m).as_str() {
            "马8进7" => count_a += 1,
            "马2进3" => count_b += 1,
            other => panic!("AI đi nước ngoài dự kiến: {other}"),
        }
    }
    assert_eq!(count_a + count_b, N);
    // Weights 1:3 -> expected 25% / 75%, tolerance 5 points.
    let ra = count_a as f64 / N as f64;
    let rb = count_b as f64 / N as f64;
    assert!(
        (ra - 0.25).abs() <= 0.05,
        "tỉ lệ nhánh A (weight 1) = {ra:.3}, mong đợi ~0.25"
    );
    assert!(
        (rb - 0.75).abs() <= 0.05,
        "tỉ lệ nhánh B (weight 3) = {rb:.3}, mong đợi ~0.75"
    );
}

#[test]
fn off_book_retry() {
    let book = load_mini();
    let lesson = book.by_id("r-mini").unwrap();
    let mut t = tutor_branch_a(lesson);
    // Đi nước ngoài sách (sách có 马二进三 / 马八进七 tùy nhánh; 马八进九
    // không có trong sách ở cả hai nhánh).
    let pos_before = t.game.pos.clone();
    let wrong = parse_cn(&pos_before, "马八进九").expect("马八进九 hợp lệ");
    let moves_before = t.game.moves.len();
    let node_before = t.node_path.clone();
    let e = t.on_user_move(wrong);
    let (grade, best) = match e {
        TutorEvent::Graded { grade, best, .. } => (grade, best),
        _ => panic!("mong đợi Graded sau nước ngoài sách"),
    };
    assert!(t.out_of_book);
    assert_ne!(grade, Grade::Book, "nước ngoài sách không được chấm Book");
    // Best phải là nước main của nút hiện tại: 马二进三.
    let best_cn = best.map(|m| render_cn(&pos_before, m));
    assert_eq!(
        best_cn.as_deref(),
        Some("马二进三"),
        "best phải là nước main"
    );
    // Retry: thế cờ và nút trở về như trước nước sai.
    t.retry();
    assert_eq!(t.game.moves.len(), moves_before);
    assert_eq!(t.node_path, node_before);
    assert!(!t.out_of_book);
    assert_eq!(t.game.pos, pos_before);
    // Đi lại đúng sách được.
    graded_book(play_learner(&mut t, "马二进三"), "马二进三");
}

#[test]
fn off_book_continue() {
    let book = load_mini();
    // Cần đi tới 10 nửa nước nên nới max_plies của fixture (mặc định 6).
    let mut lesson = book.by_id("r-mini").unwrap().clone();
    lesson.max_plies = 30;
    let mut t = tutor_branch_a(&lesson);
    let wrong = mv(&t, "马八进九");
    let e = t.on_user_move(wrong);
    assert!(
        matches!(e, TutorEvent::Graded { .. }),
        "nước ngoài sách đầu tiên phải Graded"
    );
    assert!(t.out_of_book);
    assert!(
        t.next_opponent_move().is_none(),
        "hết sách: AI không đi theo sách nữa"
    );
    // Đi tiếp 7 nửa nước nữa (tổng 8 nửa nước sau khi ra khỏi sách):
    // AI (qua game.play trực tiếp, như UI dùng engine), rồi người học.
    for _ in 0..3 {
        let ai_mv = t.game.pos.legal_moves().into_iter().next().unwrap();
        t.game.play(ai_mv).expect("AI đi nước hợp lệ");
        assert!(!t.is_complete());
        let learner_mv = t.game.pos.legal_moves().into_iter().next().unwrap();
        let e = t.on_user_move(learner_mv);
        assert!(matches!(e, TutorEvent::Graded { .. }));
        assert!(!t.is_complete());
    }
    // Nửa nước thứ 8 sau khi ra khỏi sách: tới lượt AI.
    let ai_mv = t.game.pos.legal_moves().into_iter().next().unwrap();
    t.game.play(ai_mv).expect("AI đi nước hợp lệ");
    assert_eq!(t.game.moves.len(), 2 + 8);
    assert!(t.is_complete(), "sau 8 nửa nước ngoài sách phải hoàn thành");
}

#[test]
fn leaf_completes() {
    let book = load_mini();
    let lesson = book.by_id("r-mini").unwrap();
    let mut t = Tutor::new(lesson, 3);
    graded_book(play_learner(&mut t, "炮二平五"), "炮二平五");
    // Ép AI đi nhánh B (weight 3) bằng cách thử seed cho tới khi gặp
    // 马2进3 — với seed cố định 3 thì lần gọi đầu đã có thể ra A hoặc B;
    // nếu ra A thì đi hết nhánh A rồi kiểm tra is_complete + summary.
    let pos_before = t.game.pos.clone();
    let ai = play_ai(&mut t);
    let ai_cn = render_cn(&pos_before, ai);
    if ai_cn == "马2进3" {
        graded_book(play_learner(&mut t, "马八进七"), "马八进七");
        play_ai(&mut t); // 车1平2
        let e = play_learner(&mut t, "马二进三");
        match e {
            TutorEvent::OpeningComplete { summary } => {
                assert_eq!(summary.book_moves, 3, "3 nước người học đều trong sách");
                assert_eq!(summary.total_learner_moves, 3);
                assert!(summary.grades.iter().all(|(_, g)| *g == Grade::Book));
                assert!(summary.end_note_vi.is_some());
            }
            _ => panic!("nước cuối nhánh B phải OpeningComplete"),
        }
    } else {
        assert_eq!(ai_cn, "马8进7");
        graded_book(play_learner(&mut t, "马二进三"), "马二进三");
        play_ai(&mut t); // 马2进3
        graded_book(play_learner(&mut t, "车一平二"), "车一平二");
        play_ai(&mut t); // 车9平8 -> tới lá
        assert!(t.is_complete(), "đi hết nhánh A phải hoàn thành");
        let summary = t.summary();
        assert_eq!(summary.book_moves, 3);
        assert_eq!(summary.total_learner_moves, 3);
        assert!(summary.end_note_vi.is_some());
    }
}

#[test]
fn max_plies_completes() {
    let book = load_mini();
    let mut lesson = book.by_id("r-mini").unwrap().clone();
    lesson.max_plies = 4;
    let mut t = tutor_branch_a(&lesson);
    graded_book(play_learner(&mut t, "马二进三"), "马二进三");
    // Nước thứ 4 (AI) chưa hoàn thành ngay khi đi, nhưng sau đó
    // is_complete() phải đúng vì đã đủ max_plies.
    play_ai(&mut t);
    assert_eq!(t.game.moves.len(), 4);
    assert!(t.is_complete(), "đủ 4 nửa nước (max_plies) phải hoàn thành");
    // Nước tiếp theo của người học trả OpeningComplete.
    let next = t.game.pos.legal_moves().into_iter().next().unwrap();
    match t.on_user_move(next) {
        TutorEvent::OpeningComplete { .. } => {}
        _ => panic!("sau max_plies, on_user_move phải OpeningComplete"),
    }
}

#[test]
fn learner_black_starts_with_ai() {
    let book = load_mini();
    let mut lesson = book.by_id("r-mini").unwrap().clone();
    lesson.learner = Learner::Black;
    let mut t = Tutor::new(&lesson, 9);
    // Bài learner=black: sự kiện đầu tiên là AI (Đỏ) đi trước.
    let pos_before = t.game.pos.clone();
    let first = t.next_opponent_move().expect("AI Đỏ phải đi trước");
    match first {
        TutorEvent::OpponentMoves { mv, .. } => {
            assert_eq!(render_cn(&pos_before, mv), "炮二平五");
        }
        _ => panic!("mong đợi OpponentMoves đầu tiên"),
    }
    // Người học (Đen) đáp trong sách.
    let e = play_learner(&mut t, "马8进7");
    graded_book(e, "马8进7");
    assert_eq!(t.book_moves, 1);
}
