//! `book-check`: print the book tree and engine-check every move (T4.2).
//!
//! Usage:
//!   book-check [--book <path.json>] [--check-engine] [--depth N] [--tt-mb N]
//!
//! Without `--check-engine` it prints the tree of every lesson. With it,
//! every learner move is graded by the engine (`xq-core` Searcher) at the
//! given depth (default 8) and a Markdown report is printed: lesson, move,
//! delta vs the best move, label per §4.5. `trap: true` moves are exempt.
//! Exit code is 1 when a non-trap learner move has delta > 150.

use std::process::ExitCode;
use xq_book::schema::{BookNode, Lesson};
use xq_book::Book;
use xq_core::analysis::grade_from_delta;
use xq_core::board::Position;
use xq_core::notation::{parse_cn, parse_wxf};
use xq_core::search::{SearchLimits, Searcher};
use xq_core::types::Move;

struct Args {
    book_path: Option<String>,
    check_engine: bool,
    depth: u8,
    tt_mb: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        book_path: None,
        check_engine: false,
        depth: 8,
        tt_mb: 32,
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--book" => {
                args.book_path = Some(it.next().ok_or("--book cần một đường dẫn")?);
            }
            "--check-engine" => args.check_engine = true,
            "--depth" => {
                let v = it.next().ok_or("--depth cần một số")?;
                args.depth = v
                    .parse()
                    .map_err(|_| format!("--depth không hợp lệ: {v}"))?;
            }
            "--tt-mb" => {
                let v = it.next().ok_or("--tt-mb cần một số")?;
                args.tt_mb = v
                    .parse()
                    .map_err(|_| format!("--tt-mb không hợp lệ: {v}"))?;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("đối số không rõ: {other} (xem --help)")),
        }
    }
    Ok(args)
}

fn print_help() {
    println!(
        "book-check [--book <path.json>] [--check-engine] [--depth N] [--tt-mb N]\n\
         \n\
         In cây sách khai cuộc; với --check-engine thì chấm mọi nước của người\n\
         học bằng engine và in báo cáo Markdown (mặc định depth 8)."
    );
}

fn parse_move(pos: &Position, s: &str) -> Result<Move, String> {
    parse_cn(pos, s)
        .or_else(|_| parse_wxf(pos, s))
        .map_err(|e| format!("không parse được nước {s:?}: {e:?}"))
}

fn print_tree(nodes: &[BookNode], indent: usize) {
    for node in nodes {
        let mark = match (node.main, node.trap) {
            (true, true) => " [main, trap]",
            (true, false) => " [main]",
            (false, true) => " [trap]",
            (false, false) => "",
        };
        let end = match &node.end {
            Some(e) => format!(" — kết: {:?} ({})", e.verdict, e.note_vi),
            None => String::new(),
        };
        println!(
            "{}{} (x{}){}{}",
            "  ".repeat(indent),
            node.mv,
            node.weight,
            mark,
            end
        );
        print_tree(&node.next, indent + 1);
    }
}

struct Row {
    lesson: String,
    mv: String,
    delta: i32,
    label: String,
    trap: bool,
}

struct Ctx<'a> {
    searcher: &'a mut Searcher,
    depth: u8,
    rows: &'a mut Vec<Row>,
    violations: &'a mut Vec<String>,
}

fn check_lesson(
    lesson: &Lesson,
    searcher: &mut Searcher,
    depth: u8,
    rows: &mut Vec<Row>,
    violations: &mut Vec<String>,
) {
    let start = match &lesson.start_fen {
        Some(fen) => Position::from_fen(fen).expect("start_fen hợp lệ"),
        None => Position::startpos(),
    };
    let mut ctx = Ctx {
        searcher,
        depth,
        rows,
        violations,
    };
    fn walk(
        lesson: &Lesson,
        nodes: &[BookNode],
        pos: &Position,
        path: &mut Vec<String>,
        ctx: &mut Ctx,
    ) {
        for node in nodes {
            path.push(node.mv.clone());
            let mv = match parse_move(pos, &node.mv) {
                Ok(m) => m,
                Err(e) => {
                    ctx.violations
                        .push(format!("{}: {}: {e}", lesson.id, path.join(" > ")));
                    path.pop();
                    continue;
                }
            };
            if pos.side() == lesson.learner.color() && !pos.legal_moves().is_empty() {
                if node.trap {
                    ctx.rows.push(Row {
                        lesson: lesson.id.clone(),
                        mv: node.mv.clone(),
                        delta: 0,
                        label: "Miễn (trap)".to_string(),
                        trap: true,
                    });
                } else {
                    let limits = SearchLimits::depth(ctx.depth);
                    let history: Vec<u64> = vec![pos.hash()];
                    let best = ctx.searcher.search(pos, &history, limits, &mut |_| {});
                    let mv_score = ctx.searcher.score_move(pos, &history, mv, limits);
                    let delta = best.score - mv_score;
                    let label = grade_from_delta(delta, false).label_vi().to_string();
                    if delta > 150 {
                        ctx.violations.push(format!(
                            "{}: {}: nước {:?} delta={delta} ({label})",
                            lesson.id,
                            path.join(" > "),
                            node.mv
                        ));
                    }
                    ctx.rows.push(Row {
                        lesson: lesson.id.clone(),
                        mv: node.mv.clone(),
                        delta,
                        label,
                        trap: false,
                    });
                }
            }
            let mut next_pos = pos.clone();
            next_pos.make_move(mv);
            walk(lesson, &node.next, &next_pos, path, ctx);
            path.pop();
        }
    }
    let mut path = Vec::new();
    walk(lesson, &lesson.tree, &start, &mut path, &mut ctx);
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("book-check: {e}");
            return ExitCode::from(2);
        }
    };
    let book = match &args.book_path {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => match Book::load_str(&text) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("book-check: không đọc được sách {path}: {e}");
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                eprintln!("book-check: không đọc được file {path}: {e}");
                return ExitCode::from(2);
            }
        },
        None => match Book::load_embedded() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("book-check: không đọc được sách nhúng: {e}");
                return ExitCode::from(2);
            }
        },
    };

    // 1. Tree.
    println!("# Cây sách khai cuộc");
    println!();
    for lesson in &book.lessons {
        println!(
            "## {} — {} ({}) [{}]",
            lesson.id,
            lesson.name_vi,
            lesson.name_zh,
            match lesson.learner {
                xq_book::schema::Learner::Red => "Đỏ học",
                xq_book::schema::Learner::Black => "Đen học",
            }
        );
        println!("Tóm tắt: {}", lesson.summary_vi);
        print_tree(&lesson.tree, 0);
        println!();
    }

    if !args.check_engine {
        return ExitCode::SUCCESS;
    }

    // 2. Engine check.
    let mut searcher = Searcher::new(args.tt_mb);
    let mut rows: Vec<Row> = Vec::new();
    let mut violations: Vec<String> = Vec::new();
    for lesson in &book.lessons {
        check_lesson(
            lesson,
            &mut searcher,
            args.depth,
            &mut rows,
            &mut violations,
        );
    }
    let graded = rows.iter().filter(|r| !r.trap).count();
    let trapped = rows.iter().filter(|r| r.trap).count();
    let worst = rows.iter().filter(|r| !r.trap).max_by_key(|r| r.delta);

    println!("# Báo cáo engine (depth {})", args.depth);
    println!();
    println!("- Số nước người học đã chấm: {graded} (miễn trap: {trapped})");
    if let Some(w) = worst {
        println!("- Delta lớn nhất: {} ({}, {})", w.delta, w.lesson, w.mv);
    }
    println!(
        "- Vi phạm (delta > 150, không tính trap): {}",
        violations.len()
    );
    println!();
    println!("| Bài | Nước | Delta | Nhãn |");
    println!("|-----|------|------:|------|");
    for row in &rows {
        if row.trap {
            println!("| {} | {} | — | {} |", row.lesson, row.mv, row.label);
        } else {
            println!(
                "| {} | {} | {} | {} |",
                row.lesson, row.mv, row.delta, row.label
            );
        }
    }
    if !violations.is_empty() {
        println!();
        println!("## Vi phạm");
        for v in &violations {
            println!("- {v}");
        }
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
