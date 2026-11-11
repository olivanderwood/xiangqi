//! Engine boundary for Xiangqi AI (Pikafish integration spec §1).
//!
//! Everything above this crate talks to an AI only through the [`Engine`]
//! trait. Two implementations exist: [`BuiltinEngine`] (the hand-written
//! alpha-beta searcher from `xq-core`, kept as fallback) and `PikafishEngine`
//! (`xq-pikafish`, a UCI subprocess). The rest of the app — `ai_worker`,
//! `Tutor`, the screens — must never touch `Searcher` directly.

mod builtin;

pub use builtin::BuiltinEngine;
pub use xq_core::analysis::GradeThresholds;
pub use xq_core::search::SearchLimits;

use std::fmt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use xq_core::board::Position;
use xq_core::eval::MATE;
use xq_core::types::Move;

/// A search score: centipawns, or mate in `n` moves (`n > 0`: the side to
/// move mates in `n` moves; `n < 0`: the side to move is mated in `|n|`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

impl Score {
    /// Convert a builtin `i32` score (mate encoded as `MATE - plies`) to a
    /// `Score`. Mirrors the mate handling in `Searcher::analyze`.
    #[must_use]
    pub fn from_cp(score: i32) -> Self {
        if score >= MATE - 500 {
            let plies = MATE - score;
            Score::Mate(((plies + 1) / 2).max(1))
        } else if score <= -(MATE - 500) {
            let plies = MATE + score;
            Score::Mate(-((plies + 1) / 2).max(1))
        } else {
            Score::Cp(score)
        }
    }

    /// Centipawn value for comparisons/grading: `Mate(n)` maps to
    /// `±(30000 − |n|)` so a missed mate always grades as a blunder.
    #[must_use]
    pub fn to_cp_clamped(self) -> i32 {
        match self {
            Score::Cp(cp) => cp,
            Score::Mate(n) if n >= 0 => MATE - n,
            Score::Mate(n) => -(MATE + n),
        }
    }

    /// Negate a score (used when a search is run from the child position:
    /// the score from the opponent's point of view becomes ours).
    #[must_use]
    pub fn negated(self) -> Self {
        match self {
            Score::Cp(cp) => Score::Cp(-cp),
            Score::Mate(n) => Score::Mate(-n),
        }
    }
}

/// One analysed line: a move, its score, the depth reached, and the
/// principal variation starting with that move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub mv: Move,
    pub score: Score,
    pub depth: u8,
    pub pv: Vec<Move>,
}

/// Progress info streamed while a search runs.
#[derive(Clone, Debug)]
pub struct SearchInfo {
    pub depth: u8,
    pub seldepth: u8,
    pub nodes: u64,
    pub nps: u64,
    /// Best line per MultiPV slot, best first (usually one line).
    pub lines: Vec<Line>,
}

/// Weakening for a level: pick among the top lines instead of always
/// the best one. The real Pikafish binary has no strength option, so
/// low levels are weakened this way (the old builtin used score noise
/// for the same purpose). `margin_cp` bounds how much worse a picked
/// line may be; `temperature` flattens the weights
/// (`w = exp((score − best) / temperature)`): a higher temperature
/// picks bad moves more often.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weaken {
    pub margin_cp: i32,
    pub temperature: f64,
}

/// Difficulty level, shared by both engines. `Elo(x)` is only honoured by
/// Pikafish (as a depth/movetime cap — the real binary has no `UCI_Elo`
/// option); the builtin engine maps it to the nearest classic level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Easy,
    Medium,
    Hard,
    Master,
    Elo(u16),
}

impl Level {
    /// Limits for one builtin move at this level (classic §4.4 table).
    #[must_use]
    pub fn builtin_limits(self) -> SearchLimits {
        use xq_core::search::Difficulty;
        let d = match self {
            Level::Easy => Difficulty::Easy,
            Level::Medium => Difficulty::Medium,
            Level::Hard => Difficulty::Hard,
            Level::Master => Difficulty::Master,
            Level::Elo(x) => {
                if x < 1600 {
                    Difficulty::Easy
                } else if x < 2100 {
                    Difficulty::Medium
                } else if x < 2600 {
                    Difficulty::Hard
                } else {
                    Difficulty::Master
                }
            }
        };
        d.limits()
    }
}

/// A request to an engine: the start position plus the whole move list
/// from it (never just the current FEN — the engine needs the full game
/// for repetition / perpetual-check detection).
#[derive(Clone, Debug)]
pub struct EngineRequest {
    pub start_fen: String,
    pub moves: Vec<Move>,
    pub limits: SearchLimits,
}

impl EngineRequest {
    /// Build from a `Game`-like state: start FEN + moves played so far.
    #[must_use]
    pub fn new(start_fen: String, moves: Vec<Move>, limits: SearchLimits) -> Self {
        Self {
            start_fen,
            moves,
            limits,
        }
    }

    /// Replay the request into the current position, checking every move
    /// is legal. Returns the position plus the hash history (start position
    /// first) the builtin searcher needs for repetition detection.
    pub fn position_with_history(&self) -> Result<(Position, Vec<u64>), EngineError> {
        let mut pos = Position::from_fen(&self.start_fen)
            .map_err(|e| EngineError::Protocol(format!("FEN không hợp lệ: {e}")))?;
        let mut history = vec![pos.hash()];
        for &mv in &self.moves {
            if !pos.legal_moves().contains(&mv) {
                return Err(EngineError::IllegalMove(format!(
                    "nước {} không hợp lệ ở thế {}",
                    xq_core::notation::iccs(mv),
                    pos.to_fen()
                )));
            }
            pos.make_move(mv);
            history.push(pos.hash());
        }
        Ok((pos, history))
    }

    /// Current position only (same validation as [`Self::position_with_history`]).
    pub fn position(&self) -> Result<Position, EngineError> {
        Ok(self.position_with_history()?.0)
    }
}

/// A stop request that can be triggered from another thread. For the
/// builtin engine this is the exact flag the search loop polls; for
/// Pikafish the engine polls it while waiting for `bestmove` and sends
/// `stop` when it is set.
#[derive(Clone, Debug)]
pub struct StopHandle {
    flag: Arc<AtomicBool>,
}

impl StopHandle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }

    pub fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn flag(&self) -> &Arc<AtomicBool> {
        &self.flag
    }
}

impl Default for StopHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum EngineError {
    NotFound,
    Spawn(String),
    Handshake(String),
    Timeout(&'static str),
    Crashed(Option<i32>),
    IllegalMove(String),
    Protocol(String),
    Stopped,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::NotFound => write!(f, "không tìm thấy engine"),
            EngineError::Spawn(e) => write!(f, "không khởi chạy được engine: {e}"),
            EngineError::Handshake(e) => write!(f, "bắt tay UCI thất bại: {e}"),
            EngineError::Timeout(what) => write!(f, "hết thời gian chờ: {what}"),
            EngineError::Crashed(code) => write!(f, "engine chết giữa chừng: {code:?}"),
            EngineError::IllegalMove(m) => write!(f, "engine trả nước không hợp lệ: {m}"),
            EngineError::Protocol(m) => write!(f, "lỗi giao thức engine: {m}"),
            EngineError::Stopped => write!(f, "tìm kiếm đã bị dừng"),
        }
    }
}

impl std::error::Error for EngineError {}

/// The single boundary between the app and any AI (spec §1.2).
pub trait Engine: Send {
    /// Human-readable name, e.g. `"Pikafish 2026-09-06"` or `"Builtin"`.
    fn name(&self) -> &str;
    /// Start a new game (resets repetition state / transposition table).
    fn new_game(&mut self) -> Result<(), EngineError>;
    /// Set the difficulty level (applies to the next search at the latest).
    fn set_level(&mut self, level: Level) -> Result<(), EngineError>;
    /// Best move for the request, streaming progress through `on_info`.
    fn best_move(
        &mut self,
        req: &EngineRequest,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Line, EngineError>;
    /// Top-`top_n` lines, best first.
    fn analyze(
        &mut self,
        req: &EngineRequest,
        top_n: usize,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Vec<Line>, EngineError>;
    /// Score of one specific move, from the side-to-move's perspective.
    fn score_move(&mut self, req: &EngineRequest, mv: Move) -> Result<Score, EngineError>;
    /// Handle to stop a running search from another thread.
    fn stop_handle(&self) -> StopHandle;
    /// Shut the engine down (quit a subprocess). Called on app exit;
    /// the default does nothing (in-process engines need no shutdown).
    fn shutdown(&mut self) {}
    /// Use the shared stop flag instead of the engine's own one. Called
    /// when the engine is installed behind a `SharedEngine` (and again
    /// after every swap). The default does nothing: engines that never
    /// poll a flag (mocks) do not need it.
    fn set_stop_flag(&mut self, _flag: &Arc<AtomicBool>) {}
    /// Current difficulty level (so a fallback engine can inherit it).
    fn level(&self) -> Level;
    /// Grading thresholds that match this engine's score scale.
    fn grade_thresholds(&self) -> GradeThresholds {
        GradeThresholds::BUILTIN
    }
    /// Search limits for one move at `level`, in this engine's own
    /// scale. The caller asks the ACTIVE engine (Pikafish limits differ
    /// from builtin limits at the same level).
    fn limits_for(&self, level: Level) -> SearchLimits {
        level.builtin_limits()
    }
    /// Weakening for `level`, if this engine weakens by picking among
    /// the top lines instead of always the best one. `None` means the
    /// engine always plays its best move at the level's limits.
    fn weaken(&self, _level: Level) -> Option<Weaken> {
        None
    }
    /// Whether this engine supports the Master diversity pick (§5).
    /// Only Pikafish advertises it; the builtin always plays its best
    /// move at Master.
    fn supports_diversity(&self) -> bool {
        false
    }
    /// Limits for grading one learner move (best move + score move).
    /// Kept short: grading runs synchronously on the UI thread.
    fn grade_limits(&self) -> SearchLimits {
        SearchLimits::depth(6)
    }
    /// Limits for hint analysis (`analyze`, top 3).
    fn hint_limits(&self) -> SearchLimits {
        SearchLimits::depth(4)
    }
}

/// One engine shared by the worker thread and the UI thread's synchronous
/// grading paths, behind a mutex so searches run strictly one at a time
/// (spec §6: Learn mode calls the engine sequentially). Both the worker
/// and the grading callers hold a clone of this handle; the engine itself
/// lives behind it.
///
/// The stop flag lives HERE, not in the engine: engines are swapped
/// behind this handle (Pikafish → Builtin on crash, Builtin → Pikafish
/// at startup), and the flag the worker sets must be the flag the
/// current engine polls. Every engine behind the handle is given this
/// flag via [`Engine::set_stop_flag`] when it is installed.
#[derive(Clone)]
pub struct SharedEngine {
    inner: Arc<Mutex<Box<dyn Engine>>>,
    stop: Arc<AtomicBool>,
}

impl SharedEngine {
    /// Lock the engine for one call.
    pub fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Box<dyn Engine>>> {
        self.inner.lock()
    }

    /// Handle to the shared stop flag.
    #[must_use]
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle::from_flag(Arc::clone(&self.stop))
    }

    /// The shared stop flag itself (for installing a new engine).
    #[must_use]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }
}

/// Wrap an engine for sharing between threads.
#[must_use]
pub fn shared_engine(mut engine: Box<dyn Engine>) -> SharedEngine {
    let stop = Arc::new(AtomicBool::new(false));
    engine.set_stop_flag(&stop);
    SharedEngine {
        inner: Arc::new(Mutex::new(engine)),
        stop,
    }
}
