//! Validate `openings.json` (§8.7, checks 1–9 + 10 `engine_sanity`).
//!
//! Every failure message names the lesson id and the path from the root,
//! so the content author can find the bad move at once.

use xq_book::schema::{BookNode, Lesson, Verdict};
use xq_book::Book;
use xq_core::board::Position;
use xq_core::notation::{parse_cn, parse_wxf};
use xq_core::types::Move;

const EXPECTED_IDS: [&str; 25] = [
    "r-zhongpao-pingfeng",
    "r-shunpao-hengche",
    "r-vs-yuanyang",
    "r-feixiang",
    "r-xianren",
    "r-guogong",
    "r-shunpao",
    "r-liepao",
    "r-qima",
    "r-xunhepao",
    "r-shijiaopao",
    "r-jingoupao",
    "r-paodau-phituong-maquy",
    "b-pingfeng",
    "b-yuanyang",
    "b-chuangong",
    "b-fangong",
    "b-shunpao",
    "b-zudipao",
    "b-dantima",
    "b-vs-feixiang",
    "b-vs-guogong",
    "b-vs-qima",
    "b-vs-xunhepao",
    "b-vs-liepao",
];

fn load() -> Book {
    Book::load_embedded().expect("openings.json parses")
}

/// Parse one book move at `pos`: Chinese notation first, WXF as fallback
/// (§5.4: many English sources use WXF).
fn parse_move(pos: &Position, s: &str) -> Result<Move, String> {
    parse_cn(pos, s)
        .or_else(|_| parse_wxf(pos, s))
        .map_err(|e| format!("không parse được nước {s:?}: {e:?}"))
}

/// Walk the whole tree, calling `f` at every node with its position and path.
fn walk<F: FnMut(&Lesson, &[String], &BookNode, &Position)>(
    lesson: &Lesson,
    nodes: &[BookNode],
    pos: &Position,
    path: &mut Vec<String>,
    f: &mut F,
) {
    for node in nodes {
        path.push(node.mv.clone());
        f(lesson, path, node, pos);
        let mv = parse_move(pos, &node.mv)
            .unwrap_or_else(|e| panic!("{}: đường {}: {e}", lesson.id, path.join(" > ")));
        let mut next_pos = pos.clone();
        next_pos.make_move(mv);
        walk(lesson, &node.next, &next_pos, path, f);
        path.pop();
    }
}

fn start_pos(lesson: &Lesson) -> Position {
    match &lesson.start_fen {
        Some(fen) => Position::from_fen(fen).expect("start_fen hợp lệ"),
        None => Position::startpos(),
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{2FFF}'
        | '\u{3000}'..='\u{303F}'
        | '\u{3040}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}')
}

#[test]
fn book_parses() {
    let book = load();
    assert_eq!(book.lessons.len(), 25, "phải có 25 bài học");
}

#[test]
fn ids_unique_and_prefixed() {
    let book = load();
    let mut seen = std::collections::HashSet::new();
    for lesson in &book.lessons {
        assert!(seen.insert(lesson.id.clone()), "id trùng: {}", lesson.id);
        let prefix = if lesson.learner == xq_book::schema::Learner::Red {
            "r-"
        } else {
            "b-"
        };
        assert!(
            lesson.id.starts_with(prefix),
            "id {} không khớp tiền tố {} của learner {:?}",
            lesson.id,
            prefix,
            lesson.learner
        );
        assert!(
            lesson
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "id {} chứa ký tự ngoài [a-z0-9-]",
            lesson.id
        );
        assert!(
            (1..=3).contains(&lesson.level),
            "level của {} ngoài 1..=3",
            lesson.id
        );
    }
}

#[test]
fn every_move_legal() {
    let book = load();
    let mut errors: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        let start = start_pos(lesson);
        let mut path = Vec::new();
        walk(
            lesson,
            &lesson.tree,
            &start,
            &mut path,
            &mut |les, p, node, pos| {
                if let Err(e) = parse_move(pos, &node.mv) {
                    errors.push(format!(
                        "{}: đường {}: nước lỗi {:?} ({e})",
                        les.id,
                        p.join(" > "),
                        node.mv
                    ));
                }
            },
        );
    }
    assert!(
        errors.is_empty(),
        "nước không hợp lệ:\n{}",
        errors.join("\n")
    );
}

#[test]
fn no_duplicate_children() {
    let book = load();
    for lesson in &book.lessons {
        fn check(lesson: &Lesson, nodes: &[BookNode], path: &str) {
            let mut seen = std::collections::HashSet::new();
            for node in nodes {
                assert!(
                    seen.insert(node.mv.clone()),
                    "{}: đường {}: hai nút anh em cùng nước {:?}",
                    lesson.id,
                    path,
                    node.mv
                );
                check(lesson, &node.next, &format!("{path} > {}", node.mv));
            }
        }
        check(lesson, &lesson.tree, "(gốc)");
    }
}

#[test]
fn learner_moves_have_notes() {
    let book = load();
    let mut errors: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        let learner = lesson.learner.color();
        let start = start_pos(lesson);
        let mut path = Vec::new();
        walk(
            lesson,
            &lesson.tree,
            &start,
            &mut path,
            &mut |les, p, node, pos| {
                if pos.side() == learner {
                    match &node.note_vi {
                        Some(note) if !note.trim().is_empty() => {
                            let n = note.chars().count();
                            if n > 160 {
                                errors.push(format!(
                                    "{}: đường {}: note_vi dài {n} ký tự (>160)",
                                    les.id,
                                    p.join(" > ")
                                ));
                            }
                        }
                        _ => errors.push(format!(
                            "{}: đường {}: nước của người học thiếu note_vi",
                            les.id,
                            p.join(" > ")
                        )),
                    }
                }
            },
        );
    }
    assert!(
        errors.is_empty(),
        "note_vi thiếu/sai:\n{}",
        errors.join("\n")
    );
}

#[test]
fn leaves_have_end() {
    let book = load();
    let mut errors: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        fn check(lesson: &Lesson, nodes: &[BookNode], path: &str, errors: &mut Vec<String>) {
            for node in nodes {
                let p = format!("{path} > {}", node.mv);
                if node.end.is_some() && !node.next.is_empty() {
                    errors.push(format!(
                        "{}: đường {p}: nút vừa có end vừa có next",
                        lesson.id
                    ));
                }
                if node.next.is_empty() {
                    match &node.end {
                        Some(end) => {
                            let _ = end.verdict;
                            if end.note_vi.trim().is_empty() {
                                errors.push(format!("{}: đường {p}: end.note_vi rỗng", lesson.id));
                            }
                        }
                        None => errors.push(format!("{}: đường {p}: lá thiếu end", lesson.id)),
                    }
                }
                check(lesson, &node.next, &p, errors);
            }
        }
        check(lesson, &lesson.tree, "(gốc)", &mut errors);
    }
    assert!(errors.is_empty(), "lá/end sai:\n{}", errors.join("\n"));
}

#[test]
fn sources_present() {
    let book = load();
    for lesson in &book.lessons {
        assert!(!lesson.sources.is_empty(), "{}: thiếu sources", lesson.id);
        for src in &lesson.sources {
            assert!(
                src.url.starts_with("https://"),
                "{}: nguồn {:?} không bắt đầu bằng https://",
                lesson.id,
                src.url
            );
            assert!(
                !src.title.trim().is_empty(),
                "{}: nguồn thiếu title",
                lesson.id
            );
        }
        // source_ref (nếu có) phải nằm trong phạm vi.
        fn check(lesson: &Lesson, nodes: &[BookNode]) {
            for node in nodes {
                if let Some(r) = node.source_ref {
                    assert!(
                        r < lesson.sources.len(),
                        "{}: nước {:?} có source_ref {r} vượt phạm vi ({} nguồn)",
                        lesson.id,
                        node.mv,
                        lesson.sources.len()
                    );
                }
                check(lesson, &node.next);
            }
        }
        check(lesson, &lesson.tree);
    }
    // Verdict chỉ dùng 3 giá trị hợp lệ (đã ép kiểu bởi serde); kiểm tra
    // rằng mọi lá đều có verdict hợp lệ qua việc đọc lại là đủ.
    let _ = Verdict::Equal;
}

#[test]
fn twenty_five_lessons() {
    let book = load();
    let reds = book.by_learner(xq_core::Color::Red).len();
    let blacks = book.by_learner(xq_core::Color::Black).len();
    assert_eq!(reds, 13, "phải có đúng 13 bài red, gặp {reds}");
    assert_eq!(blacks, 12, "phải có đúng 12 bài black, gặp {blacks}");
    for id in EXPECTED_IDS {
        assert!(book.by_id(id).is_some(), "thiếu bài {id}");
    }
}

/// §7: the opponent's prepared variations are deep. Along the main spine
/// (first 16 plies, the opening phase), an opponent node must have at
/// least one side direction whose subtree is a branching sub-spine, not
/// a single PV chain: a chain holds at most 8 nodes, a sub-spine ~100.
#[test]
fn opponent_variations_are_deep() {
    let book = load();
    fn count(nodes: &[BookNode]) -> usize {
        nodes.iter().map(|n| 1 + count(&n.next)).sum()
    }
    for lesson in &book.lessons {
        let learner = lesson.learner.color();
        let mut pos = start_pos(lesson);
        let mut level: &[BookNode] = &lesson.tree;
        let mut best = 0;
        for _ply in 0..16 {
            if pos.side() != learner {
                for child in level.iter().filter(|n| !n.main) {
                    best = best.max(count(&child.next));
                }
            }
            let Some(next) = level.iter().find(|n| n.main).or_else(|| level.first()) else {
                break;
            };
            let mv = parse_move(&pos, &next.mv).unwrap_or_else(|e| panic!("{}: {e}", lesson.id));
            pos.make_move(mv);
            level = &next.next;
            if level.is_empty() {
                break;
            }
        }
        assert!(
            best >= 20,
            "{}: biến đối thủ sâu nhất chỉ có {best} nút (chuỗi PV tối đa 8, sub-spine ~100)",
            lesson.id
        );
    }
}

/// Pikafish IS the book (Hieu, 2026-11-11): every node must carry the
/// Pikafish score it was generated with, so the Tutor never re-scores a
/// move with a live engine and every attempt is identical.
#[test]
fn every_node_has_score() {
    let book = load();
    let mut errors: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        let start = start_pos(lesson);
        let mut path = Vec::new();
        walk(
            lesson,
            &lesson.tree,
            &start,
            &mut path,
            &mut |les, p, node, _pos| {
                if node.score_cp.is_none() {
                    errors.push(format!(
                        "{}: đường {}: nút {:?} thiếu score_cp",
                        les.id,
                        p.join(" > "),
                        node.mv
                    ));
                }
            },
        );
    }
    assert!(
        errors.is_empty(),
        "nút thiếu score_cp:\n{}",
        errors.join("\n")
    );
}

#[test]
fn fonts_cover_book() {
    let fonts_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../xq-app/assets/fonts");
    let mut cjk_fonts: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&fonts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if name.contains("CJK") && path.extension().is_some_and(|e| e == "ttf" || e == "otf") {
                cjk_fonts.push(path);
            }
        }
    }
    if cjk_fonts.is_empty() {
        eprintln!(
            "SKIP fonts_cover_book: chưa có file font subset CJK trong {}",
            fonts_dir.display()
        );
        return;
    }
    let text = include_str!("../data/openings.json");
    let mut chars: std::collections::BTreeSet<char> = std::collections::BTreeSet::new();
    for c in text.chars() {
        if is_cjk(c) {
            chars.insert(c);
        }
    }
    assert!(!chars.is_empty(), "không tìm thấy ký tự CJK nào trong sách");
    let mut missing: Vec<String> = Vec::new();
    for font_path in &cjk_fonts {
        let data = std::fs::read(font_path)
            .unwrap_or_else(|e| panic!("không đọc được font {}: {e}", font_path.display()));
        let face = ttf_parser::Face::parse(&data, 0)
            .unwrap_or_else(|e| panic!("không parse được font {}: {e:?}", font_path.display()));
        let mut absent: Vec<char> = Vec::new();
        for &c in &chars {
            if face.glyph_index(c).is_none() {
                absent.push(c);
            }
        }
        if !absent.is_empty() {
            missing.push(format!(
                "{}: thiếu {}",
                font_path.display(),
                absent.iter().collect::<String>()
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "font subset thiếu glyph:\n{}",
        missing.join("\n")
    );
}

/// §8.7 mục 10 — chạy release, bỏ qua ở debug thường lệ.
/// Mọi nước của người học: engine depth 8 cho delta <= 150; nước
/// trap: true được miễn (hy sinh có chủ đích).
#[test]
#[ignore]
fn engine_sanity() {
    use xq_core::analysis::grade_from_delta;
    use xq_core::search::{SearchLimits, Searcher};
    let book = load();
    let mut searcher = Searcher::new(32);
    let mut rows: Vec<(String, String, i32, String)> = Vec::new();
    let mut violations: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        let learner = lesson.learner.color();
        let start = start_pos(lesson);
        let mut path = Vec::new();
        walk(
            lesson,
            &lesson.tree,
            &start,
            &mut path,
            &mut |les, p, node, pos| {
                if pos.side() != learner || node.trap {
                    return;
                }
                let mv = parse_move(pos, &node.mv).expect("nước đã hợp lệ");
                let limits = SearchLimits::depth(8);
                let best = searcher.search(pos, &[], limits, &mut |_| {});
                let best_score = best.score;
                let mv_score = searcher.score_move(pos, &[], mv, limits);
                let delta = best_score - mv_score;
                let label = grade_from_delta(delta, false).label_vi().to_string();
                rows.push((les.id.clone(), node.mv.clone(), delta, label.clone()));
                if delta > 150 {
                    violations.push(format!(
                        "{}: đường {}: nước {:?} delta={delta} ({label})",
                        les.id,
                        p.join(" > "),
                        node.mv
                    ));
                }
            },
        );
    }
    eprintln!(
        "engine_sanity: đã chấm {} nước của người học (depth 8)",
        rows.len()
    );
    for (les, mv, delta, label) in &rows {
        if *delta > 60 {
            eprintln!("  {les}: {mv} delta={delta} {label}");
        }
    }
    assert!(
        violations.is_empty(),
        "nước lệch engine >150:\n{}",
        violations.join("\n")
    );
}
