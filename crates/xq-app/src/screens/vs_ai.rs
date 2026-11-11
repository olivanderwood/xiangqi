//! Vs AI screen (§6.2) + two-player-on-one-machine mode.
//!
//! Two-player mode is not spelled out in §6.2, but T3.2 requires playing
//! two humans on one machine; it lives here as an extra "Bên" choice in
//! the start dialog (see `docs/DECISIONS-m3.md`).

use crate::ai_worker::{response_is_current, AiRequest, AiResponse};
use crate::app::{eval_bar_ui, is_narrow, move_list_ui, Screen, XiangqiApp};
use crate::board_view::{
    draw_board, legal_targets, BoardDraw, BoardGeometry, BoardInteraction, HintArrow, MoveAnim,
};
use crate::i18n::{self, Key};
use std::time::{SystemTime, UNIX_EPOCH};
use xq_core::analysis::{explain_tags, grade_from_delta_with, tags_for, Grade};
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::{Game, GameResult};
use xq_core::search::AnalysisLine;
use xq_core::types::{Color, Move, PieceKind};
use xq_engine::{EngineRequest, Level, SharedEngine};
use xq_pikafish::{pick_weighted, GameRng, DIVERSITY_MARGIN_CP, DIVERSITY_PLIES};

/// Fresh per-game seed from the clock.
fn new_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (u64::from(d.subsec_nanos()) << 32) | u64::from(d.as_secs() as u32))
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartSide {
    Red,
    Black,
    Random,
    TwoPlayer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VsOpponent {
    Ai { human: Color, difficulty: u8 },
    TwoPlayer,
}

pub struct VsAiState {
    pub game: Game,
    pub opponent: VsOpponent,
    pub flipped: bool,
    pub board: BoardInteraction,
    pub anim: Option<MoveAnim>,
    pub last_move: Option<Move>,
    pub ai_thinking: bool,
    pub ai_req_id: Option<u64>,
    pub ai_depth: u8,
    pub hint_arrows: Vec<HintArrow>,
    pub hint_lines: Vec<AnalysisLine>,
    pub hint_req_id: Option<u64>,
    pub hint_count: u32,
    pub hint_for_ply: Option<usize>,
    pub review: Option<usize>,
    pub show_start_dialog: bool,
    pub start_dialog_ready: bool,
    pub start_side: StartSide,
    pub start_difficulty: u8,
    pub started: bool,
    pub result_ack: bool,
    pub draw_message: Option<String>,
    pub check_since: Option<f64>,
    pub auto_hint: bool,
    pub from_learn: bool,
    pub last_grade: Option<(Grade, String)>,
    pub last_board: Option<(egui::Rect, BoardGeometry)>,
    /// Seed for this game's diverse/weaken picks (§5: same seed +
    /// same position → same pick, so a replay reproduces the game).
    pub game_seed: u64,
    /// Custom Elo (Settings slider) for difficulty 4.
    pub custom_elo: u16,
    /// Request id of a pending diverse/weaken analysis (its `Analysis`
    /// response is routed to the pick, not to the hint arrows).
    pub diverse_req_id: Option<u64>,
    /// §5 settings (P7 adds the UI toggles; defaults here).
    pub diversity: bool,
    pub tournament: bool,
    /// Shared engine (builtin now, Pikafish when available) for the
    /// synchronous grading in continue-from-learn mode.
    pub engine: SharedEngine,
}

impl VsAiState {
    pub fn new(engine: SharedEngine) -> Self {
        Self {
            game: Game::new(Position::startpos()),
            opponent: VsOpponent::Ai {
                human: Color::Red,
                difficulty: 1,
            },
            flipped: false,
            board: BoardInteraction::default(),
            anim: None,
            last_move: None,
            ai_thinking: false,
            ai_req_id: None,
            ai_depth: 0,
            hint_arrows: Vec::new(),
            hint_lines: Vec::new(),
            hint_req_id: None,
            hint_count: 0,
            hint_for_ply: None,
            review: None,
            show_start_dialog: false,
            start_dialog_ready: false,
            start_side: StartSide::Red,
            start_difficulty: 1,
            started: false,
            result_ack: true,
            draw_message: None,
            check_since: None,
            auto_hint: false,
            from_learn: false,
            last_grade: None,
            last_board: None,
            game_seed: new_seed(),
            custom_elo: 1900,
            diverse_req_id: None,
            diversity: true,
            tournament: false,
            engine,
        }
    }

    fn human_color(&self) -> Option<Color> {
        match self.opponent {
            VsOpponent::Ai { human, .. } => Some(human),
            VsOpponent::TwoPlayer => None,
        }
    }

    fn level(&self) -> Level {
        match self.opponent {
            VsOpponent::Ai { difficulty, .. } => match difficulty {
                0 => Level::Easy,
                2 => Level::Hard,
                3 => Level::Master,
                4 => Level::Elo(self.custom_elo),
                _ => Level::Medium,
            },
            VsOpponent::TwoPlayer => Level::Medium,
        }
    }

    /// Play a move: history, animation, hints cleared, check flash armed.
    pub fn play(&mut self, mv: Move, now: f64, animate: bool) {
        let pos_before = self.game.pos.clone();
        let moves_before = self.game.moves.clone();
        let piece = pos_before.piece_at(mv.from);
        let mover = pos_before.side();
        if self.game.play(mv).is_err() {
            return;
        }
        // Grade the move in continue-from-learn mode (§6.5).
        if self.from_learn && Some(mover) == self.human_color() {
            let req = EngineRequest::new(
                self.game.start.to_fen(),
                moves_before,
                self.engine.lock().expect("engine lock").grade_limits(),
            );
            let handle = self.engine.clone();
            let mut engine = handle.lock().expect("engine lock");
            if let Ok(best) = engine.best_move(&req, &mut |_| {}) {
                let best_cp = best.score.to_cp_clamped();
                let mv_cp = engine
                    .score_move(&req, mv)
                    .map(|s| s.to_cp_clamped())
                    .unwrap_or(best_cp);
                let grade =
                    grade_from_delta_with(best_cp - mv_cp, false, engine.grade_thresholds());
                let expl = explain_tags(&tags_for(&pos_before, mv));
                self.last_grade = Some((grade, expl));
            }
        }
        self.last_move = Some(mv);
        self.anim = if animate {
            piece.map(|p| MoveAnim::new(mv, p, now, 0.15))
        } else {
            None
        };
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_for_ply = None;
        self.board.clear();
        self.review = None;
        self.check_since = if self.game.pos.in_check(self.game.pos.side()) {
            Some(now)
        } else {
            None
        };
        if self.game.result().is_over() {
            self.result_ack = false;
        }
    }

    /// Start a fresh game from the start dialog.
    pub fn start(
        &mut self,
        side: StartSide,
        difficulty: u8,
        settings_human: &mut Option<String>,
        settings_diff: &mut u8,
    ) {
        let human = match side {
            StartSide::Red => Color::Red,
            StartSide::Black => Color::Black,
            StartSide::Random => {
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                if nanos.is_multiple_of(2) {
                    Color::Red
                } else {
                    Color::Black
                }
            }
            StartSide::TwoPlayer => Color::Red, // unused in two-player mode
        };
        self.opponent = match side {
            StartSide::TwoPlayer => VsOpponent::TwoPlayer,
            _ => VsOpponent::Ai { human, difficulty },
        };
        self.game = Game::new(Position::startpos());
        self.flipped = matches!(
            self.opponent,
            VsOpponent::Ai {
                human: Color::Black,
                ..
            }
        );
        self.ai_thinking = false;
        self.ai_req_id = None;
        self.game_seed = new_seed();
        self.diverse_req_id = None;
        self.ai_depth = 0;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_req_id = None;
        self.hint_count = 0;
        self.hint_for_ply = None;
        self.review = None;
        self.last_move = None;
        self.anim = None;
        self.board.clear();
        self.check_since = None;
        self.result_ack = true;
        self.draw_message = None;
        self.last_grade = None;
        self.auto_hint = false;
        self.from_learn = false;
        self.started = true;
        self.show_start_dialog = false;
        *settings_human = Some(match side {
            StartSide::Red => "red".to_string(),
            StartSide::Black => "black".to_string(),
            StartSide::Random => "random".to_string(),
            StartSide::TwoPlayer => "two-player".to_string(),
        });
        *settings_diff = difficulty;
    }

    /// Continue a finished opening as a Vs-AI game (§6.5): keep the whole
    /// history, human keeps the learner's colour, hints auto-on.
    pub fn start_from_learn(&mut self, game: Game, human: Color) {
        self.game = game;
        self.opponent = VsOpponent::Ai {
            human,
            difficulty: 1,
        };
        self.flipped = human == Color::Black;
        self.ai_thinking = false;
        self.ai_req_id = None;
        self.game_seed = new_seed();
        self.diverse_req_id = None;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_req_id = None;
        self.hint_count = 0;
        self.hint_for_ply = None;
        self.review = None;
        self.last_move = self.game.moves.last().copied();
        self.anim = None;
        self.board.clear();
        self.check_since = None;
        self.result_ack = !self.game.result().is_over();
        self.draw_message = None;
        self.last_grade = None;
        self.auto_hint = true;
        self.from_learn = true;
        self.started = true;
        self.show_start_dialog = false;
    }

    /// Undo: stop the AI first if it is thinking, then step back to the
    /// human's turn (2 plies vs AI, 1 ply in two-player mode).
    pub fn undo(&mut self, worker: &crate::ai_worker::AiWorker) {
        if self.ai_thinking {
            worker.request_stop();
            self.ai_thinking = false;
            self.ai_req_id = None;
            self.diverse_req_id = None;
        }
        let steps = match self.opponent {
            VsOpponent::Ai { .. } => 2,
            VsOpponent::TwoPlayer => 1,
        };
        for _ in 0..steps {
            if self.game.undo().is_none() {
                break;
            }
            // Vs AI: stop once it is the human's turn again.
            if let VsOpponent::Ai { human, .. } = self.opponent {
                if self.game.pos.side() == human {
                    break;
                }
            }
        }
        self.last_move = self.game.moves.last().copied();
        self.anim = None;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_for_ply = None;
        self.board.clear();
        self.review = None;
        self.result_ack = true;
        self.check_since = if self.game.pos.in_check(self.game.pos.side()) {
            Some(0.0)
        } else {
            None
        };
    }

    fn request_hint(&mut self, worker: &mut crate::ai_worker::AiWorker, top_n: usize) {
        let req_id = worker.alloc_req_id();
        self.hint_req_id = Some(req_id);
        self.hint_count += 1;
        let limits = self.engine.lock().expect("engine lock").hint_limits();
        let _ = worker.tx.send(AiRequest::Analyze {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            top_n,
            req_id,
        });
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
                AiResponse::Info { req_id, .. } | AiResponse::BestMove { req_id, .. } => {
                    response_is_current(*req_id, self.ai_req_id)
                }
                AiResponse::Analysis { req_id, .. } => {
                    response_is_current(*req_id, self.diverse_req_id)
                        || response_is_current(*req_id, self.hint_req_id)
                }
            };
            if !mine {
                i += 1;
                continue;
            }
            let resp = pending.remove(i);
            match resp {
                AiResponse::Info { depth, .. } => {
                    self.ai_depth = depth;
                    ctx.request_repaint();
                }
                AiResponse::BestMove { mv, .. } => {
                    self.ai_thinking = false;
                    self.ai_req_id = None;
                    self.ai_depth = 0;
                    self.play(mv, now, animate);
                    ctx.request_repaint();
                }
                AiResponse::Analysis { req_id, lines } => {
                    if Some(req_id) == self.diverse_req_id {
                        self.diverse_req_id = None;
                        self.ai_req_id = None;
                        self.ai_thinking = false;
                        self.ai_depth = 0;
                        let picked = self.pick_from_lines(&lines);
                        match picked.or_else(|| lines.first().map(|l| l.mv)) {
                            Some(mv) => self.play(mv, now, animate),
                            None => self.ai_thinking = false,
                        }
                        ctx.request_repaint();
                        continue;
                    }
                    self.hint_req_id = None;
                    self.hint_lines = lines.clone();
                    self.hint_arrows = lines
                        .iter()
                        .enumerate()
                        .map(|(j, l)| {
                            let horse = self
                                .game
                                .pos
                                .piece_at(l.mv.from)
                                .map(|p| p.kind == PieceKind::Horse)
                                .unwrap_or(false);
                            HintArrow::for_rank(l.mv.from, l.mv.to, j, horse)
                        })
                        .collect();
                    ctx.request_repaint();
                }
            }
        }
    }

    /// (margin_cp, temperature) when this AI move should be picked from
    /// an analysis instead of playing the best move: the §5 diversity
    /// pick at Master, or the level weakening at Easy/Medium.
    fn pick_config(&self) -> Option<(i32, f64)> {
        let level = self.level();
        let engine = self.engine.lock().expect("engine lock");
        if level == Level::Master {
            if !self.diversity
                || self.tournament
                || self.game.moves.len() >= DIVERSITY_PLIES
                || !engine.supports_diversity()
            {
                return None;
            }
            // Diversity only when neither side is in check (§5).
            let pos = &self.game.pos;
            if pos.in_check(pos.side()) || pos.in_check(pos.side().opposite()) {
                return None;
            }
            return Some((DIVERSITY_MARGIN_CP, 10.0));
        }
        engine.weaken(level).map(|w| (w.margin_cp, w.temperature))
    }

    /// Pick the AI's move from an analysis (diversity/weaken path).
    /// The RNG is re-derived from (game seed, ply), so a replay with
    /// the same seed reproduces the same picks.
    fn pick_from_lines(&self, lines: &[AnalysisLine]) -> Option<Move> {
        let (margin, temp) = self.pick_config()?;
        let engine_lines: Vec<xq_engine::Line> = lines
            .iter()
            .map(|l| xq_engine::Line {
                mv: l.mv,
                score: xq_engine::Score::from_cp(l.score),
                depth: l.depth,
                pv: l.pv.clone(),
            })
            .collect();
        let mut rng = GameRng::for_ply(self.game_seed, self.game.moves.len());
        pick_weighted(&engine_lines, margin, temp, &mut rng)
    }

    fn maybe_trigger_ai(&mut self, worker: &mut crate::ai_worker::AiWorker) {
        let VsOpponent::Ai { human, .. } = self.opponent else {
            return;
        };
        if !self.started
            || self.ai_thinking
            || self.review.is_some()
            || self.game.result().is_over()
            || self.game.pos.side() == human
        {
            return;
        }
        let req_id = worker.alloc_req_id();
        self.ai_req_id = Some(req_id);
        self.ai_thinking = true;
        self.ai_depth = 0;
        let limits = self
            .engine
            .lock()
            .expect("engine lock")
            .limits_for(self.level());
        let req = EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits);
        if self.pick_config().is_some() {
            self.diverse_req_id = Some(req_id);
            let _ = worker.tx.send(AiRequest::Analyze {
                req,
                top_n: 3,
                req_id,
            });
        } else {
            let _ = worker.tx.send(AiRequest::BestMove { req, req_id });
        }
    }

    fn maybe_auto_hint(&mut self, worker: &mut crate::ai_worker::AiWorker) {
        if !self.auto_hint || !self.started || self.game.result().is_over() {
            return;
        }
        let Some(human) = self.human_color() else {
            return;
        };
        if self.game.pos.side() != human || self.hint_req_id.is_some() {
            return;
        }
        if self.hint_for_ply == Some(self.game.moves.len()) {
            return;
        }
        self.hint_for_ply = Some(self.game.moves.len());
        self.request_hint(worker, 3);
    }

    fn offer_draw(&mut self) {
        let VsOpponent::Ai { human, .. } = self.opponent else {
            return;
        };
        // AI's score from its own perspective (eval is side-to-move).
        let s = eval::eval(&self.game.pos);
        let ai_score = if self.game.pos.side() == human { -s } else { s };
        if ai_score <= -50 {
            self.game.agree_draw();
            self.draw_message = Some(i18n::t(Key::AiAcceptedDraw).to_string());
            self.result_ack = false;
        } else {
            self.draw_message = Some(i18n::t(Key::AiDeclinedDraw).to_string());
        }
    }

    fn status_text(&self) -> String {
        if let Some(ply) = self.review {
            return i18n::t(Key::Reviewing)
                .replace("{n}", &ply.to_string())
                .replace("{m}", &self.game.moves.len().to_string());
        }
        match self.game.result() {
            GameResult::Ongoing => {
                if self.ai_thinking {
                    if self.ai_depth > 0 {
                        i18n::t(Key::ThinkingDepth).replace("{n}", &self.ai_depth.to_string())
                    } else {
                        i18n::t(Key::Thinking).to_string()
                    }
                } else if self.game.pos.in_check(self.game.pos.side()) {
                    format!(
                        "{} — {}",
                        i18n::t(Key::Check),
                        match self.game.pos.side() {
                            Color::Red => i18n::t(Key::RedToMove),
                            Color::Black => i18n::t(Key::BlackToMove),
                        }
                    )
                } else {
                    match self.game.pos.side() {
                        Color::Red => i18n::t(Key::RedToMove).to_string(),
                        Color::Black => i18n::t(Key::BlackToMove).to_string(),
                    }
                }
            }
            r => r.describe_vi().to_string(),
        }
    }
}

impl XiangqiApp {
    pub(crate) fn show_vs_ai(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let now = ctx.input(|i| i.time);
        let animate = self.settings.animate;
        // §5 settings live here (P7 adds the UI toggles).
        self.vs_ai.diversity = self.settings.diversity;
        self.vs_ai.tournament = self.settings.tournament;
        self.vs_ai.custom_elo = self.settings.custom_elo;
        // 1. Poll the worker (non-blocking) and trigger the AI if needed.
        self.vs_ai.poll(&mut self.pending_ai, ctx, now, animate);
        self.vs_ai.maybe_trigger_ai(&mut self.worker);
        self.vs_ai.maybe_auto_hint(&mut self.worker);
        // Keep repainting while a move slides or the check flash is fresh.
        if self.vs_ai.anim.is_some_and(|a| a.progress(now).is_some())
            || self.vs_ai.check_since.is_some_and(|s| now - s < 1.0)
        {
            ctx.request_repaint();
        }

        // 2. Keyboard shortcuts (§7.3).
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command) {
            let w = &self.worker;
            self.vs_ai.undo(w);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.vs_ai.request_hint(&mut self.worker, 1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.vs_ai.flipped = !self.vs_ai.flipped;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::N)) {
            self.vs_ai.show_start_dialog = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.vs_ai.board.clear();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.vs_ai_step_review(-1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.vs_ai_step_review(1);
        }

        // 3. Layout: board (+eval bar) left, panel right; panel below when narrow.
        // NOTE: the board size MUST be computed from the CentralPanel's
        // available size captured here. Inside `ui.horizontal` egui only
        // offers the current row height (~one line) as available height,
        // so reading `ui.available_size()` inside the row shrinks the
        // board to ~20 px (it was invisible after pressing Start).
        let avail = ui.available_size();
        let narrow = is_narrow(ctx);
        if narrow {
            ui.vertical(|ui| {
                self.vs_ai_board(ctx, ui, avail);
                ui.add_space(8.0);
                self.vs_ai_panel(ctx, ui);
            });
        } else {
            ui.horizontal(|ui| {
                let pos = self.vs_ai_display_pos();
                let s = eval::eval(&pos);
                let score_red = if pos.side() == Color::Red { s } else { -s };
                eval_bar_ui(ui, score_red, avail.y);
                // Board budget: full height, width left after eval bar (16),
                // gap (12), panel (340) and row spacing.
                let budget = egui::Vec2::new(avail.x - 16.0 - 12.0 - 340.0 - 24.0, avail.y);
                self.vs_ai_board(ctx, ui, budget);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(340.0);
                    self.vs_ai_panel(ctx, ui);
                });
            });
        }

        self.vs_ai_start_dialog(ctx);
        self.vs_ai_result_dialog(ctx);
    }

    fn vs_ai_display_pos(&self) -> Position {
        match self.vs_ai.review {
            Some(ply) => Self::position_at(&self.vs_ai.game, ply),
            None => self.vs_ai.game.pos.clone(),
        }
    }

    fn vs_ai_step_review(&mut self, delta: i32) {
        let total = self.vs_ai.game.moves.len();
        if total == 0 {
            return;
        }
        let cur = self.vs_ai.review.unwrap_or(total);
        let next = (cur as i32 + delta).clamp(0, total as i32) as usize;
        self.vs_ai.review = Some(next);
    }

    fn vs_ai_board(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, avail: egui::Vec2) {
        let now = ctx.input(|i| i.time);
        let total = avail;
        let cell = (total.x / 10.0).min(total.y / 11.0);
        let size = egui::Vec2::new(10.0 * cell, 11.0 * cell);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let geom = BoardGeometry::new(size, self.vs_ai.flipped);
        self.vs_ai.last_board = Some((rect, geom));

        let display_pos = self.vs_ai_display_pos();
        let in_review = self.vs_ai.review.is_some();
        let board_enabled = self.vs_ai.started
            && !in_review
            && self.vs_ai.game.result() == GameResult::Ongoing
            && !self.vs_ai.ai_thinking
            && match self.vs_ai.opponent {
                VsOpponent::Ai { human, .. } => self.vs_ai.game.pos.side() == human,
                VsOpponent::TwoPlayer => true,
            };
        let movable = match self.vs_ai.opponent {
            VsOpponent::Ai { human, .. } => human,
            VsOpponent::TwoPlayer => self.vs_ai.game.pos.side(),
        };
        let selected = self.vs_ai.board.selected;
        let targets = selected
            .map(|sq| legal_targets(&self.vs_ai.game.pos, sq))
            .unwrap_or_default();
        let check_sq = if display_pos.in_check(display_pos.side()) {
            Some(display_pos.king_sq(display_pos.side()))
        } else {
            None
        };
        let drag = self
            .vs_ai
            .board
            .drag_sq
            .map(|sq| (sq, self.vs_ai.board.drag_pos));
        let arrows: Vec<HintArrow> = if in_review {
            Vec::new()
        } else {
            self.vs_ai.hint_arrows.clone()
        };
        let draw = BoardDraw {
            selected,
            targets: &targets,
            last_move: self.vs_ai.last_move,
            check_sq,
            check_since: self.vs_ai.check_since,
            now,
            arrows: &arrows,
            drag,
            anim: if in_review { None } else { self.vs_ai.anim },
            hover_sq: self.vs_ai.board.hover_sq,
        };
        let painter = ui.painter_at(rect);
        draw_board(&painter, rect, &geom, &display_pos, &draw);

        if board_enabled {
            if let Some(mv) =
                self.vs_ai
                    .board
                    .handle(&resp, rect, &geom, &self.vs_ai.game.pos, movable)
            {
                self.vs_ai.play(mv, now, self.settings.animate);
            }
        } else {
            // Still track hover so the cursor feedback stays sane.
            let _ = &resp;
        }
    }

    fn vs_ai_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label(self.vs_ai.status_text());
        if self.vs_ai.from_learn {
            ui.label(i18n::t(Key::VsAiContinueNote));
        }
        if let Some((grade, expl)) = &self.vs_ai.last_grade {
            ui.label(format!(
                "{}: {} — {expl}",
                i18n::t(Key::GradeLabel),
                grade.label_vi()
            ));
        }
        if let Some(msg) = &self.vs_ai.draw_message {
            ui.label(msg.clone());
        }
        ui.separator();
        // Game info.
        let (side_txt, diff_txt) = match self.vs_ai.opponent {
            VsOpponent::Ai { human, difficulty } => (
                match human {
                    Color::Red => i18n::t(Key::RedSide),
                    Color::Black => i18n::t(Key::BlackSide),
                }
                .to_string(),
                i18n::difficulty_name(difficulty).to_string(),
            ),
            VsOpponent::TwoPlayer => (i18n::t(Key::TwoPlayers).to_string(), String::new()),
        };
        ui.label(format!("{}: {side_txt} {diff_txt}", i18n::t(Key::GameInfo)));
        ui.label(i18n::t(Key::HintsUsed).replace("{n}", &self.vs_ai.hint_count.to_string()));
        // Move list.
        let moves = self.move_strings(&self.vs_ai.game);
        let mut review = self.vs_ai.review;
        move_list_ui(ui, &moves, &mut review);
        self.vs_ai.review = review;
        if self.vs_ai.review.is_some() && ui.button(i18n::t(Key::BackToEnd)).clicked() {
            self.vs_ai.review = None;
        }
        ui.separator();
        // Buttons.
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n::t(Key::Undo)).clicked() {
                let w = &self.worker;
                self.vs_ai.undo(w);
            }
            if ui.button(i18n::t(Key::Hint)).clicked() {
                self.vs_ai.request_hint(&mut self.worker, 1);
            }
            if matches!(self.vs_ai.opponent, VsOpponent::Ai { .. })
                && ui.button(i18n::t(Key::DrawOffer)).clicked()
            {
                self.vs_ai.offer_draw();
            }
            if ui.button(i18n::t(Key::Resign)).clicked() {
                let side = match self.vs_ai.opponent {
                    VsOpponent::Ai { human, .. } => human,
                    VsOpponent::TwoPlayer => self.vs_ai.game.pos.side(),
                };
                self.vs_ai.game.resign(side);
                self.vs_ai.result_ack = false;
            }
            if ui.button(i18n::t(Key::NewGame)).clicked() {
                self.vs_ai.show_start_dialog = true;
            }
            if ui.button(i18n::t(Key::FlipBoard)).clicked() {
                self.vs_ai.flipped = !self.vs_ai.flipped;
            }
            if ui.button(i18n::t(Key::BackToMenu)).clicked() {
                self.goto(Screen::Menu);
            }
        });
        let _ = ctx;
    }

    fn vs_ai_start_dialog(&mut self, ctx: &egui::Context) {
        if !self.vs_ai.show_start_dialog {
            return;
        }
        // Restore the remembered choices once per opening of the dialog
        // (not every frame, or the user's radio clicks would be undone).
        if !self.vs_ai.start_dialog_ready {
            self.vs_ai.start_difficulty = self.settings.difficulty;
            self.vs_ai.start_side = match self.settings.human_color.as_deref() {
                Some("black") => StartSide::Black,
                Some("random") => StartSide::Random,
                Some("two-player") => StartSide::TwoPlayer,
                _ => StartSide::Red,
            };
            self.vs_ai.start_dialog_ready = true;
        }
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        egui::Window::new(i18n::t(Key::StartGameTitle))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(i18n::t(Key::Side));
                ui.radio_value(
                    &mut self.vs_ai.start_side,
                    StartSide::Red,
                    i18n::t(Key::SideRed),
                );
                ui.radio_value(
                    &mut self.vs_ai.start_side,
                    StartSide::Black,
                    i18n::t(Key::SideBlack),
                );
                ui.radio_value(
                    &mut self.vs_ai.start_side,
                    StartSide::Random,
                    i18n::t(Key::SideRandom),
                );
                ui.radio_value(
                    &mut self.vs_ai.start_side,
                    StartSide::TwoPlayer,
                    i18n::t(Key::TwoPlayers),
                );
                ui.label(i18n::t(Key::Difficulty));
                for d in 0..=4u8 {
                    ui.radio_value(
                        &mut self.vs_ai.start_difficulty,
                        d,
                        i18n::difficulty_name(d),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button(i18n::t(Key::Start)).clicked() {
                        start = true;
                    }
                    if ui.button(i18n::t(Key::Cancel)).clicked() {
                        cancel = true;
                    }
                });
            });
        if start {
            let side = self.vs_ai.start_side;
            let diff = self.vs_ai.start_difficulty;
            self.vs_ai.start(
                side,
                diff,
                &mut self.settings.human_color,
                &mut self.settings.difficulty,
            );
            self.vs_ai.start_dialog_ready = false;
            self.settings_dirty = true;
        } else if cancel || !open {
            self.vs_ai.show_start_dialog = false;
            self.vs_ai.start_dialog_ready = false;
            if !self.vs_ai.started {
                self.goto(Screen::Menu);
            }
        }
    }

    fn vs_ai_result_dialog(&mut self, ctx: &egui::Context) {
        if self.vs_ai.result_ack || !self.vs_ai.game.result().is_over() {
            return;
        }
        let result = self.vs_ai.game.result();
        let mut open = true;
        let mut new_game = false;
        let mut review = false;
        let mut close = false;
        egui::Window::new(i18n::t(Key::ResultTitle))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(result.describe_vi());
                ui.horizontal(|ui| {
                    if ui.button(i18n::t(Key::NewGame)).clicked() {
                        new_game = true;
                    }
                    if ui.button(i18n::t(Key::ReviewGame)).clicked() {
                        review = true;
                    }
                    if ui.button(i18n::t(Key::Close)).clicked() {
                        close = true;
                    }
                });
            });
        if new_game {
            self.vs_ai.result_ack = true;
            self.vs_ai.show_start_dialog = true;
        } else if review {
            self.vs_ai.result_ack = true;
            self.vs_ai.review = Some(self.vs_ai.game.moves.len());
        } else if close || !open {
            self.vs_ai.result_ack = true;
        }
    }
}
