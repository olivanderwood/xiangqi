//! Learn screen (§6.4): tutor panel, hints, grading, opening-complete.
//!
//! The screen drives the lesson through the `xq-book` `Tutor` API
//! (`Tutor::new / on_user_move / next_opponent_move / retry /
//! hints_for_learner_turn / summary / is_complete`). The Tutor is the
//! source of truth for grading and tree position: after every Tutor call
//! the screen syncs its own state from it (`sync_from_tutor`). Local
//! book-tree / engine fallbacks only run when no Tutor is attached or
//! when the Tutor returns `None` (out of book: the opponent is then
//! played by the AI worker).

use crate::ai_worker::{response_is_current, AiRequest, AiResponse};
use crate::app::{
    advantage_chip_ui, big_button, eval_bar_ui, is_narrow, move_list_ui, section_title, Screen,
    XiangqiApp,
};
use crate::board_view::{
    draw_board, legal_targets, BoardDraw, BoardGeometry, BoardInteraction, HintArrow, MoveAnim,
};
use crate::i18n::{self, Key};
use std::time::Duration;
use xq_book::schema::{BookNode, Lesson};
use xq_book::tree::node_at;
use xq_book::{BookHint, LessonSummary, Tutor, TutorEvent};
use xq_core::analysis::{explain_tags, tags_for, Grade};
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::Game;
use xq_core::notation;
use xq_core::types::{Color, Move, PieceKind};
use xq_engine::{EngineRequest, Level, SharedEngine};

pub struct LearnState {
    pub lesson: Option<Lesson>,
    // Declared before `lesson_box`: Tutor is dropped first, so the
    // borrowed lesson always outlives the Tutor that references it.
    pub tutor: Option<Tutor<'static>>,
    pub lesson_box: Option<Box<Lesson>>,
    pub game: Game,
    pub node_path: Vec<usize>,
    pub grades: Vec<(Move, Grade)>,
    /// Best move recorded per graded learner move, with the position
    /// before it (parallel to `grades`; `None` when unknown). Used for
    /// the "nước đúng" column of the Summary screen (§6.5).
    pub graded_best: Vec<Option<(Move, Position)>>,
    pub book_moves: usize,
    pub out_of_book: bool,
    pub plies_after_book: usize,
    pub hints: Vec<BookHint>,
    pub hint_hover: Option<usize>,
    pub hide_hints: bool,
    pub hint_ply: Option<usize>,
    /// Next moves preview (Hieu, 2026-11-12): the opponent's main
    /// reply + the learner's book moves after it, shown immediately
    /// after the learner moves — before the opponent's board move and
    /// before any live score. Set in `play_user_move` (in-book) from
    /// `Tutor::peek_next`; cleared when the opponent moves, when the
    /// learner moves again, or on retry / new lesson.
    pub next_preview: Option<(BookHint, Vec<BookHint>)>,
    /// The opponent's variations are on screen for the learner to pick
    /// (§8): set when the opponent's turn comes and it has 2+ children.
    pub show_opponent_choice: bool,
    pub opponent_ai_req: Option<u64>,
    pub last_grade: Option<(Grade, String, Option<Move>)>,
    /// Position before the graded move (renders its best move correctly).
    pub grade_pos: Option<Position>,
    pub off_book_pending: bool,
    pub opponent_due: Option<f64>,
    pub tutor_message: String,
    pub complete: Option<LessonSummary>,
    pub show_complete_dialog: bool,
    pub continue_difficulty: u8,
    pub board: BoardInteraction,
    pub anim: Option<MoveAnim>,
    pub last_move: Option<Move>,
    pub flipped: bool,
    pub started: bool,
    pub last_board: Option<(egui::Rect, BoardGeometry)>,
    /// Shared engine (builtin now, Pikafish when available) for the
    /// synchronous grading/hint fallbacks and for the `Tutor`.
    pub engine: SharedEngine,
    pub rng_state: u64,
}

impl LearnState {
    pub fn new(engine: SharedEngine) -> Self {
        Self {
            lesson: None,
            tutor: None,
            lesson_box: None,
            game: Game::new(Position::startpos()),
            node_path: Vec::new(),
            grades: Vec::new(),
            graded_best: Vec::new(),
            book_moves: 0,
            out_of_book: false,
            plies_after_book: 0,
            hints: Vec::new(),
            hint_hover: None,
            hide_hints: false,
            hint_ply: None,
            next_preview: None,
            show_opponent_choice: false,
            opponent_ai_req: None,
            last_grade: None,
            grade_pos: None,
            off_book_pending: false,
            opponent_due: None,
            tutor_message: String::new(),
            complete: None,
            show_complete_dialog: false,
            continue_difficulty: 1,
            board: BoardInteraction::default(),
            anim: None,
            last_move: None,
            flipped: false,
            started: false,
            last_board: None,
            engine,
            rng_state: 0x9E3779B97F4A7C15,
        }
    }

    fn learner_color(&self) -> Option<Color> {
        self.lesson.as_ref().map(|l| l.learner.color())
    }

    fn next_rand(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    /// Copy tree position / grades / book state from the Tutor, which is
    /// the source of truth after every `Tutor` call (§5.3).
    fn sync_from_tutor(&mut self) {
        if let Some(t) = &self.tutor {
            self.node_path = t.node_path.clone();
            self.book_moves = t.book_moves;
            self.grades = t.grades.clone();
            self.out_of_book = t.out_of_book;
            self.plies_after_book = t.plies_after_book;
        }
    }

    /// Children of the current tree node (§5.2 tree).
    fn children(&self) -> &[BookNode] {
        let Some(lesson) = &self.lesson else {
            return &[];
        };
        match node_at(&lesson.tree, &self.node_path) {
            Some(node) => node.next.as_slice(),
            None => lesson.tree.as_slice(),
        }
    }

    /// The opponent's prepared variations at the current node, best for
    /// the opponent first (§8): (index into the node's children, the
    /// parsed move, its stored score from the opponent's view, the CN
    /// notation, the note). Unparseable children are skipped. The board
    /// arrows and the choice panel share this order, so the numbers
    /// on the arrows match the numbers in the panel.
    fn opponent_choices(&self) -> Vec<(usize, Move, i32, String, String)> {
        let pos = self.game.pos.clone();
        let mut out: Vec<(usize, Move, i32, String, String)> = Vec::new();
        for (i, node) in self.children().iter().enumerate() {
            if let Ok(mv) = notation::parse_cn(&pos, &node.mv) {
                out.push((
                    i,
                    mv,
                    node.score_cp.unwrap_or(0),
                    node.mv.clone(),
                    node.note_vi.clone().unwrap_or_default(),
                ));
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.2));
        out
    }

    fn current_node(&self) -> Option<&BookNode> {
        let lesson = self.lesson.as_ref()?;
        node_at(&lesson.tree, &self.node_path)
    }

    /// Plies left on the main branch from the current node.
    fn main_branch_remaining(&self) -> usize {
        let mut count = 0;
        let mut level: &[BookNode] = self.children();
        while let Some(node) = level.iter().find(|n| n.main).or_else(|| level.first()) {
            count += 1;
            level = node.next.as_slice();
        }
        count
    }

    /// Build a `Tutor` over the boxed lesson, sharing the app's engine.
    /// SAFETY: a `Box`'s pointee never moves; `tutor` is declared before
    /// `lesson_box` (dropped first) and is always cleared before
    /// `lesson_box` is replaced.
    fn make_tutor(&self, lesson: &Lesson) -> Tutor<'static> {
        let lesson: &'static Lesson = unsafe { &*(lesson as *const Lesson) };
        Tutor::new(lesson, 0x1234_5678)
    }

    pub fn start_lesson(&mut self, lesson: Lesson, now: f64) {
        self.tutor = None;
        self.lesson_box = Some(Box::new(lesson.clone()));
        self.tutor = Some(self.make_tutor(self.lesson_box.as_ref().unwrap()));
        let start = lesson
            .start_fen
            .as_deref()
            .and_then(|f| Position::from_fen(f).ok())
            .unwrap_or_else(Position::startpos);
        self.lesson = Some(lesson);
        self.game = Game::new(start);
        self.node_path.clear();
        self.grades.clear();
        self.graded_best.clear();
        self.book_moves = 0;
        self.out_of_book = false;
        self.plies_after_book = 0;
        self.hints.clear();
        self.hint_hover = None;
        self.hide_hints = false;
        self.hint_ply = None;
        self.next_preview = None;
        self.show_opponent_choice = false;
        self.opponent_ai_req = None;
        self.last_grade = None;
        self.off_book_pending = false;
        self.complete = None;
        self.show_complete_dialog = false;
        self.continue_difficulty = 1;
        self.board.clear();
        self.anim = None;
        self.last_move = None;
        self.flipped = self.learner_color() == Some(Color::Black);
        self.started = true;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        // Learner is Black: the AI (Red) opens after 600 ms (§6.4.1).
        self.opponent_due = if self.learner_color() == Some(Color::Black) {
            Some(now + 0.6)
        } else {
            None
        };
        if self.learner_color() == Some(Color::Red) {
            // Hints are refreshed by the caller (needs the worker); see
            // `show_learn`, which calls `refresh_hints` on the first frame.
            self.hint_ply = None;
        }
    }

    fn is_learner_turn(&self) -> bool {
        self.learner_color() == Some(self.game.pos.side())
    }

    /// Book hints for the learner's turn + an engine top-4 request (§6.4.2).
    /// Hints for the learner's turn (§6.4.2). The Tutor's
    /// `hints_for_learner_turn` is canonical: book moves with their stored
    /// Pikafish scores, ordered best-first, keeping 3 directions (or 4
    /// when equally good). No live engine is consulted in lessons — the
    /// book IS Pikafish, so the hints are identical on every attempt.
    fn refresh_hints(&mut self) {
        if !self.is_learner_turn() || self.complete.is_some() {
            return;
        }
        if self.hint_ply == Some(self.game.moves.len()) {
            return;
        }
        self.hint_ply = Some(self.game.moves.len());
        if let Some(tutor) = &mut self.tutor {
            if let TutorEvent::YourTurn { book_moves } = tutor.hints_for_learner_turn() {
                self.hints = book_moves;
                return;
            }
        }
        // Fallback (no Tutor attached): book children with their stored
        // scores — same rule as the Tutor, no live engine.
        let pos = self.game.pos.clone();
        let mut hints: Vec<BookHint> = Vec::new();
        for node in self.children() {
            let Ok(mv) = notation::parse_cn(&pos, &node.mv) else {
                continue;
            };
            let explanation = node
                .note_vi
                .clone()
                .unwrap_or_else(|| explain_tags(&tags_for(&pos, mv)));
            hints.push(BookHint {
                mv,
                mv_cn: node.mv.clone(),
                // `validate_book` requires every node to carry a score.
                score: node.score_cp.unwrap_or(0),
                explanation_vi: explanation,
            });
        }
        // 1 = best by score; keep 3 directions, or 4 when equally good.
        hints.sort_by_key(|a| std::cmp::Reverse(a.score));
        let scores: Vec<i32> = hints.iter().map(|h| h.score).collect();
        hints.truncate(xq_book::direction_count(&scores));
        self.hints = hints;
    }

    fn play_move_common(&mut self, mv: Move, now: f64, animate: bool) {
        let piece = self.game.pos.piece_at(mv.from);
        if self.game.play(mv).is_err() {
            return;
        }
        self.last_move = Some(mv);
        self.anim = if animate {
            piece.map(|p| MoveAnim::new(mv, p, now, 0.15))
        } else {
            None
        };
        self.board.clear();
    }

    /// The learner played `mv`. Returns nothing; state (grade, dialogs,
    /// opponent timer) is updated in place.
    /// The learner played `mv`. The Tutor grades it (its event is the
    /// source of truth, including the bad-branch `Acceptable` rule); the
    /// screen plays the same move in its own game and syncs tree state
    /// from the Tutor. Off-book moves open the Retry / Continue dialog.
    fn play_user_move(&mut self, mv: Move, now: f64, animate: bool) {
        let pos_before = self.game.pos.clone();
        let in_book = self.children().iter().any(|n| {
            notation::parse_cn(&pos_before, &n.mv)
                .map(|m| m == mv)
                .unwrap_or(false)
        });
        let tutor_event = self.tutor.as_mut().map(|t| t.on_user_move(mv));
        // Play the same move in our own game (Tutor played it in its game).
        self.play_move_common(mv, now, animate);
        self.sync_from_tutor();
        self.hint_ply = None;
        match tutor_event {
            Some(TutorEvent::OpeningComplete { summary }) => {
                let (grade, expl) = summary
                    .grades
                    .last()
                    .map(|(_, g)| (*g, g.label_vi().to_string()))
                    .unwrap_or((Grade::Book, Grade::Book.label_vi().to_string()));
                self.last_grade = Some((grade, expl, None));
                self.grade_pos = Some(pos_before.clone());
                self.graded_best.push(None);
                self.tutor_message = summary.end_note_vi.clone().unwrap_or_default();
                self.complete = Some(summary);
                self.show_complete_dialog = true;
            }
            Some(TutorEvent::Graded {
                grade,
                best,
                explanation_vi,
                ..
            }) => {
                self.last_grade = Some((grade, explanation_vi.clone(), best));
                self.grade_pos = Some(pos_before.clone());
                self.graded_best.push(best.map(|b| (b, pos_before.clone())));
                self.tutor_message = explanation_vi;
                if in_book {
                    self.opponent_due = Some(now + 0.6);
                    // Show the next book moves at once (Hieu, 2026-11-12):
                    // the opponent's main reply + the learner's moves
                    // after it, from the book — before the opponent's
                    // board move and before any live score arrives.
                    self.next_preview = self.tutor.as_ref().and_then(|t| t.peek_next());
                } else {
                    self.off_book_pending = true;
                    self.next_preview = None;
                }
                self.maybe_complete();
            }
            _ => {
                // No Tutor attached: grade from the book alone (same rule
                // as the Tutor — Pikafish IS the book, no live engine in
                // lessons). In-book: the node's note; off-book: Acceptable
                // ("not in the book") with the book's main child as the
                // correct move.
                let best = self
                    .children()
                    .iter()
                    .find(|n| n.main)
                    .and_then(|n| notation::parse_cn(&pos_before, &n.mv).ok());
                let (grade, expl) = if in_book {
                    let note = self
                        .children()
                        .iter()
                        .find(|n| {
                            notation::parse_cn(&pos_before, &n.mv)
                                .map(|m| m == mv)
                                .unwrap_or(false)
                        })
                        .and_then(|n| n.note_vi.clone())
                        .unwrap_or_else(|| "Đúng sách.".to_string());
                    (Grade::Book, note)
                } else {
                    (
                        Grade::Acceptable,
                        "Nước này không có trong sách khai cuộc.".to_string(),
                    )
                };
                self.grades.push((mv, grade));
                self.last_grade = Some((grade, expl.clone(), best));
                self.grade_pos = Some(pos_before.clone());
                self.graded_best.push(best.map(|b| (b, pos_before.clone())));
                self.tutor_message = expl;
                // No Tutor attached: no peek (the tree position is not
                // tracked here), so no preview; hints refresh after the
                // opponent moves as before.
                self.next_preview = None;
                if in_book {
                    self.opponent_due = Some(now + 0.6);
                } else {
                    self.off_book_pending = true;
                }
                self.maybe_complete();
            }
        }
    }

    /// Cancel a pending out-of-book opponent AI calculation (Hieu,
    /// 2026-11-12): the learner moved on (retry / undo), so the
    /// engine's answer for the old position is useless. Stops the
    /// engine and clears the request id; a stale response that still
    /// arrives is dropped by `req_id` in `poll`.
    fn cancel_opponent_ai(&mut self, worker: &crate::ai_worker::AiWorker) {
        if self.opponent_ai_req.is_some() {
            worker.request_stop();
            self.opponent_ai_req = None;
        }
    }

    /// Undo (Hieu, 2026-11-12): go back to before the learner's last
    /// move so they can try a different one. Cancels a pending
    /// opponent move / AI calculation first. Undoes the opponent's
    /// reply too when it has already been played. Returns `false`
    /// when there is nothing to undo.
    ///
    /// The Tutor is undone in step, but only for moves it actually
    /// played: out-of-book AI opponent moves live only in the
    /// screen's game, so the Tutor is undone only when its last move
    /// is the move just undone here.
    pub fn undo(&mut self, worker: &crate::ai_worker::AiWorker) -> bool {
        self.cancel_opponent_ai(worker);
        self.opponent_due = None;
        self.show_opponent_choice = false;
        self.off_book_pending = false;
        self.next_preview = None;
        self.complete = None;
        self.show_complete_dialog = false;
        let Some(learner) = self.learner_color() else {
            return false;
        };
        if self.game.moves.is_empty() {
            return false;
        }
        let mut undone_any = false;
        while let Some(mv) = self.game.undo() {
            if let Some(tutor) = &mut self.tutor {
                if tutor.game.moves.last() == Some(&mv) {
                    tutor.undo_ply();
                }
            }
            undone_any = true;
            // The side to move now made the undone move: stop once a
            // learner move has been undone.
            if self.game.pos.side() == learner {
                break;
            }
        }
        if !undone_any {
            return false;
        }
        self.sync_from_tutor();
        self.graded_best.truncate(self.grades.len());
        self.last_move = self.game.moves.last().copied();
        self.anim = None;
        self.last_grade = None;
        self.grade_pos = None;
        self.hint_ply = None;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        self.board.clear();
        true
    }

    fn retry(&mut self, worker: &crate::ai_worker::AiWorker) {
        self.cancel_opponent_ai(worker);
        if let Some(tutor) = &mut self.tutor {
            tutor.retry();
        }
        self.game.undo();
        self.sync_from_tutor();
        self.graded_best.truncate(self.grades.len());
        self.last_move = self.game.moves.last().copied();
        self.anim = None;
        self.last_grade = None;
        self.grade_pos = None;
        self.next_preview = None;
        self.off_book_pending = false;
        self.show_opponent_choice = false;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        self.hint_ply = None;
    }

    fn continue_off_book(&mut self, now: f64) {
        self.out_of_book = true;
        self.plies_after_book = 0;
        self.off_book_pending = false;
        self.opponent_due = Some(now + 0.6);
    }

    /// Play the opponent's book move, or fall back to the engine when
    /// out of book / no children (§8): when the book node has 2+ children
    /// the learner picks the variation instead — the choice panel is
    /// shown and this returns without playing. With exactly 1 child the
    /// move plays automatically after the delay, as before.
    /// Play the opponent's move. The Tutor picks a weighted book child
    /// and plays it in its own game; the screen mirrors it and syncs.
    /// `None` from the Tutor means out of book (or game over): the engine
    /// plays via the worker instead (§5.3.4).
    fn play_opponent(&mut self, worker: &mut crate::ai_worker::AiWorker, now: f64, animate: bool) {
        self.opponent_due = None;
        if self.game.result().is_over() {
            self.maybe_complete();
            return;
        }
        if !self.out_of_book {
            let n = if let Some(tutor) = &self.tutor {
                tutor.current_children().len()
            } else {
                self.children().len()
            };
            if n >= 2 {
                self.show_opponent_choice = true;
                self.tutor_message = i18n::t(Key::OpponentChooseMsg).to_string();
                return;
            }
        }
        if let Some(tutor) = &mut self.tutor {
            if let Some(TutorEvent::OpponentMoves { mv, note_vi }) = tutor.next_opponent_move() {
                self.tutor_message = note_vi.unwrap_or_default();
                self.play_move_common(mv, now, animate);
                self.sync_from_tutor();
                self.after_opponent_move();
                return;
            }
        }
        // No Tutor attached: weighted book pick (original fallback).
        if self.tutor.is_none() && !self.out_of_book {
            let children = self.children().to_vec();
            if !children.is_empty() {
                let total: f64 = children.iter().map(|n| n.weight).sum();
                let mut roll = (self.next_rand() % 10_000) as f64 / 10_000.0 * total;
                let mut chosen = 0;
                for (i, n) in children.iter().enumerate() {
                    roll -= n.weight;
                    if roll <= 0.0 {
                        chosen = i;
                        break;
                    }
                    chosen = i;
                }
                let node = children[chosen].clone();
                if let Ok(mv) = notation::parse_cn(&self.game.pos, &node.mv) {
                    self.node_path.push(chosen);
                    self.tutor_message = node.note_vi.clone().unwrap_or_default();
                    self.play_move_common(mv, now, animate);
                    self.after_opponent_move();
                    return;
                }
            }
        }
        // Out of book: engine plays (Medium, §5.3.4).
        let req_id = worker.alloc_req_id();
        self.opponent_ai_req = Some(req_id);
        let limits = self
            .engine
            .lock()
            .expect("engine lock")
            .limits_for(Level::Medium);
        let _ = worker.tx.send(AiRequest::BestMove {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            req_id,
        });
    }

    /// Play the opponent variation the learner picked in the choice
    /// panel (§8). `idx` is the index into the current node's children
    /// (kept in tree order, so it matches the Tutor's `node_path`).
    /// A stale or out-of-range pick falls back to the automatic play.
    fn play_opponent_choice(
        &mut self,
        worker: &mut crate::ai_worker::AiWorker,
        idx: usize,
        now: f64,
        animate: bool,
    ) {
        self.show_opponent_choice = false;
        if self.game.result().is_over() {
            self.maybe_complete();
            return;
        }
        if let Some(tutor) = &mut self.tutor {
            if let Some(TutorEvent::OpponentMoves { mv, note_vi }) = tutor.play_opponent_move(idx) {
                self.tutor_message = note_vi.unwrap_or_default();
                self.play_move_common(mv, now, animate);
                self.sync_from_tutor();
                self.after_opponent_move();
                return;
            }
        }
        // No Tutor attached: play the chosen book child directly.
        if self.tutor.is_none() && !self.out_of_book {
            let children = self.children().to_vec();
            if let Some(node) = children.get(idx) {
                if let Ok(mv) = notation::parse_cn(&self.game.pos, &node.mv) {
                    self.node_path.push(idx);
                    self.tutor_message = node.note_vi.clone().unwrap_or_default();
                    self.play_move_common(mv, now, animate);
                    self.after_opponent_move();
                    return;
                }
            }
        }
        self.play_opponent(worker, now, animate);
    }

    fn after_opponent_move(&mut self) {
        // The preview is consumed: the opponent has moved, the learner's
        // turn is back and `refresh_hints` below shows the real hints.
        self.next_preview = None;
        if self.out_of_book {
            self.plies_after_book += 1;
        }
        if self.maybe_complete() {
            return;
        }
        self.refresh_hints();
    }

    /// Check the §5.3.5 completion rules; on completion store the summary.
    fn maybe_complete(&mut self) -> bool {
        if self.complete.is_some() {
            return true;
        }
        let lesson = match &self.lesson {
            Some(l) => l,
            None => return false,
        };
        let tutor_done = self.tutor.as_ref().is_some_and(|t| t.is_complete());
        let end_note = self
            .current_node()
            .and_then(|n| n.end.as_ref())
            .map(|e| e.note_vi.clone());
        let at_leaf = self.children().is_empty() && end_note.is_some();
        let maxed = self.game.moves.len() >= lesson.max_plies;
        let out_long = self.out_of_book && self.plies_after_book >= 8;
        if !(at_leaf || maxed || out_long || tutor_done) {
            return false;
        }
        let summary = match &self.tutor {
            Some(t) => t.summary(),
            None => LessonSummary {
                book_moves: self.book_moves,
                total_learner_moves: self.grades.len(),
                grades: self.grades.clone(),
                final_score: eval::eval(&self.game.pos),
                end_note_vi: end_note,
            },
        };
        self.complete = Some(summary);
        self.show_complete_dialog = true;
        true
    }

    /// Take this screen's responses out of the shared buffer; responses
    /// for the other screen stay buffered (see `XiangqiApp::pending_ai`).
    fn poll(
        &mut self,
        pending: &mut Vec<AiResponse>,
        ctx: &egui::Context,
        now: f64,
        animate: bool,
    ) {
        let mut i = 0;
        while i < pending.len() {
            let mine = match &pending[i] {
                AiResponse::BestMove { req_id, .. } => {
                    response_is_current(*req_id, self.opponent_ai_req)
                }
                AiResponse::Analysis { .. } | AiResponse::Info { .. } => false,
            };
            if !mine {
                i += 1;
                continue;
            }
            let resp = pending.remove(i);
            match resp {
                AiResponse::BestMove { mv, .. } => {
                    self.opponent_ai_req = None;
                    self.play_move_common(mv, now, animate);
                    if self.out_of_book {
                        self.plies_after_book += 1;
                    }
                    if !self.maybe_complete() {
                        // Next frame's `refresh_hints` rebuilds hints
                        // for the learner's turn.
                        self.hint_ply = None;
                    }
                    ctx.request_repaint();
                }
                AiResponse::Info { .. } => {}
                // Analysis responses belong to the Vs-AI screen; the
                // `mine` filter above never lets one reach this match.
                AiResponse::Analysis { .. } => {}
            }
        }
    }
}

impl XiangqiApp {
    pub(crate) fn show_learn(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let now = ctx.input(|i| i.time);
        let animate = self.settings.animate;
        // Poll engine answers (hints / out-of-book opponent moves).
        self.learn.poll(&mut self.pending_ai, ctx, now, animate);
        // ChessDB: a separate live score for the current position.
        let fen = self.learn.game.pos.to_fen();
        self.poll_chessdb(&fen, ctx);
        // First frame after starting (or after the opponent's AI move):
        // (re)build hints for the learner's turn.
        if self.learn.started
            && self.learn.complete.is_none()
            && self.learn.is_learner_turn()
            && !self.learn.off_book_pending
        {
            self.learn.refresh_hints();
        }
        // Opponent book move after its 600 ms delay.
        if let Some(due) = self.learn.opponent_due {
            if now >= due {
                self.learn.play_opponent(&mut self.worker, now, animate);
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        if self.learn.anim.is_some_and(|a| a.progress(now).is_some()) {
            ctx.request_repaint();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.learn.board.clear();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.learn.flipped = !self.learn.flipped;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.learn.hide_hints = false;
        }

        if self.learn.lesson.is_none() {
            ui.label(i18n::t(Key::LessonNotFound));
            if big_button(ui, i18n::t(Key::BackToMenu)).clicked() {
                self.goto(Screen::Menu);
            }
            return;
        }
        // NOTE: capture the CentralPanel's available size before the
        // horizontal row (see vs_ai.rs: inside `ui.horizontal` egui only
        // offers the current row height, which shrinks the board to ~20 px).
        let avail = ui.available_size();
        let narrow = is_narrow(ctx);
        if narrow {
            ui.vertical(|ui| {
                self.learn_board(ctx, ui, avail);
                ui.add_space(8.0);
                self.learn_panel(ctx, ui);
            });
        } else {
            ui.horizontal(|ui| {
                let pos = self.learn.game.pos.clone();
                let s = eval::eval(&pos);
                let score_red = if pos.side() == Color::Red { s } else { -s };
                eval_bar_ui(ui, score_red, avail.y);
                let budget = egui::Vec2::new(avail.x - 20.0 - 12.0 - 380.0 - 24.0, avail.y);
                self.learn_board(ctx, ui, budget);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    self.learn_panel(ctx, ui);
                });
            });
        }
        self.learn_off_book_dialog(ctx);
        self.learn_complete_dialog(ctx);
    }

    fn learn_board(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, avail: egui::Vec2) {
        let now = ctx.input(|i| i.time);
        let total = avail;
        let cell = (total.x / 10.0).min(total.y / 11.0);
        let size = egui::Vec2::new(10.0 * cell, 11.0 * cell);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let geom = BoardGeometry::new(size, self.learn.flipped);
        self.learn.last_board = Some((rect, geom));

        let pos = self.learn.game.pos.clone();
        let enabled = self.learn.started
            && self.learn.complete.is_none()
            && !self.learn.off_book_pending
            && self.learn.opponent_due.is_none()
            && self.learn.opponent_ai_req.is_none()
            && self.learn.is_learner_turn();
        let learner = self.learn.learner_color().unwrap_or(Color::Red);
        let selected = self.learn.board.selected;
        let targets = selected
            .map(|sq| legal_targets(&pos, sq))
            .unwrap_or_default();
        let check_sq = if pos.in_check(pos.side()) {
            Some(pos.king_sq(pos.side()))
        } else {
            None
        };
        let drag = self
            .learn
            .board
            .drag_sq
            .map(|sq| (sq, self.learn.board.drag_pos));
        // Hint arrows: up to 4, hovered one brighter (§6.4.2, §7.3).
        let mut arrows: Vec<HintArrow> = Vec::new();
        if self.learn.show_opponent_choice {
            // Opponent variations (§8): numbered arrows for the choices,
            // 1 = the opponent's best variation, same order as the panel.
            for (rank, (_, mv, _, _, _)) in self.learn.opponent_choices().iter().take(4).enumerate()
            {
                let horse = pos
                    .piece_at(mv.from)
                    .map(|p| p.kind == PieceKind::Horse)
                    .unwrap_or(false);
                arrows.push(HintArrow::for_rank(mv.from, mv.to, rank, horse));
            }
        } else if !self.learn.hide_hints && enabled {
            for (i, hint) in self.learn.hints.iter().take(4).enumerate() {
                let horse = pos
                    .piece_at(hint.mv.from)
                    .map(|p| p.kind == PieceKind::Horse)
                    .unwrap_or(false);
                let mut arrow = HintArrow::for_rank(hint.mv.from, hint.mv.to, i, horse);
                if self.learn.hint_hover == Some(i) {
                    arrow.color = egui::Color32::from_rgba_unmultiplied(0x00, 0xC8, 0x00, 255);
                }
                arrows.push(arrow);
            }
        }
        let draw = BoardDraw {
            selected,
            targets: &targets,
            last_move: self.learn.last_move,
            check_sq,
            check_since: None,
            now,
            arrows: &arrows,
            drag,
            anim: self.learn.anim,
            hover_sq: self.learn.board.hover_sq,
        };
        draw_board(&ui.painter_at(rect), rect, &geom, &pos, &draw);
        if enabled {
            if let Some(mv) = self.learn.board.handle(&resp, rect, &geom, &pos, learner) {
                self.learn.play_user_move(mv, now, self.settings.animate);
            }
        }
    }

    fn learn_panel(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(lesson) = self.learn.lesson.clone() else {
            return;
        };
        ui.heading(lesson.name_vi.clone());
        ui.label(lesson.name_zh.clone());
        let remaining = self.learn.main_branch_remaining();
        let total_est = self.learn.game.moves.len() + remaining;
        ui.horizontal(|ui| {
            ui.label(
                i18n::t(Key::ProgressMoves)
                    .replace("{n}", &self.learn.game.moves.len().to_string())
                    .replace("{m}", &total_est.to_string()),
            );
            // Undo (Hieu, 2026-11-12): back to before the learner's
            // last move, so they can try a different one. Disabled
            // when there is nothing to undo.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_undo = self.learn.started
                    && self.learn.complete.is_none()
                    && !self.learn.game.moves.is_empty();
                if ui
                    .add_enabled(can_undo, egui::Button::new(i18n::t(Key::Undo)))
                    .clicked()
                {
                    self.learn.undo(&self.worker);
                }
            });
        });
        ui.separator();
        section_title(ui, i18n::t(Key::TutorTitle));
        ui.label(self.learn.tutor_message.clone());
        if let Some((grade, expl, best)) = &self.learn.last_grade {
            let color = match grade {
                Grade::Book | Grade::Best => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
                Grade::Good | Grade::Acceptable => egui::Color32::GRAY,
                Grade::Inaccurate => egui::Color32::from_rgb(0xEF, 0x6C, 0x00),
                Grade::Blunder => egui::Color32::from_rgb(0xC0, 0x39, 0x2B),
            };
            ui.colored_label(
                color,
                egui::RichText::new(format!(
                    "{}: {} — {expl}",
                    i18n::t(Key::GradeLabel),
                    grade.label_vi()
                ))
                .strong(),
            );
            if let (Some(best), Some(pos_before)) = (best, &self.learn.grade_pos) {
                ui.label(
                    i18n::t(Key::BookMoveShouldBe)
                        .replace("{name}", &self.render_move(pos_before, *best)),
                );
            }
        }
        // Next moves from the book, shown at once after the learner's
        // move (Hieu, 2026-11-12) — before the opponent's board move
        // and before any live score. The opponent's main reply, then
        // the learner's book moves after it. Display-only: it is the
        // opponent's turn, so these cannot be played yet.
        if let Some((reply, follow)) = &self.learn.next_preview {
            ui.separator();
            section_title(ui, i18n::t(Key::NextMovesTitle));
            ui.horizontal(|ui| {
                ui.label(
                    i18n::t(Key::NextMovesOpponent)
                        .replace("{name}", &self.render_move(&self.learn.game.pos, reply.mv)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    advantage_chip_ui(ui, reply.score);
                });
            });
            if !follow.is_empty() {
                ui.label(i18n::t(Key::NextMovesFollow));
                // Position after the opponent's reply, to render the
                // follow-up moves in the user's notation.
                let mut after = self.learn.game.pos.clone();
                {
                    let mut scratch = Game::new(after.clone());
                    let _ = scratch.play(reply.mv);
                    after = scratch.pos.clone();
                }
                for (i, hint) in follow.iter().take(4).enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}.", i + 1))
                                .strong()
                                .size(24.0),
                        );
                        ui.label(
                            egui::RichText::new(self.render_move(&after, hint.mv))
                                .strong()
                                .size(22.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            advantage_chip_ui(ui, hint.score);
                        });
                    });
                }
            }
        }
        // ChessDB: a separate live score for the current position.
        let pos = self.learn.game.pos.clone();
        self.chessdb_section_ui(ui, &pos);
        ui.separator();
        if self.learn.show_opponent_choice {
            // Opponent variations (§8): the learner picks which prepared
            // variation the opponent plays. Rows mirror the hint rows;
            // the chip shows the advantage from the LEARNER's side, so
            // the opponent's stored score is negated (a good move for
            // the opponent reads as bad for the learner).
            section_title(ui, i18n::t(Key::OpponentChooseTitle));
            let choices = self.learn.opponent_choices();
            let mut clicked_choice: Option<usize> = None;
            for (rank, (idx, _, score, mv_cn, note)) in choices.iter().take(4).enumerate() {
                let row = egui::Frame::default()
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .corner_radius(egui::CornerRadius::same(8))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}.", rank + 1))
                                        .strong()
                                        .size(24.0),
                                );
                                ui.label(egui::RichText::new(mv_cn).strong().size(22.0));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        advantage_chip_ui(ui, -score);
                                    },
                                );
                            });
                            if !note.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.add_space(34.0);
                                    ui.label(
                                        egui::RichText::new(note)
                                            .small()
                                            .color(egui::Color32::GRAY),
                                    );
                                });
                            }
                        });
                    });
                let click = ui.interact(
                    row.response.rect,
                    ui.id().with(("opp-choice-row", rank)),
                    egui::Sense::click(),
                );
                if click.clicked() {
                    clicked_choice = Some(*idx);
                }
            }
            if let Some(idx) = clicked_choice {
                let now = ui.input(|i| i.time);
                self.learn
                    .play_opponent_choice(&mut self.worker, idx, now, self.settings.animate);
            }
        } else if self.learn.hide_hints {
            if big_button(ui, i18n::t(Key::ShowHints)).clicked() {
                self.learn.hide_hints = false;
            }
        } else {
            ui.horizontal(|ui| {
                section_title(ui, i18n::t(Key::BestDirections));
                if big_button(ui, i18n::t(Key::HideHints)).clicked() {
                    self.learn.hide_hints = true;
                }
            });
            if self.learn.hints.is_empty() {
                ui.label(i18n::t(Key::NoHintAvailable));
            }
            let mut clicked_hint: Option<Move> = None;
            let mut hovered: Option<usize> = None;
            for (i, hint) in self.learn.hints.iter().take(4).enumerate() {
                // Row highlight from the previous frame's hover state;
                // the frame paints the fill before the row content.
                let fill = if self.learn.hint_hover == Some(i) {
                    egui::Color32::from_rgba_unmultiplied(0x1E, 0x5A, 0xC8, 28)
                } else {
                    egui::Color32::TRANSPARENT
                };
                let row = egui::Frame::default()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .corner_radius(egui::CornerRadius::same(8))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}.", i + 1))
                                        .strong()
                                        .size(24.0),
                                );
                                ui.label(egui::RichText::new(&hint.mv_cn).strong().size(22.0));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        advantage_chip_ui(ui, hint.score);
                                    },
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(34.0);
                                ui.label(
                                    egui::RichText::new(&hint.explanation_vi)
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            });
                        });
                    });
                let click = ui.interact(
                    row.response.rect,
                    ui.id().with(("hint-row", i)),
                    egui::Sense::click(),
                );
                if click.hovered() {
                    hovered = Some(i);
                }
                if click.clicked() && self.learn.is_learner_turn() && !self.learn.off_book_pending {
                    clicked_hint = Some(hint.mv);
                }
            }
            self.learn.hint_hover = hovered;
            if let Some(mv) = clicked_hint {
                let now = ui.input(|i| i.time);
                self.learn.play_user_move(mv, now, self.settings.animate);
            }
        }
        ui.separator();
        // Compact move list.
        let moves = self.move_strings(&self.learn.game);
        let mut dummy_review: Option<usize> = None;
        move_list_ui(ui, &moves, &mut dummy_review);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if big_button(ui, i18n::t(Key::FlipBoard)).clicked() {
                self.learn.flipped = !self.learn.flipped;
            }
            if big_button(ui, i18n::t(Key::OtherLesson)).clicked() {
                self.goto(Screen::LearnSelect);
            }
            if big_button(ui, i18n::t(Key::BackToMenu)).clicked() {
                self.goto(Screen::Menu);
            }
        });
    }

    fn learn_off_book_dialog(&mut self, ctx: &egui::Context) {
        if !self.learn.off_book_pending {
            return;
        }
        let mut retry = false;
        let mut cont = false;
        egui::Window::new(i18n::t(Key::OffBookTitle))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(i18n::t(Key::OffBookBody));
                if let Some((_, _, best)) = &self.learn.last_grade {
                    if let (Some(best), Some(pos_before)) = (best, &self.learn.grade_pos) {
                        ui.label(
                            i18n::t(Key::BookMoveShouldBe)
                                .replace("{name}", &self.render_move(pos_before, *best)),
                        );
                    }
                }
                ui.horizontal(|ui| {
                    if big_button(ui, i18n::t(Key::RetryMove)).clicked() {
                        retry = true;
                    }
                    if big_button(ui, i18n::t(Key::ContinueMove)).clicked() {
                        cont = true;
                    }
                });
            });
        if retry {
            self.learn.retry(&self.worker);
        } else if cont {
            let now = ctx.input(|i| i.time);
            self.learn.continue_off_book(now);
        }
    }

    fn learn_complete_dialog(&mut self, ctx: &egui::Context) {
        if !self.learn.show_complete_dialog {
            return;
        }
        let Some(summary) = &self.learn.complete else {
            return;
        };
        let blunders = summary
            .grades
            .iter()
            .filter(|(_, g)| *g == Grade::Blunder)
            .count();
        let inacc = summary
            .grades
            .iter()
            .filter(|(_, g)| *g == Grade::Inaccurate)
            .count();
        let mut to_summary = false;
        let mut to_vs_ai = false;
        let mut open = true;
        egui::Window::new(i18n::t(Key::OpeningCompleteTitle))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    i18n::t(Key::CorrectBookMoves)
                        .replace("{n}", &summary.book_moves.to_string())
                        .replace("{m}", &summary.total_learner_moves.to_string()),
                );
                ui.label(
                    i18n::t(Key::MistakesAndInaccuracies)
                        .replace("{n}", &blunders.to_string())
                        .replace("{m}", &inacc.to_string()),
                );
                if let Some(note) = &summary.end_note_vi {
                    ui.label(note.clone());
                }
                ui.label(
                    i18n::t(Key::EngineScore).replace("{n}", &summary.final_score.to_string()),
                );
                section_title(ui, i18n::t(Key::Difficulty));
                for d in 0..=3u8 {
                    ui.radio_value(
                        &mut self.learn.continue_difficulty,
                        d,
                        i18n::difficulty_name(d),
                    );
                }
                ui.horizontal(|ui| {
                    if big_button(ui, i18n::t(Key::StopAndSummary)).clicked() {
                        to_summary = true;
                    }
                    if big_button(ui, i18n::t(Key::ContinueWithAi)).clicked() {
                        to_vs_ai = true;
                    }
                });
            });
        if to_summary {
            self.learn_finish_to_summary();
        } else if to_vs_ai {
            let human = self.learn.learner_color().unwrap_or(Color::Red);
            let mut game = Game::new(self.learn.game.start.clone());
            for m in &self.learn.game.moves {
                let _ = game.play(*m);
            }
            let diff = self.learn.continue_difficulty;
            self.vs_ai.start_from_learn(game, human);
            self.vs_ai.opponent = crate::screens::vs_ai::VsOpponent::Ai {
                human,
                difficulty: diff,
            };
            self.settings.difficulty = diff;
            self.settings_dirty = true;
            self.learn.show_complete_dialog = false;
            self.goto(Screen::VsAi);
        } else if !open {
            self.learn.show_complete_dialog = false;
        }
    }

    /// Build the Summary screen from the finished lesson and save progress.
    fn learn_finish_to_summary(&mut self) {
        let Some(lesson) = self.learn.lesson.clone() else {
            return;
        };
        let Some(summary) = self.learn.complete.take() else {
            return;
        };
        // Reconstruct positions + rendered moves for the summary table.
        let mut positions = vec![self.learn.game.start.clone()];
        let mut p = self.learn.game.start.clone();
        let mut rendered: Vec<String> = Vec::new();
        for m in &self.learn.game.moves {
            rendered.push(self.render_move(&p, *m));
            p.make_move(*m);
            positions.push(p.clone());
        }
        // Learner plies (game ply indices where the mover is the learner).
        let learner = lesson.learner.color();
        let start_side_is_learner = self.learn.game.start.side() == learner;
        let learner_plies: Vec<usize> = self
            .learn
            .game
            .moves
            .iter()
            .enumerate()
            .filter(|(i, _)| (i % 2 == 0) == start_side_is_learner)
            .map(|(i, _)| i + 1)
            .collect();
        let rows: Vec<crate::screens::summary::SummaryRow> = self
            .learn
            .grades
            .iter()
            .enumerate()
            .map(|(i, (_, grade))| {
                let game_ply = learner_plies.get(i).copied().unwrap_or(i + 1);
                let mv_str = rendered.get(game_ply - 1).cloned().unwrap_or_default();
                // "Nước đúng": for a book move it is the move itself; for
                // an off-book move it is the best move recorded at grading.
                let best = if *grade == Grade::Book {
                    Some(mv_str.clone())
                } else {
                    self.learn
                        .graded_best
                        .get(i)
                        .and_then(|b| b.as_ref())
                        .map(|(bm, pos)| self.render_move(pos, *bm))
                };
                crate::screens::summary::SummaryRow {
                    ply: game_ply,
                    mv: mv_str,
                    grade: *grade,
                    best,
                }
            })
            .collect();
        let blunders = summary
            .grades
            .iter()
            .filter(|(_, g)| *g == Grade::Blunder)
            .count();
        let inacc = summary
            .grades
            .iter()
            .filter(|(_, g)| *g == Grade::Inaccurate)
            .count();
        let ratio = if summary.total_learner_moves > 0 {
            summary.book_moves as f32 / summary.total_learner_moves as f32
        } else {
            0.0
        };
        self.settings.record_lesson(&lesson.id, true, ratio);
        self.settings_dirty = true;
        self.summary.lesson_id = lesson.id.clone();
        self.summary.lesson_name = lesson.name_vi.clone();
        self.summary.rows = rows;
        self.summary.book_moves = summary.book_moves;
        self.summary.total = summary.total_learner_moves;
        self.summary.blunders = blunders;
        self.summary.inaccuracies = inacc;
        self.summary.end_note = summary.end_note_vi.clone();
        self.summary.final_score = summary.final_score;
        self.summary.selected_ply = None;
        self.summary.positions = positions;
        self.summary.start = self.learn.game.start.clone();
        self.learn.show_complete_dialog = false;
        self.goto(Screen::Summary);
    }
}
