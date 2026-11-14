//! Learn-openings screen: play a lesson against the book Tutor
//! (DESIGN-iced.md §2.4–2.5, `xq-app/src/screens/learn.rs`).
//!
//! Pikafish IS the book (Hieu, 2026-11-11 — see
//! `docs/DECISIONS-learn.md` §6): hints, scores and grading all come
//! from the stored dataset (`BookNode::score_cp`), never from a live
//! engine. The only engine use on this screen is the out-of-book
//! opponent fallback (§5.3.4): after the learner leaves the book and
//! chooses "Tiếp tục", the opponent is played by the shared engine
//! (Medium) through this screen's own [`AiWorker`].
//!
//! # INVARIANT — the piece must land before any computation starts
//!
//! A move's slide animation (150 ms) must finish BEFORE the grading,
//! hint refresh or ChessDB work for the new position starts (Hieu,
//! restated 2026-11-12, three times in one day). In Elm terms:
//! `update()` only triggers that work when
//! `board.is_animating() == false` (see [`BoardState`]); the tick
//! subscription only runs while an animation, a pending opponent
//! move, or the flashing check halo needs it. [`BoardState::finish_anim`]
//! is what wakes the waiting work up: the `Tick` handler finishes
//! the animation and the end-of-`update` settle step then refreshes
//! hints and asks ChessDB for the new position.
//!
//! # Lesson handover (integration)
//!
//! This screen cannot reach the other screens: the lesson the user
//! picked lives in [`LearnSelect`](super::learn_select) until the
//! coordinator wires it across. The contract is:
//!
//! * [`LessonChoice`] (defined here) — the picked lesson id + side.
//! * [`super::learn_select::State::take_choice`] — the picker hands
//!   the choice over (once).
//! * [`State::start`] — starts the picked lesson on this screen.
//! * [`State::take_pending_summary`] + [`super::summary::State::load`]
//!   — "Dừng và tóm tắt" builds the [`SummaryData`](super::summary::SummaryData)
//!   here; the coordinator hands it to the Summary screen.
//!
//! Until the coordinator adds those two lines in `App::update`, this
//! screen starts with no lesson and shows "Không tìm thấy bài học."

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::widget::{column, container, mouse_area, row, scrollable, text, Space};
use iced::{Color, Element, Length, Point, Rectangle, Size, Subscription, Task, Theme};
use xq_book::schema::{BookNode, Lesson};
use xq_book::tree::node_at;
use xq_book::{direction_count, BookHint, LessonSummary, Tutor, TutorEvent};
use xq_core::analysis::{explain_tags, tags_for, Grade};
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::Game;
use xq_core::notation;
use xq_core::types::{Color as SideColor, Move};
use xq_engine::{EngineRequest, Level, StopHandle};

use crate::ai_worker::{response_is_current, AiRequest, AiResponse, AiWorker};
use crate::app::{Screen, Shared};
use crate::board::{Arrow, BoardMsg, BoardProgram, BoardRender, BoardState, Geom, Square};
use crate::chessdb_panel::{ChessDbPanel, ChessDbPanelMsg};
use crate::chessdb_worker::ChessDbResponse;
use crate::helpers::{move_strings, render_move};
use crate::i18n::{self, difficulty_name, Key};
use crate::icons::Icon;
use crate::screens::summary::{SummaryData, SummaryRow};
use crate::sound::sound_for_move;
use crate::theme::{
    self, BLACK_PIECE, FONT_BODY, FONT_CAPTION, FONT_SCREEN_TITLE, GOLD, RED_PIECE, SPACE_16,
    SPACE_24, SPACE_4, SPACE_8, TEXT_DIM,
};
use crate::widgets::{
    advantage_chip, board_size_for_window, card_style, disabled_button_with_icon,
    ghost_button_with_icon, header_divider, primary_button, primary_button_with_icon,
    secondary_button, secondary_button_with_icon, section_title, side_card, status_chip,
    HEADER_HEIGHT,
};

/// How long the opponent waits before playing its book move
/// (600 ms, as in the egui app).
const OPPONENT_DELAY: Duration = Duration::from_millis(600);
/// Retry delay when the shared engine is busy (§14: never block the
/// UI thread on the engine lock — defer to the next tick instead).
const ENGINE_BUSY_RETRY: Duration = Duration::from_millis(100);
/// How often the animation / opponent timer ticks.
/// Animation / check-halo tick: 16 ms (~60 fps) so a 150 ms slide
/// redraws about 9 times instead of 3 — the 50 ms tick made the
/// slide visibly choppy. Same interval as Practice.
const TICK_INTERVAL: Duration = Duration::from_millis(16);

/// The lesson the learner picked in [`super::learn_select`], handed
/// to [`State::start`] by the integration step (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonChoice {
    /// [`Lesson::id`] of the picked lesson.
    pub lesson_id: String,
    /// Whether the learner plays Red (first player).
    pub learner_is_red: bool,
}

/// Map a [`ChessDbPanelMsg`] (non-`Clone`, so it cannot live in
/// [`Msg`]) to the `Clone` variant that carries the same payload.
/// Used by both the ChessDB subscription and the panel's view.
fn map_chessdb_msg(msg: ChessDbPanelMsg) -> Msg {
    match msg {
        ChessDbPanelMsg::Response(ChessDbResponse::Evaluated { req_id, result }) => {
            Msg::ChessDbEvaluated { req_id, result }
        }
    }
}

/// Map an [`AiResponse`] (non-`Clone`) to [`Msg`]: this screen only
/// ever waits for an out-of-book opponent `BestMove`; every other
/// response is dropped.
fn map_ai_response(resp: AiResponse) -> Msg {
    match resp {
        AiResponse::BestMove { req_id, mv, .. } => Msg::OpponentAiMove { req_id, mv },
        _ => Msg::Ignored,
    }
}

/// The opponent's prepared variations at the current node, best for
/// the opponent first (§8): (index into the node's children, the
/// parsed move, its stored score from the opponent's view, the CN
/// notation, the note). Unparseable children are skipped. The board
/// arrows and the choice panel share this order, so the numbers on
/// the arrows match the numbers in the panel.
fn opponent_choices_for(
    children: &[BookNode],
    pos: &Position,
) -> Vec<(usize, Move, i32, String, String)> {
    let mut out: Vec<(usize, Move, i32, String, String)> = Vec::new();
    for (i, node) in children.iter().enumerate() {
        if let Ok(mv) = notation::parse_cn(pos, &node.mv) {
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

/// Plies left on the main branch from `level` (follow `main`
/// children, else the first child).
fn count_main_branch(mut level: &[BookNode]) -> usize {
    let mut count = 0;
    while let Some(node) = level.iter().find(|n| n.main).or_else(|| level.first()) {
        count += 1;
        level = node.next.as_slice();
    }
    count
}

/// Game plies (1-based) where the mover is the learner, for the
/// summary table.
fn learner_plies(moves_len: usize, start_side_is_learner: bool) -> Vec<usize> {
    (0..moves_len)
        .filter(|i| (i % 2 == 0) == start_side_is_learner)
        .map(|i| i + 1)
        .collect()
}

/// Colour of a grade label (same rule as the egui app).
#[must_use]
pub fn grade_color(grade: Grade) -> Color {
    match grade {
        Grade::Book | Grade::Best => theme::GREEN,
        Grade::Good | Grade::Acceptable => theme::TEXT_DIM,
        Grade::Inaccurate => Color::from_rgb8(0xEF, 0x6C, 0x00),
        Grade::Blunder => theme::RED,
    }
}

/// A 2-column-grid action button (mockup pages 2/4/5): full cell
/// width, 40 px tall.
fn grid_button<'a>(btn: iced::widget::Button<'a, Msg>) -> iced::widget::Button<'a, Msg> {
    btn.width(Length::Fill).height(Length::Fixed(40.0))
}

/// Gold progress bar (mockup page 4: a 6 px track with a gold fill
/// showing how far through the lesson the learner is).
fn progress_bar(frac: f32) -> Element<'static, Msg> {
    let pct = (frac.clamp(0.0, 1.0) * 100.0).round() as u16;
    let gold = container(text(""))
        .height(Length::Fixed(6.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::GOLD)),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    let inner: Element<'static, Msg> = if pct == 0 {
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fixed(6.0))
            .into()
    } else if pct >= 100 {
        gold.width(Length::Fill).into()
    } else {
        row![
            gold.width(Length::FillPortion(pct)),
            container(text("")).width(Length::FillPortion(100 - pct)),
        ]
        .spacing(0)
        .into()
    };
    container(inner)
        .width(Length::Fill)
        .height(Length::Fixed(6.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::BORDER)),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Container style for a hint / opponent-choice row: pale gold when
/// hovered, transparent otherwise (the egui app's row highlight).
fn row_container_style(hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_: &Theme| container::Style {
        background: if hovered {
            Some(iced::Background::Color(Color {
                a: 0.11,
                ..theme::GOLD
            }))
        } else {
            None
        },
        border: iced::Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// Animation / opponent-timer tick.
    Tick(Instant),
    /// The window was resized: the board's display size follows it.
    WindowResized(Size),
    /// A board interaction (click / drag / drop).
    Board(BoardMsg),
    /// The cursor entered / left a hint row (`None` = left).
    HoverHint(Option<usize>),
    /// Play a hinted move (clicked hint row).
    PlayHint(Move),
    /// Play the opponent variation the learner picked.
    OpponentChoice(usize),
    /// Undo back to before the learner's last move.
    Undo,
    /// Show / hide the hint list.
    ToggleHints,
    /// Restart the current lesson from the start ("Đi lại").
    Restart,
    /// Retry the off-book move (back to the book position).
    Retry,
    /// Continue off-book: the opponent answers with the engine.
    ContinueOffBook,
    /// Stop the lesson and show its summary.
    StopAndSummary,
    /// Close the lesson-complete dialog (stay on the board).
    HideComplete,
    /// Continue the finished lesson as a Vs-AI game (intercepted by
    /// `App::update` — see [`State::take_continue_game`]).
    ContinueWithAi,
    /// Flip the board (Red/Black at the bottom).
    FlipBoard,
    /// A ChessDB evaluation arrived (mapped from `ChessDbPanelMsg`).
    ChessDbEvaluated {
        req_id: u64,
        result: Result<xq_chessdb::ChessDbEval, xq_chessdb::ChessDbError>,
    },
    /// The engine's out-of-book opponent move arrived.
    OpponentAiMove { req_id: u64, mv: Move },
    /// A worker response this screen does not need.
    Ignored,
}

/// Learn screen state. Mirrors the egui `LearnState`
/// (`xq-app/src/screens/learn.rs`) minus the egui-only fields
/// (`BoardInteraction` / `MoveAnim` / `last_board` live in
/// [`BoardState`] + [`BoardProgram`] now; `flipped` lives in
/// [`BoardRender`] + this `State`, restored at integration).
pub struct State {
    /// The lesson being studied (clone, for display).
    lesson: Option<Lesson>,
    // Declared before `lesson_box`: Tutor is dropped first, so the
    // borrowed lesson always outlives the Tutor that references it
    // (same pattern as the egui app).
    tutor: Option<Tutor<'static>>,
    lesson_box: Option<Box<Lesson>>,
    game: Game,
    node_path: Vec<usize>,
    grades: Vec<(Move, Grade)>,
    /// Best move recorded per graded learner move, with the position
    /// before it (parallel to `grades`; `None` when unknown). Used
    /// for the "nước đúng" column of the Summary screen.
    graded_best: Vec<Option<(Move, Position)>>,
    book_moves: usize,
    out_of_book: bool,
    plies_after_book: usize,
    hints: Vec<BookHint>,
    /// Position the hints were generated for (set in
    /// `refresh_hints`). Move names are rendered from this position,
    /// not the current one: after the learner moves, the hints list
    /// still holds the previous position's moves until the next
    /// refresh, and rendering them against the new position panics
    /// (the source square is empty).
    hint_pos: Option<Position>,
    hint_hover: Option<usize>,
    hide_hints: bool,
    hint_ply: Option<usize>,
    /// Next moves preview (Hieu, 2026-11-12): the opponent's main
    /// reply + the learner's book moves after it, shown immediately
    /// after the learner moves — before the opponent's board move
    /// and before any live score.
    next_preview: Option<(BookHint, Vec<BookHint>)>,
    /// The opponent's variations are on screen for the learner to
    /// pick (§8): set when the opponent's turn comes and it has 2+
    /// children.
    show_opponent_choice: bool,
    opponent_ai_req: Option<u64>,
    last_grade: Option<(Grade, String, Option<Move>)>,
    /// Position before the graded move (renders its best move).
    grade_pos: Option<Position>,
    off_book_pending: bool,
    opponent_due: Option<Instant>,
    tutor_message: String,
    complete: Option<LessonSummary>,
    show_complete_dialog: bool,
    /// Summary data built by "Dừng và tóm tắt", waiting for the
    /// integration step to hand it to the Summary screen (see
    /// [`State::take_pending_summary`]).
    pending_summary: Option<SummaryData>,
    board: BoardState,
    selected: Option<Square>,
    last_move: Option<Move>,
    /// Whether the board is flipped (Black at the bottom). Auto-set
    /// when the learner plays Black; the Flip button toggles it
    /// (egui `LearnState::flipped`).
    flipped: bool,
    started: bool,
    /// ChessDB panel (`show_win_rate = false`: only the score chip +
    /// best move in Learn — Hieu, 2026-11-12).
    chessdb: ChessDbPanel,
    ai_tx: Sender<AiRequest>,
    ai_rx: Arc<Mutex<Receiver<AiResponse>>>,
    ai_stop: StopHandle,
    next_req_id: u64,
    rng_state: u64,
    /// When the screen was created (drives the check-halo pulse).
    boot: Instant,
    /// Current window size (tracked via `Msg::WindowResized`, seeded
    /// with the default 1280 × 800): the board's display size comes
    /// from it ([`board_size_for_window`](crate::widgets::board_size_for_window)).
    window: Size,
}

impl State {
    /// Create the learn screen (no lesson yet — see [`State::start`]).
    #[must_use]
    pub fn new(shared: &Shared) -> Self {
        // Destructure the worker to take its receiver out for the
        // bridge (see `bridge.rs`): the forwarder thread is the
        // receiver's only consumer while bridged.
        let AiWorker { tx, rx, stop, .. } = AiWorker::new(shared.engine.clone());
        let now = Instant::now();
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
            hint_pos: None,
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
            pending_summary: None,
            board: BoardState::default(),
            selected: None,
            last_move: None,
            flipped: false,
            started: false,
            chessdb: ChessDbPanel::new(false, "chessdb-learn"),
            ai_tx: tx,
            ai_rx: Arc::new(Mutex::new(rx)),
            ai_stop: stop,
            next_req_id: 1,
            rng_state: 0x9E37_79B9_7F4A_7C15,
            boot: now,
            window: Size::new(1280.0, 800.0),
        }
    }

    /// Start the picked lesson (integration entry point — see the
    /// module docs). The lesson is looked up in the shared embedded
    /// book; an unknown id leaves the screen without a lesson.
    /// The book's learner is authoritative: the choice's side only
    /// records what the picker showed.
    pub fn start(&mut self, lesson_id: &str, learner_is_red: bool, shared: &mut Shared) {
        let _ = learner_is_red;
        let Some(lesson) = shared.book.by_id(lesson_id).cloned() else {
            return;
        };
        self.start_lesson(lesson);
    }

    /// Take the summary built by "Dừng và tóm tắt" (if any) so the
    /// integration step can hand it to the Summary screen.
    pub fn take_pending_summary(&mut self) -> Option<SummaryData> {
        self.pending_summary.take()
    }

    /// Take the finished lesson's game for "Chơi tiếp với máy"
    /// (integration entry point): the game moves to the Play screen
    /// as a Vs-AI game (`Play::State::start_from_learn`). `Game` is
    /// not `Clone`, so the game is moved out and replaced with a
    /// fresh startpos game; the Learn screen is re-started via
    /// [`State::start`] before it is shown again.
    pub fn take_continue_game(&mut self) -> Option<(Game, SideColor)> {
        if self.game.moves.is_empty() && self.complete.is_none() {
            return None;
        }
        let human = self.learner_color().unwrap_or(SideColor::Red);
        let game = std::mem::replace(&mut self.game, Game::new(Position::startpos()));
        Some((game, human))
    }

    fn learner_color(&self) -> Option<SideColor> {
        self.lesson.as_ref().map(|l| l.learner.color())
    }

    fn next_rand(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    /// Copy tree position / grades / book state from the Tutor, which
    /// is the source of truth after every `Tutor` call.
    fn sync_from_tutor(&mut self) {
        if let Some(t) = &self.tutor {
            self.node_path = t.node_path.clone();
            self.book_moves = t.book_moves;
            self.grades = t.grades.clone();
            self.out_of_book = t.out_of_book;
            self.plies_after_book = t.plies_after_book;
        }
    }

    /// Children of the current tree node.
    fn children(&self) -> &[BookNode] {
        let Some(lesson) = &self.lesson else {
            return &[];
        };
        match node_at(&lesson.tree, &self.node_path) {
            Some(node) => node.next.as_slice(),
            None => lesson.tree.as_slice(),
        }
    }

    /// The opponent's prepared variations at the current node, best
    /// for the opponent first (see [`opponent_choices_for`]).
    fn opponent_choices(&self) -> Vec<(usize, Move, i32, String, String)> {
        opponent_choices_for(self.children(), &self.game.pos)
    }

    fn current_node(&self) -> Option<&BookNode> {
        let lesson = self.lesson.as_ref()?;
        node_at(&lesson.tree, &self.node_path)
    }

    /// Plies left on the main branch from the current node.
    fn main_branch_remaining(&self) -> usize {
        count_main_branch(self.children())
    }

    /// Build a `Tutor` over the boxed lesson.
    /// SAFETY: a `Box`'s pointee never moves; `tutor` is declared
    /// before `lesson_box` (dropped first) and is always cleared
    /// before `lesson_box` is replaced (same pattern as the egui app).
    fn make_tutor(lesson: &Lesson) -> Tutor<'static> {
        let lesson: &'static Lesson = unsafe { &*(lesson as *const Lesson) };
        Tutor::new(lesson, 0x1234_5678)
    }

    fn start_lesson(&mut self, lesson: Lesson) {
        self.cancel_opponent_ai();
        self.tutor = None;
        self.lesson_box = Some(Box::new(lesson.clone()));
        self.tutor = Some(Self::make_tutor(
            self.lesson_box.as_ref().expect("lesson just boxed"),
        ));
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
        self.hint_pos = None;
        self.hint_hover = None;
        self.hide_hints = false;
        self.hint_ply = None;
        self.next_preview = None;
        self.show_opponent_choice = false;
        self.opponent_ai_req = None;
        self.last_grade = None;
        self.grade_pos = None;
        self.off_book_pending = false;
        self.complete = None;
        self.show_complete_dialog = false;
        self.pending_summary = None;
        self.board = BoardState::default();
        self.selected = None;
        self.last_move = None;
        // Auto-flip when the learner plays Black (egui).
        self.flipped = self.learner_color() == Some(SideColor::Black);
        self.started = true;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        // Learner is Black: the opponent (Red) opens after 600 ms.
        self.opponent_due = if self.learner_color() == Some(SideColor::Black) {
            Some(Instant::now() + OPPONENT_DELAY)
        } else {
            None
        };
        // Hints + ChessDB for the starting position, right away: the
        // learner (Red) must see the book directions without waiting
        // for a tick that may never come (no animation, no due move).
        self.refresh_hints();
        self.request_chessdb();
    }

    fn is_learner_turn(&self) -> bool {
        self.learner_color() == Some(self.game.pos.side())
    }

    /// Whether board input is accepted right now (egui `enabled` +
    /// the animation gate: no input while a piece is still sliding).
    fn can_input(&self) -> bool {
        self.started
            && self.lesson.is_some()
            && self.complete.is_none()
            && !self.off_book_pending
            && self.opponent_due.is_none()
            && self.opponent_ai_req.is_none()
            && !self.show_opponent_choice
            && !self.board.is_animating()
            && self.is_learner_turn()
    }

    /// Book hints for the learner's turn. The Tutor's
    /// `hints_for_learner_turn` is canonical: book moves with their
    /// stored Pikafish scores, ordered best-first, keeping 3
    /// directions (or 4 when equally good). No live engine is
    /// consulted in lessons — the book IS Pikafish.
    fn refresh_hints(&mut self) {
        if !self.is_learner_turn() || self.complete.is_some() {
            return;
        }
        if self.hint_ply == Some(self.game.moves.len()) {
            return;
        }
        self.hint_ply = Some(self.game.moves.len());
        // Remember the position these hints belong to, so the display
        // renders their move names against it (not the position after
        // the learner's next move, where the source square is empty).
        self.hint_pos = Some(self.game.pos.clone());
        if let Some(tutor) = &mut self.tutor {
            if let TutorEvent::YourTurn { book_moves } = tutor.hints_for_learner_turn() {
                self.hints = book_moves;
                return;
            }
        }
        // Fallback (no Tutor attached): book children with their
        // stored scores — same rule as the Tutor, no live engine.
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
        hints.truncate(direction_count(&scores));
        self.hints = hints;
    }

    /// Ask ChessDB for the current position — only when a lesson is
    /// running and the board is NOT animating (see the invariant).
    /// The panel dedups by FEN, so calling this liberally is cheap.
    fn request_chessdb(&mut self) {
        if self.lesson.is_none() || self.board.is_animating() {
            return;
        }
        let fen = self.game.pos.to_fen();
        self.chessdb.request(&fen);
    }

    /// Post-move settle step (called at the end of `update` and after
    /// an animation finishes): refresh hints and ask ChessDB, but
    /// only once the piece has landed.
    fn settle(&mut self, _shared: &Shared) {
        if self.board.is_animating() {
            return;
        }
        self.refresh_hints();
        self.request_chessdb();
    }

    fn play_move_common(&mut self, mv: Move, shared: &Shared) {
        let pos_before = self.game.pos.clone();
        if self.game.play(mv).is_err() {
            return;
        }
        self.last_move = Some(mv);
        if shared.settings.animate {
            self.board.start_anim(mv.from, mv.to);
        } else {
            self.board.anim = None;
        }
        self.selected = None;
        // Sound for the move just played: move / capture / check /
        // checkmate. Non-blocking and silent when sound is off or no
        // audio device is present.
        sound_for_move(&shared.sound, &pos_before, mv, &self.game.pos);
    }

    /// The learner played `mv`. The Tutor grades it (its event is the
    /// source of truth, including the bad-branch `Acceptable` rule);
    /// the screen plays the same move in its own game and syncs tree
    /// state from the Tutor. Off-book moves open the Retry /
    /// Continue dialog.
    fn play_user_move(&mut self, mv: Move, shared: &Shared) {
        let pos_before = self.game.pos.clone();
        let in_book = self.children().iter().any(|n| {
            notation::parse_cn(&pos_before, &n.mv)
                .map(|m| m == mv)
                .unwrap_or(false)
        });
        let tutor_event = self.tutor.as_mut().map(|t| t.on_user_move(mv));
        // Play the same move in our own game (Tutor played it in its game).
        self.play_move_common(mv, shared);
        self.sync_from_tutor();
        self.hint_ply = None;
        match tutor_event {
            Some(TutorEvent::OpeningComplete { summary }) => {
                let (grade, expl) = summary
                    .grades
                    .last()
                    .map(|(_, g)| (*g, i18n::grade_label(*g).to_string()))
                    .unwrap_or((Grade::Book, i18n::grade_label(Grade::Book).to_string()));
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
                    self.opponent_due = Some(Instant::now() + OPPONENT_DELAY);
                    // Show the next book moves at once (Hieu,
                    // 2026-11-12): the opponent's main reply + the
                    // learner's moves after it, from the book —
                    // before the opponent's board move and before any
                    // live score arrives.
                    self.next_preview = self.tutor.as_ref().and_then(|t| t.peek_next());
                } else {
                    self.off_book_pending = true;
                    self.next_preview = None;
                }
                self.maybe_complete();
            }
            _ => {
                // No Tutor attached: grade from the book alone (same
                // rule as the Tutor — Pikafish IS the book, no live
                // engine in lessons). In-book: the node's note;
                // off-book: Acceptable ("not in the book") with the
                // book's main child as the correct move.
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
                self.next_preview = None;
                if in_book {
                    self.opponent_due = Some(Instant::now() + OPPONENT_DELAY);
                } else {
                    self.off_book_pending = true;
                }
                self.maybe_complete();
            }
        }
    }

    /// Cancel a pending out-of-book opponent AI calculation (Hieu,
    /// 2026-11-12): the learner moved on (retry / undo), so the
    /// engine's answer for the old position is useless. A stale
    /// response that still arrives is dropped by `req_id`.
    fn cancel_opponent_ai(&mut self) {
        if self.opponent_ai_req.is_some() {
            self.ai_stop.stop();
            let _ = self.ai_tx.send(AiRequest::Stop);
            self.opponent_ai_req = None;
        }
    }

    /// Undo (Hieu, 2026-11-12): go back to before the learner's last
    /// move so they can try a different one. Cancels a pending
    /// opponent move / AI calculation first. Undoes the opponent's
    /// reply too when it has already been played. Returns `false`
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.cancel_opponent_ai();
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
        self.board.anim = None;
        self.last_grade = None;
        self.grade_pos = None;
        self.hint_ply = None;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        self.selected = None;
        true
    }

    fn retry(&mut self) {
        self.cancel_opponent_ai();
        if let Some(tutor) = &mut self.tutor {
            tutor.retry();
        }
        self.game.undo();
        self.sync_from_tutor();
        self.graded_best.truncate(self.grades.len());
        self.last_move = self.game.moves.last().copied();
        self.board.anim = None;
        self.last_grade = None;
        self.grade_pos = None;
        self.next_preview = None;
        self.off_book_pending = false;
        self.show_opponent_choice = false;
        self.tutor_message = i18n::t(Key::YourTurnLearn).to_string();
        self.hint_ply = None;
    }

    fn continue_off_book(&mut self) {
        self.out_of_book = true;
        self.plies_after_book = 0;
        self.off_book_pending = false;
        self.opponent_due = Some(Instant::now() + OPPONENT_DELAY);
    }

    /// Play the opponent's book move, or fall back to the engine when
    /// out of book / no children (§8): when the book node has 2+
    /// children the learner picks the variation instead — the choice
    /// panel is shown and this returns without playing. With exactly
    /// 1 child the move plays automatically after the delay.
    fn play_opponent(&mut self, shared: &Shared) {
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
                self.play_move_common(mv, shared);
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
                    self.play_move_common(mv, shared);
                    self.after_opponent_move();
                    return;
                }
            }
        }
        // Out of book: the engine plays (Medium, §5.3.4). If the
        // engine is busy, defer instead of blocking (§14).
        if !self.request_opponent_ai(shared) {
            self.opponent_due = Some(Instant::now() + ENGINE_BUSY_RETRY);
        }
    }

    /// Play the opponent variation the learner picked in the choice
    /// panel (§8). `idx` is the index into the current node's
    /// children (kept in tree order, so it matches the Tutor's
    /// `node_path`). A stale or out-of-range pick falls back to the
    /// automatic play.
    fn play_opponent_choice(&mut self, idx: usize, shared: &Shared) {
        self.show_opponent_choice = false;
        if self.game.result().is_over() {
            self.maybe_complete();
            return;
        }
        if let Some(tutor) = &mut self.tutor {
            if let Some(TutorEvent::OpponentMoves { mv, note_vi }) = tutor.play_opponent_move(idx) {
                self.tutor_message = note_vi.unwrap_or_default();
                self.play_move_common(mv, shared);
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
                    self.play_move_common(mv, shared);
                    self.after_opponent_move();
                    return;
                }
            }
        }
        self.play_opponent(shared);
    }

    fn after_opponent_move(&mut self) {
        // The preview is consumed: the opponent has moved, the
        // learner's turn is back and `refresh_hints` below shows the
        // real hints.
        self.next_preview = None;
        if self.out_of_book {
            self.plies_after_book += 1;
        }
        if self.maybe_complete() {
            return;
        }
        self.refresh_hints();
    }

    /// Ask the shared engine (Medium) for the out-of-book opponent
    /// move. Returns `false` when the engine is busy — the caller
    /// defers the request instead of blocking the UI thread (§14).
    /// The request carries FEN + moves (not a ready `EngineRequest`)
    /// only for grading; for a plain best move the request is built
    /// here from `try_lock`, never from a blocking lock.
    fn request_opponent_ai(&mut self, shared: &Shared) -> bool {
        let Some(guard) = shared.engine.try_lock() else {
            return false;
        };
        let limits = guard.limits_for(Level::Medium);
        drop(guard);
        self.next_req_id += 1;
        let req_id = self.next_req_id;
        self.opponent_ai_req = Some(req_id);
        let _ = self.ai_tx.send(AiRequest::BestMove {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            req_id,
        });
        true
    }

    /// Check the §5.3.5 completion rules; on completion store the summary.
    fn maybe_complete(&mut self) -> bool {
        if self.complete.is_some() {
            return true;
        }
        let Some(lesson) = &self.lesson else {
            return false;
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

    /// Build the Summary screen's data from the finished lesson —
    /// or from the lesson so far, when the learner stops early
    /// ("Dừng và tóm tắt"). Also records the lesson progress in
    /// settings (mirrors the egui `learn_finish_to_summary`).
    pub fn build_summary(&mut self, shared: &mut Shared) -> Option<SummaryData> {
        let l = self.lesson.as_ref()?;
        let (lesson_id, lesson_name, learner) =
            (l.id.clone(), l.name_vi.clone(), l.learner.color());
        let was_complete = self.complete.is_some();
        let summary = match self.complete.take() {
            Some(s) => s,
            None => {
                let t = self.tutor.as_ref()?;
                t.summary()
            }
        };
        // Reconstruct positions + rendered moves for the summary table.
        let mut positions = vec![self.game.start.clone()];
        let mut p = self.game.start.clone();
        let mut rendered: Vec<String> = Vec::new();
        for m in &self.game.moves {
            rendered.push(render_move(&p, *m, shared.settings.notation));
            p.make_move(*m);
            positions.push(p.clone());
        }
        // Learner plies (game ply indices where the mover is the learner).
        let start_side_is_learner = self.game.start.side() == learner;
        let learner_plies = learner_plies(self.game.moves.len(), start_side_is_learner);
        let rows: Vec<SummaryRow> = self
            .grades
            .iter()
            .enumerate()
            .map(|(i, (_, grade))| {
                let game_ply = learner_plies.get(i).copied().unwrap_or(i + 1);
                let mv_str = rendered.get(game_ply - 1).cloned().unwrap_or_default();
                // "Nước đúng": for a book move it is the move itself;
                // for an off-book move it is the best move recorded
                // at grading.
                let best = if *grade == Grade::Book {
                    Some(mv_str.clone())
                } else {
                    self.graded_best
                        .get(i)
                        .and_then(|b| b.as_ref())
                        .map(|(bm, pos)| render_move(pos, *bm, shared.settings.notation))
                };
                SummaryRow {
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
        shared
            .settings
            .record_lesson(&lesson_id, was_complete, ratio);
        shared.settings.save();
        self.show_complete_dialog = false;
        Some(SummaryData {
            lesson_id,
            lesson_name,
            rows,
            book_moves: summary.book_moves,
            total: summary.total_learner_moves,
            blunders,
            inaccuracies: inacc,
            end_note: summary.end_note_vi,
            final_score: summary.final_score,
            positions,
            learner_is_red: learner == SideColor::Red,
        })
    }

    /// Check-halo pulse 0..1 (~2 Hz, from the screen's clock).
    fn pulse(&self) -> f32 {
        let t = self.boot.elapsed().as_secs_f32();
        0.5 + 0.5 * (t * 4.0 * std::f32::consts::PI).sin()
    }

    /// Whether the tick subscription must run: an animation is
    /// sliding, the opponent's move is due, or the check halo is
    /// flashing.
    fn needs_tick(&self) -> bool {
        self.board.is_animating()
            || self.opponent_due.is_some()
            || (self.started && self.game.pos.in_check(self.game.pos.side()))
    }

    /// Drop point (canvas-local) → board square. The canvas is
    /// [`board_size_for_window`](crate::widgets::board_size_for_window)
    /// at the screen's current window size, so its local coordinates
    /// map through that geometry — not the hardcoded design bounds.
    fn square_at_drop(&self, p: Point) -> Option<Square> {
        let size = board_size_for_window(self.window);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        };
        Geom::from_bounds_flipped(bounds, self.flipped).square_at(p)
    }

    /// One click / drag on the board: select a friendly piece, or
    /// play the selected piece's move to a legal destination.
    fn click_square(&mut self, sq: Square, shared: &Shared) {
        let pos = self.game.pos.clone();
        if let Some(sel) = self.selected {
            if sel != sq
                && pos
                    .legal_moves()
                    .iter()
                    .any(|m| m.from == sel && m.to == sq)
            {
                self.play_user_move(Move::new(sel, sq), shared);
                return;
            }
        }
        match pos.piece_at(sq) {
            Some(piece) if Some(piece.color) == self.learner_color() => {
                self.selected = Some(sq);
            }
            _ => {
                self.selected = None;
            }
        }
    }

    fn handle_board(&mut self, msg: BoardMsg, shared: &Shared) {
        if !self.can_input() {
            return;
        }
        match msg {
            BoardMsg::DragStarted(sq) | BoardMsg::SquareClicked(sq) => {
                self.click_square(sq, shared);
            }
            BoardMsg::DragEnded(p) => {
                if let Some(sq) = self.square_at_drop(p) {
                    self.click_square(sq, shared);
                }
            }
            BoardMsg::DragMoved(_) | BoardMsg::HoverSquare(_) => {}
        }
    }
}

impl State {
    /// `Navigate` / `ShowSettings` / `ShowAbout` are intercepted by
    /// `App` before they reach here, so this never handles them; the
    /// match stays exhaustive for future variants.
    pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout | Msg::ContinueWithAi => {
                Task::none()
            }
            Msg::WindowResized(size) => {
                self.window = size;
                Task::none()
            }
            Msg::Ignored => Task::none(),
            Msg::Tick(now) => {
                // The piece has landed: the work that waited for the
                // slide may run now (see the settle step below).
                let _ = self.board.finish_anim(now);
                if let Some(due) = self.opponent_due {
                    if now >= due {
                        self.play_opponent(shared);
                    }
                }
                self.settle(shared);
                Task::none()
            }
            Msg::Board(board_msg) => {
                self.handle_board(board_msg, shared);
                self.settle(shared);
                Task::none()
            }
            Msg::HoverHint(hovered) => {
                self.hint_hover = hovered;
                Task::none()
            }
            Msg::PlayHint(mv) => {
                if self.can_input() {
                    self.play_user_move(mv, shared);
                }
                self.settle(shared);
                Task::none()
            }
            Msg::OpponentChoice(idx) => {
                self.play_opponent_choice(idx, shared);
                self.settle(shared);
                Task::none()
            }
            Msg::Undo => {
                self.undo();
                self.settle(shared);
                Task::none()
            }
            Msg::ToggleHints => {
                self.hide_hints = !self.hide_hints;
                Task::none()
            }
            Msg::Restart => {
                if let Some(lesson) = self.lesson.clone() {
                    // One user action (not per-frame): re-box the
                    // lesson for a fresh attempt.
                    self.start_lesson(lesson);
                }
                Task::none()
            }
            Msg::Retry => {
                self.retry();
                self.settle(shared);
                Task::none()
            }
            Msg::ContinueOffBook => {
                self.continue_off_book();
                Task::none()
            }
            Msg::StopAndSummary => {
                if let Some(data) = self.build_summary(shared) {
                    self.pending_summary = Some(data);
                    return Task::done(Msg::Navigate(Screen::Summary));
                }
                Task::none()
            }
            Msg::HideComplete => {
                self.show_complete_dialog = false;
                Task::none()
            }
            Msg::FlipBoard => {
                self.flipped = !self.flipped;
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
            Msg::OpponentAiMove { req_id, mv } => {
                if response_is_current(req_id, self.opponent_ai_req) {
                    self.opponent_ai_req = None;
                    self.play_move_common(mv, shared);
                    if self.out_of_book {
                        self.plies_after_book += 1;
                    }
                    if !self.maybe_complete() {
                        // Next frame's `refresh_hints` rebuilds hints
                        // for the learner's turn.
                        self.hint_ply = None;
                    }
                }
                self.settle(shared);
                Task::none()
            }
        }
    }

    /// The learn screen: header, eval bar + board + fixed 380 px
    /// side panel (scrollable, so long content never flexes the
    /// board). The complete / off-book dialogs render as cards at
    /// the top of the panel — Iced 0.14 has no modal dialog
    /// (see `docs/SPIKE-iced.md` §2).
    pub fn view(&self, shared: &Shared) -> Element<'_, Msg> {
        // Borrowed, never cloned: a `Lesson` carries its whole tree,
        // and `view` runs on every frame.
        let Some(lesson) = self.lesson.as_ref() else {
            return container(
                column![
                    text(i18n::t(Key::LessonNotFound)).size(FONT_SCREEN_TITLE),
                    secondary_button(
                        i18n::t(Key::OtherLesson),
                        Msg::Navigate(Screen::LearnSelect)
                    ),
                    secondary_button(i18n::t(Key::BackToMenu), Msg::Navigate(Screen::Home)),
                ]
                .spacing(12),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        };
        let pos = self.game.pos.clone();
        let side_text = match self.learner_color() {
            Some(SideColor::Black) => i18n::t(Key::LearnSecond),
            _ => i18n::t(Key::LearnFirst),
        };
        // Header (mockup pages 4/5): 64 px surface bar — ghost "Quay
        // lại", 20 px Semibold title, chips for the lesson, the
        // learner's side and the engine, ghost "Cài đặt".
        let header = container(
            row![
                ghost_button_with_icon(
                    i18n::t(Key::BackToMenu),
                    Icon::ArrowLeft,
                    Msg::Navigate(Screen::Home)
                ),
                text(i18n::t(Key::LearnOpenings))
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold())
                    .width(Length::Fill),
                // No i18n label keys exist for the mockup's
                // "Khai cuộc: …" / "Bạn: …" prefixes (and i18n.rs is
                // out of scope here), so the chips show the values
                // alone: the lesson name and the learner's side.
                status_chip(lesson.name_vi.clone(), None, TEXT_DIM),
                status_chip(side_text.to_string(), None, TEXT_DIM),
                status_chip(shared.engine_name.clone(), None, TEXT_DIM),
                ghost_button_with_icon(
                    i18n::t(Key::Settings),
                    Icon::SlidersHorizontal,
                    Msg::ShowSettings
                ),
            ]
            .spacing(SPACE_16)
            .align_y(iced::Alignment::Center),
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
            background: Some(iced::Background::Color(theme::SURFACE)),
            ..Default::default()
        });

        // Board: canvas + side panel. (No eval bar: Hieu 2026-11-13
        // asked to remove the colour bar at the far left; the
        // advantage chip in the panel still shows the score.)
        let board_el = self.board_view(&pos);
        let panel = self.panel_view(lesson, &pos, shared);

        container(
            column![
                header,
                header_divider(),
                container(
                    row![board_el, panel,]
                        .spacing(SPACE_24)
                        .width(Length::Fill)
                        .align_y(iced::Alignment::Start),
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
            .spacing(0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// The board canvas: selection ring, legal targets, last move,
    /// hint / opponent arrows, check halo, slide animation.
    fn board_view(&self, pos: &Position) -> Element<'_, Msg> {
        let selected = self.selected;
        let targets: Vec<Square> = selected.map_or(Vec::new(), |sq| {
            pos.legal_moves()
                .into_iter()
                .filter(|m| m.from == sq)
                .map(|m| m.to)
                .collect()
        });
        let check_sq = if pos.in_check(pos.side()) {
            Some(pos.king_sq(pos.side()))
        } else {
            None
        };
        // Hint arrows: up to 4, hovered one brighter (§6.4.2, §7.3).
        // Opponent variations (§8) draw as numbered PURPLE dashed
        // arrows — same order as the choice panel, so the numbers
        // match (the egui app drew them blue; DESIGN-iced.md §1 and
        // the task require purple dashed for the opponent's turn).
        let mut arrows: Vec<Arrow> = Vec::new();
        if self.show_opponent_choice {
            for (rank, (_, mv, _, _, _)) in self.opponent_choices().iter().take(4).enumerate() {
                arrows.push(Arrow::opponent(mv.from, mv.to, rank + 1));
            }
        } else if !self.hide_hints && self.can_input() {
            for (i, hint) in self.hints.iter().take(4).enumerate() {
                let mut arrow = Arrow::suggestion(hint.mv.from, hint.mv.to, i + 1);
                if self.hint_hover == Some(i) {
                    arrow.highlighted = true;
                }
                arrows.push(arrow);
            }
        }
        let render = BoardRender {
            position: pos.clone(),
            selected,
            legal_targets: targets,
            last_move: self.last_move.map(|m| (m.from, m.to)),
            arrows,
            check_square: check_sq,
            anim: self.board.anim,
            pulse: self.pulse(),
            flipped: self.flipped,
            // Only the learner's pieces can be dragged; the
            // opponent's pieces never lift (a click on one still
            // captures with the selected piece).
            draggable: self.learner_color(),
        };
        let board_size = board_size_for_window(self.window);
        container(BoardProgram::canvas(render).map(Msg::Board))
            .width(Length::Fixed(board_size.width))
            .height(Length::Fixed(board_size.height))
            .into()
    }

    /// The side panel (6 sections, as in the egui app):
    /// `Length::Fill`, so it takes all the width the window leaves
    /// after the eval bar and the board — at least 380 px, because
    /// [`board_size_for_window`] reserves that minimum in the
    /// board's horizontal budget (see
    /// [`panel_width_for_window`](crate::widgets::panel_width_for_window)).
    /// Its column is Fill too, so cards and buttons stretch with
    /// the panel instead of staying 380 px wide in a wide window.
    fn panel_view(&self, lesson: &Lesson, pos: &Position, shared: &Shared) -> Element<'_, Msg> {
        let mut col = column![].spacing(SPACE_16).width(Length::Fill);

        // Lesson-complete / off-book dialogs (cards at the top).
        if self.show_complete_dialog {
            if let Some(summary) = &self.complete {
                col = col.push(self.complete_card(summary));
            }
        } else if self.off_book_pending {
            col = col.push(self.off_book_card(shared));
        }

        // Opponent + player cards (mockup pages 4/5: 44 px piece disc,
        // Semibold 16 name, 13 px Medium subtitle, status chip; gold
        // border on the side to move).
        let learner = self.learner_color().unwrap_or(SideColor::Red);
        let (opp_glyph, opp_glyph_color) = match learner {
            SideColor::Red => ("將", BLACK_PIECE),
            SideColor::Black => ("帥", RED_PIECE),
        };
        let (player_glyph, player_glyph_color) = match learner {
            SideColor::Red => ("帥", RED_PIECE),
            SideColor::Black => ("將", BLACK_PIECE),
        };
        let learner_turn = self.can_input() && self.complete.is_none();
        col = col.push(side_card(
            opp_glyph,
            opp_glyph_color,
            "Pikafish",
            difficulty_name(shared.settings.difficulty).to_string(),
            status_chip(
                if learner_turn {
                    i18n::t(Key::PlayWaitingForYou).to_string()
                } else {
                    i18n::t(Key::Thinking).to_string()
                },
                None,
                TEXT_DIM,
            ),
            false,
        ));
        let side_text = match learner {
            SideColor::Black => i18n::t(Key::LearnSecond),
            _ => i18n::t(Key::LearnFirst),
        };
        col = col.push(side_card(
            player_glyph,
            player_glyph_color,
            i18n::t(Key::PlayerYou),
            side_text.to_string(),
            status_chip(
                if learner_turn {
                    i18n::t(Key::PlayYourTurn).to_string()
                } else {
                    i18n::t(Key::Thinking).to_string()
                },
                if learner_turn { Some(GOLD) } else { None },
                if learner_turn { GOLD } else { TEXT_DIM },
            ),
            learner_turn,
        ));

        // Section 1 — lesson + progress + Undo.
        let remaining = self.main_branch_remaining();
        let total_est = self.game.moves.len() + remaining;
        let progress = text(
            i18n::t(Key::ProgressMoves)
                .replace("{n}", &self.game.moves.len().to_string())
                .replace("{m}", &total_est.to_string()),
        )
        .size(FONT_BODY);
        let can_undo = self.started && self.complete.is_none() && !self.game.moves.is_empty();
        let undo_btn: Element<'_, Msg> = if can_undo {
            secondary_button_with_icon(i18n::t(Key::Undo), Icon::Undo, Msg::Undo).into()
        } else {
            disabled_button_with_icon(i18n::t(Key::Undo), Icon::Undo).into()
        };
        col = col.push(text(lesson.name_vi.clone()).size(FONT_SCREEN_TITLE));
        col = col.push(
            row![progress, Space::new().width(Length::Fill), undo_btn]
                .spacing(8)
                .align_y(iced::Alignment::Center),
        );

        // Section 2 — tutor feedback (message + grade).
        col = col.push(section_title(i18n::t(Key::TutorTitle)));
        col = col.push(text(self.tutor_message.clone()).size(FONT_BODY));
        if let Some((grade, expl, best)) = &self.last_grade {
            let grade_line = text(format!(
                "{}: {} — {expl}",
                i18n::t(Key::GradeLabel),
                i18n::grade_label(*grade)
            ))
            .size(FONT_BODY)
            .style(move |_: &Theme| iced::widget::text::Style {
                color: Some(grade_color(*grade)),
            });
            col = col.push(grade_line);
            if let (Some(best), Some(pos_before)) = (best, &self.grade_pos) {
                col = col.push(
                    text(i18n::t(Key::BookMoveShouldBe).replace(
                        "{name}",
                        &render_move(pos_before, *best, shared.settings.notation),
                    ))
                    .size(FONT_BODY),
                );
            }
        }

        // Section 3 — next moves from the book, shown at once after
        // the learner's move (Hieu, 2026-11-12).
        if let Some((reply, follow)) = &self.next_preview {
            col = col.push(section_title(i18n::t(Key::NextMovesTitle)));
            col = col.push(
                row![
                    text(i18n::t(Key::NextMovesOpponent).replace(
                        "{name}",
                        &render_move(pos, reply.mv, shared.settings.notation),
                    ),)
                    .size(FONT_BODY)
                    .width(Length::Fill),
                    advantage_chip(reply.score),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
            if !follow.is_empty() {
                col = col.push(text(i18n::t(Key::NextMovesFollow)).size(FONT_BODY));
                let mut after = pos.clone();
                {
                    let mut scratch = Game::new(after.clone());
                    let _ = scratch.play(reply.mv);
                    after = scratch.pos.clone();
                }
                for (i, hint) in follow.iter().take(4).enumerate() {
                    col = col.push(
                        row![
                            text(format!("{}.", i + 1))
                                .size(FONT_BODY)
                                .font(theme::inter_font_medium()),
                            text(render_move(&after, hint.mv, shared.settings.notation))
                                .size(FONT_BODY)
                                .font(theme::inter_font_medium())
                                .width(Length::Fill),
                            advantage_chip(hint.score),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    );
                }
            }
        }

        // Section 4 — ChessDB: a separate live score for the current
        // position. No win/draw/loss percentages here (Hieu,
        // 2026-11-12): only the score chip + best move.
        col = col.push(
            self.chessdb
                .view(pos, &shared.settings)
                .map(map_chessdb_msg),
        );

        // Section 5 — opponent variations to pick from, or the hint
        // list (best directions) for the learner's turn.
        if self.show_opponent_choice {
            col = col.push(section_title(i18n::t(Key::OpponentChooseTitle)));
            let choices = self.opponent_choices();
            for (rank, (idx, mv, score, _, note)) in choices.iter().take(4).enumerate() {
                let mut row_col = column![row![
                    text(format!("{}.", rank + 1))
                        .size(FONT_BODY)
                        .font(theme::inter_font_medium()),
                    text(render_move(pos, *mv, shared.settings.notation))
                        .size(FONT_BODY)
                        .font(theme::inter_font_medium())
                        .width(Length::Fill),
                    // The chip shows the advantage from the
                    // LEARNER's side, so the opponent's stored
                    // score is negated.
                    advantage_chip(-score),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),]
                .spacing(4);
                if !note.is_empty() {
                    row_col = row_col.push(text(note.clone()).size(FONT_CAPTION));
                }
                let row_box = container(row_col)
                    .padding(8)
                    .width(Length::Fill)
                    .style(row_container_style(false));
                col = col.push(mouse_area(row_box).on_press(Msg::OpponentChoice(*idx)));
            }
        } else if self.hide_hints {
            col = col.push(secondary_button(i18n::t(Key::ShowHints), Msg::ToggleHints));
        } else {
            col = col.push(
                row![
                    section_title(i18n::t(Key::BestDirections)),
                    Space::new().width(Length::Fill),
                    secondary_button(i18n::t(Key::HideHints), Msg::ToggleHints),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
            if self.hints.is_empty() {
                col = col.push(text(i18n::t(Key::NoHintAvailable)).size(FONT_BODY));
            }
            let hint_pos = self
                .hint_pos
                .clone()
                .unwrap_or_else(|| self.game.pos.clone());
            for (i, hint) in self.hints.iter().take(4).enumerate() {
                let hovered = self.hint_hover == Some(i);
                let row_col = column![
                    row![
                        text(format!("{}.", i + 1))
                            .size(FONT_BODY)
                            .font(theme::inter_font_medium()),
                        text(render_move(&hint_pos, hint.mv, shared.settings.notation))
                            .size(FONT_BODY)
                            .font(theme::inter_font_medium())
                            .width(Length::Fill),
                        advantage_chip(hint.score),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    text(hint.explanation_vi.clone()).size(FONT_CAPTION),
                ]
                .spacing(4);
                let row_box = container(row_col)
                    .padding(8)
                    .width(Length::Fill)
                    .style(row_container_style(hovered));
                col = col.push(
                    mouse_area(row_box)
                        .on_enter(Msg::HoverHint(Some(i)))
                        .on_exit(Msg::HoverHint(None))
                        .on_press(Msg::PlayHint(hint.mv)),
                );
            }
        }

        // Section 6 — compact move list, in a card with the gold
        // progress bar from mockup page 4 ("Bước n / [N]" + bar).
        let moves = move_strings(&self.game, shared.settings.notation);
        let mut moves_col = column![].spacing(SPACE_4);
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
            for mv in moves.iter().skip(i).take(2) {
                r = r.push(
                    text(mv.clone())
                        .size(FONT_BODY)
                        .font(theme::inter_font_medium())
                        .width(Length::Fill),
                );
            }
            moves_col = moves_col.push(r);
            i += 2;
        }
        let total_est = self.game.moves.len() + self.main_branch_remaining();
        let progress = if total_est == 0 {
            0.0
        } else {
            self.game.moves.len() as f32 / total_est as f32
        };
        let moves_card = column![
            row![
                section_title(i18n::t(Key::MovesTitle)),
                Space::new().width(Length::Fill),
                text(
                    i18n::t(Key::ProgressMoves)
                        .replace("{n}", &self.game.moves.len().to_string())
                        .replace("{m}", &total_est.to_string()),
                )
                .size(FONT_CAPTION)
                .font(theme::inter_font_medium())
                .style(|_: &Theme| iced::widget::text::Style {
                    color: Some(TEXT_DIM),
                }),
            ]
            .spacing(SPACE_8)
            .align_y(iced::Alignment::Center),
            progress_bar(progress),
            scrollable(moves_col)
                .height(Length::Fixed(200.0))
                .width(Length::Fill),
        ]
        .spacing(SPACE_8);
        col = col.push(
            container(moves_card)
                .padding(SPACE_16)
                .width(Length::Fill)
                .style(card_style),
        );

        // Buttons (mockup pages 4/5: 2-column grid, 40 px, then the
        // full-width gold "Dừng và tóm tắt"). "Đặt lại" and
        // "Đổi khai cuộc" carry the Undo / BookOpen icons;
        // "Dừng và tóm tắt" and "Lật bàn" have none in the mockup.
        col = col.push(
            row![
                grid_button(secondary_button_with_icon(
                    i18n::t(Key::RetryMove),
                    Icon::Undo,
                    Msg::Restart
                )),
                grid_button(secondary_button_with_icon(
                    i18n::t(Key::OtherLesson),
                    Icon::BookOpen,
                    Msg::Navigate(Screen::LearnSelect)
                )),
            ]
            .spacing(SPACE_8),
        );
        col = col.push(
            row![
                grid_button(secondary_button(i18n::t(Key::FlipBoard), Msg::FlipBoard)),
                grid_button(secondary_button(
                    i18n::t(Key::BackToMenu),
                    Msg::Navigate(Screen::Home)
                )),
            ]
            .spacing(SPACE_8),
        );
        col = col.push(
            primary_button(i18n::t(Key::StopAndSummary), Msg::StopAndSummary)
                .width(Length::Fill)
                .height(Length::Fixed(44.0)),
        );

        scrollable(col)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    /// Lesson-complete dialog card (stats + "Dừng và tóm tắt").
    fn complete_card(&self, summary: &LessonSummary) -> Element<'_, Msg> {
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
        let mut col = column![
            text(i18n::t(Key::OpeningCompleteTitle)).size(FONT_SCREEN_TITLE),
            text(
                i18n::t(Key::CorrectBookMoves)
                    .replace("{n}", &summary.book_moves.to_string())
                    .replace("{m}", &summary.total_learner_moves.to_string()),
            )
            .size(FONT_BODY),
            text(
                i18n::t(Key::MistakesAndInaccuracies)
                    .replace("{n}", &blunders.to_string())
                    .replace("{m}", &inacc.to_string()),
            )
            .size(FONT_BODY),
        ]
        .spacing(8);
        if let Some(note) = &summary.end_note_vi {
            col = col.push(text(note.clone()).size(FONT_BODY));
        }
        col = col.push(
            text(i18n::t(Key::EngineScore).replace("{n}", &summary.final_score.to_string()))
                .size(FONT_BODY),
        );
        col = col.push(
            row![
                primary_button(i18n::t(Key::StopAndSummary), Msg::StopAndSummary),
                secondary_button_with_icon(
                    i18n::t(Key::ContinueWithAi),
                    Icon::Maximize,
                    Msg::ContinueWithAi
                ),
                secondary_button(i18n::t(Key::Close), Msg::HideComplete),
            ]
            .spacing(8),
        );
        container(col)
            .padding(SPACE_16)
            .width(Length::Fill)
            .style(card_style)
            .into()
    }

    /// Off-book dialog card (Retry / Continue).
    fn off_book_card(&self, shared: &Shared) -> Element<'_, Msg> {
        let mut col = column![
            text(i18n::t(Key::OffBookTitle)).size(FONT_SCREEN_TITLE),
            text(i18n::t(Key::OffBookBody)).size(FONT_BODY),
        ]
        .spacing(8);
        if let Some((_, _, best)) = &self.last_grade {
            if let (Some(best), Some(pos_before)) = (best, &self.grade_pos) {
                col = col.push(
                    text(i18n::t(Key::BookMoveShouldBe).replace(
                        "{name}",
                        &render_move(pos_before, *best, shared.settings.notation),
                    ))
                    .size(FONT_BODY),
                );
            }
        }
        col = col.push(
            row![
                primary_button_with_icon(i18n::t(Key::RetryMove), Icon::Undo, Msg::Retry),
                secondary_button(i18n::t(Key::ContinueMove), Msg::ContinueOffBook),
            ]
            .spacing(8),
        );
        container(col)
            .padding(SPACE_16)
            .width(Length::Fill)
            .style(card_style)
            .into()
    }

    /// ChessDB bridge + AI bridge + the animation / opponent tick.
    /// Only the ACTIVE screen's subscription runs (see `App`), so
    /// this panel's `"chessdb"` bridge never collides with Play's.
    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            self.chessdb.subscription().map(map_chessdb_msg),
            crate::bridge::from_mpsc("ai-learn", Arc::clone(&self.ai_rx)).map(map_ai_response),
            iced::window::resize_events().map(|(_id, size)| Msg::WindowResized(size)),
        ];
        if self.needs_tick() {
            subs.push(iced::time::every(TICK_INTERVAL).map(Msg::Tick));
        }
        Subscription::batch(subs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_book::schema::BookNode;
    use xq_core::analysis::GradeThresholds;

    fn node(mv: &str, main: bool, score_cp: i32) -> BookNode {
        BookNode {
            mv: mv.to_string(),
            note_vi: None,
            next: Vec::new(),
            weight: 1.0,
            main,
            trap: false,
            source_ref: None,
            end: None,
            score_cp: Some(score_cp),
        }
    }

    #[test]
    fn grade_thresholds_are_the_pikafish_set_15_50_120_250() {
        let t = GradeThresholds::PIKAFISH;
        assert_eq!(
            (t.best, t.good, t.acceptable, t.inaccurate),
            (15, 50, 120, 250)
        );
        // Boundary deltas (the Tutor grades book moves itself; these
        // thresholds are the engine-grading rule the book was built
        // with — see `docs/DECISIONS-learn.md` §6).
        use xq_core::analysis::grade_from_delta_with;
        assert_eq!(grade_from_delta_with(15, false, t), Grade::Best);
        assert_eq!(grade_from_delta_with(50, false, t), Grade::Good);
        assert_eq!(grade_from_delta_with(120, false, t), Grade::Acceptable);
        assert_eq!(grade_from_delta_with(250, false, t), Grade::Inaccurate);
        assert_eq!(grade_from_delta_with(251, false, t), Grade::Blunder);
        assert_eq!(grade_from_delta_with(999, true, t), Grade::Book);
    }

    #[test]
    fn opponent_choices_sorted_best_for_opponent_first() {
        let pos = Position::startpos();
        let children = vec![
            node("炮二平五", true, 30),
            node("马二进三", false, 10),
            node("兵七进一", false, 20),
        ];
        let choices = opponent_choices_for(&children, &pos);
        assert_eq!(choices.len(), 3);
        // Best for the opponent first (30, 20, 10).
        assert_eq!(choices[0].2, 30);
        assert_eq!(choices[1].2, 20);
        assert_eq!(choices[2].2, 10);
        // Child indices are kept (tree order), so the panel index
        // matches `Tutor::play_opponent_move`.
        assert_eq!(choices[0].0, 0);
        assert_eq!(choices[1].0, 2);
        assert_eq!(choices[2].0, 1);
    }

    #[test]
    fn main_branch_count_follows_main_child() {
        let mut root = node("炮二平五", true, 30);
        root.next = vec![node("马8进7", true, -10), node("卒3进1", false, -20)];
        root.next[0].next = vec![node("马二进三", true, 25)];
        assert_eq!(count_main_branch(&[root]), 3);
        assert_eq!(count_main_branch(&[]), 0);
    }

    #[test]
    fn learner_plies_alternate_from_start_side() {
        // Learner is Red at the start: plies 1, 3, 5, ...
        assert_eq!(learner_plies(5, true), vec![1, 3, 5]);
        // Learner is Black: plies 2, 4, 6, ...
        assert_eq!(learner_plies(5, false), vec![2, 4]);
        assert!(learner_plies(0, true).is_empty());
    }

    #[test]
    fn start_loads_lesson_from_shared_book() {
        let mut shared = Shared::new();
        assert!(!shared.book.lessons.is_empty());
        let lesson = shared.book.lessons[0].clone();
        let red = lesson.learner.color() == SideColor::Red;
        let mut st = State::new(&shared);
        st.start(&lesson.id, red, &mut shared);
        assert_eq!(st.lesson.as_ref().expect("lesson").id, lesson.id);
        assert!(st.started);
        assert!(st.tutor.is_some());
    }

    #[test]
    fn stop_and_summary_builds_rows_from_played_moves() {
        let mut shared = Shared::new();
        let lesson = shared
            .book
            .by_learner(SideColor::Red)
            .into_iter()
            .next()
            .expect("a Red lesson")
            .clone();
        let mut st = State::new(&shared);
        st.start(&lesson.id, true, &mut shared);
        assert!(!st.hints.is_empty(), "hints for the first learner turn");
        let mv = st.hints[0].mv;
        st.play_user_move(mv, &shared);
        let data = st.build_summary(&mut shared).expect("summary");
        assert_eq!(data.rows.len(), 1);
        assert_eq!(data.book_moves, 1);
        assert_eq!(data.total, 1);
        assert_eq!(data.lesson_id, lesson.id);
        assert_eq!(data.rows[0].grade, Grade::Book);
        assert!(data.rows[0].best.is_some());
        assert_eq!(data.positions.len(), 2);
    }
}
