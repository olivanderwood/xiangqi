//! `Tutor`: guided opening study (§5.3, T5.1).
//!
//! Flow: the learner plays a move (`on_user_move`); if it is in the book
//! it grades `Book` and follows the tree, otherwise the engine grades it
//! (§4.5) and the lesson leaves the book. The AI replies with
//! `next_opponent_move`, picking a child weighted by `weight`. The lesson
//! ends at a leaf (`end`), at `max_plies`, or 8 plies after leaving the
//! book; the UI checks `is_complete()` and reads `summary()`.
//!
//! One deliberate extension of §5.3 rule 3 (see `docs/DECISIONS-m4.md`):
//! a book move that walks into a branch whose every leaf is bad for the
//! learner (e.g. the 甲 branch of `r-vs-yuanyang`) grades `Acceptable`
//! instead of `Book`, so a bad book line is not praised.

use crate::schema::{BookNode, Lesson, Verdict};
use xq_core::analysis::{explain_tags, grade_from_delta_with, tags_with_mate, Grade};
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::Game;
use xq_core::notation::{parse_cn, parse_wxf, render_cn};
use xq_core::types::{Color, Move};
use xq_engine::{shared_engine, BuiltinEngine, EngineRequest, SharedEngine};

/// Plies allowed after leaving the book before the lesson ends (§5.3).
const OFF_BOOK_LIMIT: usize = 8;

/// How close the 4th direction must be to the 3rd to be shown: within
/// 20 cp (~0.2 pawn) counts as "equally good" (Hieu, 2026-11-11).
pub const DIRECTION_EQUAL_CP: i32 = 20;

/// Number of directions to show for scores sorted best-first: 3, or 4
/// when the 4th scores within `DIRECTION_EQUAL_CP` of the 3rd.
#[must_use]
pub fn direction_count(scores: &[i32]) -> usize {
    let n = scores.len().min(3);
    if scores.len() >= 4 && scores[3] >= scores[2] - DIRECTION_EQUAL_CP {
        4
    } else {
        n
    }
}

pub struct BookHint {
    pub mv: Move,
    pub mv_cn: String,
    pub in_book: bool,
    pub score: i32,
    pub explanation_vi: String,
}

pub struct LessonSummary {
    pub book_moves: usize,
    pub total_learner_moves: usize,
    pub grades: Vec<(Move, Grade)>,
    pub final_score: i32,
    pub end_note_vi: Option<String>,
}

pub enum TutorEvent {
    YourTurn {
        book_moves: Vec<BookHint>,
        engine_needed: bool,
    },
    OpponentMoves {
        mv: Move,
        note_vi: Option<String>,
    },
    Graded {
        mv: Move,
        grade: Grade,
        best: Option<Move>,
        explanation_vi: String,
    },
    OpeningComplete {
        summary: LessonSummary,
    },
}

/// Snapshot taken just before the first off-book learner move, so `retry()`
/// can restore the game and the tree position exactly.
#[derive(Clone)]
struct Snapshot {
    moves_len: usize,
    node_path: Vec<usize>,
    grades_len: usize,
    book_moves: usize,
    out_of_book: bool,
    plies_after_book: usize,
    exit_ply: usize,
}

pub struct Tutor<'a> {
    pub lesson: &'a Lesson,
    pub game: Game,
    pub out_of_book: bool,
    pub node_path: Vec<usize>,
    pub grades: Vec<(Move, Grade)>,
    pub book_moves: usize,
    rng_state: u64,
    pub plies_after_book: usize,
    /// Engine used for off-book grading and hints, shared with the app's
    /// worker thread (see `xq-engine::SharedEngine`). Lazily created as a
    /// `BuiltinEngine` — a 32 MB TT per `Tutor::new` would be wasteful:
    /// the weight test alone builds 1000 tutors.
    engine: Option<SharedEngine>,
    snapshot: Option<Snapshot>,
    exit_ply: usize,
}

impl<'a> Tutor<'a> {
    #[must_use]
    pub fn new(lesson: &'a Lesson, rng_seed: u64) -> Self {
        let start = lesson
            .start_fen
            .as_deref()
            .map(|f| Position::from_fen(f).expect("lesson start_fen is valid"))
            .unwrap_or_else(Position::startpos);
        Self {
            lesson,
            game: Game::new(start),
            out_of_book: false,
            node_path: Vec::new(),
            grades: Vec::new(),
            book_moves: 0,
            rng_state: if rng_seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                rng_seed
            },
            plies_after_book: 0,
            engine: None,
            snapshot: None,
            exit_ply: usize::MAX,
        }
    }

    /// Build with a shared engine (e.g. the app's active engine, Pikafish
    /// when available) instead of a lazily created builtin one.
    #[must_use]
    pub fn with_engine(lesson: &'a Lesson, rng_seed: u64, engine: SharedEngine) -> Self {
        let mut t = Self::new(lesson, rng_seed);
        t.engine = Some(engine);
        t
    }

    /// xorshift64star: uniform enough for weighted child picks.
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform double in [0, 1) from the high 53 bits.
    fn next_unit(&mut self) -> f64 {
        ((self.next_rand() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    /// Lazily built engine (a 32 MB TT per `Tutor::new` would be wasteful:
    /// the weight test alone builds 1000 tutors).
    fn engine(&mut self) -> SharedEngine {
        if self.engine.is_none() {
            self.engine = Some(shared_engine(Box::new(BuiltinEngine::new())));
        }
        self.engine.as_ref().unwrap().clone()
    }

    /// Tags for an analysed line: the rule-based tags plus `MateIn` when
    /// the line mates (mirrors `Searcher::analyze`).
    fn line_tags(pos: &Position, line: &xq_engine::Line) -> Vec<xq_core::search::MoveTag> {
        tags_with_mate(pos, line.mv, line.score.to_cp_clamped())
    }

    /// Current tree node (children = legal book replies here).
    #[must_use]
    pub fn current_children(&self) -> &[BookNode] {
        crate::tree::node_at(&self.lesson.tree, &self.node_path)
            .map(|n| n.next.as_slice())
            .unwrap_or(&self.lesson.tree)
    }

    /// Current tree node, if we are past the root.
    fn current_node(&self) -> Option<&BookNode> {
        crate::tree::node_at(&self.lesson.tree, &self.node_path)
    }

    /// Parse one book move at the current position (Chinese first, WXF
    /// fallback — §5.4 sources use both).
    fn parse_book_mv(&self, s: &str) -> Option<Move> {
        parse_cn(&self.game.pos, s)
            .or_else(|_| parse_wxf(&self.game.pos, s))
            .ok()
    }

    /// The `main` child move at this node, if any.
    fn main_child_move(&self) -> Option<Move> {
        self.current_children()
            .iter()
            .find(|n| n.main)
            .and_then(|n| self.parse_book_mv(&n.mv))
    }

    /// True when every leaf below `node` is bad for the learner.
    fn all_leaves_bad_for_learner(node: &BookNode, learner: Color) -> bool {
        let mut leaves: Vec<Verdict> = Vec::new();
        fn collect(n: &BookNode, out: &mut Vec<Verdict>) {
            if let Some(end) = &n.end {
                out.push(end.verdict);
            }
            for c in &n.next {
                collect(c, out);
            }
        }
        collect(node, &mut leaves);
        !leaves.is_empty()
            && leaves.iter().all(|v| match learner {
                Color::Red => *v == Verdict::BlackBetter,
                Color::Black => *v == Verdict::RedBetter,
            })
    }

    /// Summary of the lesson so far (§5.3). `final_score` is the static
    /// eval from the learner's point of view.
    #[must_use]
    pub fn summary(&self) -> LessonSummary {
        let raw = eval::eval(&self.game.pos);
        let final_score = if self.game.pos.side() == self.lesson.learner.color() {
            raw
        } else {
            -raw
        };
        LessonSummary {
            book_moves: self.book_moves,
            total_learner_moves: self.grades.len(),
            grades: self.grades.clone(),
            final_score,
            end_note_vi: self
                .current_node()
                .and_then(|n| n.end.as_ref())
                .map(|e| e.note_vi.clone()),
        }
    }

    pub fn on_user_move(&mut self, mv: Move) -> TutorEvent {
        if self.is_complete() {
            return TutorEvent::OpeningComplete {
                summary: self.summary(),
            };
        }
        let learner = self.lesson.learner.color();
        // Illegal move: never play it; grade it as a blunder.
        if !self.game.pos.legal_moves().contains(&mv) {
            let best = self.main_child_move();
            return TutorEvent::Graded {
                mv,
                grade: Grade::Blunder,
                best,
                explanation_vi: "Nước đi không hợp lệ ở thế cờ này.".to_string(),
            };
        }
        // In-book child?
        let child_idx = self
            .current_children()
            .iter()
            .position(|n| self.parse_book_mv(&n.mv).is_some_and(|m| m == mv));
        if let Some(idx) = child_idx {
            let node = &self.current_children()[idx];
            let bad_branch = Self::all_leaves_bad_for_learner(node, learner);
            let grade = if bad_branch {
                Grade::Acceptable
            } else {
                Grade::Book
            };
            let explanation = if bad_branch {
                "Nước này có trong sách, nhưng biến này dẫn tới thế xấu cho bạn — nên xem lại."
                    .to_string()
            } else {
                node.note_vi
                    .clone()
                    .unwrap_or_else(|| "Đúng sách.".to_string())
            };
            self.game.play(mv).expect("nước trong sách hợp lệ");
            self.node_path.push(idx);
            self.book_moves += 1;
            self.grades.push((mv, grade));
            if self.is_complete() {
                return TutorEvent::OpeningComplete {
                    summary: self.summary(),
                };
            }
            return TutorEvent::Graded {
                mv,
                grade,
                best: None,
                explanation_vi: explanation,
            };
        }
        // Off-book: snapshot once, before the first mistake.
        if self.snapshot.is_none() {
            self.snapshot = Some(Snapshot {
                moves_len: self.game.moves.len(),
                node_path: self.node_path.clone(),
                grades_len: self.grades.len(),
                book_moves: self.book_moves,
                out_of_book: self.out_of_book,
                plies_after_book: self.plies_after_book,
                exit_ply: self.exit_ply,
            });
        }
        let best_main = self.main_child_move();
        let handle = self.engine();
        let mut engine = handle.lock().expect("engine lock");
        let req = EngineRequest::new(
            self.game.start.to_fen(),
            self.game.moves.clone(),
            engine.grade_limits(),
        );
        let (grade, engine_best, delta) = match engine.best_move(&req, &mut |_| {}) {
            Ok(best_line) => {
                let best_score = best_line.score.to_cp_clamped();
                let mv_score = engine
                    .score_move(&req, mv)
                    .map(|s| s.to_cp_clamped())
                    .unwrap_or(best_score);
                let delta = best_score - mv_score;
                let grade = grade_from_delta_with(delta, false, engine.grade_thresholds());
                (grade, Some(best_line.mv), delta)
            }
            Err(_) => (Grade::Acceptable, None, 0),
        };
        drop(engine);
        let best = best_main.or(engine_best);
        let explanation = format!(
            "{} (chênh {} so với nước tốt nhất).",
            grade.label_vi(),
            delta.max(0)
        );
        self.game.play(mv).expect("nước ngoài sách hợp lệ");
        if !self.out_of_book {
            self.exit_ply = self.game.moves.len() - 1;
            self.out_of_book = true;
        }
        self.plies_after_book = self.game.moves.len() - self.exit_ply;
        self.grades.push((mv, grade));
        if self.is_complete() {
            return TutorEvent::OpeningComplete {
                summary: self.summary(),
            };
        }
        TutorEvent::Graded {
            mv,
            grade,
            best,
            explanation_vi: explanation,
        }
    }

    pub fn next_opponent_move(&mut self) -> Option<TutorEvent> {
        if self.out_of_book || self.game.result().is_over() {
            return None;
        }
        if self.game.pos.side() == self.lesson.learner.color() {
            return None;
        }
        // Copy the child data first: picking needs `&mut self` (RNG).
        let children: Vec<(f64, String, Option<String>)> = self
            .current_children()
            .iter()
            .map(|n| (n.weight.max(0.0), n.mv.clone(), n.note_vi.clone()))
            .collect();
        if children.is_empty() {
            return None;
        }
        let total: f64 = children.iter().map(|(w, _, _)| w).sum();
        if total <= 0.0 {
            return None;
        }
        let mut pick = self.next_unit() * total;
        let mut idx = children.len() - 1;
        for (i, (w, _, _)) in children.iter().enumerate() {
            pick -= w;
            if pick <= 0.0 {
                idx = i;
                break;
            }
        }
        let (_, mv_str, note_vi) = &children[idx];
        let mv = self.parse_book_mv(mv_str)?;
        self.game.play(mv).expect("nước của AI trong sách hợp lệ");
        self.node_path.push(idx);
        Some(TutorEvent::OpponentMoves {
            mv,
            note_vi: note_vi.clone(),
        })
    }

    pub fn retry(&mut self) {
        let Some(snap) = self.snapshot.take() else {
            return;
        };
        while self.game.moves.len() > snap.moves_len {
            self.game.undo();
        }
        self.node_path = snap.node_path;
        self.grades.truncate(snap.grades_len);
        self.book_moves = snap.book_moves;
        self.out_of_book = snap.out_of_book;
        self.plies_after_book = snap.plies_after_book;
        self.exit_ply = snap.exit_ply;
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        if self.game.result().is_over() {
            return true;
        }
        if self.game.moves.len() >= self.lesson.max_plies {
            return true;
        }
        if let Some(node) = self.current_node() {
            if node.end.is_some() {
                return true;
            }
        }
        if self.out_of_book {
            let after = self.game.moves.len().saturating_sub(self.exit_ply);
            if after >= OFF_BOOK_LIMIT {
                return true;
            }
        }
        false
    }

    /// Hints for the learner's turn: book children plus engine lines,
    /// ordered by engine score (1 = best), keeping 3 directions, or 4
    /// when the 4th is evaluated as equally good as the 3rd (§5.3).
    #[must_use]
    pub fn suggestions(&mut self) -> Vec<BookHint> {
        let pos = self.game.pos.clone();
        // Copy the child data first: scoring needs `&mut self` (engine).
        let child_data: Vec<(bool, String, String)> = self
            .current_children()
            .iter()
            .map(|n| (n.main, n.mv.clone(), n.note_vi.clone().unwrap_or_default()))
            .collect();
        let handle = self.engine();
        let mut engine = handle.lock().expect("engine lock");
        let req = EngineRequest::new(
            self.game.start.to_fen(),
            self.game.moves.clone(),
            engine.hint_limits(),
        );
        let mut hints: Vec<BookHint> = Vec::new();
        let mut book_moves: Vec<Move> = Vec::new();
        let mut scored: Vec<(i32, Move, String)> = Vec::new();
        for (_is_main, mv_str, note) in &child_data {
            let Some(mv) = self.parse_book_mv(mv_str) else {
                continue;
            };
            let Ok(score) = engine.score_move(&req, mv) else {
                continue;
            };
            scored.push((score.to_cp_clamped(), mv, note.clone()));
            book_moves.push(mv);
        }
        // Ordered by engine score, so #1 is always the best move.
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        for (score, mv, note) in scored {
            hints.push(BookHint {
                mv,
                mv_cn: render_cn(&pos, mv),
                in_book: true,
                score,
                explanation_vi: note,
            });
        }
        if !self.out_of_book {
            // Top 4 engine lines; the 4th survives only when it scores
            // within `DIRECTION_EQUAL_CP` of the 3rd (see below).
            let lines = engine.analyze(&req, 4, &mut |_| {}).unwrap_or_default();
            for line in lines {
                if book_moves.contains(&line.mv) {
                    continue;
                }
                let tags = Self::line_tags(&pos, &line);
                hints.push(BookHint {
                    mv: line.mv,
                    mv_cn: render_cn(&pos, line.mv),
                    in_book: false,
                    score: line.score.to_cp_clamped(),
                    explanation_vi: explain_tags(&tags),
                });
            }
        }
        // 1 = best by score; keep 3 directions, or 4 when the 4th is
        // evaluated as equally good as the 3rd.
        hints.sort_by_key(|a| std::cmp::Reverse(a.score));
        let scores: Vec<i32> = hints.iter().map(|h| h.score).collect();
        hints.truncate(direction_count(&scores));
        hints
    }

    /// The `YourTurn` event for the UI to ask when the learner should move.
    #[must_use]
    pub fn hints_for_learner_turn(&mut self) -> TutorEvent {
        let engine_needed = self.out_of_book || self.current_children().is_empty();
        let book_moves = self.suggestions();
        TutorEvent::YourTurn {
            book_moves,
            engine_needed,
        }
    }

    #[must_use]
    pub fn progress_text(&self) -> String {
        format!(
            "Nước {} / khoảng {}",
            self.game.moves.len(),
            self.lesson.max_plies
        )
    }
}

#[cfg(test)]
mod tests {
    use super::direction_count;

    #[test]
    fn direction_count_keeps_three() {
        assert_eq!(direction_count(&[]), 0);
        assert_eq!(direction_count(&[50]), 1);
        assert_eq!(direction_count(&[50, 30]), 2);
        assert_eq!(direction_count(&[50, 30, 10]), 3);
        // 4th far below the 3rd: still 3.
        assert_eq!(direction_count(&[50, 30, 10, -50]), 3);
    }

    #[test]
    fn direction_count_keeps_four_when_equal() {
        // 4th within 20 cp of the 3rd: equally good, show it.
        assert_eq!(direction_count(&[50, 30, 10, 0]), 4);
        assert_eq!(direction_count(&[50, 30, 10, -10]), 4);
        // Exactly at the boundary (20 cp): still equal.
        assert_eq!(direction_count(&[50, 30, 10, -10]), 4);
        // 21 cp below: not equal.
        assert_eq!(direction_count(&[50, 30, 10, -11]), 3);
    }
}
