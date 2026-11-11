//! `gen-book`: regenerate `openings.json` from Pikafish (MultiPV=4).
//!
//! Usage:
//!   gen-book [--src <lessons-src.json>] [--out <openings.json>]
//!            [--depth N] [--plies N] [--side-plies N] [--leaf-depth N]
//!            [--max-gap N]
//!
//! For each lesson in the source file, the defining prefix (the opening
//! system's signature moves) is played, then Pikafish grows the spine:
//! at every spine node the top 3 directions (or 4 when the 4th is within
//! `DIRECTION_EQUAL_CP` of the 3rd, the same rule the Tutor shows) become
//! the node's children, the best move continues the spine to `--plies`
//! plies, and every side direction carries its principal variation as a
//! short chain (`--side-plies` plies). Every leaf gets an `end` verdict
//! from a final analysis, and every learner-turn node gets a `note_vi`.
//! Every node — prefix, spine, side chain — carries its Pikafish score
//! (`BookNode::score_cp`): the dataset IS the book, so the Tutor never
//! re-scores a move with a live engine and every attempt is identical.
//! The output is self-checked (every move legal, notes/ends/scores
//! present, no duplicate siblings) before it is written.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use serde::Deserialize;
use xq_book::direction_count;
use xq_book::schema::{BookFile, BookNode, End, Learner, Lesson, Source, Verdict};
use xq_core::board::Position;
use xq_core::eval::MATE;
use xq_core::notation::{parse_cn, render_cn};
use xq_core::types::{Color, Move};
use xq_engine::{Engine, EngineRequest, SearchLimits};
use xq_pikafish::{locate, EngineOptions, PikafishEngine};

/// One lesson definition: the system's identity; Pikafish grows the tree.
#[derive(Deserialize)]
struct LessonDef {
    id: String,
    name_vi: String,
    name_zh: String,
    learner: Learner,
    level: u8,
    summary_vi: String,
    sources: Vec<Source>,
    prefix: Vec<String>,
}

struct Args {
    src: PathBuf,
    out: PathBuf,
    depth: u8,
    plies: usize,
    side_plies: usize,
    leaf_depth: u8,
    /// A direction this far below the best (cp) is not stored: beyond
    /// ~3 pawns it is a mistake, not a direction to teach.
    max_gap: i32,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        src: PathBuf::from("crates/xq-book/data/lessons-src.json"),
        out: PathBuf::from("crates/xq-book/data/openings.json"),
        depth: 12,
        plies: 40,
        side_plies: 8,
        leaf_depth: 8,
        max_gap: 300,
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--src" => {
                args.src = PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--src cần một đường dẫn".to_string())?,
                );
            }
            "--out" => {
                args.out = PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--out cần một đường dẫn".to_string())?,
                );
            }
            "--depth" => {
                args.depth = it
                    .next()
                    .ok_or_else(|| "--depth cần một số".to_string())?
                    .parse()
                    .map_err(|_| "--depth phải là số".to_string())?;
            }
            "--plies" => {
                args.plies = it
                    .next()
                    .ok_or_else(|| "--plies cần một số".to_string())?
                    .parse()
                    .map_err(|_| "--plies phải là số".to_string())?;
            }
            "--side-plies" => {
                args.side_plies = it
                    .next()
                    .ok_or_else(|| "--side-plies cần một số".to_string())?
                    .parse()
                    .map_err(|_| "--side-plies phải là số".to_string())?;
            }
            "--leaf-depth" => {
                args.leaf_depth = it
                    .next()
                    .ok_or_else(|| "--leaf-depth cần một số".to_string())?
                    .parse()
                    .map_err(|_| "--leaf-depth phải là số".to_string())?;
            }
            "--max-gap" => {
                args.max_gap = it
                    .next()
                    .ok_or_else(|| "--max-gap cần một số".to_string())?
                    .parse()
                    .map_err(|_| "--max-gap phải là số".to_string())?;
            }
            _ => return Err(format!("tham số lạ: {a}")),
        }
    }
    Ok(args)
}

/// A centipawn score as a short pawn string: `+0.8`, `-0.3`, `M3`.
fn fmt_score(cp: i32) -> String {
    if cp >= MATE - 500 {
        return format!("M{}", (MATE - cp + 1) / 2);
    }
    if cp <= -MATE + 500 {
        return format!("-M{}", (MATE + cp + 1) / 2);
    }
    format!("{:+.1}", cp as f32 / 100.0)
}

/// Verdict + note for a leaf position, from Red's perspective.
fn verdict_for(red_cp: i32) -> (Verdict, String) {
    if red_cp >= MATE - 500 {
        (
            Verdict::RedBetter,
            format!("Pikafish: Đỏ chiếu hết (M{})", (MATE - red_cp + 1) / 2),
        )
    } else if red_cp <= -MATE + 500 {
        (
            Verdict::BlackBetter,
            format!("Pikafish: Đen chiếu hết (M{})", (MATE + red_cp + 1) / 2),
        )
    } else if red_cp > 50 {
        (
            Verdict::RedBetter,
            format!("Pikafish: Đỏ hơn {:.1} tốt", red_cp as f32 / 100.0),
        )
    } else if red_cp < -50 {
        (
            Verdict::BlackBetter,
            format!("Pikafish: Đen hơn {:.1} tốt", -red_cp as f32 / 100.0),
        )
    } else {
        (Verdict::Equal, "Pikafish: cân bằng".to_string())
    }
}

struct Gen<'a> {
    engine: &'a mut PikafishEngine,
    start_fen: String,
    depth: u8,
    leaf_depth: u8,
    side_plies: usize,
    learner: Color,
    analyses: usize,
    leaf_analyses: usize,
    max_gap: i32,
    /// Directions dropped for being more than `max_gap` below the best.
    dropped_far: usize,
    /// Worst (largest) gap between the best direction and a stored
    /// 2nd/3rd/4th direction, per lesson, for the report.
    worst_gap: i32,
}

impl<'a> Gen<'a> {
    /// Analyse `pos` (MultiPV=4) and return the lines' moves, scores and PVs.
    fn analyze(
        &mut self,
        moves: &[Move],
        top_n: usize,
    ) -> Result<Vec<(Move, i32, Vec<Move>)>, String> {
        let req = EngineRequest::new(
            self.start_fen.clone(),
            moves.to_vec(),
            SearchLimits::depth(self.depth),
        );
        let lines = self
            .engine
            .analyze(&req, top_n, &mut |_| {})
            .map_err(|e| format!("Pikafish lỗi: {e}"))?;
        self.analyses += 1;
        Ok(lines
            .into_iter()
            .map(|l| (l.mv, l.score.to_cp_clamped(), l.pv))
            .collect())
    }

    /// Score ONE move at `moves` (the path to the position): the move's
    /// Pikafish score from the mover's perspective — the same convention
    /// as the MultiPV line scores. Used for the hand-written prefix
    /// moves, which no analysis covers.
    fn score_move(&mut self, moves: &[Move], mv: Move) -> Result<i32, String> {
        let req = EngineRequest::new(
            self.start_fen.clone(),
            moves.to_vec(),
            SearchLimits::depth(self.depth),
        );
        let score = self
            .engine
            .score_move(&req, mv)
            .map_err(|e| format!("Pikafish lỗi: {e}"))?;
        self.analyses += 1;
        Ok(score.to_cp_clamped())
    }

    /// Children of `pos`: the top directions from Pikafish. The best
    /// direction grows the spine (`remaining` plies left); side directions
    /// carry their PV as a short chain. `moves` is the path from the start
    /// to `pos`; `seen` holds the spine-path FENs (repetition guard).
    fn children_at(
        &mut self,
        pos: &Position,
        moves: &[Move],
        remaining: usize,
        seen: &mut HashSet<String>,
    ) -> Result<Vec<BookNode>, String> {
        if remaining == 0 {
            return Ok(Vec::new());
        }
        let lines = self.analyze(moves, 4)?;
        if lines.is_empty() {
            return Ok(Vec::new());
        }
        let scores: Vec<i32> = lines.iter().map(|(_, s, _)| *s).collect();
        let keep = direction_count(&scores);
        let best_cp = scores[0];
        // Decisive already: keep the directions, but grow no further.
        let grow = remaining > 1 && best_cp.abs() <= 600;
        let mut children: Vec<BookNode> = Vec::new();
        for (i, (mv, score, pv)) in lines.into_iter().take(keep).enumerate() {
            // A direction far below the best is a mistake, not a lesson.
            if i > 0 && best_cp - score > self.max_gap {
                self.dropped_far += 1;
                continue;
            }
            if i > 0 {
                self.worst_gap = self.worst_gap.max(best_cp - score);
            }
            let is_main = i == 0;
            let mv_cn = render_cn(pos, mv);
            let mut next_pos = pos.clone();
            next_pos.make_move(mv);
            let mut next_moves = moves.to_vec();
            next_moves.push(mv);
            let note = if pos.side() == self.learner {
                if is_main {
                    Some(format!("Pikafish: {}", fmt_score(score)))
                } else {
                    Some("Hướng theo Pikafish".to_string())
                }
            } else {
                None
            };
            let next = if is_main {
                if grow && !seen.contains(&next_pos.to_fen()) {
                    seen.insert(next_pos.to_fen());
                    let sub = self.children_at(&next_pos, &next_moves, remaining - 1, seen)?;
                    seen.remove(&next_pos.to_fen());
                    sub
                } else {
                    Vec::new()
                }
            } else {
                let cap = self.side_plies.min(remaining - 1);
                self.side_chain(mv, &next_pos, &pv, score, cap, seen)?
            };
            children.push(BookNode {
                mv: mv_cn,
                note_vi: note,
                next,
                weight: if is_main { 1.0 } else { 0.2 },
                main: is_main,
                trap: false,
                source_ref: None,
                end: None,
                // The dataset IS the book: every direction carries the
                // Pikafish score it was generated with, so the Tutor
                // never re-scores a move with a live engine.
                score_cp: Some(score),
            });
        }
        Ok(children)
    }

    /// A side direction's principal variation as a chain of single-child
    /// nodes (at most `cap` plies). `pv[0]` must be the child move itself,
    /// so the chain is `pv[1..]` played from `pos_after_child`; every move
    /// is validated as legal, and the chain stops at a repeated position.
    /// Each chain node's score is the line score propagated down the PV
    /// with alternating sign (`-S, +S, -S, ...`): the PV is optimal play,
    /// so the position after the first PV move is worth `-S` to the new
    /// side to move, and so on. An approximation (a fresh analysis would
    /// differ by a few dozen cp), but deterministic — and the dataset
    /// must be identical on every run.
    fn side_chain(
        &mut self,
        first: Move,
        pos_after_child: &Position,
        pv: &[Move],
        line_score: i32,
        cap: usize,
        seen: &HashSet<String>,
    ) -> Result<Vec<BookNode>, String> {
        if pv.first() != Some(&first) {
            return Ok(Vec::new());
        }
        let mut chain: Vec<(Position, Move, i32)> = Vec::new();
        let mut cur = pos_after_child.clone();
        let mut chain_seen: HashSet<String> = seen.clone();
        chain_seen.insert(cur.to_fen());
        for &mv in pv.iter().skip(1).take(cap) {
            if !cur.legal_moves().contains(&mv) {
                break;
            }
            let mut after = cur.clone();
            after.make_move(mv);
            if chain_seen.contains(&after.to_fen()) {
                break;
            }
            chain_seen.insert(after.to_fen());
            // chain[j] is pv[j+1]: its score is (-1)^(j+1) * line_score.
            let j = chain.len();
            let score = if j.is_multiple_of(2) {
                -line_score
            } else {
                line_score
            };
            chain.push((cur.clone(), mv, score));
            cur = after;
        }
        // Build the chain inside-out: the last node has no children.
        let mut next: Vec<BookNode> = Vec::new();
        for (p, mv, score) in chain.into_iter().rev() {
            let note = if p.side() == self.learner {
                Some("Hướng theo Pikafish".to_string())
            } else {
                None
            };
            next = vec![BookNode {
                mv: render_cn(&p, mv),
                note_vi: note,
                next,
                weight: 1.0,
                main: true,
                trap: false,
                source_ref: None,
                end: None,
                score_cp: Some(score),
            }];
        }
        Ok(next)
    }

    /// Every leaf gets an `end` verdict from a final analysis of its
    /// position (mate/stalemate handled without the engine).
    fn finish_leaves(
        &mut self,
        nodes: &mut [BookNode],
        pos: &Position,
        moves: &[Move],
    ) -> Result<(), String> {
        for node in nodes {
            let mv = parse_cn(pos, &node.mv)
                .map_err(|e| format!("nước trong cây không parse được {:?}: {e:?}", node.mv))?;
            let mut next_pos = pos.clone();
            next_pos.make_move(mv);
            let mut next_moves = moves.to_vec();
            next_moves.push(mv);
            if node.next.is_empty() {
                node.end = Some(self.verdict_at(&next_pos, &next_moves)?);
            } else {
                self.finish_leaves(&mut node.next, &next_pos, &next_moves)?;
            }
        }
        Ok(())
    }

    fn verdict_at(&mut self, pos: &Position, moves: &[Move]) -> Result<End, String> {
        if pos.legal_moves().is_empty() {
            // Mate or stalemate: no engine needed.
            if pos.in_check(pos.side()) {
                let (verdict, note) = if pos.side() == Color::Red {
                    (Verdict::BlackBetter, "Chiếu hết: Đen thắng.".to_string())
                } else {
                    (Verdict::RedBetter, "Chiếu hết: Đỏ thắng.".to_string())
                };
                return Ok(End {
                    verdict,
                    note_vi: note,
                });
            }
            return Ok(End {
                verdict: Verdict::Equal,
                note_vi: "Hòa cờ (hết nước đi).".to_string(),
            });
        }
        let req = EngineRequest::new(
            self.start_fen.clone(),
            moves.to_vec(),
            SearchLimits::depth(self.leaf_depth),
        );
        self.leaf_analyses += 1;
        let lines = self
            .engine
            .analyze(&req, 1, &mut |_| {})
            .unwrap_or_default();
        let Some(line) = lines.into_iter().next() else {
            return Ok(End {
                verdict: Verdict::Equal,
                note_vi: "Pikafish không đánh giá được thế này.".to_string(),
            });
        };
        let cp = line.score.to_cp_clamped();
        let red_cp = if pos.side() == Color::Red { cp } else { -cp };
        let (verdict, note_vi) = verdict_for(red_cp);
        Ok(End { verdict, note_vi })
    }
}

fn count_nodes(nodes: &[BookNode], leaves: &mut usize) -> usize {
    let mut n = 0;
    for node in nodes {
        n += 1;
        if node.next.is_empty() {
            *leaves += 1;
        } else {
            n += count_nodes(&node.next, leaves);
        }
    }
    n
}

/// Self-check mirroring `validate_book.rs`: every move legal, learner
/// nodes have notes (<=160 chars), leaves have ends, no duplicate
/// siblings, `end` only on leaves, every node has a Pikafish score.
fn self_check(lesson: &Lesson) -> Result<(), String> {
    fn walk(
        lesson: &Lesson,
        nodes: &[BookNode],
        pos: &Position,
        path: &mut Vec<String>,
    ) -> Result<(), String> {
        let learner = lesson.learner.color();
        let mut seen: HashSet<String> = HashSet::new();
        for node in nodes {
            if !seen.insert(node.mv.clone()) {
                return Err(format!(
                    "{}: đường {}: hai nút anh em cùng nước {:?}",
                    lesson.id,
                    path.join(" > "),
                    node.mv
                ));
            }
            let mv = parse_cn(pos, &node.mv)
                .map_err(|e| format!("{}: nước {:?} không hợp lệ: {e:?}", lesson.id, node.mv))?;
            if node.score_cp.is_none() {
                return Err(format!(
                    "{}: đường {}: nút {:?} thiếu score_cp",
                    lesson.id,
                    path.join(" > "),
                    node.mv
                ));
            }
            if pos.side() == learner {
                match &node.note_vi {
                    Some(note) if !note.trim().is_empty() && note.chars().count() <= 160 => {}
                    _ => {
                        return Err(format!(
                            "{}: đường {}: nước của người học thiếu note_vi",
                            lesson.id,
                            path.join(" > ")
                        ));
                    }
                }
            }
            if node.end.is_some() && !node.next.is_empty() {
                return Err(format!(
                    "{}: đường {}: nút vừa có end vừa có next",
                    lesson.id,
                    path.join(" > ")
                ));
            }
            if node.next.is_empty() {
                match &node.end {
                    Some(end) if !end.note_vi.trim().is_empty() => {}
                    _ => {
                        return Err(format!(
                            "{}: đường {}: lá thiếu end",
                            lesson.id,
                            path.join(" > ")
                        ));
                    }
                }
            }
            path.push(node.mv.clone());
            let mut next_pos = pos.clone();
            next_pos.make_move(mv);
            walk(lesson, &node.next, &next_pos, path)?;
            path.pop();
        }
        Ok(())
    }
    let start = Position::startpos();
    walk(lesson, &lesson.tree, &start, &mut Vec::new())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-book lỗi: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let text = fs::read_to_string(&args.src)
        .map_err(|e| format!("không đọc được {}: {e}", args.src.display()))?;
    let defs: Vec<LessonDef> = serde_json::from_str(&text)
        .map_err(|e| format!("{} không parse được: {e}", args.src.display()))?;
    if defs.is_empty() {
        return Err("không có bài học nào trong file nguồn".to_string());
    }
    let located = locate(None).map_err(|e| format!("không tìm thấy Pikafish: {e}"))?;
    let mut options = EngineOptions::new(located.nnue.clone());
    options.threads = 2;
    options.hash_mb = 128;
    let mut engine = PikafishEngine::spawn(located, options)
        .map_err(|e| format!("không chạy được Pikafish: {e}"))?;
    let start_fen = Position::startpos().to_fen();
    let mut lessons: Vec<Lesson> = Vec::new();
    let mut total_nodes = 0;
    let mut total_leaves = 0;
    let mut total_analyses = 0;
    let t0 = Instant::now();
    for def in &defs {
        let lt0 = Instant::now();
        let learner = def.learner.color();
        let mut gen = Gen {
            engine: &mut engine,
            start_fen: start_fen.clone(),
            depth: args.depth,
            leaf_depth: args.leaf_depth,
            side_plies: args.side_plies,
            learner,
            analyses: 0,
            leaf_analyses: 0,
            max_gap: args.max_gap,
            dropped_far: 0,
            worst_gap: 0,
        };
        // Play the prefix; every move must be legal. `positions[i]` is
        // the position before prefix move `i`. Each prefix move is scored
        // with Pikafish (`score_move`, same depth as the spine) — the
        // prefix is part of the dataset too, so its nodes need scores.
        let mut positions: Vec<Position> = vec![Position::startpos()];
        let mut moves: Vec<Move> = Vec::new();
        let mut prefix_scores: Vec<i32> = Vec::new();
        {
            let mut p = Position::startpos();
            for s in &def.prefix {
                let mv = parse_cn(&p, s)
                    .map_err(|e| format!("{}: prefix {:?} không hợp lệ: {e:?}", def.id, s))?;
                let score = gen
                    .score_move(&moves, mv)
                    .map_err(|e| format!("{}: prefix {:?}: {e}", def.id, s))?;
                prefix_scores.push(score);
                p.make_move(mv);
                moves.push(mv);
                positions.push(p.clone());
            }
        }
        if moves.len() >= args.plies {
            return Err(format!(
                "{}: prefix dài {} nửa nước, không dưới {}",
                def.id,
                moves.len(),
                args.plies
            ));
        }
        let pos = positions.last().expect("prefix đã đi").clone();
        let mut seen: HashSet<String> = positions.iter().map(|p| p.to_fen()).collect();
        let remaining = args.plies - moves.len();
        let grown = gen.children_at(&pos, &moves, remaining, &mut seen)?;
        // Wrap the prefix as a chain of single-child nodes above `grown`.
        let mut tree = grown;
        for (i, s) in def.prefix.iter().enumerate().rev() {
            let before = &positions[i];
            let mv =
                parse_cn(before, s).map_err(|e| format!("{}: prefix {:?}: {e:?}", def.id, s))?;
            let note = if before.side() == learner {
                Some("Nước mở đầu của hệ khai cuộc.".to_string())
            } else {
                None
            };
            tree = vec![BookNode {
                mv: render_cn(before, mv),
                note_vi: note,
                next: tree,
                weight: 1.0,
                main: true,
                trap: false,
                source_ref: None,
                end: None,
                score_cp: Some(prefix_scores[i]),
            }];
        }
        gen.finish_leaves(&mut tree, &Position::startpos(), &[])?;
        let mut leaves = 0;
        let nodes = count_nodes(&tree, &mut leaves);
        let lesson = Lesson {
            id: def.id.clone(),
            name_vi: def.name_vi.clone(),
            name_zh: def.name_zh.clone(),
            learner: def.learner,
            level: def.level,
            summary_vi: def.summary_vi.clone(),
            start_fen: None,
            max_plies: args.plies,
            sources: def.sources.clone(),
            tree,
        };
        self_check(&lesson)?;
        println!(
            "{}: {} nút, {} lá, {} phân tích (+{} lá), lệch lớn nhất {} cp ({} hướng bỏ vì lệch >{} cp), {:.1}s",
            lesson.id,
            nodes,
            leaves,
            gen.analyses,
            gen.leaf_analyses,
            gen.worst_gap,
            gen.dropped_far,
            args.max_gap,
            lt0.elapsed().as_secs_f32()
        );
        total_nodes += nodes;
        total_leaves += leaves;
        total_analyses += gen.analyses + gen.leaf_analyses;
        lessons.push(lesson);
    }
    let file = BookFile {
        schema_version: 1,
        lessons,
    };
    // Minified: the file has ~17k nodes and pretty-printing would be
    // ~10x bigger (23 MB); the human-readable source is lessons-src.json.
    let out = serde_json::to_string(&file).map_err(|e| format!("không ghi được JSON: {e}"))?;
    fs::write(&args.out, out).map_err(|e| format!("không ghi {}: {e}", args.out.display()))?;
    println!(
        "xong: {} bài, {} nút, {} lá, {} lần phân tích, tổng {:.1}s -> {}",
        defs.len(),
        total_nodes,
        total_leaves,
        total_analyses,
        t0.elapsed().as_secs_f32(),
        args.out.display()
    );
    Ok(())
}
