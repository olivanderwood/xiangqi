//! Play-vs-AI screen (DESIGN-iced.md §2.2), ported from the egui
//! app (`crates/xq-app/src/screens/vs_ai.rs`, 1070 lines) — every
//! feature is kept, no feature is added: the start dialog (side +
//! 5 difficulty levels, level 4 stays "Tùy chỉnh" with the custom
//! Elo from Settings), Pikafish over UCI with the builtin fallback,
//! the AI worker, the 150 ms slide, the move list with review, the
//! eval bar + advantage chip, the ChessDB section, Undo / Hint /
//! Draw / Resign / New game, and the move / capture / check /
//! checkmate sounds.
//!
//! # INVARIANT — the piece must land before any computation starts
//!
//! A move's slide animation (150 ms) must finish BEFORE the AI,
//! grading, hint or ChessDB work for the new position starts (Hieu,
//! restated 2026-11-12, three times in one day). Enforced here:
//!
//! * `trigger_work()` — the only place that starts AI / grade /
//!   hint / ChessDB work — returns immediately while
//!   `board.is_animating()`;
//! * board input is ignored while the board is animating, so a new
//!   move cannot start mid-slide either;
//! * the tick subscription only runs while an animation or the
//!   flashing check halo is active, and the tick is what finishes
//!   the animation (`BoardState::finish_anim`) and then calls
//!   `trigger_work()`;
//! * a manual Hint pressed mid-slide is parked in `hint_pending`
//!   and only sent after the piece lands.
//!
//! # Layout (DESIGN-iced.md §2.2)
//!
//! Header (Back · "Chơi với máy" · level · engine name · Settings),
//! then a row of the eval bar, the 592 × 656 board canvas and the
//! fixed 380 px side panel (opponent card, player card, advantage
//! chip, grade, ChessDB section, move list, hint list, buttons).
//! The start and result dialogs are screen-local overlays built
//! with `Stack` (Iced 0.14 has no modal dialog).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iced::widget::{column, container, mouse_area, row, scrollable, text, Column, Stack};
use iced::{
    Alignment, Background, Border, Element, Length, Point, Rectangle, Size, Subscription, Task,
    Theme,
};
use xq_chessdb::{ChessDbError, ChessDbEval};
use xq_core::analysis::Grade;
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::{Game, GameResult};
use xq_core::search::AnalysisLine;
use xq_core::types::{Color, Move};
use xq_engine::{Engine, EngineRequest, Level, Line, Score, StopHandle};
use xq_pikafish::{pick_weighted, GameRng, DIVERSITY_MARGIN_CP, DIVERSITY_PLIES};

use crate::ai_worker::{response_is_current, AiRequest, AiResponse, AiWorker};
use crate::app::{Screen, Shared};
use crate::board::{Arrow, BoardMsg, BoardProgram, BoardRender, BoardState, Geom, Square};
use crate::bridge;
use crate::chessdb_panel::{ChessDbPanel, ChessDbPanelMsg};
use crate::chessdb_worker::ChessDbResponse;
use crate::helpers::{move_strings, position_at, render_move};
use crate::i18n::{self, difficulty_name, Key};
use crate::icons::Icon;
use crate::settings::Settings;
use crate::sound::sound_for_move;
use crate::theme::{
    self, BLACK_PIECE, BORDER, FONT_BODY, FONT_CAPTION, FONT_SCREEN_TITLE, GOLD, GREEN, RADIUS_6,
    RED, RED_PIECE, SPACE_16, SPACE_24, SPACE_4, SPACE_8, SURFACE2, TEXT_DIM,
};
use crate::widgets::{
    advantage_chip, board_size_for_window, card, danger_button_with_icon, ghost_button_with_icon,
    header_divider, primary_button, primary_button_with_icon, secondary_button,
    secondary_button_with_icon, side_card, status_chip, HEADER_HEIGHT,
};

/// Fresh per-game seed from the clock (same as the egui app).
fn new_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (u64::from(d.subsec_nanos()) << 32) | u64::from(d.as_secs() as u32))
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

/// Animation / check-halo tick: 16 ms (~60 fps) so a 150 ms slide
/// redraws about 9 times instead of 3 — the 50 ms tick made the
/// slide visibly choppy. Same interval as Practice.
const TICK_INTERVAL: Duration = Duration::from_millis(16);

/// The engine score from Red's perspective (§6.4): `eval` is
/// side-to-move's, so flip it when Black is to move.
fn score_red(pos: &Position) -> i32 {
    let s = eval::eval(pos);
    if pos.side() == Color::Red {
        s
    } else {
        -s
    }
}

/// A Red-perspective score from one side's perspective: the human's
/// in AI games, Red's in two-player mode.
fn perspective_score(score_red: i32, human: Option<Color>) -> i32 {
    match human {
        Some(Color::Black) => -score_red,
        _ => score_red,
    }
}

/// Difficulty index (0..=4) to its engine level (same table as the
/// egui app; 4 is the custom Elo from Settings).
fn difficulty_level(difficulty: u8, custom_elo: u16) -> Level {
    match difficulty {
        0 => Level::Easy,
        2 => Level::Hard,
        3 => Level::Master,
        4 => Level::Elo(custom_elo),
        _ => Level::Medium,
    }
}

/// Whether the AI accepts a draw offer: its own score (from its
/// perspective) is at most −50 cp (same rule as the egui app).
fn ai_accepts_draw(ai_score: i32) -> bool {
    ai_score <= -50
}

/// Legal destinations for the piece on `sq` (egui
/// `board_view::legal_targets`).
fn legal_targets(pos: &Position, sq: Square) -> Vec<Square> {
    pos.legal_moves()
        .into_iter()
        .filter(|m| m.from == sq)
        .map(|m| m.to)
        .collect()
}

/// Convert a canvas-local drop point to a square. The canvas is
/// [`board_size_for_window`](crate::widgets::board_size_for_window)
/// at the screen's current window size — NOT always the design
/// 592 × 656 — so the geometry must use that same size, or a drop
/// lands on the wrong square as soon as the window shrinks the
/// board below scale 1 (the old code hardcoded the design bounds).
fn square_from_canvas_point(p: Point, size: Size, flipped: bool) -> Option<Square> {
    Geom::from_bounds_flipped(
        Rectangle {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        },
        flipped,
    )
    .square_at(p)
}

/// Which side the human plays (egui `VsAiState::human_color`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartSide {
    Red,
    Black,
    Random,
    TwoPlayer,
}

/// The opponent: the AI (with the human's colour and difficulty) or
/// a second human on the same machine (egui `VsOpponent`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VsOpponent {
    Ai { human: Color, difficulty: u8 },
    TwoPlayer,
}

/// A worker answer, converted from [`AiResponse`] into a `Clone`
/// payload: `App::Message` derives `Clone`, so the screen's `Msg`
/// must be `Clone` too, and `AiResponse` is not.
#[derive(Debug, Clone)]
pub enum AiEvent {
    Info {
        req_id: u64,
        depth: u8,
        score: i32,
        pv: Vec<Move>,
    },
    BestMove {
        req_id: u64,
        mv: Move,
        score: i32,
    },
    Analysis {
        req_id: u64,
        lines: Vec<AnalysisLine>,
    },
    Graded {
        req_id: u64,
        grade: Grade,
        best: Option<Move>,
        explanation: String,
    },
}

impl From<AiResponse> for AiEvent {
    fn from(resp: AiResponse) -> Self {
        match resp {
            AiResponse::Info {
                req_id,
                depth,
                score,
                pv,
            } => AiEvent::Info {
                req_id,
                depth,
                score,
                pv,
            },
            AiResponse::BestMove { req_id, mv, score } => AiEvent::BestMove { req_id, mv, score },
            AiResponse::Analysis { req_id, lines } => AiEvent::Analysis { req_id, lines },
            AiResponse::Graded {
                req_id,
                grade,
                best,
                explanation,
            } => AiEvent::Graded {
                req_id,
                grade,
                best,
                explanation,
            },
        }
    }
}

/// Bridge output mapping: worker answer → screen message.
fn ai_bridge_msg(resp: AiResponse) -> Msg {
    Msg::Ai(AiEvent::from(resp))
}

/// Bridge output mapping: ChessDB answer → screen message.
fn chessdb_bridge_msg(msg: ChessDbPanelMsg) -> Msg {
    match msg {
        ChessDbPanelMsg::Response(ChessDbResponse::Evaluated { req_id, result }) => {
            Msg::ChessDbEvaluated { req_id, result }
        }
    }
}

/// Style of one move-list cell: the reviewed ply gets a surface-2
/// background + border, the rest stay plain.
fn review_cell_style(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_: &Theme| container::Style {
        background: if selected {
            Some(Background::Color(SURFACE2))
        } else {
            None
        },
        border: Border {
            color: BORDER,
            width: if selected { 1.0 } else { 0.0 },
            radius: RADIUS_6.into(),
        },
        ..Default::default()
    }
}

/// Dimmed full-screen layer behind a dialog overlay (same frame as
/// the Settings / About dialog in `screens::settings_dialog`).
fn dim_background(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color {
            a: 0.6,
            ..crate::theme::BG
        })),
        ..Default::default()
    }
}

/// Play-vs-AI screen state (a port of egui `VsAiState`).
pub struct State {
    game: Game,
    opponent: VsOpponent,
    difficulty: u8,
    custom_elo: u16,
    started: bool,
    show_start_dialog: bool,
    start_side: StartSide,
    start_difficulty: u8,
    board: BoardState,
    /// Whether the board is flipped (Black at the bottom). Auto-set
    /// when the human plays Black; the Flip button toggles it
    /// (egui `VsAiState::flipped`).
    flipped: bool,
    selected: Option<Square>,
    hover_sq: Option<Square>,
    last_move: Option<Move>,
    ai_thinking: bool,
    ai_req_id: Option<u64>,
    ai_depth: u8,
    diverse_req_id: Option<u64>,
    hint_arrows: Vec<Arrow>,
    hint_lines: Vec<AnalysisLine>,
    hint_req_id: Option<u64>,
    hint_count: u32,
    hint_for_ply: Option<usize>,
    /// A Hint pressed while a piece was still sliding: sent once the
    /// piece lands (see the invariant at the top of this file).
    hint_pending: bool,
    hovered_hint: Option<usize>,
    review: Option<usize>,
    result_ack: bool,
    draw_message: Option<String>,
    check_since: Option<Instant>,
    last_grade: Option<(Grade, String)>,
    pending_grade: Option<(String, Vec<Move>, Move)>,
    grade_req_id: Option<u64>,
    game_seed: u64,
    from_learn: bool,
    auto_hint: bool,
    engine_ready: bool,
    engine_banner: Option<String>,
    engine_dir: Option<PathBuf>,
    ai_tx: Sender<AiRequest>,
    ai_rx: Arc<Mutex<Receiver<AiResponse>>>,
    ai_stop: StopHandle,
    next_req_id: u64,
    chessdb: ChessDbPanel,
    /// Current window size (tracked via `Msg::WindowResized`, seeded
    /// with the default 1280 × 800): the board's display size comes
    /// from it ([`board_size_for_window`](crate::widgets::board_size_for_window)),
    /// so the board shrinks with the window instead of overflowing.
    window: Size,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// A board interaction (click / drag).
    Board(BoardMsg),
    /// The window was resized: the board's display size follows it.
    WindowResized(Size),
    /// Animation / check-halo tick.
    Tick(Instant),
    /// An answer from the AI worker.
    Ai(AiEvent),
    /// A ChessDB evaluation (rebuilt into a panel message on receipt).
    ChessDbEvaluated {
        req_id: u64,
        result: Result<ChessDbEval, ChessDbError>,
    },
    /// Pikafish installed and is now the active engine.
    EngineInstalled { name: String, dir: PathBuf },
    /// Pikafish is unavailable; the builtin engine stays active.
    EngineUnavailable(String),
    /// Start-dialog: pick a side.
    StartDialogSide(StartSide),
    /// Start-dialog: pick a difficulty.
    StartDialogDifficulty(u8),
    /// Start-dialog: start the game.
    StartGame,
    /// Start-dialog: cancel (back to the menu when no game started).
    CancelStart,
    /// Undo (back to the human's turn).
    Undo,
    /// Flip the board (Red/Black at the bottom).
    FlipBoard,
    /// Ask for a hint.
    Hint,
    /// Play the `n`-th hint line.
    PlayHint(usize),
    /// Hover a hint row (`None` = left the list).
    HoverHint(Option<usize>),
    /// Offer a draw to the AI.
    OfferDraw,
    /// Resign.
    Resign,
    /// Open the start dialog for a new game.
    NewGame,
    /// Review the position after ply `n` (1-based).
    Review(usize),
    /// Leave review mode, back to the live position.
    BackToEnd,
    /// Result dialog: new game.
    ResultNewGame,
    /// Result dialog: review the game.
    ResultReview,
    /// Result dialog: close.
    ResultClose,
}

impl State {
    /// Create the screen: the AI worker starts here, so the first
    /// game does not wait for it. The Pikafish pre-warm is owned by
    /// `App` (not here): `App` forwards the install outcome as
    /// `Msg::EngineInstalled` / `Msg::EngineUnavailable`, which is
    /// what sets `engine_ready` / the banner below. Pikafish
    /// installs behind the shared engine; when it is unavailable
    /// the builtin engine stays active and the banner (from
    /// `EngineEvent::Unavailable`) says so.
    #[must_use]
    pub fn new(shared: &Shared) -> Self {
        let worker = AiWorker::new(shared.engine.clone());
        // Move the worker's channels out for the bridge (see
        // `bridge.rs`: the forwarder thread is the receiver's only
        // consumer while bridged). `AiWorker` has no `Drop`, so
        // moving its public fields out is legal; the private
        // fallback notice is dropped with the shell — the worker
        // thread keeps its own clone, and the screen detects a
        // mid-game engine swap by the engine's name instead (see
        // `check_engine_fallback`).
        let ai_tx = worker.tx;
        let ai_rx = Arc::new(Mutex::new(worker.rx));
        let ai_stop = worker.stop;
        let next_req_id = worker.next_req_id;
        let (start_side, start_difficulty) = (
            match shared.settings.human_color.as_deref() {
                Some("black") => StartSide::Black,
                Some("random") => StartSide::Random,
                Some("two-player") => StartSide::TwoPlayer,
                _ => StartSide::Red,
            },
            shared.settings.difficulty,
        );
        Self {
            game: Game::new(Position::startpos()),
            opponent: VsOpponent::Ai {
                human: Color::Red,
                difficulty: start_difficulty,
            },
            difficulty: start_difficulty,
            custom_elo: shared.settings.custom_elo,
            started: false,
            show_start_dialog: true,
            start_side,
            start_difficulty,
            board: BoardState::default(),
            flipped: false,
            selected: None,
            hover_sq: None,
            last_move: None,
            ai_thinking: false,
            ai_req_id: None,
            ai_depth: 0,
            diverse_req_id: None,
            hint_arrows: Vec::new(),
            hint_lines: Vec::new(),
            hint_req_id: None,
            hint_count: 0,
            hint_for_ply: None,
            hint_pending: false,
            hovered_hint: None,
            review: None,
            result_ack: true,
            draw_message: None,
            check_since: None,
            last_grade: None,
            pending_grade: None,
            grade_req_id: None,
            game_seed: new_seed(),
            from_learn: false,
            auto_hint: false,
            engine_ready: false,
            engine_banner: None,
            engine_dir: None,
            ai_tx,
            ai_rx,
            ai_stop,
            next_req_id,
            chessdb: ChessDbPanel::new(true, "chessdb-play"),
            window: Size::new(1280.0, 800.0),
        }
    }

    /// Iced `update`: the three shared variants are intercepted by
    /// `App` and never reach here.
    pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout => Task::none(),
            Msg::WindowResized(size) => {
                self.window = size;
                Task::none()
            }
            Msg::Board(bm) => {
                self.handle_board(bm, shared);
                Task::none()
            }
            Msg::Tick(now) => {
                // The piece just landed (or the tick fired while no
                // animation was running): start the work that was
                // waiting for the landing.
                let _ = self.board.finish_anim(now);
                self.trigger_work(shared);
                self.check_engine_fallback(shared);
                Task::none()
            }
            Msg::Ai(ev) => {
                self.handle_ai(ev, shared);
                Task::none()
            }
            Msg::ChessDbEvaluated { req_id, result } => {
                let _ =
                    self.chessdb
                        .update(ChessDbPanelMsg::Response(ChessDbResponse::Evaluated {
                            req_id,
                            result,
                        }));
                Task::none()
            }
            Msg::EngineInstalled { name, dir } => {
                shared.engine_name = name;
                self.engine_ready = true;
                self.engine_banner = None;
                self.engine_dir = Some(dir);
                Task::none()
            }
            Msg::EngineUnavailable(msg) => {
                self.engine_ready = false;
                self.engine_banner = Some(msg);
                Task::none()
            }
            Msg::StartDialogSide(side) => {
                self.start_side = side;
                Task::none()
            }
            Msg::StartDialogDifficulty(d) => {
                self.start_difficulty = d;
                Task::none()
            }
            Msg::StartGame => {
                let side = self.start_side;
                let diff = self.start_difficulty;
                self.start(side, diff, shared);
                // No animation is running at the start of a game, so
                // the AI moves at once when it has the first move.
                self.trigger_work(shared);
                Task::none()
            }
            Msg::CancelStart => {
                self.show_start_dialog = false;
                if !self.started {
                    return Task::done(Msg::Navigate(Screen::Home));
                }
                Task::none()
            }
            Msg::Undo => {
                self.undo();
                self.trigger_work(shared);
                Task::none()
            }
            Msg::FlipBoard => {
                self.flipped = !self.flipped;
                Task::none()
            }
            Msg::Hint => {
                if self.board.is_animating() {
                    // Park it: the hint analysis starts only after
                    // the piece lands (see the invariant).
                    self.hint_pending = true;
                } else {
                    self.request_hint_now(shared, 1);
                }
                Task::none()
            }
            Msg::PlayHint(n) => {
                if let Some(line) = self.hint_lines.get(n) {
                    let mv = line.mv;
                    self.play_move(mv, shared);
                }
                Task::none()
            }
            Msg::HoverHint(n) => {
                self.hovered_hint = n;
                Task::none()
            }
            Msg::OfferDraw => {
                self.offer_draw();
                Task::none()
            }
            Msg::Resign => {
                let side = match self.opponent {
                    VsOpponent::Ai { human, .. } => human,
                    VsOpponent::TwoPlayer => self.game.pos.side(),
                };
                self.game.resign(side);
                self.result_ack = false;
                Task::none()
            }
            Msg::NewGame => {
                self.open_start_dialog(shared);
                Task::none()
            }
            Msg::Review(ply) => {
                self.review = Some(ply);
                self.trigger_work(shared);
                Task::none()
            }
            Msg::BackToEnd => {
                self.review = None;
                self.trigger_work(shared);
                Task::none()
            }
            Msg::ResultNewGame => {
                self.result_ack = true;
                self.open_start_dialog(shared);
                Task::none()
            }
            Msg::ResultReview => {
                self.result_ack = true;
                self.review = Some(self.game.moves.len());
                Task::none()
            }
            Msg::ResultClose => {
                self.result_ack = true;
                Task::none()
            }
        }
    }

    /// Iced `view`: header + (eval bar, board, panel), plus the
    /// start / result dialog overlay when one is open.
    pub fn view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Msg> {
        let base = self.view_base(shared);
        if self.show_start_dialog {
            return Stack::with_children([base, self.start_dialog_overlay()])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        if !self.result_ack && self.game.result().is_over() {
            return Stack::with_children([base, self.result_dialog_overlay()])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        base
    }

    /// Iced `subscription`: the AI bridge and the ChessDB bridge
    /// always run; the tick only runs while a piece is sliding or
    /// the check halo is flashing. (The engine-install bridge
    /// lives on `App`, not here — `App` forwards its outcome as
    /// `Msg::EngineInstalled` / `Msg::EngineUnavailable`.)
    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            bridge::from_mpsc("ai", Arc::clone(&self.ai_rx)).map(ai_bridge_msg),
            self.chessdb.subscription().map(chessdb_bridge_msg),
            iced::window::resize_events().map(|(_id, size)| Msg::WindowResized(size)),
        ];
        if self.board.is_animating() || self.check_flash_active() {
            subs.push(iced::time::every(TICK_INTERVAL).map(Msg::Tick));
        }
        Subscription::batch(subs)
    }

    // ------------------------------------------------------------------
    // Game state
    // ------------------------------------------------------------------

    /// The human's colour, if any (egui `VsAiState::human_color`).
    fn human_color(&self) -> Option<Color> {
        match self.opponent {
            VsOpponent::Ai { human, .. } => Some(human),
            VsOpponent::TwoPlayer => None,
        }
    }

    /// The engine level for the current game (egui `VsAiState::level`).
    fn level(&self) -> Level {
        match self.opponent {
            VsOpponent::Ai { difficulty, .. } => difficulty_level(difficulty, self.custom_elo),
            VsOpponent::TwoPlayer => Level::Medium,
        }
    }

    /// The position on display: the live one, or the reviewed ply.
    fn display_pos(&self) -> Position {
        match self.review {
            Some(ply) => position_at(&self.game, ply),
            None => self.game.pos.clone(),
        }
    }

    /// Whether the board accepts input right now (egui
    /// `board_enabled` in `vs_ai_board`).
    fn board_enabled(&self) -> bool {
        self.started
            && self.review.is_none()
            && self.game.result() == GameResult::Ongoing
            && !self.ai_thinking
            && match self.opponent {
                VsOpponent::Ai { human, .. } => self.game.pos.side() == human,
                VsOpponent::TwoPlayer => true,
            }
    }

    /// Which colour may be moved by the human right now.
    fn movable_color(&self) -> Color {
        match self.opponent {
            VsOpponent::Ai { human, .. } => human,
            VsOpponent::TwoPlayer => self.game.pos.side(),
        }
    }

    /// Whether the check halo is still flashing (under 1 s since
    /// the checking move — the tick only runs while this is true).
    fn check_flash_active(&self) -> bool {
        self.check_since
            .is_some_and(|s| s.elapsed() < Duration::from_secs(1))
    }

    /// Check-halo pulse 0..1 (~2 Hz while flashing, 0.5 otherwise).
    fn check_pulse(&self) -> f32 {
        match self.check_since {
            Some(since) => {
                let e = since.elapsed().as_secs_f32();
                if e < 1.0 {
                    0.5 + 0.5 * (e * 4.0 * std::f32::consts::PI).sin()
                } else {
                    0.5
                }
            }
            None => 0.5,
        }
    }

    /// Allocate the next worker request id.
    fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }

    /// Ask the running search to stop (egui `AiWorker::request_stop`).
    fn request_stop(&self) {
        self.ai_stop.stop();
        let _ = self.ai_tx.send(AiRequest::Stop);
    }

    /// Play a move: history, animation, hints cleared, check flash
    /// armed, sound played (egui `VsAiState::play`). The grade is
    /// only recorded, never computed here — the worker grades it
    /// after the piece lands.
    fn play(&mut self, mv: Move, shared: &Shared) {
        let pos_before = self.game.pos.clone();
        let moves_before = self.game.moves.clone();
        let mover = pos_before.side();
        if self.game.play(mv).is_err() {
            return;
        }
        if self.from_learn && Some(mover) == self.human_color() {
            self.pending_grade = Some((self.game.start.to_fen(), moves_before, mv));
        }
        self.last_move = Some(mv);
        if shared.settings.animate {
            self.board.start_anim(mv.from, mv.to);
        } else {
            self.board.anim = None;
        }
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_for_ply = None;
        // A hint in flight is for the old position: drop its id so
        // the stale Analysis response is ignored when it arrives.
        self.hint_req_id = None;
        self.selected = None;
        self.review = None;
        self.check_since = if self.game.pos.in_check(self.game.pos.side()) {
            Some(Instant::now())
        } else {
            None
        };
        if self.game.result().is_over() {
            self.result_ack = false;
        }
        sound_for_move(&shared.sound, &pos_before, mv, &self.game.pos);
    }

    /// Play a move from the board / hint list, then start the work
    /// that follows it (a no-op while the piece is still sliding).
    fn play_move(&mut self, mv: Move, shared: &mut Shared) {
        self.play(mv, shared);
        self.trigger_work(shared);
    }

    /// Start a fresh game from the start dialog (egui `VsAiState::start`).
    fn start(&mut self, side: StartSide, difficulty: u8, shared: &mut Shared) {
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
        // Auto-flip when the human plays Black (egui).
        self.flipped = human == Color::Black;
        self.difficulty = difficulty;
        self.custom_elo = shared.settings.custom_elo;
        self.game = Game::new(Position::startpos());
        self.game_seed = new_seed();
        self.ai_thinking = false;
        self.ai_req_id = None;
        self.diverse_req_id = None;
        self.ai_depth = 0;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_req_id = None;
        self.hint_count = 0;
        self.hint_for_ply = None;
        self.hint_pending = false;
        self.hovered_hint = None;
        self.review = None;
        self.last_move = None;
        self.board.anim = None;
        self.selected = None;
        self.check_since = None;
        self.result_ack = true;
        self.draw_message = None;
        self.last_grade = None;
        self.pending_grade = None;
        self.grade_req_id = None;
        self.auto_hint = false;
        self.from_learn = false;
        self.started = true;
        self.show_start_dialog = false;
        shared.settings.human_color = Some(match side {
            StartSide::Red => "red".to_string(),
            StartSide::Black => "black".to_string(),
            StartSide::Random => "random".to_string(),
            StartSide::TwoPlayer => "two-player".to_string(),
        });
        shared.settings.difficulty = difficulty;
        shared.settings.save();
        // Best effort: keep the engine's stored level in sync (the
        // per-request `limits_for(level)` is authoritative, so a
        // busy engine is fine to skip).
        if let Some(mut engine) = shared.engine.try_lock() {
            let _ = engine.set_level(self.level());
        }
    }

    /// Continue a finished opening as a Vs-AI game (egui
    /// `VsAiState::start_from_learn`, §6.5): keep the whole history,
    /// human keeps the learner's colour, hints auto-on. Called by
    /// the Learn screen's "Chơi tiếp với máy" (P4 wiring).
    pub fn start_from_learn(&mut self, game: Game, human: Color) {
        self.game = game;
        self.opponent = VsOpponent::Ai {
            human,
            difficulty: 1,
        };
        self.difficulty = 1;
        self.ai_thinking = false;
        self.ai_req_id = None;
        self.game_seed = new_seed();
        self.diverse_req_id = None;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_req_id = None;
        self.hint_count = 0;
        self.hint_for_ply = None;
        self.hint_pending = false;
        self.hovered_hint = None;
        self.review = None;
        self.last_move = self.game.moves.last().copied();
        self.board.anim = None;
        self.selected = None;
        self.check_since = None;
        self.result_ack = !self.game.result().is_over();
        self.draw_message = None;
        self.last_grade = None;
        self.pending_grade = None;
        self.grade_req_id = None;
        self.auto_hint = true;
        self.from_learn = true;
        self.flipped = human == Color::Black;
        self.started = true;
        self.show_start_dialog = false;
    }

    /// Undo: stop the AI first if it is thinking, then step back to
    /// the human's turn (2 plies vs AI, 1 ply in two-player mode).
    fn undo(&mut self) {
        if self.ai_thinking {
            self.request_stop();
            self.ai_thinking = false;
            self.ai_req_id = None;
            self.diverse_req_id = None;
        }
        self.pending_grade = None;
        self.grade_req_id = None;
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
        self.board.anim = None;
        self.hint_arrows.clear();
        self.hint_lines.clear();
        self.hint_for_ply = None;
        self.hint_req_id = None;
        self.hint_pending = false;
        self.selected = None;
        self.review = None;
        self.result_ack = true;
        self.check_since = if self.game.pos.in_check(self.game.pos.side()) {
            Some(Instant::now())
        } else {
            None
        };
    }

    /// Offer a draw: the AI accepts when its own score is at most
    /// −50 cp (egui `VsAiState::offer_draw`).
    fn offer_draw(&mut self) {
        let VsOpponent::Ai { human, .. } = self.opponent else {
            return;
        };
        let s = eval::eval(&self.game.pos);
        let ai_score = if self.game.pos.side() == human { -s } else { s };
        if ai_accepts_draw(ai_score) {
            self.game.agree_draw();
            self.draw_message = Some(i18n::t(Key::AiAcceptedDraw).to_string());
            self.result_ack = false;
        } else {
            self.draw_message = Some(i18n::t(Key::AiDeclinedDraw).to_string());
        }
    }

    /// Status line (egui `VsAiState::status_text`).
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

    // ------------------------------------------------------------------
    // Worker triggers (all gated by the landing invariant)
    // ------------------------------------------------------------------

    /// Start AI / grade / hint / ChessDB work for the current
    /// position — but ONLY when no piece is sliding (see the
    /// invariant at the top of this file). Safe to call on every
    /// tick: each trigger is idempotent or deduped (ChessDB by FEN,
    /// the grade by `pending_grade`, the AI by `ai_thinking`).
    fn trigger_work(&mut self, shared: &mut Shared) {
        if self.board.is_animating() {
            return;
        }
        self.maybe_trigger_ai(shared);
        // Grade AFTER the AI's request, so the grade never delays
        // the AI's reply (Hieu, 2026-11-12).
        self.maybe_send_grade();
        self.maybe_auto_hint(shared);
        if self.hint_pending {
            self.hint_pending = false;
            self.request_hint_now(shared, 1);
        }
        // ChessDB: a separate live score for the position on display.
        let fen = self.display_pos().to_fen();
        self.chessdb.request(&fen);
    }

    /// Ask the AI to move, when it is its turn (egui
    /// `VsAiState::maybe_trigger_ai`). Never blocks on the engine:
    /// a busy engine defers the trigger to the next tick.
    fn maybe_trigger_ai(&mut self, shared: &Shared) {
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
        let Some(engine) = shared.engine.try_lock() else {
            return;
        };
        let limits = engine.limits_for(self.level());
        let config = self.pick_config(&**engine, &shared.settings);
        drop(engine);
        let req_id = self.alloc_req_id();
        self.ai_req_id = Some(req_id);
        self.ai_thinking = true;
        self.ai_depth = 0;
        let req = EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits);
        if config.is_some() {
            self.diverse_req_id = Some(req_id);
            let _ = self.ai_tx.send(AiRequest::Analyze {
                req,
                top_n: 3,
                req_id,
            });
        } else {
            let _ = self.ai_tx.send(AiRequest::BestMove { req, req_id });
        }
    }

    /// Send a pending move grade to the worker (egui
    /// `VsAiState::maybe_send_grade`). The worker builds the
    /// `EngineRequest` itself with this engine's `grade_limits()`
    /// (Pikafish `time_ms(1500)`, builtin depth 6), so the UI never
    /// locks the engine to grade a move.
    fn maybe_send_grade(&mut self) {
        let Some((start_fen, moves, mv)) = self.pending_grade.take() else {
            return;
        };
        let req_id = self.alloc_req_id();
        self.grade_req_id = Some(req_id);
        let _ = self.ai_tx.send(AiRequest::GradeMove {
            start_fen,
            moves,
            mv,
            req_id,
        });
    }

    /// Auto-hint in continue-from-learn mode (egui
    /// `VsAiState::maybe_auto_hint`).
    fn maybe_auto_hint(&mut self, shared: &Shared) {
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
        // Mark the ply as hinted only once the request is actually
        // sent; if the engine is busy `send_hint_request` does
        // nothing and the next tick retries.
        let before = self.hint_req_id;
        self.send_hint_request(shared, 3);
        if self.hint_req_id != before {
            self.hint_for_ply = Some(self.game.moves.len());
        }
    }

    /// (margin_cp, temperature) when this AI move should be picked
    /// from an analysis instead of playing the best move: the §5
    /// diversity pick at Master, or the level weakening at
    /// Easy/Medium (egui `VsAiState::pick_config`).
    fn pick_config(&self, engine: &dyn Engine, settings: &Settings) -> Option<(i32, f64)> {
        let level = self.level();
        if level == Level::Master {
            if !settings.diversity
                || settings.tournament
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
    fn pick_from_lines(&self, lines: &[AnalysisLine], shared: &Shared) -> Option<Move> {
        let engine = shared.engine.try_lock()?;
        let (margin, temp) = self.pick_config(&**engine, &shared.settings)?;
        let engine_lines: Vec<Line> = lines
            .iter()
            .map(|l| Line {
                mv: l.mv,
                score: Score::from_cp(l.score),
                depth: l.depth,
                pv: l.pv.clone(),
            })
            .collect();
        let mut rng = GameRng::for_ply(self.game_seed, self.game.moves.len());
        pick_weighted(&engine_lines, margin, temp, &mut rng)
    }

    /// Send a hint analysis for the current position (egui
    /// `VsAiState::request_hint`, top part). Returns nothing: when
    /// the engine is busy the caller retries later instead of
    /// marking the ply as hinted.
    fn send_hint_request(&mut self, shared: &Shared, top_n: usize) {
        let Some(engine) = shared.engine.try_lock() else {
            return;
        };
        let limits = engine.hint_limits();
        drop(engine);
        let req_id = self.alloc_req_id();
        self.hint_req_id = Some(req_id);
        self.hint_count += 1;
        let _ = self.ai_tx.send(AiRequest::Analyze {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            top_n,
            req_id,
        });
    }

    /// A manual hint request: only on the human's turn, in a live
    /// game, outside review mode.
    fn request_hint_now(&mut self, shared: &Shared, top_n: usize) {
        if !self.started || self.game.result().is_over() || self.review.is_some() {
            return;
        }
        let human_turn = match self.opponent {
            VsOpponent::Ai { human, .. } => self.game.pos.side() == human,
            VsOpponent::TwoPlayer => true,
        };
        if !human_turn {
            return;
        }
        self.send_hint_request(shared, top_n);
    }

    /// Detect a mid-game engine swap (the worker fell back from
    /// Pikafish to the builtin): the worker's own fallback notice
    /// is not reachable from this screen (see `State::new`), so the
    /// swap is detected by the engine's name instead. Shows the
    /// banner once and keeps the game running on the builtin.
    fn check_engine_fallback(&mut self, shared: &mut Shared) {
        if !self.engine_ready {
            return;
        }
        let Some(engine) = shared.engine.try_lock() else {
            return;
        };
        if engine.name() == "Builtin" {
            drop(engine);
            self.engine_ready = false;
            shared.engine_name = "AI tích hợp".to_string();
            self.engine_banner =
                Some("Không chạy được Pikafish, đang dùng AI tích hợp".to_string());
        }
    }

    /// Handle one worker answer, dropping stale responses by
    /// `req_id` (egui `VsAiState::poll`).
    fn handle_ai(&mut self, ev: AiEvent, shared: &mut Shared) {
        match ev {
            AiEvent::Info { req_id, depth, .. } => {
                if response_is_current(req_id, self.ai_req_id) {
                    self.ai_depth = depth;
                }
            }
            AiEvent::BestMove { req_id, mv, .. } => {
                if !response_is_current(req_id, self.ai_req_id) {
                    return;
                }
                self.ai_thinking = false;
                self.ai_req_id = None;
                self.ai_depth = 0;
                self.play(mv, shared);
                self.trigger_work(shared);
            }
            AiEvent::Analysis { req_id, lines } => {
                let is_diverse = response_is_current(req_id, self.diverse_req_id);
                let is_hint = response_is_current(req_id, self.hint_req_id);
                if !is_diverse && !is_hint {
                    return;
                }
                if is_diverse {
                    self.diverse_req_id = None;
                    self.ai_req_id = None;
                    self.ai_thinking = false;
                    self.ai_depth = 0;
                    let picked = self.pick_from_lines(&lines, shared);
                    if let Some(mv) = picked.or_else(|| lines.first().map(|l| l.mv)) {
                        self.play(mv, shared);
                        self.trigger_work(shared);
                    }
                    return;
                }
                self.hint_req_id = None;
                self.hint_arrows = lines
                    .iter()
                    .enumerate()
                    .map(|(j, l)| Arrow::suggestion(l.mv.from, l.mv.to, j + 1))
                    .collect();
                self.hint_lines = lines;
            }
            AiEvent::Graded {
                req_id,
                grade,
                explanation,
                ..
            } => {
                if !response_is_current(req_id, self.grade_req_id) {
                    return;
                }
                self.grade_req_id = None;
                self.last_grade = Some((grade, explanation));
            }
        }
        self.check_engine_fallback(shared);
    }

    // ------------------------------------------------------------------
    // Board input (mirrors egui `BoardInteraction::handle`)
    // ------------------------------------------------------------------

    fn handle_board(&mut self, bm: BoardMsg, shared: &mut Shared) {
        match bm {
            BoardMsg::SquareClicked(sq) => {
                if !self.board_enabled() || self.board.is_animating() {
                    return;
                }
                self.try_move_to(sq, shared);
            }
            BoardMsg::DragStarted(sq) => {
                if !self.board_enabled() || self.board.is_animating() {
                    return;
                }
                self.click_square(sq, shared);
            }
            BoardMsg::DragMoved(_) => {}
            BoardMsg::DragEnded(p) => {
                if !self.board_enabled() || self.board.is_animating() {
                    return;
                }
                if let Some(sq) =
                    square_from_canvas_point(p, board_size_for_window(self.window), self.flipped)
                {
                    self.drop_on(sq, shared);
                }
            }
            BoardMsg::HoverSquare(sq) => {
                self.hover_sq = sq;
            }
        }
    }

    /// A press on a piece (click, or the start of a drag): select
    /// your own piece, capture with the selected piece, or clear
    /// the selection — exactly like a click in the egui app.
    fn click_square(&mut self, sq: Square, shared: &mut Shared) {
        if self.selected == Some(sq) {
            self.selected = None;
            return;
        }
        if let Some(piece) = self.game.pos.piece_at(sq) {
            if piece.color == self.movable_color() {
                self.selected = Some(sq);
                return;
            }
        }
        self.try_move_to(sq, shared);
    }

    /// Try to move the selected piece to `sq`; an illegal target
    /// clears the selection (egui click behaviour).
    fn try_move_to(&mut self, sq: Square, shared: &mut Shared) {
        if let Some(from) = self.selected {
            let mv = Move::new(from, sq);
            if self.game.pos.legal_moves().contains(&mv) {
                self.play_move(mv, shared);
                return;
            }
        }
        self.selected = None;
    }

    /// A drag ending on `sq`: only a legal move from the selected
    /// (dragged) piece counts; an illegal drop leaves the selection
    /// so the user can click a destination (egui).
    fn drop_on(&mut self, sq: Square, shared: &mut Shared) {
        if let Some(from) = self.selected {
            let mv = Move::new(from, sq);
            if self.game.pos.legal_moves().contains(&mv) {
                self.play_move(mv, shared);
            }
        }
    }

    /// Open the start dialog, restoring the remembered choices
    /// from Settings (egui `vs_ai_start_dialog`).
    fn open_start_dialog(&mut self, shared: &Shared) {
        self.start_difficulty = shared.settings.difficulty;
        self.start_side = match shared.settings.human_color.as_deref() {
            Some("black") => StartSide::Black,
            Some("random") => StartSide::Random,
            Some("two-player") => StartSide::TwoPlayer,
            _ => StartSide::Red,
        };
        self.show_start_dialog = true;
    }

    // ------------------------------------------------------------------
    // View
    // ------------------------------------------------------------------

    fn view_base<'a>(&'a self, shared: &'a Shared) -> Element<'a, Msg> {
        let display_pos = self.display_pos();
        let board_size = board_size_for_window(self.window);
        // Header (64) + divider (1) + content (16 px vertical
        // padding + the board height) add up to exactly the window
        // height — the board never overflows at the 1100 × 720
        // minimum, it shrinks to `board_size` instead.
        // (No eval bar: Hieu 2026-11-13 asked to remove the colour
        // bar at the far left; the advantage chip in the panel still
        // shows the score.)
        column![
            self.header(shared),
            header_divider(),
            container(
                row![
                    container(BoardProgram::canvas(self.board_render()).map(Msg::Board))
                        .width(Length::Fixed(board_size.width))
                        .height(Length::Fixed(board_size.height)),
                    self.panel(shared, &display_pos),
                ]
                .spacing(SPACE_24)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .padding(iced::Padding {
                top: SPACE_16,
                right: SPACE_24,
                bottom: SPACE_16,
                left: SPACE_24,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Header (mockup page 2): 64 px surface bar — ghost "Quay lại"
    /// with arrow icon, the 20 px Semibold title (flex), level chip,
    /// engine-status chip (green dot when Pikafish is ready), ghost
    /// "Cài đặt" with icon.
    fn header<'a>(&'a self, shared: &'a Shared) -> Element<'a, Msg> {
        let level = match self.opponent {
            VsOpponent::Ai { difficulty, .. } => format!(
                "{}: {}",
                i18n::t(Key::Difficulty),
                difficulty_name(difficulty)
            ),
            VsOpponent::TwoPlayer => i18n::t(Key::TwoPlayers).to_string(),
        };
        let (engine_label, engine_dot) = (
            shared.engine_name.clone(),
            if self.engine_ready { Some(GREEN) } else { None },
        );
        container(
            row![
                ghost_button_with_icon(
                    i18n::t(Key::BackToMenu),
                    Icon::ArrowLeft,
                    Msg::Navigate(Screen::Home)
                ),
                text(i18n::t(Key::PlayVsAi))
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold())
                    .width(Length::Fill),
                status_chip(level, None, TEXT_DIM),
                status_chip(engine_label, engine_dot, TEXT_DIM),
                ghost_button_with_icon(
                    i18n::t(Key::Settings),
                    Icon::SlidersHorizontal,
                    Msg::ShowSettings
                ),
            ]
            .spacing(SPACE_16)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: SPACE_24,
            bottom: 0.0,
            left: SPACE_24,
        })
        .width(Length::Fill)
        .height(Length::Fixed(HEADER_HEIGHT))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(theme::SURFACE)),
            ..Default::default()
        })
        .into()
    }

    /// Everything the canvas program needs to draw one frame.
    fn board_render(&self) -> BoardRender {
        let display_pos = self.display_pos();
        let in_review = self.review.is_some();
        let selected = if in_review { None } else { self.selected };
        let targets = selected.map_or_else(Vec::new, |sq| legal_targets(&display_pos, sq));
        let last_move = if in_review {
            self.review.and_then(|ply| {
                self.game
                    .moves
                    .get(ply.wrapping_sub(1))
                    .map(|m| (m.from, m.to))
            })
        } else {
            self.last_move.map(|m| (m.from, m.to))
        };
        let check_square = if display_pos.in_check(display_pos.side()) {
            Some(display_pos.king_sq(display_pos.side()))
        } else {
            None
        };
        let arrows = if in_review {
            Vec::new()
        } else {
            self.hint_arrows
                .iter()
                .enumerate()
                .map(|(j, a)| {
                    let mut a = a.clone();
                    a.highlighted = self.hovered_hint == Some(j);
                    a
                })
                .collect()
        };
        BoardRender {
            position: display_pos,
            selected,
            legal_targets: targets,
            last_move,
            arrows,
            check_square,
            anim: if in_review { None } else { self.board.anim },
            pulse: self.check_pulse(),
            flipped: self.flipped,
            // Only the side to move can be dragged; in review the
            // board is read-only, so nothing lifts.
            draggable: if in_review {
                None
            } else {
                Some(self.movable_color())
            },
        }
    }

    /// Side panel: `Length::Fill`, so it takes all the width the
    /// window leaves after the eval bar and the board — at least
    /// 380 px, because [`board_size_for_window`] reserves that
    /// minimum in the board's horizontal budget (see
    /// [`panel_width_for_window`](crate::widgets::panel_width_for_window)).
    /// Its column is Fill too, so cards and buttons stretch with
    /// the panel instead of staying 380 px wide in a wide window.
    /// Scrollable, so long content never flexes the board — the
    /// egui layout rule.
    fn panel<'a>(&'a self, shared: &'a Shared, display_pos: &Position) -> Element<'a, Msg> {
        let mut col = column![text(self.status_text()).size(FONT_BODY)]
            .spacing(SPACE_16)
            .width(Length::Fill);

        // Opponent + player cards (mockup page 2: 44 px piece disc,
        // Semibold 16 name, 13 px Medium subtitle, status chip;
        // gold border on the side to move).
        let (opp_color, player_color) = match self.opponent {
            VsOpponent::Ai { human, .. } => (human.opposite(), human),
            VsOpponent::TwoPlayer => (self.game.pos.side().opposite(), self.game.pos.side()),
        };
        let (opp_glyph, opp_glyph_color) = match opp_color {
            Color::Red => ("帥", RED_PIECE),
            Color::Black => ("將", BLACK_PIECE),
        };
        let (player_glyph, player_glyph_color) = match player_color {
            Color::Red => ("帥", RED_PIECE),
            Color::Black => ("將", BLACK_PIECE),
        };
        let opp_status = self.opponent_status();
        let opp_subtitle = match self.opponent {
            VsOpponent::Ai { difficulty, .. } => difficulty_name(difficulty).to_string(),
            VsOpponent::TwoPlayer => i18n::t(Key::TwoPlayers).to_string(),
        };
        col = col.push(side_card(
            opp_glyph,
            opp_glyph_color,
            "Pikafish",
            opp_subtitle,
            status_chip(opp_status, None, TEXT_DIM),
            false,
        ));

        // Player card.
        let (player_subtitle, player_status) = match self.opponent {
            VsOpponent::Ai { human, .. } => (
                match human {
                    Color::Red => i18n::t(Key::SideRed).to_string(),
                    Color::Black => i18n::t(Key::SideBlack).to_string(),
                },
                if self.game.result().is_over() {
                    self.game.result().describe_vi().to_string()
                } else if self.game.pos.side() == human {
                    i18n::t(Key::PlayYourTurn).to_string()
                } else if self.ai_thinking {
                    i18n::t(Key::Thinking).to_string()
                } else {
                    i18n::t(Key::PlayWaitingForYou).to_string()
                },
            ),
            VsOpponent::TwoPlayer => (i18n::t(Key::TwoPlayers).to_string(), self.status_text()),
        };
        let player_active =
            self.game.result() == GameResult::Ongoing && self.game.pos.side() == player_color;
        col = col.push(side_card(
            player_glyph,
            player_glyph_color,
            i18n::t(Key::PlayerYou),
            player_subtitle,
            status_chip(
                player_status,
                if player_active { Some(GOLD) } else { None },
                if player_active { GOLD } else { TEXT_DIM },
            ),
            player_active,
        ));

        // Advantage chip (from the human's perspective in AI games,
        // Red's in two-player mode).
        let human_score = perspective_score(score_red(display_pos), self.human_color());
        col = col.push(
            row![
                text(i18n::t(Key::AdvantageScore)).size(FONT_BODY),
                advantage_chip(human_score),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );

        if let Some((grade, expl)) = &self.last_grade {
            col = col.push(
                text(format!(
                    "{}: {} — {expl}",
                    i18n::t(Key::GradeLabel),
                    i18n::grade_label(*grade)
                ))
                .size(FONT_BODY),
            );
        }
        if let Some(msg) = &self.draw_message {
            col = col.push(text(msg).size(FONT_BODY));
        }
        if let Some(banner) = &self.engine_banner {
            col = col.push(
                text(banner)
                    .size(FONT_BODY)
                    .style(|_: &Theme| text::Style { color: Some(RED) }),
            );
        }

        // ChessDB section (its own live score, win rates shown).
        col = col.push(
            self.chessdb
                .view(display_pos, &shared.settings)
                .map(|m| match m {
                    ChessDbPanelMsg::Response(ChessDbResponse::Evaluated { req_id, result }) => {
                        Msg::ChessDbEvaluated { req_id, result }
                    }
                }),
        );

        // Game info + hints used.
        let (side_txt, diff_txt) = match self.opponent {
            VsOpponent::Ai { human, difficulty } => (
                match human {
                    Color::Red => i18n::t(Key::RedSide),
                    Color::Black => i18n::t(Key::BlackSide),
                }
                .to_string(),
                difficulty_name(difficulty).to_string(),
            ),
            VsOpponent::TwoPlayer => (i18n::t(Key::TwoPlayers).to_string(), String::new()),
        };
        col = col.push(
            text(format!("{}: {side_txt} {diff_txt}", i18n::t(Key::GameInfo))).size(FONT_CAPTION),
        );
        col = col.push(
            text(i18n::t(Key::HintsUsed).replace("{n}", &self.hint_count.to_string()))
                .size(FONT_CAPTION),
        );

        // Move list card (mockup page 2: card, flex-1, padding 16;
        // click a move to review that position).
        let mut moves_content = column![self.move_list_view(shared)].spacing(SPACE_8);
        if self.review.is_some() {
            moves_content =
                moves_content.push(secondary_button(i18n::t(Key::BackToEnd), Msg::BackToEnd));
        }
        col = col.push(card(i18n::t(Key::MovesTitle), moves_content));

        // Hint list card (hover highlights its arrow, click plays it).
        if !self.hint_lines.is_empty() && self.review.is_none() {
            col = col.push(card(i18n::t(Key::Hint), self.hint_list_view(shared)));
        }

        // Buttons (mockup page 2: 2-column grid, 40 px, then the
        // full-width gold "Ván mới"). "Xin hòa" has no icon in the
        // mockup; "Lật bàn" is a kept feature with no mockup icon.
        let action = |btn: iced::widget::Button<'a, Msg>| {
            btn.width(Length::Fill).height(Length::Fixed(40.0))
        };
        col = col.push(
            row![
                action(secondary_button_with_icon(
                    i18n::t(Key::Undo),
                    Icon::Undo,
                    Msg::Undo
                )),
                action(secondary_button_with_icon(
                    i18n::t(Key::Hint),
                    Icon::Lightbulb,
                    Msg::Hint
                )),
            ]
            .spacing(SPACE_8),
        );
        if matches!(self.opponent, VsOpponent::Ai { .. }) {
            col = col.push(
                row![
                    action(secondary_button(i18n::t(Key::DrawOffer), Msg::OfferDraw)),
                    action(danger_button_with_icon(
                        i18n::t(Key::Resign),
                        Icon::Flag,
                        Msg::Resign
                    )),
                ]
                .spacing(SPACE_8),
            );
            col = col.push(action(secondary_button(
                i18n::t(Key::FlipBoard),
                Msg::FlipBoard,
            )));
        } else {
            col = col.push(
                row![
                    action(secondary_button(i18n::t(Key::FlipBoard), Msg::FlipBoard,)),
                    action(danger_button_with_icon(
                        i18n::t(Key::Resign),
                        Icon::Flag,
                        Msg::Resign
                    )),
                ]
                .spacing(SPACE_8),
            );
        }
        col = col.push(
            primary_button_with_icon(i18n::t(Key::NewGame), Icon::Plus, Msg::NewGame)
                .width(Length::Fill)
                .height(Length::Fixed(44.0)),
        );

        scrollable(col)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The opponent card's status line.
    fn opponent_status(&self) -> String {
        if self.game.result().is_over() {
            return self.game.result().describe_vi().to_string();
        }
        if self.ai_thinking {
            return self.status_text();
        }
        match self.opponent {
            VsOpponent::Ai { human, .. } => {
                if self.game.pos.side() == human {
                    i18n::t(Key::PlayWaitingForYou).to_string()
                } else {
                    i18n::t(Key::Thinking).to_string()
                }
            }
            VsOpponent::TwoPlayer => self.status_text(),
        }
    }

    /// Two-column (Red / Black) move list; clicking a move reviews
    /// the position after it.
    fn move_list_view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Msg> {
        let moves = move_strings(&self.game, shared.settings.notation);
        let mut col = Column::new().spacing(SPACE_4);
        let mut i = 0;
        while i < moves.len() {
            let no = i / 2 + 1;
            let mut r = row![text(format!("{no}."))
                .size(FONT_BODY)
                .font(theme::inter_font_medium())
                .style(|_: &Theme| iced::widget::text::Style {
                    color: Some(TEXT_DIM),
                })
                .width(Length::Fixed(32.0))]
            .spacing(10);
            for (j, mv) in moves.iter().skip(i).take(2).enumerate() {
                let ply = i + j + 1;
                let selected = self.review == Some(ply);
                // `text` takes the move name by value (an owned
                // `String`): the `moves` vec is local, so a borrow
                // would not live long enough for the returned element.
                let cell = container(
                    text(mv.clone())
                        .size(FONT_BODY)
                        .font(theme::inter_font_medium()),
                )
                .padding(SPACE_4)
                .style(review_cell_style(selected));
                r = r.push(mouse_area(cell).on_press(Msg::Review(ply)));
            }
            col = col.push(r);
            i += 2;
        }
        scrollable(col)
            .height(Length::Fixed(200.0))
            .width(Length::Fill)
            .into()
    }

    /// Hint list: rank, move name, score chip. Hovering a row
    /// highlights its arrow on the board; clicking it plays it.
    fn hint_list_view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Msg> {
        let mut col = Column::new().spacing(SPACE_4);
        for (j, line) in self.hint_lines.iter().enumerate() {
            let name = render_move(&self.game.pos, line.mv, shared.settings.notation);
            let row_el = row![
                text(format!("{}.", j + 1))
                    .size(FONT_BODY)
                    .font(theme::inter_font_medium())
                    .width(Length::Fixed(24.0)),
                text(name)
                    .size(FONT_BODY)
                    .font(theme::inter_font_medium())
                    .width(Length::Fill),
                advantage_chip(line.score),
            ]
            .spacing(SPACE_8)
            .align_y(Alignment::Center);
            col = col.push(
                mouse_area(row_el)
                    .on_press(Msg::PlayHint(j))
                    .on_enter(Msg::HoverHint(Some(j)))
                    .on_exit(Msg::HoverHint(None)),
            );
        }
        col.into()
    }

    /// Start dialog overlay: side + difficulty, then Start / Cancel.
    ///
    /// Centred card in the Learn-select style (UI-FIX-2 part 2):
    /// surface card, radius 14, padding 24, 640 px wide — wide
    /// enough for the two sections ("Bên chơi" and "Độ khó") to sit
    /// side by side instead of stacking in a 340 px column. Every
    /// option button is 48 px tall with 15 px Semibold text (the
    /// shared `primary_button` / `secondary_button` label size).
    /// Behaviour is unchanged: the same four sides, five difficulty
    /// levels, and Start / Cancel messages.
    fn start_dialog_overlay(&self) -> Element<'_, Msg> {
        // One option button: gold when selected, surface-2 otherwise,
        // full width of its column, 48 px tall.
        let option = |label: &'static str, selected: bool, msg: Msg| {
            let btn = if selected {
                primary_button(label, msg)
            } else {
                secondary_button(label, msg)
            };
            btn.width(Length::Fill).height(Length::Fixed(48.0))
        };
        let side_col = column![
            text(i18n::t(Key::Side))
                .size(crate::theme::FONT_CARD_TITLE)
                .font(theme::inter_font_semibold()),
            option(
                i18n::t(Key::SideRed),
                self.start_side == StartSide::Red,
                Msg::StartDialogSide(StartSide::Red),
            ),
            option(
                i18n::t(Key::SideBlack),
                self.start_side == StartSide::Black,
                Msg::StartDialogSide(StartSide::Black),
            ),
            option(
                i18n::t(Key::SideRandom),
                self.start_side == StartSide::Random,
                Msg::StartDialogSide(StartSide::Random),
            ),
            option(
                i18n::t(Key::TwoPlayers),
                self.start_side == StartSide::TwoPlayer,
                Msg::StartDialogSide(StartSide::TwoPlayer),
            ),
        ]
        .spacing(crate::theme::SPACE_8)
        .width(Length::Fill);
        let mut diff_col = column![text(i18n::t(Key::Difficulty))
            .size(crate::theme::FONT_CARD_TITLE)
            .font(theme::inter_font_semibold()),]
        .spacing(crate::theme::SPACE_8)
        .width(Length::Fill);
        for d in 0..=4u8 {
            diff_col = diff_col.push(option(
                difficulty_name(d),
                self.start_difficulty == d,
                Msg::StartDialogDifficulty(d),
            ));
        }
        let card_el = container(
            column![
                text(i18n::t(Key::StartGameTitle))
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold()),
                row![side_col, diff_col].spacing(crate::theme::SPACE_16),
                row![
                    primary_button_with_icon(i18n::t(Key::Start), Icon::Plus, Msg::StartGame)
                        .width(Length::Fill)
                        .height(Length::Fixed(48.0)),
                    secondary_button(i18n::t(Key::Cancel), Msg::CancelStart)
                        .width(Length::Fill)
                        .height(Length::Fixed(48.0)),
                ]
                .spacing(crate::theme::SPACE_12),
            ]
            .spacing(crate::theme::SPACE_16),
        )
        .padding(crate::theme::SPACE_24)
        .width(Length::Fixed(640.0))
        .style(crate::widgets::card_style);
        container(card_el)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(dim_background)
            .into()
    }

    /// Result dialog overlay: result text + New game / Review / Close.
    fn result_dialog_overlay(&self) -> Element<'_, Msg> {
        let card_el = container(
            column![
                text(i18n::t(Key::ResultTitle)).size(FONT_SCREEN_TITLE),
                text(self.game.result().describe_vi()).size(FONT_BODY),
                row![
                    primary_button_with_icon(i18n::t(Key::NewGame), Icon::Plus, Msg::ResultNewGame),
                    secondary_button(i18n::t(Key::ReviewGame), Msg::ResultReview),
                    secondary_button(i18n::t(Key::Close), Msg::ResultClose),
                ]
                .spacing(12),
            ]
            .spacing(12),
        )
        .padding(16)
        .width(Length::Fixed(340.0))
        .style(crate::theme::card_container_style);
        container(card_el)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(dim_background)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn startpos() -> Position {
        Position::from_fen(xq_core::board::START_FEN).expect("start FEN")
    }

    #[test]
    fn difficulty_level_mapping_matches_egui() {
        assert_eq!(difficulty_level(0, 1900), Level::Easy);
        assert_eq!(difficulty_level(1, 1900), Level::Medium);
        assert_eq!(difficulty_level(2, 1900), Level::Hard);
        assert_eq!(difficulty_level(3, 1900), Level::Master);
        assert_eq!(difficulty_level(4, 1900), Level::Elo(1900));
        assert_eq!(difficulty_level(4, 2400), Level::Elo(2400));
        assert_eq!(difficulty_level(9, 1900), Level::Medium);
    }

    #[test]
    fn perspective_score_flips_for_black() {
        assert_eq!(perspective_score(80, Some(Color::Red)), 80);
        assert_eq!(perspective_score(80, Some(Color::Black)), -80);
        assert_eq!(perspective_score(80, None), 80);
        assert_eq!(perspective_score(-80, Some(Color::Black)), 80);
    }

    #[test]
    fn score_red_is_tempo_at_startpos() {
        // `eval` adds the tempo bonus (+10) for the side to move, so
        // the symmetric start position scores +10 from Red's view.
        assert_eq!(score_red(&startpos()), 10);
    }

    #[test]
    fn legal_targets_of_the_opening_cannon() {
        // Red cannon on (file 1, rank 2), sq 19: 6 squares along its
        // rank + 5 up its file + the capture of the black horse on
        // sq 82, jumping the black cannon on sq 64 as its screen
        // (the screen itself, sq 64, is not a target).
        let targets = legal_targets(&startpos(), 19);
        assert_eq!(targets.len(), 12);
        assert!(targets.contains(&22)); // P8-5, the classic opener
        assert!(targets.contains(&10));
        assert!(targets.contains(&55));
        assert!(targets.contains(&82));
        assert!(!targets.contains(&19));
        assert!(!targets.contains(&64));
    }

    #[test]
    fn ai_accepts_draw_only_when_clearly_worse() {
        assert!(ai_accepts_draw(-50));
        assert!(ai_accepts_draw(-500));
        assert!(!ai_accepts_draw(-49));
        assert!(!ai_accepts_draw(0));
    }

    #[test]
    fn drag_end_point_maps_to_squares() {
        // Canvas-local points at the design size (592 × 656):
        // (40, 616) is Red's back-rank corner, sq 0.
        let design = Size::new(592.0, 656.0);
        assert_eq!(
            square_from_canvas_point(Point::new(40.0, 616.0), design, false),
            Some(0)
        );
        assert_eq!(
            square_from_canvas_point(Point::new(552.0, 40.0), design, false),
            Some(89)
        );
        assert_eq!(
            square_from_canvas_point(Point::new(10.0, 10.0), design, false),
            None
        );
        // Flipped: the same screen points map to mirrored squares.
        assert_eq!(
            square_from_canvas_point(Point::new(40.0, 616.0), design, true),
            Some(89)
        );
        assert_eq!(
            square_from_canvas_point(Point::new(552.0, 40.0), design, true),
            Some(0)
        );
        // A shrunk board (the 1100 × 720 minimum window): the same
        // corner squares still map, with the scaled geometry — the
        // old hardcoded 592 × 656 bounds mis-mapped these drops.
        // Round-trip through the drawn centres: the centre of a
        // square must map back to that square at the scaled size
        // (naive `design_point * scale` is off by the board's
        // vertical centring offset `by`, a fraction of a pixel,
        // which is enough to push the top row out of range).
        let small = board_size_for_window(Size::new(1100.0, 720.0));
        let small_geom = Geom::from_bounds_flipped(
            Rectangle {
                x: 0.0,
                y: 0.0,
                width: small.width,
                height: small.height,
            },
            false,
        );
        assert_eq!(
            square_from_canvas_point(small_geom.square_point(0), small, false),
            Some(0)
        );
        assert_eq!(
            square_from_canvas_point(small_geom.square_point(89), small, false),
            Some(89)
        );
    }

    #[test]
    fn window_resize_is_remembered_for_the_board_size() {
        let shared = Shared::new();
        let mut state = State::new(&shared);
        assert_eq!(state.window, Size::new(1280.0, 800.0));
        let _ = state.update(
            Msg::WindowResized(Size::new(1100.0, 720.0)),
            &mut Shared::new(),
        );
        assert_eq!(state.window, Size::new(1100.0, 720.0));
        assert!(
            board_size_for_window(state.window).height < 656.0,
            "the board shrinks at the minimum window"
        );
    }

    #[test]
    fn new_screen_waits_for_the_start_dialog() {
        let shared = Shared::new();
        let state = State::new(&shared);
        assert!(!state.started);
        assert!(state.show_start_dialog);
        assert!(!state.engine_ready);
        assert!(!state.ai_thinking);
    }

    #[test]
    fn forwarded_install_event_marks_engine_ready() {
        // `App` owns the install now and forwards its outcome here
        // (`Msg::EngineInstalled` / `Msg::EngineUnavailable`).
        let mut shared = Shared::new();
        let mut state = State::new(&shared);
        assert!(!state.engine_ready);
        let _ = state.update(
            Msg::EngineInstalled {
                name: "Pikafish 2026-09-06".to_string(),
                dir: PathBuf::from("/tmp"),
            },
            &mut shared,
        );
        assert!(state.engine_ready);
        assert!(state.engine_banner.is_none());
        assert_eq!(shared.engine_name, "Pikafish 2026-09-06");
        let _ = state.update(
            Msg::EngineUnavailable("Không tìm thấy Pikafish".to_string()),
            &mut shared,
        );
        assert!(!state.engine_ready);
        assert!(state.engine_banner.is_some());
    }

    #[test]
    fn playing_a_move_starts_the_slide_and_blocks_work() {
        let mut shared = Shared::new();
        shared.settings.animate = true;
        let mut state = State::new(&shared);
        state.start(StartSide::Red, 1, &mut shared);
        assert!(state.started);
        // Red cannon (file 1, rank 2) to the centre: P8-5.
        let mv = Move::new(19, 22);
        state.play(mv, &shared);
        assert_eq!(state.game.moves.len(), 1);
        assert_eq!(state.last_move, Some(mv));
        assert!(state.selected.is_none());
        // The piece is still sliding: no computation may start.
        assert!(state.board.is_animating());
        // ...until the slide finishes, 150 ms later.
        let later = Instant::now() + Duration::from_millis(200);
        assert!(state.board.finish_anim(later));
        assert!(!state.board.is_animating());
    }

    #[test]
    fn undo_returns_to_the_human_turn() {
        let mut shared = Shared::new();
        shared.settings.animate = false;
        let mut state = State::new(&shared);
        state.start(StartSide::Red, 1, &mut shared);
        // Two plies by hand (Red, then Black), as if the AI replied.
        state.play(Move::new(19, 22), &shared);
        state.play(Move::new(82, 65), &shared);
        assert_eq!(state.game.moves.len(), 2);
        state.undo();
        // Back to the human's turn: the human's own last move is
        // taken back too, so no moves remain and Red is to move.
        assert_eq!(state.game.moves.len(), 0);
        assert_eq!(state.game.pos.side(), Color::Red);
        assert!(state.last_move.is_none());
    }
}
