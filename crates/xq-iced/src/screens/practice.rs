//! Self-practice screen for the Iced rewrite (agent P3).
//!
//! Ported from the egui screen (`crates/xq-app/src/screens/practice.rs`,
//! 1440 lines): the player plays BOTH sides (Red and Black) against no
//! AI, with a suggestion + a grade after every move. Everything the
//! egui screen does is kept: difficulty pick for Pikafish, start from
//! the standard position / a FEN / a board photo + the manual board
//! editor, ChessDB top-4 suggestions with a Pikafish `Analyze` fallback,
//! grading of every played move (including moves outside the
//! suggestions), and Reset / Undo.
//!
//! # INVARIANT — the piece must land before any computation starts
//!
//! A move's slide animation (150 ms) must finish BEFORE the grading,
//! suggestion or ChessDB work for the new position starts (Hieu,
//! restated 2026-11-12, three times in one day). In Elm terms:
//! `update()` only triggers that work when `board.is_animating() ==
//! false`; the grade request is held in `pending_grade` until the
//! tick reports the animation finished
//! ([`BoardState::finish_anim`]); the tick subscription only runs
//! while an animation or the flashing check halo is active.
//!
//! # Notes for the integrator (P4) / coordinator
//!
//! - **ChessDB panel.** The egui practice screen never rendered the
//!   app-level ChessDB section (`chessdb_section_ui` is only called
//!   by `vs_ai.rs` with `show_win_rate = true` and by `learn.rs`
//!   with `false`); it used ChessDB internally for the top-4
//!   suggestions + the advantage score, and it *did* show a win rate
//!   per suggestion line (`W{:.0}%`). This screen embeds a
//!   [`ChessDbPanel`] with `show_win_rate = true` (the Play value),
//!   and reuses the panel's single evaluation both for the section
//!   display and for the top-4 suggestion lines — one request per
//!   position, no duplicate worker.
//! - **Board photo (raster display).** The uploaded photo IS shown
//!   (`iced::widget::image`, feature `image` enabled for `iced` at
//!   integration): the kept bytes are rendered below the file name.
//!   Recognition is attempted via [`recognize_board_image`] — it
//!   always fails for now (no vision model), so the UI reports
//!   "image could not be recognised" and the user rebuilds the
//!   position in the editor below.
//! - **Flip.** The playing board flips with the shared `flipped`
//!   state (button in the panel; `BoardRender::flipped` +
//!   `Geom::from_bounds_flipped`, restored at integration). The
//!   editor canvas below is this file's own program and flips
//!   independently.
//! - **Board origin (fixed at integration).** `Geom` is widget-local
//!   now (`board.rs`): the canvas `Frame` is created from
//!   `bounds.size()` and hit-testing uses `cursor.position_in`, so
//!   the board draws and hit-tests correctly at any position. The
//!   eval bar is back on the LEFT of the board (the egui position).
//! - **Engine limits.** The egui screen asks the fallback `Analyze`
//!   with `engine.limits_for(level)` (not `hint_limits()` — that is
//!   the Learn screen's helper); `GradeMove` is built on the worker
//!   with `grade_limits()`, so the UI never locks the engine to
//!   grade. Both are kept as-is.

use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::alignment::Vertical;
use iced::mouse::{self, Cursor};
use iced::widget::canvas::{
    Action, Canvas, Event, Frame, Geometry, Path, Program, Stroke, Text as CanvasText,
};
use iced::widget::text::Alignment as TextAlignment;
use iced::widget::{
    column, container, mouse_area, radio, row, scrollable, slider, text, text_input, Button, Column,
};
use iced::{Element, Length, Pixels, Point, Rectangle, Renderer, Size, Subscription, Task, Theme};
use xq_chessdb::{ChessDbError, ChessDbEval};
use xq_core::analysis::Grade;
use xq_core::board::Position;
use xq_core::game::{Game, GameResult, Reason};
use xq_core::notation;
use xq_core::types::{file_of, rank_of, sq, Color, Move, Piece, PieceKind};
use xq_engine::{EngineRequest, Level};

use crate::ai_worker::{response_is_current, AiRequest, AiResponse, AiWorker};
use crate::app::{Screen, Shared};
use crate::board::{Arrow, BoardMsg, BoardProgram, BoardRender, BoardState, Geom, Square};
use crate::bridge;
use crate::chessdb_panel::{ChessDbPanel, ChessDbPanelMsg};
use crate::chessdb_worker::{ChessDbResponse, ChessDbView};
use crate::helpers::{format_advantage, position_at, render_move};
use crate::i18n::{self, difficulty_name, Key};
use crate::sound::sound_for_move;
use crate::theme::{
    self, BLACK_PIECE, BOARD_HEIGHT, BOARD_LINE, BOARD_WIDTH, FONT_BODY, FONT_CAPTION, FONT_PIECE,
    FONT_SCREEN_TITLE, PIECE_BG, PIECE_INNER_RADIUS, PIECE_RADIUS, RED, RED_PIECE, WOOD,
};
use crate::widgets::{
    advantage_chip, eval_bar, primary_button, secondary_button, section_title, PANEL_WIDTH,
};

/// How long an engine suggestion / grade may take before the screen
/// stops waiting and shows "no suggestions" / leaves the move
/// ungraded (the worker may have failed silently; the game itself
/// never waits). Same 30 s as the egui screen.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(30);

/// How many suggested moves to show (ChessDB top 4 / Pikafish top 4).
const SUGGEST_TOP_N: usize = 4;

/// How often the animation / check-halo tick fires while one is
/// active (the 150 ms slide needs several frames to look smooth).
const TICK_INTERVAL: Duration = Duration::from_millis(16);

/// The editor board canvas size (the board is drawn centred in it
/// at the design's 592×656 ratio).
const EDITOR_WIDTH: f32 = 400.0;
/// The editor board canvas size (the board is drawn centred in it
/// at the design's 592×656 ratio).
const EDITOR_HEIGHT: f32 = 440.0;

/// Which phase the practice screen is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PracticePhase {
    /// Pick a difficulty + a position first.
    Setup,
    /// Playing both sides on the board.
    Playing,
}

/// Where the starting position comes from (setup phase).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PracticeSource {
    /// The standard starting position.
    Start,
    /// A FEN pasted by the user.
    Fen,
    /// A board photo + the manual board editor.
    Image,
}

/// Try to read a Xiangqi position from a board photo (`bytes` are
/// the image file contents). Returns the position on success, or an
/// error when the photo cannot be recognised.
///
/// There is NO vision model in the app yet, so this always fails for
/// now and the caller reports "image could not be recognised" and
/// falls back to the manual board editor. When a recognition model
/// is added (a CV model or an on-device ML model), implement it
/// here: decode the image, find the board grid, classify the piece
/// on each square, and build the `Position` from the resulting
/// board. [`State::editor_from_position`] is ready to pour the
/// recognised position into the editor for the user to check.
fn recognize_board_image(_bytes: &[u8]) -> Result<Position, &'static str> {
    Err("no board-recognition model available")
}

/// Where the current suggestions came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SuggestSource {
    /// Nothing yet (still loading, or nothing available).
    None,
    /// ChessDB's top moves (the preferred source, Hieu's golden rules).
    ChessDb,
    /// Pikafish `Analyze` (fallback when ChessDB has no data).
    Engine,
}

/// One suggested move, normalised to a Red-perspective centipawn
/// score so ChessDB lines and engine lines display the same way.
#[derive(Clone, Copy, Debug)]
pub struct SuggestLine {
    pub mv: Move,
    /// Centipawn score from Red's perspective.
    pub score_cp_red: i32,
    /// Red win rate in percent (ChessDB only).
    pub winrate: Option<f32>,
}

/// A grade request held back until the piece has landed (see the
/// invariant at the top of this file).
struct PendingGrade {
    /// FEN the game started from.
    start_fen: String,
    /// Moves before the played move.
    moves_before: Vec<Move>,
    /// The played move.
    mv: Move,
    /// Index into `game.moves` / `move_grades` the grade is for.
    idx: usize,
}

/// Practice screen state.
pub struct State {
    phase: PracticePhase,
    /// Difficulty index (0..=4) for Pikafish, same scale as the app.
    difficulty: u8,
    /// Custom Elo when `difficulty == 4`.
    custom_elo: u16,
    /// Which setup source tab is open.
    source: PracticeSource,
    /// The game being played (both sides by the user).
    game: Game,
    /// The position the game started from (Reset returns to it).
    start_pos: Position,
    /// FEN text field (setup, FEN source).
    fen_text: String,
    /// FEN error message (setup).
    fen_error: Option<String>,
    /// Loaded photo path (setup, image source).
    image_path: String,
    /// Loaded photo bytes (kept as proof of load; the raster picture
    /// itself is not displayed — see the module docs: the `iced`
    /// `image` feature is not enabled).
    image_bytes: Option<Arc<[u8]>>,
    /// Image load / recognition error (setup).
    image_error: Option<String>,
    /// Board editor pieces (setup, image source): one slot per square.
    editor_board: [Option<Piece>; 90],
    /// Editor palette selection: `Some(piece)` places it, `None` erases.
    editor_selected: Option<Piece>,
    /// Side to move of the editor-built position.
    editor_side: Color,
    /// Editor error (setup).
    editor_error: Option<String>,
    /// Selected square on the playing board (`None` = nothing selected).
    selected: Option<Square>,
    /// The slide animation state (screen-owned so `update` can gate
    /// all computation on it — see the invariant above).
    board: BoardState,
    /// Whether the playing board is flipped (Black at the bottom).
    /// The Flip button toggles it (egui `PracticeState::flipped`).
    flipped: bool,
    /// Last move played (both squares get a gold tint).
    last_move: Option<Move>,
    /// When the last checking move was played (check halo flashes
    /// for ~1 s after it).
    check_since: Option<Instant>,
    /// FEN the current suggestions were requested for.
    suggest_fen: Option<String>,
    suggest_source: SuggestSource,
    suggest_lines: Vec<SuggestLine>,
    /// `req_id` of the in-flight engine `Analyze` (suggestion fallback).
    suggest_req_id: Option<u64>,
    suggest_requested_at: Option<Instant>,
    /// `req_id` of the in-flight `GradeMove` for the last played move.
    grade_req_id: Option<u64>,
    /// Index into `game.moves` / `move_grades` the grade is for.
    grade_move_index: Option<usize>,
    grade_requested_at: Option<Instant>,
    /// The engine's best move at the last graded position (shown next
    /// to the grade, so the user sees what was better).
    last_engine_best: Option<Move>,
    /// Grade per played move, parallel to `game.moves` (`None` = the
    /// grade has not arrived yet).
    move_grades: Vec<Option<Grade>>,
    /// The ChessDB section (score + best move) and the source of the
    /// top-4 suggestions. `show_win_rate = true` — see the module
    /// docs for why (the Play value: practice shows win rates).
    chessdb: ChessDbPanel,
    /// The AI worker for the Pikafish fallback (`Analyze`) and the
    /// move grades (`GradeMove`). Its receiver is bridged to a
    /// subscription (see `subscription()`); the worker itself is
    /// kept so `update` can send requests and allocate `req_id`s.
    worker: AiWorker,
    /// The worker's response receiver, shared with the bridge
    /// subscription. Taken out of `worker` at construction (see
    /// `State::new`): the bridge's forwarder thread is the
    /// receiver's only consumer while subscribed.
    ai_rx: Arc<Mutex<Receiver<AiResponse>>>,
    /// Grade request waiting for the piece to land (see the
    /// invariant above). `None` when nothing is waiting.
    pending_grade: Option<PendingGrade>,
    /// Which suggestion row the mouse is over (`None` = none); its
    /// arrow draws at full opacity, the rest at 38% (DESIGN-iced.md §1).
    hover_suggest: Option<usize>,
}

/// i18n key for a `Grade` label.
fn grade_key(grade: Grade) -> Key {
    match grade {
        Grade::Book | Grade::Best => Key::PracticeGradeBest,
        Grade::Good => Key::PracticeGradeGood,
        Grade::Acceptable => Key::PracticeGradeAcceptable,
        Grade::Inaccurate => Key::PracticeGradeInaccurate,
        Grade::Blunder => Key::PracticeGradeBlunder,
    }
}

/// FEN placement + side-to-move for the editor board, so
/// `Position::from_fen` can validate it (missing/wrongly placed pieces
/// are rejected there).
fn editor_fen(board: &[Option<Piece>; 90], side: Color) -> String {
    let mut out = String::new();
    for rank in (0..10).rev() {
        let mut empty = 0;
        for file in 0..9 {
            let s = sq(file, rank).unwrap();
            match board[s as usize] {
                None => empty += 1,
                Some(p) => {
                    if empty > 0 {
                        out.push_str(&empty.to_string());
                        empty = 0;
                    }
                    out.push(p.fen_char());
                }
            }
        }
        if empty > 0 {
            out.push_str(&empty.to_string());
        }
        if rank > 0 {
            out.push('/');
        }
    }
    out.push(' ');
    out.push_str(match side {
        Color::Red => "w",
        Color::Black => "b",
    });
    out
}

/// Do the two generals face each other on one file with nothing
/// between them? `Position::from_fen` does not check this, so the
/// editor checks it before accepting a position.
fn editor_generals_face(board: &[Option<Piece>; 90]) -> bool {
    let mut red = None;
    let mut black = None;
    for (i, p) in board.iter().enumerate() {
        if let Some(piece) = p {
            if piece.kind == PieceKind::King {
                if piece.color == Color::Red {
                    red = Some(i as u8);
                } else {
                    black = Some(i as u8);
                }
            }
        }
    }
    let (Some(r), Some(b)) = (red, black) else {
        return false;
    };
    if file_of(r) != file_of(b) {
        return false;
    }
    let f = file_of(r);
    let (lo, hi) = if rank_of(r) < rank_of(b) {
        (rank_of(r), rank_of(b))
    } else {
        (rank_of(b), rank_of(r))
    };
    for rank in (lo + 1)..hi {
        if let Some(s) = sq(f, rank) {
            if board[s as usize].is_some() {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// The user picked another difficulty (0..=4).
    DifficultyChanged(u8),
    /// The custom-Elo slider moved (difficulty 4).
    CustomEloChanged(u16),
    /// The user picked another setup source tab.
    SourceChanged(PracticeSource),
    /// Start from the standard position.
    StartFromStart,
    /// The FEN text field changed.
    FenChanged(String),
    /// Load the pasted FEN as the starting position.
    LoadFen,
    /// Open the native file dialog to pick a board photo.
    PickImage,
    /// The file dialog finished: `Some((path, bytes))` when the user
    /// picked a file, `None` when they cancelled.
    ImageDialogDone(Option<(String, Arc<[u8]>)>),
    /// The user picked a palette piece (`None` = the eraser).
    EditorPaletteSelected(Option<Piece>),
    /// The user picked the side to move of the editor position.
    EditorSideChanged(Color),
    /// The user clicked a square on the editor board.
    EditorSquareClicked(Square),
    /// Clear the editor board.
    EditorClear,
    /// Start playing from the editor-built position.
    UseEditorPosition,
    /// Reset to the starting position (keeps the difficulty).
    Reset,
    /// Take back one move.
    Undo,
    /// Flip the playing board (Red/Black at the bottom).
    FlipBoard,
    /// Go back to the setup phase (pick another position).
    BackToSetup,
    /// A message from the playing board canvas.
    Board(BoardMsg),
    /// Animation / check-halo tick (only subscribed while one is active).
    Tick(Instant),
    /// A ChessDB evaluation arrived (rebuilt into the panel's message
    /// in `update`; the panel drops it unless its `req_id` is current).
    ChessDbEvaluated {
        req_id: u64,
        result: Result<ChessDbEval, ChessDbError>,
    },
    /// A Pikafish `Analyze` answer arrived (top suggestions).
    AiAnalysis {
        req_id: u64,
        lines: Vec<xq_core::search::AnalysisLine>,
    },
    /// A `GradeMove` answer arrived for a played move.
    AiGraded {
        req_id: u64,
        grade: Grade,
        best: Option<Move>,
    },
    /// The suggestion row the mouse is over (`None` = none) — its
    /// arrow draws at full opacity.
    HoverSuggest(Option<usize>),
    /// No-op (mapping target for widgets that emit no message, e.g.
    /// the ChessDB panel view).
    Noop,
}

impl State {
    /// Fresh screen, in the setup phase. The difficulty defaults
    /// come from the app settings; the setup screen lets the user
    /// change the difficulty without touching the global Vs-AI
    /// difficulty.
    #[must_use]
    pub fn new(shared: &Shared) -> Self {
        // The AI worker's receiver must be handed to the bridge
        // subscription (its forwarder thread is the receiver's only
        // consumer — see `bridge.rs`). `AiWorker` keeps a private
        // field, so it cannot be destructured; swap the receiver out
        // for a dummy channel instead. The worker thread sends its
        // answers to the real receiver; the dummy is never read.
        let mut worker = AiWorker::new(shared.engine.clone());
        let (_dummy_tx, dummy_rx) = channel::<AiResponse>();
        let real_rx = std::mem::replace(&mut worker.rx, dummy_rx);
        Self {
            phase: PracticePhase::Setup,
            difficulty: shared.settings.difficulty,
            custom_elo: shared.settings.custom_elo,
            source: PracticeSource::Start,
            game: Game::new(Position::startpos()),
            start_pos: Position::startpos(),
            fen_text: String::new(),
            fen_error: None,
            image_path: String::new(),
            image_bytes: None,
            image_error: None,
            editor_board: [None; 90],
            editor_selected: None,
            editor_side: Color::Red,
            editor_error: None,
            selected: None,
            board: BoardState::default(),
            flipped: false,
            last_move: None,
            check_since: None,
            suggest_fen: None,
            suggest_source: SuggestSource::None,
            suggest_lines: Vec::new(),
            suggest_req_id: None,
            suggest_requested_at: None,
            grade_req_id: None,
            grade_move_index: None,
            grade_requested_at: None,
            last_engine_best: None,
            move_grades: Vec::new(),
            chessdb: ChessDbPanel::new(true),
            worker,
            ai_rx: Arc::new(Mutex::new(real_rx)),
            pending_grade: None,
            hover_suggest: None,
        }
    }

    /// The practice difficulty as an engine `Level` (same scale as
    /// `Settings::level()`).
    fn level(&self) -> Level {
        match self.difficulty {
            0 => Level::Easy,
            2 => Level::Hard,
            3 => Level::Master,
            4 => Level::Elo(self.custom_elo),
            _ => Level::Medium,
        }
    }

    /// Best available advantage score (Red perspective) for the eval
    /// bar: the best suggestion, else the ChessDB score, else `None`.
    /// Suggestions only count for the position they were computed
    /// for (while a piece is still sliding they belong to the
    /// previous position).
    fn score_red(&self) -> Option<i32> {
        if self.suggest_fen.as_deref() == Some(&self.game.pos.to_fen()) {
            if let Some(best) = self.suggest_lines.first() {
                return Some(best.score_cp_red);
            }
        }
        if let ChessDbView::Ready(eval) = self.chessdb.view_state() {
            return eval.score;
        }
        None
    }

    /// Result text for the status line (`None` while the game is ongoing).
    fn result_text(&self) -> Option<String> {
        let reason = |r: Reason| match r {
            Reason::Checkmate => Some(i18n::t(Key::PracticeReasonCheckmate)),
            Reason::Stalemate => Some(i18n::t(Key::PracticeReasonStalemate)),
            Reason::Repetition => Some(i18n::t(Key::PracticeReasonRepetition)),
            _ => None,
        };
        match self.game.result() {
            GameResult::Ongoing => None,
            GameResult::RedWins(r) => Some(match reason(r) {
                Some(why) => format!("{} — {why}", i18n::t(Key::PracticeRedWins)),
                None => i18n::t(Key::PracticeRedWins).to_string(),
            }),
            GameResult::BlackWins(r) => Some(match reason(r) {
                Some(why) => format!("{} — {why}", i18n::t(Key::PracticeBlackWins)),
                None => i18n::t(Key::PracticeBlackWins).to_string(),
            }),
            GameResult::Draw(r) => Some(match reason(r) {
                Some(why) => format!("{} — {why}", i18n::t(Key::PracticeDraw)),
                None => i18n::t(Key::PracticeDraw).to_string(),
            }),
        }
    }

    /// `Navigate` / `ShowSettings` / `ShowAbout` are intercepted by
    /// `App` before they reach here; the arms stay so the match is
    /// exhaustive.
    pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout | Msg::Noop => Task::none(),
            Msg::DifficultyChanged(d) => {
                self.difficulty = d;
                Task::none()
            }
            Msg::CustomEloChanged(elo) => {
                self.custom_elo = elo;
                Task::none()
            }
            Msg::SourceChanged(source) => {
                self.source = source;
                Task::none()
            }
            Msg::StartFromStart => {
                self.start_from(Position::startpos());
                self.maybe_suggest(shared, Instant::now());
                Task::none()
            }
            Msg::FenChanged(text) => {
                self.fen_text = text;
                Task::none()
            }
            Msg::LoadFen => {
                match Position::from_fen(self.fen_text.trim()) {
                    Ok(pos) => {
                        self.fen_error = None;
                        self.start_from(pos);
                        self.maybe_suggest(shared, Instant::now());
                    }
                    Err(_) => {
                        self.fen_error = Some(i18n::t(Key::PracticeFenInvalid).to_string());
                    }
                }
                Task::none()
            }
            Msg::PickImage => {
                // Native file dialog (rfd): pick an image from the
                // device. `AsyncFileDialog` runs as a `Task`, so the
                // dialog shows detached (Iced exposes no window
                // handle to parent it to — see `docs/SPIKE-iced.md`
                // §3). `None` = the user cancelled.
                Task::perform(
                    async {
                        let picked = rfd::AsyncFileDialog::new()
                            .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "webp"])
                            .pick_file()
                            .await;
                        match picked {
                            Some(handle) => {
                                let path = handle.path().display().to_string();
                                let bytes: Arc<[u8]> = Arc::from(handle.read().await);
                                Msg::ImageDialogDone(Some((path, bytes)))
                            }
                            None => Msg::ImageDialogDone(None),
                        }
                    },
                    |msg| msg,
                )
            }
            Msg::ImageDialogDone(picked) => {
                if let Some((path, bytes)) = picked {
                    self.image_path = path;
                    // Try to read a position from the photo. There is
                    // no vision model yet, so this reports that the
                    // photo could not be recognised; the user sets
                    // the position up by hand in the editor below.
                    match recognize_board_image(&bytes) {
                        Ok(pos) => {
                            self.image_error = None;
                            self.image_bytes = Some(bytes);
                            self.editor_from_position(&pos);
                        }
                        Err(_) => {
                            self.image_bytes = Some(bytes);
                            self.image_error =
                                Some(i18n::t(Key::PracticeImageNotRecognized).to_string());
                        }
                    }
                }
                Task::none()
            }
            Msg::EditorPaletteSelected(piece) => {
                self.editor_selected = piece;
                Task::none()
            }
            Msg::EditorSideChanged(side) => {
                self.editor_side = side;
                Task::none()
            }
            Msg::EditorSquareClicked(sq) => {
                self.editor_click(sq);
                Task::none()
            }
            Msg::EditorClear => {
                self.editor_board = [None; 90];
                self.editor_error = None;
                Task::none()
            }
            Msg::UseEditorPosition => {
                let fen = editor_fen(&self.editor_board, self.editor_side);
                if editor_generals_face(&self.editor_board) {
                    self.editor_error = Some(i18n::t(Key::PracticeEditorInvalid).to_string());
                } else {
                    match Position::from_fen(&fen) {
                        Ok(pos) => {
                            self.editor_error = None;
                            self.start_from(pos);
                            self.maybe_suggest(shared, Instant::now());
                        }
                        Err(_) => {
                            self.editor_error =
                                Some(i18n::t(Key::PracticeEditorInvalid).to_string());
                        }
                    }
                }
                Task::none()
            }
            Msg::Board(board_msg) => {
                self.handle_board(board_msg, shared);
                Task::none()
            }
            Msg::Reset => {
                self.reset();
                self.maybe_suggest(shared, Instant::now());
                Task::none()
            }
            Msg::Undo => {
                self.undo(shared);
                Task::none()
            }
            Msg::FlipBoard => {
                self.flipped = !self.flipped;
                Task::none()
            }
            Msg::BackToSetup => {
                self.phase = PracticePhase::Setup;
                Task::none()
            }
            Msg::Tick(now) => {
                // The piece has landed: flush the work that was
                // waiting for the animation (see the invariant at the
                // top of this file).
                if self.board.finish_anim(now) {
                    self.flush_pending_grade();
                    self.maybe_suggest(shared, now);
                }
                self.check_timeouts(now);
                Task::none()
            }
            Msg::ChessDbEvaluated { req_id, result } => {
                // Feed the answer into the panel (it drops it unless
                // its `req_id` is the one waited for), then derive
                // the suggestions from the panel's new view — or fall
                // back to Pikafish when ChessDB has no data.
                let _ =
                    self.chessdb
                        .update(ChessDbPanelMsg::Response(ChessDbResponse::Evaluated {
                            req_id,
                            result,
                        }));
                self.maybe_suggest(shared, Instant::now());
                self.check_timeouts(Instant::now());
                Task::none()
            }
            Msg::AiAnalysis { req_id, lines } => {
                if response_is_current(req_id, self.suggest_req_id) {
                    self.suggest_req_id = None;
                    self.suggest_requested_at = None;
                    // The lines are for the position the request was
                    // sent for; if the user has moved on, the FEN
                    // changed and these lines are stale — drop them.
                    if self.suggest_fen.as_deref() == Some(&self.game.pos.to_fen()) {
                        let side = self.game.pos.side();
                        self.suggest_source = SuggestSource::Engine;
                        self.suggest_lines = lines
                            .iter()
                            .take(SUGGEST_TOP_N)
                            .map(|l| SuggestLine {
                                mv: l.mv,
                                // Engine scores are side-to-move; the
                                // panel shows Red-perspective scores.
                                score_cp_red: if side == Color::Red {
                                    l.score
                                } else {
                                    -l.score
                                },
                                winrate: None,
                            })
                            .collect();
                    }
                }
                // Retrying here covers the case where the fallback
                // `Analyze` was skipped because the engine was busy
                // grading (see `maybe_suggest`).
                self.maybe_suggest(shared, Instant::now());
                self.check_timeouts(Instant::now());
                Task::none()
            }
            Msg::AiGraded {
                req_id,
                grade,
                best,
            } => {
                if response_is_current(req_id, self.grade_req_id) {
                    self.grade_req_id = None;
                    self.grade_requested_at = None;
                    if let Some(idx) = self.grade_move_index.take() {
                        if idx < self.move_grades.len() {
                            self.move_grades[idx] = Some(grade);
                        }
                    }
                    self.last_engine_best = best;
                }
                // The grade search just freed the engine: if the
                // fallback `Analyze` was skipped while it ran, retry
                // it now.
                self.maybe_suggest(shared, Instant::now());
                self.check_timeouts(Instant::now());
                Task::none()
            }
            Msg::HoverSuggest(hover) => {
                self.hover_suggest = hover;
                Task::none()
            }
        }
    }

    /// Engine timeouts: stop waiting so the UI shows "no
    /// suggestions" / leaves the move ungraded instead of spinning
    /// forever when the worker failed silently.
    fn check_timeouts(&mut self, now: Instant) {
        if let Some(at) = self.suggest_requested_at {
            if self.suggest_req_id.is_some() && now.duration_since(at) > ENGINE_TIMEOUT {
                self.suggest_req_id = None;
                self.suggest_requested_at = None;
            }
        }
        if let Some(at) = self.grade_requested_at {
            if self.grade_req_id.is_some() && now.duration_since(at) > ENGINE_TIMEOUT {
                self.grade_req_id = None;
                self.grade_requested_at = None;
                self.grade_move_index = None;
            }
        }
    }

    /// Ask for suggestions for the current position: ChessDB first
    /// (the shared panel worker), Pikafish `Analyze` when ChessDB
    /// has no data. One request per position; the FEN change resets
    /// the state. Only ever called when the board is NOT animating
    /// (see the invariant at the top of this file).
    fn maybe_suggest(&mut self, shared: &Shared, now: Instant) {
        if self.phase != PracticePhase::Playing
            || self.game.result() != GameResult::Ongoing
            || self.board.is_animating()
        {
            return;
        }
        let fen = self.game.pos.to_fen();
        if self.suggest_fen.as_deref() != Some(&fen) {
            self.suggest_fen = Some(fen.clone());
            self.suggest_lines.clear();
            self.suggest_source = SuggestSource::None;
            self.suggest_req_id = None;
            self.suggest_requested_at = None;
        }
        // ChessDB first (its request is a no-op when the FEN has not
        // changed, and it resets the view to Loading on a new FEN).
        self.chessdb.request(&fen);
        match self.chessdb.view_state().clone() {
            ChessDbView::Ready(eval) => {
                if !eval.top.is_empty() {
                    self.suggest_source = SuggestSource::ChessDb;
                    self.suggest_req_id = None;
                    self.suggest_requested_at = None;
                    let legal = self.game.pos.legal_moves();
                    self.suggest_lines = eval
                        .top
                        .iter()
                        .take(SUGGEST_TOP_N)
                        .filter_map(|m| {
                            notation::parse_iccs(&m.mv)
                                .ok()
                                .filter(|mv| legal.contains(mv))
                                .map(|mv| SuggestLine {
                                    mv,
                                    score_cp_red: m.score,
                                    winrate: Some(m.winrate),
                                })
                        })
                        .collect();
                }
            }
            ChessDbView::Unavailable(_) => {
                // No ChessDB data: fall back to Pikafish, once per position.
                if self.suggest_lines.is_empty() && self.suggest_req_id.is_none() {
                    self.request_analyze(shared, now);
                }
            }
            ChessDbView::Loading | ChessDbView::Idle => {}
        }
    }

    /// Send the Pikafish `Analyze` (top 4) for the current
    /// position. Never blocks the UI on the engine: if a search is
    /// running, the request is skipped and retried when the next
    /// engine answer arrives (see `Msg::AiGraded`).
    fn request_analyze(&mut self, shared: &Shared, now: Instant) {
        let Some(engine) = shared.engine.try_lock() else {
            return;
        };
        let limits = engine.limits_for(self.level());
        drop(engine);
        let req_id = self.worker.alloc_req_id();
        self.suggest_req_id = Some(req_id);
        self.suggest_requested_at = Some(now);
        let _ = self.worker.tx.send(AiRequest::Analyze {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            top_n: SUGGEST_TOP_N,
            req_id,
        });
    }

    /// Send the grade request that was held back while the piece was
    /// sliding (see the invariant at the top of this file). The
    /// worker builds the request itself with `grade_limits()`, so
    /// the UI never locks the engine here.
    fn flush_pending_grade(&mut self) {
        let Some(pending) = self.pending_grade.take() else {
            return;
        };
        let req_id = self.worker.alloc_req_id();
        self.grade_req_id = Some(req_id);
        self.grade_move_index = Some(pending.idx);
        self.grade_requested_at = Some(Instant::now());
        let _ = self.worker.tx.send(AiRequest::GradeMove {
            start_fen: pending.start_fen,
            moves: pending.moves_before,
            mv: pending.mv,
            req_id,
        });
    }

    /// Handle one message from the playing board canvas. Clicks and
    /// drags are ignored while a piece is still sliding (the piece
    /// must land before anything else happens — see the invariant).
    fn handle_board(&mut self, msg: BoardMsg, shared: &mut Shared) {
        if self.phase != PracticePhase::Playing {
            return;
        }
        match msg {
            BoardMsg::SquareClicked(sq) => {
                if self.board.is_animating() {
                    return;
                }
                self.click_square(sq, shared);
            }
            BoardMsg::DragStarted(sq) => {
                if self.board.is_animating() {
                    return;
                }
                // Select the dragged piece when it belongs to the
                // side to move (both sides are the user here).
                if self
                    .game
                    .pos
                    .piece_at(sq)
                    .is_some_and(|p| p.color == self.game.pos.side())
                {
                    self.selected = Some(sq);
                }
            }
            BoardMsg::DragMoved(_) => {}
            BoardMsg::DragEnded(point) => {
                if self.board.is_animating() {
                    return;
                }
                // The drop point is canvas-relative (see
                // `BoardProgram`); the canvas is exactly the design
                // size, so a zero-origin geometry maps it.
                let bounds = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: BOARD_WIDTH,
                    height: BOARD_HEIGHT,
                };
                if let Some(sq) = Geom::from_bounds_flipped(bounds, self.flipped).square_at(point) {
                    self.click_square(sq, shared);
                } else {
                    self.selected = None;
                }
            }
            BoardMsg::HoverSquare(_) => {}
        }
    }

    /// A click (or a drop) on `sq`: select one of the side-to-move's
    /// pieces, play a legal move from the selected piece, or clear
    /// the selection.
    fn click_square(&mut self, sq: Square, shared: &mut Shared) {
        let pos = self.game.pos.clone();
        if let Some(from) = self.selected {
            if from != sq {
                let legal: Vec<Move> = pos
                    .legal_moves()
                    .into_iter()
                    .filter(|m| m.from == from && m.to == sq)
                    .collect();
                if let Some(&mv) = legal.first() {
                    self.play_move(mv, shared);
                    return;
                }
            }
        }
        if pos.piece_at(sq).is_some_and(|p| p.color == pos.side()) {
            self.selected = Some(sq);
        } else {
            self.selected = None;
        }
    }

    /// Play one move by the user (both sides are the user). The
    /// piece is placed first and the slide starts; the grade is only
    /// requested once the animation has finished (held in
    /// `pending_grade` — see the invariant at the top of this file).
    /// With animation off, the grade + suggestions start at once.
    fn play_move(&mut self, mv: Move, shared: &mut Shared) {
        if self.phase != PracticePhase::Playing
            || self.game.result() != GameResult::Ongoing
            || self.board.is_animating()
        {
            return;
        }
        let pos_before = self.game.pos.clone();
        if !pos_before.legal_moves().contains(&mv) {
            return;
        }
        let gives_check = pos_before.gives_check(mv);
        let start_fen = self.game.start.to_fen();
        let moves_before = self.game.moves.clone();
        if self.game.play(mv).is_err() {
            return;
        }
        let idx = self.move_grades.len();
        self.move_grades.push(None);
        self.last_move = Some(mv);
        if gives_check {
            self.check_since = Some(Instant::now());
        }
        if shared.settings.animate {
            self.board.start_anim(mv.from, mv.to);
        }
        self.selected = None;
        // Sound for the move just played: move / capture / check /
        // checkmate. Non-blocking and silent when sound is off or no
        // audio device is present.
        sound_for_move(&shared.sound, &pos_before, mv, &self.game.pos);
        self.pending_grade = Some(PendingGrade {
            start_fen,
            moves_before,
            mv,
            idx,
        });
        if !self.board.is_animating() {
            self.flush_pending_grade();
            self.maybe_suggest(shared, Instant::now());
        }
    }

    /// Enter the playing phase from a position (setup sources call this).
    fn start_from(&mut self, pos: Position) {
        self.start_pos = pos.clone();
        self.game = Game::new(pos);
        self.phase = PracticePhase::Playing;
        self.reset_state();
    }

    /// Reset to the starting position (standard or loaded), keeping
    /// the difficulty. Clears the game, grades and suggestions.
    fn reset(&mut self) {
        let start = self.start_pos.clone();
        self.game = Game::new(start);
        self.reset_state();
    }

    /// Clear per-game transient state (grades, suggestions, animation).
    /// The ChessDB panel is left alone: its per-FEN cache still
    /// holds the right evaluation for the starting position, and
    /// `maybe_suggest` rebuilds the suggestion lines from it.
    fn reset_state(&mut self) {
        self.move_grades.clear();
        self.suggest_fen = None;
        self.suggest_lines.clear();
        self.suggest_source = SuggestSource::None;
        self.suggest_req_id = None;
        self.suggest_requested_at = None;
        self.grade_req_id = None;
        self.grade_move_index = None;
        self.grade_requested_at = None;
        self.pending_grade = None;
        self.last_engine_best = None;
        self.last_move = None;
        self.board = BoardState::default();
        self.check_since = None;
        self.selected = None;
        self.hover_suggest = None;
    }

    /// Take back one move (lets the player try a different move from
    /// the same position). Drops the undone move's grade and cancels
    /// any in-flight grade; the suggestions restart for the earlier
    /// position.
    fn undo(&mut self, shared: &Shared) {
        if self.game.undo().is_some() {
            self.move_grades.pop();
            self.last_move = self.game.moves.last().copied();
            self.board = BoardState::default();
            self.check_since = None;
            self.selected = None;
            self.pending_grade = None;
            self.grade_req_id = None;
            self.grade_move_index = None;
            self.grade_requested_at = None;
            self.last_engine_best = None;
            self.maybe_suggest(shared, Instant::now());
        }
    }

    /// Fill the manual board editor from a recognised position (used
    /// when `recognize_board_image` succeeds, so the user can check
    /// and fix the pieces before playing).
    fn editor_from_position(&mut self, pos: &Position) {
        for sq in 0..90u8 {
            self.editor_board[sq as usize] = pos.piece_at(sq);
        }
        self.editor_side = pos.side();
        self.editor_error = None;
    }

    /// Place the palette piece on a square (or erase it). A general
    /// replaces the same side's other general, so the board can never
    /// hold two generals of one colour.
    fn editor_click(&mut self, sq: u8) {
        let Some(piece) = self.editor_selected else {
            self.editor_board[sq as usize] = None;
            return;
        };
        if piece.kind == PieceKind::King {
            for slot in self.editor_board.iter_mut() {
                if let Some(old) = *slot {
                    if old.color == piece.color && old.kind == PieceKind::King {
                        *slot = None;
                    }
                }
            }
        }
        self.editor_board[sq as usize] = Some(piece);
    }
}

impl State {
    /// Setup phase: difficulty + where the position comes from.
    fn view_setup(&self) -> Element<'_, Msg> {
        let mut diff_col = Column::new().spacing(4);
        for d in 0..=4u8 {
            diff_col = diff_col.push(radio(
                difficulty_name(d),
                d,
                Some(self.difficulty),
                Msg::DifficultyChanged,
            ));
        }
        let mut col = column![
            text(i18n::t(Key::PracticeTitle)).size(FONT_SCREEN_TITLE),
            section_title(i18n::t(Key::PracticeDifficulty)),
            diff_col,
        ]
        .spacing(8);
        if self.difficulty == 4 {
            col = col.push(
                row![
                    text(i18n::t(Key::EngineCustomElo)).size(FONT_BODY),
                    slider(1200..=3000u16, self.custom_elo, Msg::CustomEloChanged)
                        .step(50u16)
                        .width(Length::Fixed(220.0)),
                    text(format!("Elo {}", self.custom_elo)).size(FONT_BODY),
                ]
                .spacing(12),
            );
        }
        col = col.push(
            row![
                radio(
                    i18n::t(Key::PracticeStartPos),
                    PracticeSource::Start,
                    Some(self.source),
                    Msg::SourceChanged,
                ),
                radio(
                    i18n::t(Key::PracticeFromFen),
                    PracticeSource::Fen,
                    Some(self.source),
                    Msg::SourceChanged,
                ),
                radio(
                    i18n::t(Key::PracticeFromImage),
                    PracticeSource::Image,
                    Some(self.source),
                    Msg::SourceChanged,
                ),
            ]
            .spacing(16),
        );
        col = col.push(match self.source {
            PracticeSource::Start => {
                column![primary_button(i18n::t(Key::Start), Msg::StartFromStart),]
                    .spacing(8)
                    .into()
            }
            PracticeSource::Fen => self.view_setup_fen(),
            PracticeSource::Image => self.view_setup_image(),
        });
        col = col.push(secondary_button(
            i18n::t(Key::BackToMenu),
            Msg::Navigate(Screen::Home),
        ));
        container(scrollable(col.spacing(12).width(Length::Fill)).width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }

    /// Setup (FEN source): paste a FEN and load it.
    fn view_setup_fen(&self) -> Element<'_, Msg> {
        let mut col = column![
            text(i18n::t(Key::PracticeFenLabel)).size(FONT_BODY),
            text_input(i18n::t(Key::PracticeFenHint), &self.fen_text)
                .on_input(Msg::FenChanged)
                .on_submit(Msg::LoadFen),
            primary_button(i18n::t(Key::PracticeLoadFen), Msg::LoadFen),
        ]
        .spacing(8);
        if let Some(err) = &self.fen_error {
            col = col.push(
                text(err)
                    .size(FONT_BODY)
                    .style(|_: &Theme| text::Style { color: Some(RED) }),
            );
        }
        col.into()
    }

    /// Setup (image source): pick a photo from the device, show its
    /// file name + size as proof of load (the raster picture itself
    /// is not displayed — see the module docs), and set the position
    /// up by hand in the board editor below.
    fn view_setup_image(&self) -> Element<'_, Msg> {
        let mut col = column![primary_button(
            i18n::t(Key::PracticePickImage),
            Msg::PickImage
        ),]
        .spacing(8);
        if let Some(err) = &self.image_error {
            col = col.push(
                text(err)
                    .size(FONT_BODY)
                    .style(|_: &Theme| text::Style { color: Some(RED) }),
            );
        }
        col = col.push(text(i18n::t(Key::PracticeImageNote)).size(FONT_CAPTION));
        col = col.push(section_title(i18n::t(Key::PracticeImageRef)));
        match &self.image_bytes {
            Some(bytes) => {
                col = col.push(
                    text(format!("{} ({} KB)", self.image_path, bytes.len() / 1024))
                        .size(FONT_BODY),
                );
                // Raster photo (feature `image`, enabled at
                // integration): the kept upload bytes, scaled down.
                col = col.push(
                    iced::widget::image(iced::widget::image::Handle::from_bytes(bytes.to_vec()))
                        .width(Length::Fixed(300.0)),
                );
            }
            None => {
                col = col.push(text(i18n::t(Key::PracticeImageNotFound)).size(FONT_BODY));
            }
        }
        col = col.push(self.view_editor());
        col.into()
    }

    /// The board editor section (setup, image source): palette,
    /// board, side to move, and the "use this position" button.
    fn view_editor(&self) -> Element<'_, Msg> {
        let kinds = [
            PieceKind::King,
            PieceKind::Advisor,
            PieceKind::Elephant,
            PieceKind::Horse,
            PieceKind::Rook,
            PieceKind::Cannon,
            PieceKind::Pawn,
        ];
        let mut col = column![
            section_title(i18n::t(Key::PracticeEditorTitle)),
            text(i18n::t(Key::PracticeEditorHint)).size(FONT_BODY),
        ]
        .spacing(8);
        for color in [Color::Red, Color::Black] {
            let mut palette = row![].spacing(4);
            for kind in kinds {
                let piece = Piece::new(color, kind);
                palette = palette.push(palette_button(piece, self.editor_selected == Some(piece)));
            }
            col = col.push(palette);
        }
        let eraser = if self.editor_selected.is_none() {
            Button::new(text(i18n::t(Key::PracticeEraser)).size(FONT_BODY))
                .on_press(Msg::EditorPaletteSelected(None))
                .padding(6)
                .style(theme::primary_button_style)
        } else {
            Button::new(text(i18n::t(Key::PracticeEraser)).size(FONT_BODY))
                .on_press(Msg::EditorPaletteSelected(None))
                .padding(6)
                .style(theme::secondary_button_style)
        };
        col = col.push(eraser);
        col = col.push(
            Canvas::new(EditorProgram {
                board: self.editor_board,
            })
            .width(Length::Fixed(EDITOR_WIDTH))
            .height(Length::Fixed(EDITOR_HEIGHT)),
        );
        col = col.push(
            row![
                text(i18n::t(Key::PracticeSideToMove)).size(FONT_BODY),
                radio(
                    i18n::t(Key::RedSide),
                    Color::Red,
                    Some(self.editor_side),
                    Msg::EditorSideChanged,
                ),
                radio(
                    i18n::t(Key::BlackSide),
                    Color::Black,
                    Some(self.editor_side),
                    Msg::EditorSideChanged,
                ),
                secondary_button(i18n::t(Key::PracticeClearBoard), Msg::EditorClear),
            ]
            .spacing(12),
        );
        col = col.push(primary_button(
            i18n::t(Key::PracticeUsePosition),
            Msg::UseEditorPosition,
        ));
        if let Some(err) = &self.editor_error {
            col = col.push(
                text(err)
                    .size(FONT_BODY)
                    .style(|_: &Theme| text::Style { color: Some(RED) }),
            );
        }
        col.into()
    }

    /// Playing phase: board left (at the window's top-left corner —
    /// see the module docs on the P0b board origin), eval bar to the
    /// right of the board, fixed 380 px panel on the right.
    fn view_playing(&self, shared: &Shared) -> Element<'_, Msg> {
        let pos = self.game.pos.clone();
        let now = Instant::now();
        // Check-halo pulse 0..1 (~2 Hz, from the last checking move).
        let pulse = match self.check_since {
            Some(since) => {
                0.5 + 0.5
                    * (now.duration_since(since).as_secs_f32() * 2.0 * std::f32::consts::PI * 2.0)
                        .sin()
            }
            None => 0.5,
        };
        // Suggestions only belong to the position they were computed
        // for: while a just-played piece is still sliding, the stored
        // lines are for the previous position and must not be drawn
        // (or rendered — `render_move` panics on an empty source
        // square) against the new one.
        let lines_current = self.suggest_fen.as_deref() == Some(&pos.to_fen());
        let legal_targets = match self.selected {
            Some(sq) => pos
                .legal_moves()
                .into_iter()
                .filter(|m| m.from == sq)
                .map(|m| m.to)
                .collect(),
            None => Vec::new(),
        };
        let mut arrows = Vec::new();
        if lines_current {
            for (rank, line) in self.suggest_lines.iter().take(3).enumerate() {
                let mut arrow = Arrow::suggestion(line.mv.from, line.mv.to, rank + 1);
                arrow.highlighted = self.hover_suggest == Some(rank);
                arrows.push(arrow);
            }
        }
        let mut render = BoardRender::new(pos.clone());
        render.selected = self.selected;
        render.legal_targets = legal_targets;
        render.last_move = self.last_move.map(|m| (m.from, m.to));
        render.arrows = arrows;
        render.check_square = if pos.in_check(pos.side()) {
            Some(pos.king_sq(pos.side()))
        } else {
            None
        };
        render.anim = self.board.anim;
        render.pulse = pulse;
        render.flipped = self.flipped;
        let board_el = container(BoardProgram::canvas(render).map(Msg::Board))
            .width(Length::Fixed(BOARD_WIDTH))
            .height(Length::Fixed(BOARD_HEIGHT));
        // Eval bar on the LEFT of the board (egui position; the
        // right-side placement was a workaround for the pre-fix
        // `Geom` origin bug — see the module docs).
        let mut content = row![].spacing(12).align_y(iced::Alignment::Start);
        if let Some(score) = self.score_red() {
            content = content.push(eval_bar(score, BOARD_HEIGHT));
        }
        content = content.push(board_el);
        content = content.push(self.view_panel(shared, &pos, lines_current));
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The right panel (playing phase): status, advantage,
    /// suggestions, last move grade, move list, and the action
    /// buttons. Fixed 380 px wide so the board never flexes.
    fn view_panel(&self, shared: &Shared, pos: &Position, lines_current: bool) -> Element<'_, Msg> {
        let mut col = Column::new().spacing(8);
        // Status: result, check, or whose turn it is.
        let status = if let Some(result) = self.result_text() {
            result
        } else if pos.in_check(pos.side()) {
            format!(
                "{} — {}",
                i18n::t(Key::Check),
                match pos.side() {
                    Color::Red => i18n::t(Key::RedToMove),
                    Color::Black => i18n::t(Key::BlackToMove),
                }
            )
        } else {
            match pos.side() {
                Color::Red => i18n::t(Key::RedToMove).to_string(),
                Color::Black => i18n::t(Key::BlackToMove).to_string(),
            }
        };
        col = col.push(text(status).size(FONT_BODY));
        // Advantage chip (best suggestion / ChessDB score, Red perspective).
        if let Some(score) = self.score_red() {
            col = col.push(
                row![
                    advantage_chip(score),
                    text(i18n::t(Key::AdvantageScore)).size(FONT_BODY),
                ]
                .spacing(12),
            );
        }
        // ChessDB section (score chip + best move; win rate shown —
        // see the module docs).
        col = col.push(self.chessdb.view(pos, &shared.settings).map(|_| Msg::Noop));
        // Suggestions for the side to move.
        col = col.push(section_title(i18n::t(Key::PracticeSuggestions)));
        let source_label = match self.suggest_source {
            SuggestSource::ChessDb => i18n::t(Key::PracticeSuggestChessDb),
            SuggestSource::Engine => i18n::t(Key::PracticeSuggestEngine),
            SuggestSource::None => "",
        };
        if !source_label.is_empty() && lines_current {
            col = col.push(text(source_label).size(FONT_BODY));
        }
        if !lines_current || self.suggest_lines.is_empty() {
            if self.game.result() == GameResult::Ongoing {
                if self.suggest_req_id.is_some()
                    || matches!(self.chessdb.view_state(), ChessDbView::Loading)
                {
                    col = col.push(text(i18n::t(Key::PracticeThinking)).size(FONT_BODY));
                } else {
                    col = col.push(text(i18n::t(Key::PracticeNoSuggestions)).size(FONT_BODY));
                }
            }
        } else {
            for (rank, line) in self.suggest_lines.iter().enumerate() {
                // `lines_current` (checked above) guarantees the move
                // is legal in this position, so rendering cannot panic.
                let name = render_move(pos, line.mv, shared.settings.notation);
                let mut label = format!(
                    "{}. {} ({})",
                    rank + 1,
                    name,
                    format_advantage(line.score_cp_red)
                );
                if let Some(w) = line.winrate {
                    label.push_str(&format!(" — W{w:.0}%"));
                }
                col = col.push(
                    mouse_area(text(label).size(FONT_BODY))
                        .on_enter(Msg::HoverSuggest(Some(rank)))
                        .on_exit(Msg::HoverSuggest(None)),
                );
            }
        }
        // Grade of the last played move.
        if let Some(&mv) = self.game.moves.last() {
            let idx = self.game.moves.len() - 1;
            let pos_before = position_at(&self.game, idx);
            let name = render_move(&pos_before, mv, shared.settings.notation);
            let grade_text = match self.move_grades.get(idx).copied().flatten() {
                Some(g) => i18n::t(grade_key(g)).to_string(),
                None => i18n::t(Key::PracticeThinking).to_string(),
            };
            col = col.push(section_title(i18n::t(Key::PracticeLastMove)));
            col = col.push(text(format!("{name} — {grade_text}")).size(FONT_BODY));
            if let Some(best) = self.last_engine_best {
                let best_name = if pos_before.legal_moves().contains(&best) {
                    render_move(&pos_before, best, shared.settings.notation)
                } else {
                    notation::iccs(best)
                };
                col = col.push(
                    text(i18n::t(Key::ChessDbBestMove).replace("{name}", &best_name))
                        .size(FONT_BODY),
                );
            }
        }
        // Move list (with grades), scrollable.
        col = col.push(section_title(i18n::t(Key::MovesTitle)));
        let mut list = Column::new().spacing(4);
        let mut list_pos = self.game.start.clone();
        let mut row_text = String::new();
        for (i, m) in self.game.moves.iter().enumerate() {
            let name = render_move(&list_pos, *m, shared.settings.notation);
            let grade = self
                .move_grades
                .get(i)
                .copied()
                .flatten()
                .map(|g| i18n::t(grade_key(g)).to_string())
                .unwrap_or_default();
            list_pos.make_move(*m);
            if i % 2 == 0 {
                row_text = format!("{}. {name}", i / 2 + 1);
                if !grade.is_empty() {
                    row_text.push_str(&format!(" ({grade})"));
                }
            } else {
                row_text.push_str(&format!("  {name}"));
                if !grade.is_empty() {
                    row_text.push_str(&format!(" ({grade})"));
                }
                list = list.push(text(std::mem::take(&mut row_text)).size(FONT_BODY));
            }
        }
        if !row_text.is_empty() {
            list = list.push(text(row_text).size(FONT_BODY));
        }
        col = col.push(
            scrollable(list)
                .height(Length::Fixed(200.0))
                .width(Length::Fill),
        );
        // Actions: reset (keeps the difficulty), undo, flip, change position.
        col = col.push(
            row![
                primary_button(i18n::t(Key::NewGame), Msg::Reset),
                secondary_button(i18n::t(Key::Undo), Msg::Undo),
                secondary_button(i18n::t(Key::FlipBoard), Msg::FlipBoard),
            ]
            .spacing(12),
        );
        col = col.push(
            row![
                secondary_button(i18n::t(Key::PracticeBackToSetup), Msg::BackToSetup),
                secondary_button(i18n::t(Key::BackToMenu), Msg::Navigate(Screen::Home)),
            ]
            .spacing(12),
        );
        container(scrollable(col.width(Length::Fill)).width(Length::Fill))
            .width(Length::Fixed(PANEL_WIDTH))
            .height(Length::Fill)
            .padding(12)
            .style(theme::card_container_style)
            .into()
    }

    /// Render the active phase.
    pub fn view(&self, shared: &Shared) -> Element<'_, Msg> {
        match self.phase {
            PracticePhase::Setup => self.view_setup(),
            PracticePhase::Playing => self.view_playing(shared),
        }
    }

    /// Subscriptions: the ChessDB bridge, the AI bridge (Pikafish
    /// fallback + grades), and — only while a piece is sliding or
    /// the check halo is fresh — the animation tick. Nothing else
    /// ticks: with no animation and no halo there is nothing to
    /// animate, and worker answers arrive as pushed messages.
    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            self.chessdb
                .subscription()
                .map(|panel_msg| match panel_msg {
                    ChessDbPanelMsg::Response(ChessDbResponse::Evaluated { req_id, result }) => {
                        Msg::ChessDbEvaluated { req_id, result }
                    }
                }),
            bridge::from_mpsc("practice-ai", Arc::clone(&self.ai_rx)).filter_map(|resp| {
                match resp {
                    AiResponse::Analysis { req_id, lines } => {
                        Some(Msg::AiAnalysis { req_id, lines })
                    }
                    AiResponse::Graded {
                        req_id,
                        grade,
                        best,
                        ..
                    } => Some(Msg::AiGraded {
                        req_id,
                        grade,
                        best,
                    }),
                    // `Info` progress lines are not shown on this screen.
                    _ => None,
                }
            }),
        ];
        let ticking = self.board.is_animating()
            || self
                .check_since
                .is_some_and(|s| s.elapsed() < Duration::from_secs(1));
        if ticking {
            subs.push(iced::time::every(TICK_INTERVAL).map(Msg::Tick));
        }
        Subscription::batch(subs)
    }
}

/// One palette button: the piece's Han character in its colour,
/// gold when it is the selected palette piece.
fn palette_button(piece: Piece, selected: bool) -> Button<'static, Msg> {
    let color = match piece.color {
        Color::Red => RED_PIECE,
        Color::Black => BLACK_PIECE,
    };
    let label = text(piece.han_char().to_string())
        .size(22.0)
        .font(theme::cjk_font())
        .style(move |_: &Theme| text::Style { color: Some(color) });
    let btn = Button::new(label)
        .on_press(Msg::EditorPaletteSelected(Some(piece)))
        .padding(6)
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(40.0));
    if selected {
        btn.style(theme::primary_button_style)
    } else {
        btn.style(theme::secondary_button_style)
    }
}

/// The editor board canvas: the grid + the pieces placed so far.
/// Clicking a square places the palette piece (or erases with the
/// eraser). Unlike P0b's `board::Geom::from_bounds`, the geometry
/// here is origin-relative in BOTH `draw` and `update` — the canvas
/// widget translates the frame by the bounds origin when drawing,
/// and `cursor.position_in` is origin-relative — so this canvas
/// works wherever it is placed (see the module docs).
struct EditorProgram {
    board: [Option<Piece>; 90],
}

impl Program<Msg> for EditorProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let g = Geom::from_bounds(Rectangle {
            x: 0.0,
            y: 0.0,
            width: bounds.width,
            height: bounds.height,
        });
        let scale = g.scale;
        // Wooden background + frame.
        frame.fill(&Path::rectangle(Point::ORIGIN, bounds.size()), theme::BG);
        let board_pos = Point::new(g.bx, g.by);
        let board_size = Size::new(BOARD_WIDTH * scale, BOARD_HEIGHT * scale);
        frame.fill(&Path::rectangle(board_pos, board_size), WOOD);
        frame.stroke(
            &Path::rectangle(board_pos, board_size),
            Stroke::default()
                .with_color(BOARD_LINE)
                .with_width(2.0 * scale),
        );
        // Grid: horizontals, then verticals (border files cross the
        // river, the rest stop at it).
        let line = Stroke::default()
            .with_color(BOARD_LINE)
            .with_width(1.5 * scale);
        for rank in 0..10 {
            frame.stroke(&Path::line(g.point(0, rank), g.point(8, rank)), line);
        }
        for file in 0..9 {
            if file == 0 || file == 8 {
                frame.stroke(&Path::line(g.point(file, 0), g.point(file, 9)), line);
            } else {
                frame.stroke(&Path::line(g.point(file, 5), g.point(file, 9)), line);
                frame.stroke(&Path::line(g.point(file, 0), g.point(file, 4)), line);
            }
        }
        // Palace diagonals.
        for (f0, r0, f1, r1) in [(3, 0, 5, 2), (5, 0, 3, 2), (3, 7, 5, 9), (5, 7, 3, 9)] {
            frame.stroke(&Path::line(g.point(f0, r0), g.point(f1, r1)), line);
        }
        // Pieces.
        for sq in 0..90u8 {
            if let Some(piece) = self.board[sq as usize] {
                draw_editor_piece(&mut frame, g.square_point(sq), scale, piece);
            }
        }
        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Msg>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let g = Geom::from_bounds(Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: bounds.width,
                    height: bounds.height,
                });
                let sq = g.square_at(pos)?;
                Some(Action::publish(Msg::EditorSquareClicked(sq)))
            }
            _ => None,
        }
    }
}

/// Draw one editor piece: outer disc, inner ring, Han character.
fn draw_editor_piece(frame: &mut Frame<Renderer>, center: Point, scale: f32, piece: Piece) {
    let color = match piece.color {
        Color::Red => RED_PIECE,
        Color::Black => BLACK_PIECE,
    };
    frame.fill(&Path::circle(center, PIECE_RADIUS * scale), PIECE_BG);
    frame.stroke(
        &Path::circle(center, PIECE_RADIUS * scale),
        Stroke::default().with_color(color).with_width(2.0 * scale),
    );
    frame.stroke(
        &Path::circle(center, PIECE_INNER_RADIUS * scale),
        Stroke::default().with_color(color).with_width(1.0 * scale),
    );
    frame.fill_text(CanvasText {
        content: piece.han_char().to_string(),
        position: center,
        size: Pixels(FONT_PIECE * scale),
        color,
        font: theme::cjk_font(),
        align_x: TextAlignment::Center,
        align_y: Vertical::Center,
        ..Default::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::board::START_FEN;

    fn startpos() -> Position {
        Position::from_fen(START_FEN).expect("start FEN")
    }

    #[test]
    fn editor_fen_round_trips_startpos() {
        let pos = startpos();
        let mut board = [None; 90];
        for sq in 0..90u8 {
            board[sq as usize] = pos.piece_at(sq);
        }
        let fen = editor_fen(&board, Color::Red);
        let back = Position::from_fen(&fen).expect("editor FEN parses");
        assert_eq!(back, pos);
    }

    #[test]
    fn editor_fen_empty_board() {
        let board = [None; 90];
        assert_eq!(editor_fen(&board, Color::Black), "9/9/9/9/9/9/9/9/9/9 b");
        assert_eq!(editor_fen(&board, Color::Red), "9/9/9/9/9/9/9/9/9/9 w");
    }

    #[test]
    fn generals_facing_is_detected() {
        let mut board = [None; 90];
        // Red general file 4 rank 0 (sq 4), Black general file 4 rank 9 (sq 85).
        board[4] = Some(Piece::new(Color::Red, PieceKind::King));
        board[85] = Some(Piece::new(Color::Black, PieceKind::King));
        assert!(editor_generals_face(&board));
        // A piece between them breaks the face-off.
        board[40] = Some(Piece::new(Color::Red, PieceKind::Pawn));
        assert!(!editor_generals_face(&board));
        // Different files: no face-off.
        board[40] = None;
        board[85] = None;
        board[86] = Some(Piece::new(Color::Black, PieceKind::King));
        assert!(!editor_generals_face(&board));
    }

    #[test]
    fn recognize_board_image_always_fails_without_a_model() {
        assert!(recognize_board_image(&[0u8; 16]).is_err());
        assert!(recognize_board_image(&[]).is_err());
    }

    #[test]
    fn grade_keys_cover_every_grade() {
        for (grade, key) in [
            (Grade::Book, Key::PracticeGradeBest),
            (Grade::Best, Key::PracticeGradeBest),
            (Grade::Good, Key::PracticeGradeGood),
            (Grade::Acceptable, Key::PracticeGradeAcceptable),
            (Grade::Inaccurate, Key::PracticeGradeInaccurate),
            (Grade::Blunder, Key::PracticeGradeBlunder),
        ] {
            assert_eq!(grade_key(grade), key);
        }
    }

    #[test]
    fn editor_click_places_erases_and_dedupes_generals() {
        let shared = Shared::new();
        let mut state = State::new(&shared);
        state.editor_selected = Some(Piece::new(Color::Red, PieceKind::Rook));
        state.editor_click(0);
        assert_eq!(
            state.editor_board[0],
            Some(Piece::new(Color::Red, PieceKind::Rook))
        );
        state.editor_selected = None;
        state.editor_click(0);
        assert_eq!(state.editor_board[0], None);
        // Two red generals: placing the second removes the first.
        state.editor_selected = Some(Piece::new(Color::Red, PieceKind::King));
        state.editor_click(4);
        state.editor_click(5);
        assert_eq!(state.editor_board[4], None);
        assert_eq!(
            state.editor_board[5],
            Some(Piece::new(Color::Red, PieceKind::King))
        );
    }

    #[test]
    fn new_state_starts_in_setup_with_settings_difficulty() {
        let shared = Shared::new();
        let state = State::new(&shared);
        assert_eq!(state.phase, PracticePhase::Setup);
        assert_eq!(state.source, PracticeSource::Start);
        assert_eq!(state.difficulty, shared.settings.difficulty);
        assert_eq!(state.custom_elo, shared.settings.custom_elo);
        assert!(state.game.moves.is_empty());
        assert!(!state.board.is_animating());
    }
}
