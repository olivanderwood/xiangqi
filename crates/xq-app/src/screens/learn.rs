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
use crate::app::{eval_bar_ui, is_narrow, move_list_ui, Screen, XiangqiApp};
use crate::board_view::{
    draw_board, legal_targets, BoardDraw, BoardGeometry, BoardInteraction, HintArrow, MoveAnim,
};
use crate::i18n::{self, Key};
use std::time::Duration;
use xq_book::schema::{BookNode, Lesson};
use xq_book::tree::node_at;
use xq_book::{BookHint, LessonSummary, Tutor, TutorEvent};
use xq_core::analysis::{explain_tags, grade_from_delta_with, tags_for, Grade};
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
        Tutor::with_engine(lesson, 0x1234_5678, self.engine.clone())
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

    /// Book hints for the learner's turn + an engine top-3 request (§6.4.2).
    /// Hints for the learner's turn (§6.4.2). The Tutor's
    /// `hints_for_learner_turn` is canonical: book moves (main first) plus
    /// up to 2 engine lines. Runs once per learner turn (guarded by
    /// `hint_ply`); the Tutor's engine pass runs synchronously here.
    fn refresh_hints(&mut self) {
        if !self.is_learner_turn() || self.complete.is_some() {
            return;
        }
        if self.hint_ply == Some(self.game.moves.len()) {
            return;
        }
        self.hint_ply = Some(self.game.moves.len());
        if let Some(tutor) = &mut self.tutor {
            if let TutorEvent::YourTurn { book_moves, .. } = tutor.hints_for_learner_turn() {
                self.hints = book_moves;
                return;
            }
        }
        // Fallback (no Tutor attached): book children scored locally.
        let pos = self.game.pos.clone();
        let handle = self.engine.clone();
        let mut engine = handle.lock().expect("engine lock");
        let req = EngineRequest::new(
            self.game.start.to_fen(),
            self.game.moves.clone(),
            engine.hint_limits(),
        );
        let mut hints: Vec<BookHint> = Vec::new();
        let mut children: Vec<(usize, BookNode)> =
            self.children().iter().cloned().enumerate().collect();
        children.sort_by_key(|(_, n)| !n.main);
        for (_, node) in &children {
            let Ok(mv) = notation::parse_cn(&pos, &node.mv) else {
                continue;
            };
            let Ok(score) = engine.score_move(&req, mv) else {
                continue;
            };
            let score = score.to_cp_clamped();
            let explanation = node
                .note_vi
                .clone()
                .unwrap_or_else(|| explain_tags(&tags_for(&pos, mv)));
            hints.push(BookHint {
                mv,
                mv_cn: node.mv.clone(),
                in_book: true,
                score,
                explanation_vi: explanation,
            });
        }
        drop(engine);
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
                } else {
                    self.off_book_pending = true;
                }
                self.maybe_complete();
            }
            _ => {
                // No Tutor attached: local engine grading, same §4.5 rule.
                // NOTE: `play_move_common` above already played `mv`, so
                // the request must use the moves *before* it.
                let mut moves_before = self.game.moves.clone();
                moves_before.pop();
                let handle = self.engine.clone();
                let mut engine = handle.lock().expect("engine lock");
                let req = EngineRequest::new(
                    self.game.start.to_fen(),
                    moves_before,
                    engine.grade_limits(),
                );
                if let Ok(best) = engine.best_move(&req, &mut |_| {}) {
                    let best_cp = best.score.to_cp_clamped();
                    let mv_cp = engine
                        .score_move(&req, mv)
                        .map(|s| s.to_cp_clamped())
                        .unwrap_or(best_cp);
                    let grade =
                        grade_from_delta_with(best_cp - mv_cp, false, engine.grade_thresholds());
                    let expl = explain_tags(&tags_for(&pos_before, mv));
                    self.grades.push((mv, grade));
                    self.last_grade = Some((grade, expl.clone(), Some(best.mv)));
                    self.grade_pos = Some(pos_before.clone());
                    self.graded_best.push(Some((best.mv, pos_before.clone())));
                    self.tutor_message = expl;
                }
                if in_book {
                    self.opponent_due = Some(now + 0.6);
                } else {
                    self.off_book_pending = true;
                }
                self.maybe_complete();
            }
        }
    }

    fn retry(&mut self) {
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
        self.off_book_pending = false;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        self.hint_ply = None;
    }

    fn continue_off_book(&mut self, now: f64) {
        self.out_of_book = true;
        self.plies_after_book = 0;
        self.off_book_pending = false;
        self.opponent_due = Some(now + 0.6);
    }

    /// Play the opponent's book move (weighted random child), or fall back
    /// to the engine when out of book / no children.
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

    fn after_opponent_move(&mut self) {
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
            if ui.button(i18n::t(Key::BackToMenu)).clicked() {
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
                let budget = egui::Vec2::new(avail.x - 16.0 - 12.0 - 340.0 - 24.0, avail.y);
                self.learn_board(ctx, ui, budget);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(340.0);
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
        // Hint arrows: up to 3, hovered one brighter (§6.4.2, §7.3).
        let mut arrows: Vec<HintArrow> = Vec::new();
        if !self.learn.hide_hints && enabled {
            for (i, hint) in self.learn.hints.iter().take(3).enumerate() {
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
        ui.label(
            i18n::t(Key::ProgressMoves)
                .replace("{n}", &self.learn.game.moves.len().to_string())
                .replace("{m}", &total_est.to_string()),
        );
        ui.separator();
        ui.label(i18n::t(Key::TutorTitle));
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
                format!(
                    "{}: {} — {expl}",
                    i18n::t(Key::GradeLabel),
                    grade.label_vi()
                ),
            );
            if let (Some(best), Some(pos_before)) = (best, &self.learn.grade_pos) {
                ui.label(
                    i18n::t(Key::BookMoveShouldBe)
                        .replace("{name}", &self.render_move(pos_before, *best)),
                );
            }
        }
        ui.separator();
        // Hints (§6.4.2).
        if self.learn.hide_hints {
            if ui.button(i18n::t(Key::ShowHints)).clicked() {
                self.learn.hide_hints = false;
            }
        } else {
            ui.horizontal(|ui| {
                ui.label(i18n::t(Key::Hint));
                if ui.button(i18n::t(Key::HideHints)).clicked() {
                    self.learn.hide_hints = true;
                }
            });
            if self.learn.hints.is_empty() {
                ui.label(i18n::t(Key::NoHintAvailable));
            }
            let mut clicked_hint: Option<Move> = None;
            let mut hovered: Option<usize> = None;
            for (i, hint) in self.learn.hints.iter().take(3).enumerate() {
                let tag = if hint.in_book {
                    i18n::t(Key::SuggestionBook)
                } else {
                    i18n::t(Key::SuggestionEngine)
                };
                let text = format!(
                    "{} • {tag} • {} — {}",
                    hint.mv_cn, hint.score, hint.explanation_vi
                );
                let resp = ui.selectable_label(self.learn.hint_hover == Some(i), text);
                if resp.hovered() {
                    hovered = Some(i);
                }
                if resp.clicked() && self.learn.is_learner_turn() && !self.learn.off_book_pending {
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
            if ui.button(i18n::t(Key::FlipBoard)).clicked() {
                self.learn.flipped = !self.learn.flipped;
            }
            if ui.button(i18n::t(Key::OtherLesson)).clicked() {
                self.goto(Screen::LearnSelect);
            }
            if ui.button(i18n::t(Key::BackToMenu)).clicked() {
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
                    if ui.button(i18n::t(Key::RetryMove)).clicked() {
                        retry = true;
                    }
                    if ui.button(i18n::t(Key::ContinueMove)).clicked() {
                        cont = true;
                    }
                });
            });
        if retry {
            self.learn.retry();
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
                ui.label(i18n::t(Key::Difficulty));
                for d in 0..=3u8 {
                    ui.radio_value(
                        &mut self.learn.continue_difficulty,
                        d,
                        i18n::difficulty_name(d),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button(i18n::t(Key::StopAndSummary)).clicked() {
                        to_summary = true;
                    }
                    if ui.button(i18n::t(Key::ContinueWithAi)).clicked() {
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
