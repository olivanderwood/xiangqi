//! Self practice mode (Hieu, 2026-11-12): the player plays BOTH sides
//! (Red and Black) against no AI, with a suggestion + a grade after
//! every move.
//!
//! Flow:
//! 1. Setup: pick a difficulty for Pikafish (0 Easy / 1 Medium / 2 Hard
//!    / 3 Master / 4 Custom Elo — the same scale as the app), then pick
//!    a position: the standard start, a FEN, or a board photo.
//! 2. Playing: after every position, ChessDB scores it and suggests the
//!    next moves for the side to move; when ChessDB has no data for the
//!    position, Pikafish (`Analyze`, top 4) computes the suggestions
//!    instead. The player may play ANY legal move — including one outside
//!    the suggestions — and the app grades it (engine `GradeMove`, the
//!    same rule as continue-from-learn) and suggests again from the new
//!    position.
//! 3. Reset returns to the starting position (standard or loaded),
//!    keeping the difficulty. Undo takes back one move.
//!
//! Design: this screen is SELF-CONTAINED. It owns all its state
//! (`PracticeScreen`) and receives the app handles it needs through
//! `PracticeHandles` — it does NOT reach into `XiangqiApp` fields, so
//! it compiles and can be reviewed without any other file changing.
//! The ChessDB view state (`chessdb_view` / `req_id` / `last_fen`) is
//! kept locally here (mirroring `XiangqiApp::poll_chessdb`); the
//! `ChessDbWorker` thread itself is shared with the other screens.
//!
//! WIRING (for the integrator):
//! - `screens/mod.rs`: add `pub mod practice;`
//! - `app.rs`, `Screen`: add a `Practice` variant.
//! - `app.rs`, `XiangqiApp`: add a `pub practice: PracticeScreen`
//!   field, initialised in `new()` with
//!   `PracticeScreen::new(settings.difficulty, settings.custom_elo)`.
//! - `app.rs`, `update()`: add
//!   ```rust
//!   Screen::Practice => {
//!       let mut h = PracticeHandles {
//!           worker: &mut self.worker,
//!           chessdb: &mut self.chessdb,
//!           pending_ai: &mut self.pending_ai,
//!           engine: &self.engine,
//!           settings: &self.settings,
//!       };
//!       if self.practice.show(ctx, ui, &mut h) == PracticeAction::BackToMenu {
//!           self.goto(Screen::Menu);
//!       }
//!   }
//!   ```
//!   to the screen match, AND add `self.practice.suggest_req_id` and
//!   `self.practice.grade_req_id` to the `wanted` array — otherwise
//!   `update()` drops the engine responses this screen is waiting for
//!   (`AiResponse::Analysis` / `AiResponse::Graded`) before it can read
//!   them. (ChessDB answers travel on their own channel and are not
//!   affected.)
//! - `screens/menu.rs`: add a button with `i18n::t(Key::PracticeTitle)`
//!   that calls `self.goto(Screen::Practice)`.
//!
//! LIMITS (Hieu, 2026-11-12):
//! - Photo upload: the file is read and kept (`image_bytes`) and shown
//!   with `egui::Image::from_bytes`, but egui 0.31 ships NO raster image
//!   decoder — displaying a PNG/JPEG needs `egui_extras` (or the `image`
//!   crate) plus `egui_extras::install_image_loaders(ctx)` at startup.
//!   Until that dependency is added the photo panel shows the file name
//!   and a note instead of the picture. The manual board editor below it
//!   is therefore the way to build the position from a photo for now:
//!   pick a piece, tap a square to place it, tap a placed piece to
//!   remove it, then "Use this position".
//! - Automatic board recognition from a photo needs a separate vision
//!   model and is deliberately NOT done here.
//! - The engine fallback and the move grades need the `wanted` wiring
//!   above; without it only the ChessDB suggestions work.

use std::sync::Arc;

use crate::ai_worker::{response_is_current, AiRequest, AiResponse};
use crate::app::{
    advantage_chip_ui, big_button, eval_bar_ui, format_advantage, is_narrow, section_title,
};
use crate::board_view::{
    draw_board, legal_targets, BoardDraw, BoardGeometry, BoardInteraction, HintArrow, MoveAnim,
    BLACK_PIECE, LINE_COLOR, PIECE_BG, RED_PIECE, WOOD,
};
use crate::chessdb_worker::{
    response_is_current as chessdb_is_current, ChessDbResponse, ChessDbView, ChessDbWorker,
};
use crate::i18n::{self, Key};
use crate::settings::{NotationChoice, Settings};
use crate::sound::{sound_for_move, SharedSound};
use xq_core::analysis::Grade;
use xq_core::board::Position;
use xq_core::game::{Game, GameResult, Reason};
use xq_core::notation;
use xq_core::types::{file_of, rank_of, sq, Color, Move, Piece, PieceKind};
use xq_engine::{EngineRequest, Level, SharedEngine};

/// How long an engine suggestion / grade may take before the screen
/// stops waiting and shows "no suggestions" / leaves the move ungraded
/// (the worker may have failed silently; the game itself never waits).
const ENGINE_TIMEOUT_S: f64 = 30.0;

/// How many suggested moves to show (ChessDB top 4 / Pikafish top 4).
const SUGGEST_TOP_N: usize = 4;

/// What the practice screen wants the app to do after this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PracticeAction {
    /// Stay on the practice screen.
    None,
    /// Go back to the main menu.
    BackToMenu,
}

/// The app handles the practice screen needs, passed in by the caller
/// (see the wiring note at the top of this file). All of them are
/// disjoint fields of `XiangqiApp`, so the caller can build this from
/// `&mut self` without conflicting with the `&mut self.practice`
/// borrow.
pub struct PracticeHandles<'a> {
    /// The AI worker (sends `Analyze` / `GradeMove`, allocates req ids).
    pub worker: &'a mut crate::ai_worker::AiWorker,
    /// The shared ChessDB worker thread.
    pub chessdb: &'a mut ChessDbWorker,
    /// The central buffer of worker responses (this screen takes its own).
    pub pending_ai: &'a mut Vec<AiResponse>,
    /// The shared engine (only `try_lock`ed, never blocked on).
    pub engine: &'a SharedEngine,
    /// Sound effects (shared with the app; `try_lock`ed, never blocking).
    pub sfx: &'a SharedSound,
    /// App settings (animation on/off, move notation).
    pub settings: &'a Settings,
}

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

/// Try to read a Xiangqi position from a board photo (`bytes` are the
/// image file contents). Returns the position on success, or an error
/// when the photo cannot be recognised.
///
/// There is NO vision model in the app yet, so this always fails for
/// now and the caller reports "image could not be recognised" and
/// falls back to the manual board editor. When a recognition model is
/// added (a CV model or an on-device ML model), implement it here:
/// decode the image, find the board grid, classify the piece on each
/// square, and build the `Position` from the resulting board.
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

/// One suggested move, normalised to a Red-perspective centipawn score
/// so ChessDB lines and engine lines display the same way.
#[derive(Clone, Copy, Debug)]
pub struct SuggestLine {
    pub mv: Move,
    /// Centipawn score from Red's perspective.
    pub score_cp_red: i32,
    /// Red win rate in percent (ChessDB only).
    pub winrate: Option<f32>,
}

/// Self-contained state of the practice screen (owned by
/// `XiangqiApp::practice` once wired; see the wiring note above).
pub struct PracticeScreen {
    pub phase: PracticePhase,
    /// Difficulty index (0..=4) for Pikafish, same scale as the app.
    pub difficulty: u8,
    /// Custom Elo when `difficulty == 4`.
    pub custom_elo: u16,
    /// Which setup source tab is open.
    pub source: PracticeSource,
    /// The game being played (both sides by the user).
    pub game: Game,
    /// The position the game started from (Reset returns to it).
    pub start_pos: Position,
    /// FEN text field (setup, FEN source).
    pub fen_text: String,
    /// FEN error message (setup).
    pub fen_error: Option<String>,
    /// Image path text field (setup, image source).
    pub image_path: String,
    /// Loaded photo bytes (shown once an image decoder is installed).
    pub image_bytes: Option<Arc<[u8]>>,
    /// Counter so each loaded photo gets a fresh image URI.
    pub image_version: u32,
    /// Image load error (setup).
    pub image_error: Option<String>,
    /// Board editor pieces (setup, image source): one slot per square.
    pub editor_board: [Option<Piece>; 90],
    /// Editor palette selection: `Some(piece)` places it, `None` erases.
    pub editor_selected: Option<Piece>,
    /// Side to move of the editor-built position.
    pub editor_side: Color,
    /// Editor error (setup).
    pub editor_error: Option<String>,
    /// Board click/drag state (playing).
    pub board: BoardInteraction,
    pub flipped: bool,
    pub anim: Option<MoveAnim>,
    pub last_move: Option<Move>,
    pub check_since: Option<f64>,
    /// FEN the current suggestions were requested for.
    pub suggest_fen: Option<String>,
    pub suggest_source: SuggestSource,
    pub suggest_lines: Vec<SuggestLine>,
    /// `req_id` of the in-flight engine `Analyze` (suggestion fallback).
    pub suggest_req_id: Option<u64>,
    pub suggest_requested_at: Option<f64>,
    /// `req_id` of the in-flight `GradeMove` for the last played move.
    pub grade_req_id: Option<u64>,
    /// Index into `game.moves` / `move_grades` the grade is for.
    pub grade_move_index: Option<usize>,
    pub grade_requested_at: Option<f64>,
    /// The engine's best move at the last graded position (shown next
    /// to the grade, so the user sees what was better).
    pub last_engine_best: Option<Move>,
    /// Grade per played move, parallel to `game.moves` (`None` = the
    /// grade has not arrived yet).
    pub move_grades: Vec<Option<Grade>>,
    /// ChessDB view for the current position (local copy of the
    /// app-level ChessDB section state; see the design note above).
    pub chessdb_view: ChessDbView,
    /// `req_id` of the ChessDB evaluation being waited for.
    pub chessdb_req_id: Option<u64>,
    /// FEN of the last position a ChessDB evaluation was requested for.
    pub chessdb_last_fen: Option<String>,
}

impl PracticeScreen {
    /// Fresh screen, in the setup phase. The difficulty defaults come
    /// from the app settings (the caller passes them); the setup screen
    /// lets the user change the difficulty without touching the global
    /// Vs-AI difficulty.
    #[must_use]
    pub fn new(difficulty: u8, custom_elo: u16) -> Self {
        Self {
            phase: PracticePhase::Setup,
            difficulty,
            custom_elo,
            source: PracticeSource::Start,
            game: Game::new(Position::startpos()),
            start_pos: Position::startpos(),
            fen_text: String::new(),
            fen_error: None,
            image_path: String::new(),
            image_bytes: None,
            image_version: 0,
            image_error: None,
            editor_board: [None; 90],
            editor_selected: None,
            editor_side: Color::Red,
            editor_error: None,
            board: BoardInteraction {
                selected: None,
                drag_sq: None,
                drag_pos: egui::Pos2::ZERO,
                drag_moved: false,
                hover_sq: None,
            },
            flipped: false,
            anim: None,
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
            chessdb_view: ChessDbView::Idle,
            chessdb_req_id: None,
            chessdb_last_fen: None,
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

    /// Render a move in the user's notation (same rule as
    /// `XiangqiApp::render_move`).
    fn render_move(&self, pos: &Position, mv: Move, settings: &Settings) -> String {
        match settings.notation {
            NotationChoice::Vietnamese => notation::render_vn(pos, mv),
            NotationChoice::English => notation::render_en(pos, mv),
        }
    }

    /// Best available advantage score (Red perspective) for the eval bar:
    /// the best suggestion, else the ChessDB score, else `None`.
    fn score_red(&self) -> Option<i32> {
        if let Some(best) = self.suggest_lines.first() {
            return Some(best.score_cp_red);
        }
        if let ChessDbView::Ready(eval) = &self.chessdb_view {
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

impl PracticeScreen {
    /// Render one frame. Returns what the app should do afterwards
    /// (`BackToMenu` when the user pressed the back button).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        h: &mut PracticeHandles,
    ) -> PracticeAction {
        match self.phase {
            PracticePhase::Setup => self.show_setup(ctx, ui),
            PracticePhase::Playing => self.show_playing(ctx, ui, h),
        }
    }

    /// Setup phase: difficulty + where the position comes from.
    fn show_setup(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> PracticeAction {
        ui.vertical_centered(|ui| {
            ui.heading(i18n::t(Key::PracticeTitle));
        });
        ui.add_space(12.0);

        // Difficulty for Pikafish (same scale as the app).
        section_title(ui, i18n::t(Key::PracticeDifficulty));
        for d in 0..=4u8 {
            ui.radio_value(&mut self.difficulty, d, i18n::difficulty_name(d));
        }
        if self.difficulty == 4 {
            ui.horizontal(|ui| {
                ui.label(i18n::t(Key::EngineCustomElo));
                ui.add(egui::Slider::new(&mut self.custom_elo, 1200..=3000));
                ui.label(format!("Elo {}", self.custom_elo));
            });
        }
        ui.add_space(12.0);

        // Position source tabs.
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.source,
                PracticeSource::Start,
                i18n::t(Key::PracticeStartPos),
            );
            ui.radio_value(
                &mut self.source,
                PracticeSource::Fen,
                i18n::t(Key::PracticeFromFen),
            );
            ui.radio_value(
                &mut self.source,
                PracticeSource::Image,
                i18n::t(Key::PracticeFromImage),
            );
        });
        ui.add_space(8.0);

        match self.source {
            PracticeSource::Start => {
                if big_button(ui, i18n::t(Key::Start)).clicked() {
                    self.start_from(Position::startpos());
                }
            }
            PracticeSource::Fen => self.setup_fen(ui),
            PracticeSource::Image => self.setup_image(ctx, ui),
        }
        ui.add_space(16.0);
        if big_button(ui, i18n::t(Key::BackToMenu)).clicked() {
            return PracticeAction::BackToMenu;
        }
        PracticeAction::None
    }

    /// Setup (FEN source): paste a FEN and load it.
    fn setup_fen(&mut self, ui: &mut egui::Ui) {
        ui.label(i18n::t(Key::PracticeFenLabel));
        ui.add(
            egui::TextEdit::singleline(&mut self.fen_text)
                .hint_text(i18n::t(Key::PracticeFenHint))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);
        if big_button(ui, i18n::t(Key::PracticeLoadFen)).clicked() {
            match Position::from_fen(self.fen_text.trim()) {
                Ok(pos) => {
                    self.fen_error = None;
                    self.start_from(pos);
                }
                Err(_) => {
                    self.fen_error = Some(i18n::t(Key::PracticeFenInvalid).to_string());
                }
            }
        }
        if let Some(err) = &self.fen_error {
            ui.colored_label(egui::Color32::from_rgb(0xC0, 0x39, 0x2B), err);
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

    /// Setup (image source): pick a photo from the device, show it as
    /// a reference, and set the position up by hand in the board
    /// editor. The app tries to recognise the board from the photo;
    /// when it cannot (there is no vision model yet — see the limits
    /// note at the top of this file) it says so plainly instead of
    /// guessing a wrong position.
    fn setup_image(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if big_button(ui, i18n::t(Key::PracticePickImage)).clicked() {
            // Native file dialog (rfd): pick an image from the device.
            // `pick_file` is synchronous on desktop; it returns None
            // when the user cancels.
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "webp"])
                .pick_file()
            {
                self.image_path = path.display().to_string();
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        self.image_version += 1;
                        let uri = format!("bytes://practice-image-{}", self.image_version);
                        let shared: Arc<[u8]> = Arc::from(bytes);
                        ctx.include_bytes(uri, Arc::clone(&shared));
                        self.image_bytes = Some(Arc::clone(&shared));
                        // Try to read a position from the photo. There
                        // is no vision model yet, so this reports that
                        // the photo could not be recognised; the user
                        // sets the position up by hand below.
                        match recognize_board_image(&shared) {
                            Ok(pos) => {
                                self.image_error = None;
                                self.editor_from_position(&pos);
                            }
                            Err(_) => {
                                self.image_error =
                                    Some(i18n::t(Key::PracticeImageNotRecognized).to_string());
                            }
                        }
                    }
                    Err(_) => {
                        self.image_bytes = None;
                        self.image_error = Some(i18n::t(Key::PracticeImageNotFound).to_string());
                    }
                }
            }
        }
        if let Some(err) = &self.image_error {
            ui.colored_label(egui::Color32::from_rgb(0xC0, 0x39, 0x2B), err);
        }
        ui.add_space(8.0);
        // The photo is only a reference; the app cannot read a position
        // from it yet (see the limits note at the top of this file).
        ui.label(i18n::t(Key::PracticeImageNote));
        ui.add_space(8.0);

        let narrow = is_narrow(ctx);
        if narrow {
            ui.vertical(|ui| {
                self.image_ref(ui);
                ui.add_space(8.0);
                self.setup_editor(ui);
            });
        } else {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(320.0);
                    self.image_ref(ui);
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    self.setup_editor(ui);
                });
            });
        }
    }

    /// The loaded photo, as a reference next to the editor. Without an
    /// image decoder installed this shows the file name + the note
    /// (see the limits note at the top of this file).
    fn image_ref(&mut self, ui: &mut egui::Ui) {
        section_title(ui, i18n::t(Key::PracticeImageRef));
        if let Some(bytes) = &self.image_bytes {
            let uri = format!("bytes://practice-image-{}", self.image_version);
            let size = egui::Vec2::new(300.0, 300.0);
            ui.add(egui::Image::from_bytes(uri, Arc::clone(bytes)).fit_to_exact_size(size));
            ui.label(self.image_path.trim().to_string());
        } else {
            ui.label(i18n::t(Key::PracticeImageNotFound));
        }
    }

    /// The board editor section (setup, image source): palette, board,
    /// side to move, and the "use this position" button.
    fn setup_editor(&mut self, ui: &mut egui::Ui) {
        section_title(ui, i18n::t(Key::PracticeEditorTitle));
        ui.label(i18n::t(Key::PracticeEditorHint));
        self.editor_palette(ui);
        self.editor_board(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(i18n::t(Key::PracticeSideToMove));
            ui.radio_value(&mut self.editor_side, Color::Red, i18n::t(Key::RedSide));
            ui.radio_value(&mut self.editor_side, Color::Black, i18n::t(Key::BlackSide));
            if big_button(ui, i18n::t(Key::PracticeClearBoard)).clicked() {
                self.editor_board = [None; 90];
                self.editor_error = None;
            }
        });
        ui.add_space(8.0);
        if big_button(ui, i18n::t(Key::PracticeUsePosition)).clicked() {
            let fen = editor_fen(&self.editor_board, self.editor_side);
            if editor_generals_face(&self.editor_board) {
                self.editor_error = Some(i18n::t(Key::PracticeEditorInvalid).to_string());
            } else {
                match Position::from_fen(&fen) {
                    Ok(pos) => {
                        self.editor_error = None;
                        self.start_from(pos);
                    }
                    Err(_) => {
                        self.editor_error = Some(i18n::t(Key::PracticeEditorInvalid).to_string());
                    }
                }
            }
        }
        if let Some(err) = &self.editor_error {
            ui.colored_label(egui::Color32::from_rgb(0xC0, 0x39, 0x2B), err);
        }
    }

    /// The editor palette: one row of Red pieces, one row of Black
    /// pieces, plus the eraser. The buttons show the piece glyphs
    /// (the same han characters as on the board).
    fn editor_palette(&mut self, ui: &mut egui::Ui) {
        let kinds = [
            PieceKind::King,
            PieceKind::Advisor,
            PieceKind::Elephant,
            PieceKind::Horse,
            PieceKind::Rook,
            PieceKind::Cannon,
            PieceKind::Pawn,
        ];
        for color in [Color::Red, Color::Black] {
            ui.horizontal(|ui| {
                for kind in kinds {
                    let piece = Piece::new(color, kind);
                    let selected = self.editor_selected == Some(piece);
                    let text = egui::RichText::new(piece.han_char().to_string())
                        .size(22.0)
                        .color(match color {
                            Color::Red => RED_PIECE,
                            Color::Black => BLACK_PIECE,
                        });
                    let resp = ui.add_sized(
                        egui::Vec2::new(36.0, 36.0),
                        egui::Button::new(text).selected(selected),
                    );
                    if resp.clicked() {
                        self.editor_selected = Some(piece);
                    }
                }
                let eraser_selected = self.editor_selected.is_none();
                if ui
                    .add_sized(
                        egui::Vec2::new(52.0, 36.0),
                        egui::Button::new(i18n::t(Key::PracticeEraser)).selected(eraser_selected),
                    )
                    .clicked()
                {
                    self.editor_selected = None;
                }
            });
        }
    }

    /// The editor board: the grid + the pieces placed so far. Clicking a
    /// square places the palette piece (or erases with the eraser).
    fn editor_board(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let cell = (avail.x / 10.0).min(300.0 / 11.0);
        let size = egui::Vec2::new(10.0 * cell, 11.0 * cell);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let geom = BoardGeometry::new(size, self.flipped);
        let painter = ui.painter_at(rect);
        draw_editor_board(&painter, rect, &geom, &self.editor_board);
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if let Some(sq) = geom.from_screen(p - rect.min.to_vec2()) {
                    self.editor_click(sq);
                }
            }
        }
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

impl PracticeScreen {
    /// Playing phase: both sides are the user; every position gets a
    /// suggestion and every played move gets a grade.
    fn show_playing(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        h: &mut PracticeHandles,
    ) -> PracticeAction {
        let now = ctx.input(|i| i.time);
        // 1. Drain the worker's answers (non-blocking).
        self.poll(ctx, h, now);
        // Animation first, calculations after (Hieu, 2026-11-12): while
        // a just-played piece is still sliding, start NOTHING — no
        // ChessDB request, no engine analysis. Draining responses above
        // stays ungated: it only reads results already computed.
        let animating = self.anim.is_some_and(|a| a.progress(now).is_some());
        if !animating {
            self.maybe_suggest(ctx, h, now);
        }
        // Keep repainting while a move slides or the check flash is fresh.
        if self.anim.is_some_and(|a| a.progress(now).is_some())
            || self.check_since.is_some_and(|s| now - s < 1.0)
        {
            ctx.request_repaint();
        }
        // Keyboard shortcuts.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.board.clear();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.flipped = !self.flipped;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.command) {
            self.undo();
        }

        // Layout: board (+eval bar) left, panel right; panel below when narrow.
        // NOTE: the board size MUST be computed from the CentralPanel's
        // available size captured here (see vs_ai.rs: inside `ui.horizontal`
        // egui only offers the current row height, which shrinks the board).
        let avail = ui.available_size();
        let narrow = is_narrow(ctx);
        let mut action = PracticeAction::None;
        if narrow {
            ui.vertical(|ui| {
                self.show_board(ctx, ui, avail, h);
                ui.add_space(8.0);
                action = self.show_panel(ctx, ui, h);
            });
        } else {
            ui.horizontal(|ui| {
                if let Some(score_red) = self.score_red() {
                    eval_bar_ui(ui, score_red, avail.y);
                }
                // Board budget: full height, width left after eval bar (20),
                // gap (12), panel (380) and row spacing.
                let budget = egui::Vec2::new(avail.x - 20.0 - 12.0 - 380.0 - 24.0, avail.y);
                self.show_board(ctx, ui, budget, h);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    action = self.show_panel(ctx, ui, h);
                });
            });
        }
        action
    }

    /// Take this screen's responses out of the shared buffer; responses
    /// for the other screens stay buffered (see `XiangqiApp::pending_ai`).
    fn poll(&mut self, ctx: &egui::Context, h: &mut PracticeHandles, now: f64) {
        let mut i = 0;
        while i < h.pending_ai.len() {
            let mine = match &h.pending_ai[i] {
                AiResponse::Analysis { req_id, .. } => {
                    response_is_current(*req_id, self.suggest_req_id)
                }
                AiResponse::Graded { req_id, .. } => {
                    response_is_current(*req_id, self.grade_req_id)
                }
                _ => false,
            };
            if !mine {
                i += 1;
                continue;
            }
            let resp = h.pending_ai.remove(i);
            match resp {
                AiResponse::Analysis { lines, .. } => {
                    self.suggest_req_id = None;
                    self.suggest_requested_at = None;
                    // The lines are for the position the request was
                    // sent for; if the user has moved on, the FEN
                    // changed and these lines are stale — drop them.
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
                    ctx.request_repaint();
                }
                AiResponse::Graded { grade, best, .. } => {
                    self.grade_req_id = None;
                    self.grade_requested_at = None;
                    if let Some(idx) = self.grade_move_index.take() {
                        if idx < self.move_grades.len() {
                            self.move_grades[idx] = Some(grade);
                        }
                    }
                    self.last_engine_best = best;
                    ctx.request_repaint();
                }
                _ => {}
            }
        }
        // Engine timeouts: stop waiting so the UI shows "no
        // suggestions" / leaves the move ungraded instead of spinning
        // forever when the worker failed silently.
        if let Some(at) = self.suggest_requested_at {
            if self.suggest_req_id.is_some() && now - at > ENGINE_TIMEOUT_S {
                self.suggest_req_id = None;
                self.suggest_requested_at = None;
            }
        }
        if let Some(at) = self.grade_requested_at {
            if self.grade_req_id.is_some() && now - at > ENGINE_TIMEOUT_S {
                self.grade_req_id = None;
                self.grade_requested_at = None;
                self.grade_move_index = None;
            }
        }
    }

    /// Drain ChessDB responses and (re)request when the position changed.
    /// Mirrors `XiangqiApp::poll_chessdb`, but keeps the view state
    /// locally so this screen stays self-contained.
    fn poll_chessdb(&mut self, fen: &str, ctx: &egui::Context, h: &mut PracticeHandles) {
        while let Ok(resp) = h.chessdb.rx.try_recv() {
            let ChessDbResponse::Evaluated { req_id, result } = resp;
            if !chessdb_is_current(req_id, self.chessdb_req_id) {
                continue;
            }
            self.chessdb_req_id = None;
            self.chessdb_view = match result {
                Ok(eval) => {
                    if eval.top.is_empty() {
                        ChessDbView::Unavailable(i18n::t(Key::ChessDbNoData).to_string())
                    } else {
                        ChessDbView::Ready(eval)
                    }
                }
                Err(err) => ChessDbView::Unavailable(err.to_string()),
            };
            ctx.request_repaint();
        }
        if self.chessdb_last_fen.as_deref() != Some(fen) {
            self.chessdb_last_fen = Some(fen.to_string());
            let req_id = h.chessdb.request(fen.to_string());
            self.chessdb_req_id = Some(req_id);
            self.chessdb_view = ChessDbView::Loading;
        }
    }

    /// Ask for suggestions for the current position: ChessDB first (the
    /// shared worker), Pikafish `Analyze` when ChessDB has no data. One
    /// request per position; the FEN change resets the state.
    fn maybe_suggest(&mut self, ctx: &egui::Context, h: &mut PracticeHandles, now: f64) {
        if self.phase != PracticePhase::Playing || self.game.result() != GameResult::Ongoing {
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
        // ChessDB first (drains its responses into `self.chessdb_view`
        // and requests the new FEN when it changed).
        self.poll_chessdb(&fen, ctx, h);
        match self.chessdb_view.clone() {
            ChessDbView::Ready(eval) => {
                if !eval.top.is_empty() {
                    self.suggest_source = SuggestSource::ChessDb;
                    self.suggest_req_id = None;
                    self.suggest_requested_at = None;
                    self.suggest_lines = eval
                        .top
                        .iter()
                        .take(SUGGEST_TOP_N)
                        .filter_map(|m| {
                            notation::parse_iccs(&m.mv).ok().map(|mv| SuggestLine {
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
                    self.request_analyze(h, now);
                }
            }
            ChessDbView::Loading | ChessDbView::Idle => {}
        }
    }

    /// Send the Pikafish `Analyze` (top 4) for the current position.
    /// Never blocks the UI on the engine: if a search is running, the
    /// request is skipped and the next frame retries.
    fn request_analyze(&mut self, h: &mut PracticeHandles, now: f64) {
        let Some(engine) = h.engine.try_lock() else {
            return;
        };
        let limits = engine.limits_for(self.level());
        drop(engine);
        let req_id = h.worker.alloc_req_id();
        self.suggest_req_id = Some(req_id);
        self.suggest_requested_at = Some(now);
        let _ = h.worker.tx.send(AiRequest::Analyze {
            req: EngineRequest::new(self.game.start.to_fen(), self.game.moves.clone(), limits),
            top_n: SUGGEST_TOP_N,
            req_id,
        });
    }

    /// Play one move by the user (both sides are the user). The piece is
    /// placed first; the grade is requested on the worker and arrives
    /// later as a `Graded` response.
    fn play(&mut self, mv: Move, now: f64, animate: bool, h: &mut PracticeHandles) {
        let start_fen = self.game.start.to_fen();
        let moves_before = self.game.moves.clone();
        let pos_before = self.game.pos.clone();
        let piece = pos_before.piece_at(mv.from);
        let gives_check = pos_before.gives_check(mv);
        if self.game.play(mv).is_err() {
            return;
        }
        let idx = self.move_grades.len();
        self.move_grades.push(None);
        self.last_move = Some(mv);
        self.anim = if animate {
            piece.map(|p| MoveAnim::new(mv, p, now, 0.15))
        } else {
            None
        };
        if gives_check {
            self.check_since = Some(now);
        }
        self.board.clear();
        // Sound for the move just played (Hieu, 2026-11-12): move /
        // capture / check / checkmate. Non-blocking and silent when
        // sound is off or no audio device is present.
        sound_for_move(h.sfx, &pos_before, mv, &self.game.pos);
        // Grade the move on the worker (async). The worker builds the
        // request itself, so the UI never locks the engine here.
        let req_id = h.worker.alloc_req_id();
        self.grade_req_id = Some(req_id);
        self.grade_move_index = Some(idx);
        self.grade_requested_at = Some(now);
        let _ = h.worker.tx.send(AiRequest::GradeMove {
            start_fen,
            moves: moves_before,
            mv,
            req_id,
        });
    }

    /// Enter the playing phase from a position (setup sources call this).
    fn start_from(&mut self, pos: Position) {
        self.start_pos = pos.clone();
        self.game = Game::new(pos);
        self.phase = PracticePhase::Playing;
        self.reset_state();
    }

    /// Reset to the starting position (standard or loaded), keeping the
    /// difficulty. Clears the game, grades and suggestions.
    fn reset(&mut self) {
        let start = self.start_pos.clone();
        self.game = Game::new(start);
        self.reset_state();
    }

    /// Clear per-game transient state (grades, suggestions, animation).
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
        self.last_engine_best = None;
        self.last_move = None;
        self.anim = None;
        self.check_since = None;
        self.board.clear();
    }

    /// Take back one move (lets the player try a different move from the
    /// same position). Drops the undone move's grade and cancels any
    /// in-flight grade; the suggestions restart for the earlier position.
    fn undo(&mut self) {
        if self.game.undo().is_some() {
            self.move_grades.pop();
            self.last_move = self.game.moves.last().copied();
            self.anim = None;
            self.check_since = None;
            self.board.clear();
            self.grade_req_id = None;
            self.grade_move_index = None;
            self.grade_requested_at = None;
            self.last_engine_best = None;
        }
    }

    /// The board (playing phase): same interaction as Vs AI, except both
    /// sides are movable (the user plays Red and Black).
    fn show_board(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        avail: egui::Vec2,
        h: &mut PracticeHandles,
    ) {
        let now = ctx.input(|i| i.time);
        let total = avail;
        let cell = (total.x / 10.0).min(total.y / 11.0);
        let size = egui::Vec2::new(10.0 * cell, 11.0 * cell);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let geom = BoardGeometry::new(size, self.flipped);

        let display_pos = self.game.pos.clone();
        let animating = self.anim.is_some_and(|a| a.progress(now).is_some());
        let board_enabled = self.game.result() == GameResult::Ongoing && !animating;
        let movable = display_pos.side();
        let selected = self.board.selected;
        let targets = selected
            .map(|sq| legal_targets(&display_pos, sq))
            .unwrap_or_default();
        let check_sq = if display_pos.in_check(display_pos.side()) {
            Some(display_pos.king_sq(display_pos.side()))
        } else {
            None
        };
        let drag = self.board.drag_sq.map(|sq| (sq, self.board.drag_pos));
        // Arrows for the top suggested moves (best first).
        let arrows: Vec<HintArrow> = self
            .suggest_lines
            .iter()
            .take(3)
            .enumerate()
            .map(|(rank, line)| {
                let horse = display_pos
                    .piece_at(line.mv.from)
                    .is_some_and(|p| p.kind == PieceKind::Horse);
                HintArrow::for_rank(line.mv.from, line.mv.to, rank, horse)
            })
            .collect();
        let draw = BoardDraw {
            selected,
            targets: &targets,
            last_move: self.last_move,
            check_sq,
            check_since: self.check_since,
            now,
            arrows: &arrows,
            drag,
            anim: self.anim,
            hover_sq: self.board.hover_sq,
        };
        let painter = ui.painter_at(rect);
        draw_board(&painter, rect, &geom, &display_pos, &draw);

        if board_enabled {
            if let Some(mv) = self
                .board
                .handle(&resp, rect, &geom, &self.game.pos, movable)
            {
                let animate = h.settings.animate;
                self.play(mv, now, animate, h);
            }
        }
    }

    /// The right panel (playing phase): status, advantage, suggestions,
    /// last move grade, move list, and the action buttons.
    fn show_panel(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        h: &mut PracticeHandles,
    ) -> PracticeAction {
        // Status: result, check, or whose turn it is.
        if let Some(result) = self.result_text() {
            ui.label(egui::RichText::new(result).strong());
        } else if self.game.pos.in_check(self.game.pos.side()) {
            ui.label(egui::RichText::new(format!(
                "{} — {}",
                i18n::t(Key::Check),
                match self.game.pos.side() {
                    Color::Red => i18n::t(Key::RedToMove),
                    Color::Black => i18n::t(Key::BlackToMove),
                }
            )));
        } else {
            ui.label(egui::RichText::new(match self.game.pos.side() {
                Color::Red => i18n::t(Key::RedToMove),
                Color::Black => i18n::t(Key::BlackToMove),
            }));
        }
        // Advantage chip (best suggestion / ChessDB score, Red perspective).
        if let Some(score_red) = self.score_red() {
            ui.horizontal(|ui| {
                advantage_chip_ui(ui, score_red);
                ui.label(i18n::t(Key::AdvantageScore));
            });
        }
        ui.add_space(8.0);

        // Suggestions for the side to move.
        section_title(ui, i18n::t(Key::PracticeSuggestions));
        let source_label = match self.suggest_source {
            SuggestSource::ChessDb => i18n::t(Key::PracticeSuggestChessDb),
            SuggestSource::Engine => i18n::t(Key::PracticeSuggestEngine),
            SuggestSource::None => "",
        };
        if !source_label.is_empty() {
            ui.label(egui::RichText::new(source_label).strong());
        }
        if self.suggest_lines.is_empty() {
            if self.game.result() == GameResult::Ongoing {
                if self.suggest_req_id.is_some()
                    || matches!(self.chessdb_view, ChessDbView::Loading)
                {
                    ui.label(i18n::t(Key::PracticeThinking));
                } else {
                    ui.label(i18n::t(Key::PracticeNoSuggestions));
                }
            }
        } else {
            let pos = self.game.pos.clone();
            for (rank, line) in self.suggest_lines.iter().enumerate() {
                let name = self.render_move(&pos, line.mv, h.settings);
                let mut text = format!(
                    "{}. {} ({})",
                    rank + 1,
                    name,
                    format_advantage(line.score_cp_red)
                );
                if let Some(w) = line.winrate {
                    text.push_str(&format!(" — W{w:.0}%"));
                }
                let label = if rank == 0 {
                    egui::RichText::new(text).strong()
                } else {
                    egui::RichText::new(text)
                };
                ui.label(label);
            }
        }
        ui.add_space(8.0);

        // Grade of the last played move.
        if let Some(&mv) = self.game.moves.last() {
            let idx = self.game.moves.len() - 1;
            let mut pos_before = self.game.start.clone();
            for m in self.game.moves.iter().take(idx) {
                pos_before.make_move(*m);
            }
            let name = self.render_move(&pos_before, mv, h.settings);
            let grade = self.move_grades.get(idx).copied().flatten();
            let grade_text = match grade {
                Some(g) => i18n::t(grade_key(g)).to_string(),
                None => i18n::t(Key::PracticeThinking).to_string(),
            };
            section_title(ui, i18n::t(Key::PracticeLastMove));
            ui.label(format!("{name} — {grade_text}"));
            if let Some(best) = self.last_engine_best {
                let best_name = self.render_move(&pos_before, best, h.settings);
                ui.label(i18n::t(Key::ChessDbBestMove).replace("{name}", &best_name));
            }
        }
        ui.add_space(8.0);

        // Move list (with grades), scrollable.
        section_title(ui, i18n::t(Key::MovesTitle));
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                let mut pos = self.game.start.clone();
                let mut row = String::new();
                for (i, m) in self.game.moves.iter().enumerate() {
                    let name = self.render_move(&pos, *m, h.settings);
                    let grade = self
                        .move_grades
                        .get(i)
                        .copied()
                        .flatten()
                        .map(|g| i18n::t(grade_key(g)).to_string())
                        .unwrap_or_default();
                    pos.make_move(*m);
                    if i % 2 == 0 {
                        row = format!("{}. {name}", i / 2 + 1);
                        if !grade.is_empty() {
                            row.push_str(&format!(" ({grade})"));
                        }
                    } else {
                        row.push_str(&format!("  {name}"));
                        if !grade.is_empty() {
                            row.push_str(&format!(" ({grade})"));
                        }
                        ui.label(row.clone());
                        row.clear();
                    }
                }
                if !row.is_empty() {
                    ui.label(row);
                }
            });
        ui.add_space(8.0);

        // Actions: reset (keeps the difficulty), undo, flip, change position.
        ui.horizontal(|ui| {
            if big_button(ui, i18n::t(Key::NewGame)).clicked() {
                self.reset();
            }
            if big_button(ui, i18n::t(Key::Undo)).clicked() {
                self.undo();
            }
        });
        ui.horizontal(|ui| {
            if big_button(ui, i18n::t(Key::FlipBoard)).clicked() {
                self.flipped = !self.flipped;
            }
            if big_button(ui, i18n::t(Key::PracticeBackToSetup)).clicked() {
                self.phase = PracticePhase::Setup;
            }
        });
        if big_button(ui, i18n::t(Key::BackToMenu)).clicked() {
            return PracticeAction::BackToMenu;
        }
        let _ = ctx;
        PracticeAction::None
    }
}

/// Draw the editor board: the grid + the pieces placed so far. The
/// editor state is a plain piece array (it can be incomplete mid-edit),
/// so it cannot be a `Position` and `draw_board` cannot draw it — this
/// mirrors the grid of `draw_board` in a compact form (no river text,
/// no column labels).
fn draw_editor_board(
    painter: &egui::Painter,
    rect: egui::Rect,
    geom: &BoardGeometry,
    board: &[Option<Piece>; 90],
) {
    use egui::{Align2, Color32, Pos2, Stroke, Vec2};
    let cell = geom.cell;
    let sq_at = |file: i32, rank: i32| -> Pos2 {
        let f = file as f32;
        let r = rank as f32;
        let (f, r) = if geom.flipped {
            (8.0 - f, 9.0 - r)
        } else {
            (f, r)
        };
        Pos2::new(
            rect.min.x + geom.origin.x + f * cell,
            rect.min.y + geom.origin.y + (9.0 - r) * cell,
        )
    };
    // Wooden background + frame.
    let board_rect = egui::Rect::from_min_size(rect.min, geom.board_size());
    painter.rect_filled(board_rect, egui::CornerRadius::ZERO, WOOD);
    painter.rect_stroke(
        board_rect,
        egui::CornerRadius::ZERO,
        Stroke::new(3.0_f32, LINE_COLOR),
        egui::StrokeKind::Inside,
    );
    // Grid: verticals (border files cross the river, others stop at it).
    for f in 0..9 {
        let x0 = sq_at(f, 0).x;
        let y_top = sq_at(f, 9).y;
        let y_bot = sq_at(f, 0).y;
        if f == 0 || f == 8 {
            painter.line_segment(
                [Pos2::new(x0, y_top), Pos2::new(x0, y_bot)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
        } else {
            let y_river_top = sq_at(f, 5).y;
            let y_river_bot = sq_at(f, 4).y;
            painter.line_segment(
                [Pos2::new(x0, y_top), Pos2::new(x0, y_river_top)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
            painter.line_segment(
                [Pos2::new(x0, y_river_bot), Pos2::new(x0, y_bot)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
        }
    }
    // Horizontals.
    for r in 0..10 {
        let y = sq_at(0, r).y;
        painter.line_segment(
            [Pos2::new(sq_at(0, r).x, y), Pos2::new(sq_at(8, r).x, y)],
            Stroke::new(1.5_f32, LINE_COLOR),
        );
    }
    // Palace diagonals.
    for (f0, r0, f1, r1) in [(3, 0, 5, 2), (5, 0, 3, 2), (3, 7, 5, 9), (5, 7, 3, 9)] {
        painter.line_segment(
            [sq_at(f0, r0), sq_at(f1, r1)],
            Stroke::new(1.5_f32, LINE_COLOR),
        );
    }
    // Pieces.
    let radius = 0.44 * cell;
    for (i, slot) in board.iter().enumerate() {
        let Some(piece) = slot else {
            continue;
        };
        let s = i as u8;
        let center = geom.to_screen(s) + rect.min.to_vec2();
        let color = match piece.color {
            Color::Red => RED_PIECE,
            Color::Black => BLACK_PIECE,
        };
        painter.circle_filled(
            center + Vec2::new(2.0, 2.0),
            radius,
            Color32::from_rgba_unmultiplied(0, 0, 0, 64),
        );
        painter.circle_filled(center, radius, PIECE_BG);
        painter.circle_stroke(center, radius, Stroke::new(2.0_f32, color));
        painter.text(
            center,
            Align2::CENTER_CENTER,
            piece.han_char().to_string(),
            egui::FontId::new(1.1 * radius, egui::FontFamily::Name("cjk-bold".into())),
            color,
        );
    }
}
