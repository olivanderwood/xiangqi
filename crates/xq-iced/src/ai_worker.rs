//! AI worker thread (§2): requests/responses with `req_id` (M3/T3.3).
//!
//! Design notes (see `docs/DECISIONS-m3.md` and the Pikafish integration
//! spec §1.3):
//! - The worker does NOT own the engine: the app shares one engine
//!   (`xq-engine::SharedEngine`) between this thread and the UI thread's
//!   synchronous grading paths, behind a mutex so searches run strictly
//!   one at a time.
//! - The worker holds the engine's [`StopHandle`], which is the exact flag
//!   a running search polls (builtin) or the flag the engine's wait loop
//!   polls to send `stop` (Pikafish). Setting the flag therefore takes
//!   effect immediately even while a search is running — sending
//!   `AiRequest::Stop` alone would queue behind the running search and
//!   could not interrupt it.
//! - The UI keeps the `req_id` of the request it is currently waiting for
//!   and drops every response whose `req_id` does not match
//!   (`response_is_current`). This is what makes Undo-while-thinking safe.

#[cfg(target_arch = "wasm32")]
use crate::engine_wasm::{WasmEngine, WasmEngineError, WasmEngineEvent};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use xq_core::analysis::tags_with_mate;
#[cfg(target_arch = "wasm32")]
use xq_core::analysis::GradeThresholds;
use xq_core::analysis::{explain_tags, grade_from_delta_with, tags_for, Grade};
use xq_core::search::AnalysisLine;
#[cfg(target_arch = "wasm32")]
use xq_engine::SearchLimits;
use xq_engine::{
    BuiltinEngine, Engine, EngineError, EngineRequest, Line, SharedEngine, StopHandle,
};

#[derive(Debug)]
pub enum AiRequest {
    BestMove {
        req: EngineRequest,
        req_id: u64,
    },
    Analyze {
        req: EngineRequest,
        top_n: usize,
        req_id: u64,
    },
    /// Grade one move the human just played (continue-from-learn mode):
    /// the engine's best move + the played move's score at the position
    /// before it, turned into a grade. Runs on the worker thread so the
    /// UI never blocks on it (Hieu, 2026-11-12) — the piece is placed
    /// first and the grade arrives later. The request carries the start
    /// FEN + the moves before the played move (not a ready
    /// `EngineRequest`), so the UI never locks the engine to build it:
    /// the worker builds the request with `grade_limits()` once it
    /// holds the engine (Hieu, 2026-11-12 — a lock in `play()` froze
    /// the board while a hint analysis was running).
    GradeMove {
        start_fen: String,
        moves: Vec<xq_core::types::Move>,
        mv: xq_core::types::Move,
        req_id: u64,
    },
    Stop,
}

/// Responses from the worker. Fields mirror the spec (§2) API; not every
/// field is consumed by the current UI (e.g. `Info.score` is shown only
/// via depth), so dead-code lints are silenced for this enum.
#[allow(dead_code)]
#[derive(Debug)]
pub enum AiResponse {
    Info {
        req_id: u64,
        depth: u8,
        score: i32,
        pv: Vec<xq_core::types::Move>,
    },
    BestMove {
        req_id: u64,
        mv: xq_core::types::Move,
        score: i32,
    },
    Analysis {
        req_id: u64,
        lines: Vec<AnalysisLine>,
    },
    /// The grade for a `GradeMove` request: the grade itself, the
    /// engine's best move at that position (for "nước đúng"), the
    /// played move's search score from the mover's perspective
    /// (`played_score_cp`, centipawns — the UI turns it into the
    /// post-move position value for the advantage chip), and the
    /// explanation.
    Graded {
        req_id: u64,
        grade: Grade,
        best: Option<xq_core::types::Move>,
        played_score_cp: i32,
        explanation: String,
    },
}

impl AiResponse {
    /// `req_id` carried by this response.
    #[allow(dead_code)] // used by tests; the UI matches on the field directly
    #[must_use]
    pub fn req_id(&self) -> u64 {
        match self {
            AiResponse::Info { req_id, .. }
            | AiResponse::BestMove { req_id, .. }
            | AiResponse::Analysis { req_id, .. }
            | AiResponse::Graded { req_id, .. } => *req_id,
        }
    }
}

/// A response is current only if its `req_id` is the one the UI is still
/// waiting for. Anything else (e.g. a search overtaken by Undo) is stale
/// and must be ignored.
#[must_use]
pub fn response_is_current(resp_req_id: u64, current: Option<u64>) -> bool {
    Some(resp_req_id) == current
}

/// Convert an engine [`Line`] to the UI's [`AnalysisLine`], adding the
/// rule-based tags (plus `MateIn`) computed from the position.
fn to_analysis_line(pos: &xq_core::board::Position, line: &Line) -> AnalysisLine {
    AnalysisLine {
        mv: line.mv,
        score: line.score.to_cp_clamped(),
        depth: line.depth,
        pv: line.pv.clone(),
        tags: tags_with_mate(pos, line.mv, line.score.to_cp_clamped()),
    }
}

/// A fixed worker thread driving one shared engine.
/// (On wasm the screens use [`WasmAiWorker`] instead, so this
/// struct is unconstructed there.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct AiWorker {
    pub tx: Sender<AiRequest>,
    pub rx: Receiver<AiResponse>,
    /// Stop handle of the shared engine (see module docs).
    pub stop: StopHandle,
    pub next_req_id: u64,
    /// Set once when the worker falls back from Pikafish to the builtin
    /// engine; the UI shows it as a banner and clears it by taking it.
    // NOTE (integration): production screens detect the fallback via
    // the shared engine's name (`screens/play.rs::check_engine_fallback`)
    // after destructuring the worker, so nothing in the app calls
    // `take_fallback_notice()` — the notice channel is exercised by
    // the worker's unit tests below. Keep it: it is the documented
    // fallback-notification contract, not dead weight.
    #[allow(dead_code)]
    fallback_notice: std::sync::Arc<Mutex<Option<String>>>,
}

/// Swap the shared engine for the builtin one (once) and record why, so
/// the UI can show a banner. A no-op when the engine is already builtin.
/// Desktop only: the wasm worker answers failures with its own builtin.
#[cfg(not(target_arch = "wasm32"))]
fn swap_to_builtin(
    engine: &SharedEngine,
    notice: &std::sync::Arc<Mutex<Option<String>>>,
    reason: &str,
) {
    let mut guard = engine.lock().expect("engine lock");
    if guard.name() == "Builtin" {
        return;
    }
    let level = guard.level();
    *guard = Box::new(BuiltinEngine::with_level(level));
    // The new engine must poll the same shared stop flag, or the
    // worker's stop handle goes stale after the swap.
    guard.set_stop_flag(&engine.stop_flag());
    let mut slot = notice.lock().expect("notice lock");
    if slot.is_none() {
        *slot = Some(format!(
            "Không chạy được Pikafish ({reason}), đang dùng AI tích hợp"
        ));
    }
}

/// Run one best-move request against an engine, forwarding `Info` and
/// sending the `BestMove` response. Returns the engine error, if any.
fn run_best_move_on(
    engine: &mut dyn Engine,
    req: &EngineRequest,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let result = engine.best_move(req, &mut |info| {
        let (score, pv) = info
            .lines
            .first()
            .map(|l| (l.score.to_cp_clamped(), l.pv.clone()))
            .unwrap_or((0, Vec::new()));
        let _ = res_tx.send(AiResponse::Info {
            req_id,
            depth: info.depth,
            score,
            pv,
        });
    });
    match result {
        Ok(line) => {
            let _ = res_tx.send(AiResponse::BestMove {
                req_id,
                mv: line.mv,
                score: line.score.to_cp_clamped(),
            });
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Run one best-move request against the shared engine.
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
fn run_best_move(
    engine: &SharedEngine,
    req: &EngineRequest,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let mut guard = engine.lock().expect("engine lock");
    run_best_move_on(&mut **guard, req, req_id, res_tx)
}

/// Run one analysis request against an engine and send the `Analysis`
/// response. Returns the engine error, if any.
fn run_analyze_on(
    engine: &mut dyn Engine,
    req: &EngineRequest,
    top_n: usize,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let result = engine.analyze(req, top_n, &mut |_| {});
    match result {
        Ok(lines) => {
            if let Ok(pos) = req.position() {
                let lines = lines.iter().map(|l| to_analysis_line(&pos, l)).collect();
                let _ = res_tx.send(AiResponse::Analysis { req_id, lines });
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Run one analysis request against the shared engine.
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
fn run_analyze(
    engine: &SharedEngine,
    req: &EngineRequest,
    top_n: usize,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let mut guard = engine.lock().expect("engine lock");
    run_analyze_on(&mut **guard, req, top_n, req_id, res_tx)
}

/// Grade one move the human just played, against the engine: the best
/// move + the played move's score at the position before it, turned
/// into a grade (same rule as the old synchronous grading in
/// `VsAiState::play`, now on the worker thread). Sends the `Graded`
/// response. Returns the engine error, if any. The `EngineRequest` is
/// built here (with this engine's `grade_limits()`), so the UI thread
/// never locks the engine to grade a move.
fn run_grade_on(
    engine: &mut dyn Engine,
    start_fen: &str,
    moves: &[xq_core::types::Move],
    mv: xq_core::types::Move,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let req = EngineRequest::new(start_fen.to_string(), moves.to_vec(), engine.grade_limits());
    let best = engine.best_move(&req, &mut |_| {})?;
    let best_cp = best.score.to_cp_clamped();
    let mv_cp = engine
        .score_move(&req, mv)
        .map(|s| s.to_cp_clamped())
        .unwrap_or(best_cp);
    let grade = grade_from_delta_with(best_cp - mv_cp, false, engine.grade_thresholds());
    let explanation = match req.position() {
        Ok(pos) => explain_tags(&tags_for(&pos, mv)),
        Err(_) => String::new(),
    };
    let _ = res_tx.send(AiResponse::Graded {
        req_id,
        grade,
        best: Some(best.mv),
        played_score_cp: mv_cp,
        explanation,
    });
    Ok(())
}

/// Run one grade request against the shared engine.
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
fn run_grade(
    engine: &SharedEngine,
    start_fen: &str,
    moves: &[xq_core::types::Move],
    mv: xq_core::types::Move,
    req_id: u64,
    res_tx: &Sender<AiResponse>,
) -> Result<(), EngineError> {
    let mut guard = engine.lock().expect("engine lock");
    run_grade_on(&mut **guard, start_fen, moves, mv, req_id, res_tx)
}

/// What to do after an engine error (§7.3).
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
enum Fallback {
    /// Do nothing (user cancelled).
    None,
    /// Swap to builtin permanently, then retry on the shared engine.
    RetryShared,
    /// Answer this request with a one-off builtin; keep Pikafish.
    RetryOneOff,
}

/// Run `f` on the one-off builtin engine (created lazily, level synced
/// to the shared engine's level) for a single illegal-move fallback.
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
fn retry_one_off(
    engine: &SharedEngine,
    one_off: &mut Option<BuiltinEngine>,
    f: impl FnOnce(&mut BuiltinEngine),
) {
    let level = engine.lock().expect("engine lock").level();
    let fb = one_off.get_or_insert_with(|| BuiltinEngine::with_level(level));
    if fb.level() != level {
        let _ = fb.set_level(level);
    }
    f(fb);
}

/// Fallback policy for an engine error (§7.3).
/// Desktop only (see `swap_to_builtin`).
#[cfg(not(target_arch = "wasm32"))]
fn fallback_action(
    e: &EngineError,
    engine: &SharedEngine,
    notice: &std::sync::Arc<Mutex<Option<String>>>,
    illegal_count: &mut u32,
) -> Fallback {
    match e {
        EngineError::Stopped => Fallback::None,
        EngineError::IllegalMove(m) => {
            *illegal_count += 1;
            if *illegal_count > 3 {
                swap_to_builtin(engine, notice, &format!("nước không hợp lệ: {m}"));
                Fallback::RetryShared
            } else {
                Fallback::RetryOneOff
            }
        }
        _ => {
            swap_to_builtin(engine, notice, &e.to_string());
            Fallback::RetryShared
        }
    }
}

/// Run one worker request against the shared engine, applying the
/// §7.3 fallback policy on error (desktop worker thread only — the
/// wasm worker answers failures with its own builtin, see
/// [`WasmAiWorker`]).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn handle_request(
    req: AiRequest,
    engine: &SharedEngine,
    res_tx: &Sender<AiResponse>,
    one_off: &mut Option<BuiltinEngine>,
    illegal_count: &mut u32,
    notice: &std::sync::Arc<Mutex<Option<String>>>,
    stop: &StopHandle,
) {
    match req {
        AiRequest::BestMove { req, req_id } => {
            let result = run_best_move(engine, &req, req_id, res_tx);
            if let Err(e) = result {
                let action = fallback_action(&e, engine, notice, illegal_count);
                match action {
                    Fallback::None => {}
                    Fallback::RetryShared => {
                        let _ = run_best_move(engine, &req, req_id, res_tx);
                    }
                    Fallback::RetryOneOff => {
                        retry_one_off(engine, one_off, |fb| {
                            let _ = run_best_move_on(fb, &req, req_id, res_tx);
                        });
                    }
                }
            }
        }
        AiRequest::Analyze { req, top_n, req_id } => {
            let result = run_analyze(engine, &req, top_n, req_id, res_tx);
            if let Err(e) = result {
                let action = fallback_action(&e, engine, notice, illegal_count);
                match action {
                    Fallback::None => {}
                    Fallback::RetryShared => {
                        let _ = run_analyze(engine, &req, top_n, req_id, res_tx);
                    }
                    Fallback::RetryOneOff => {
                        retry_one_off(engine, one_off, |fb| {
                            let _ = run_analyze_on(fb, &req, top_n, req_id, res_tx);
                        });
                    }
                }
            }
        }
        AiRequest::GradeMove {
            start_fen,
            moves,
            mv,
            req_id,
        } => {
            let result = run_grade(engine, &start_fen, &moves, mv, req_id, res_tx);
            if let Err(e) = result {
                let action = fallback_action(&e, engine, notice, illegal_count);
                match action {
                    Fallback::None => {}
                    Fallback::RetryShared => {
                        let _ = run_grade(engine, &start_fen, &moves, mv, req_id, res_tx);
                    }
                    Fallback::RetryOneOff => {
                        retry_one_off(engine, one_off, |fb| {
                            let _ = run_grade_on(fb, &start_fen, &moves, mv, req_id, res_tx);
                        });
                    }
                }
            }
        }
        AiRequest::Stop => {
            stop.stop();
        }
    }
}

/// Drain queued requests, keeping only the latest; a Stop queued
/// between searches still takes effect. A BestMove is the AI's move,
/// not best-effort work: it must be answered or the game stalls on
/// "máy đang nghĩ" (Hieu, 2026-11-14), so a later hint/grade never
/// drops it — only a newer BestMove replaces one.
#[cfg(not(target_arch = "wasm32"))]
fn drain_queue(req_rx: &Receiver<AiRequest>, pending: &mut Option<AiRequest>, stop: &StopHandle) {
    while let Ok(q) = req_rx.try_recv() {
        match q {
            AiRequest::Stop => stop.stop(),
            other => {
                let pending_is_best = matches!(pending, Some(AiRequest::BestMove { .. }));
                let new_is_best = matches!(other, AiRequest::BestMove { .. });
                if !pending_is_best || new_is_best {
                    *pending = Some(other);
                }
            }
        }
    }
}

/// wasm32 only: the AI worker without a thread.
///
/// `std::thread::spawn` panics on `wasm32-unknown-unknown` without
/// atomics/shared-memory, and the app has no COOP/COEP setup for
/// that — so on wasm there is no worker thread at all. The engine is
/// Pikafish-WASM, running in a Web Worker behind
/// [`WasmEngine`](crate::engine_wasm::WasmEngine) (see
/// `assets/engine-wasm/INTEGRATION.md`); this struct is the
/// channel-shaped front end the screens already know from
/// [`AiWorker`] — same [`AiRequest`]/[`AiResponse`] types, same
/// `req_id` discipline (`response_is_current`), same `tx` / `rx` /
/// `stop` / `alloc_req_id` surface.
///
/// Shape: the screens take the channels out of the worker at
/// construction (like the desktop worker) and keep the driver
/// ([`WasmEngineDriver`], taken out with [`WasmAiWorker::take_driver`])
/// — Play/Learn — or keep the whole worker and call
/// [`WasmAiWorker::pump`] from their tick handler — Practice. The
/// driver is pumped from the tick: it forwards at most one queued
/// request to the engine, drains the engine's events into `rx`, and
/// answers a failed request with the builtin engine so the game
/// never stalls on "máy đang nghĩ".
///
/// Two-step grading (`GradeMove`): best move first, then the played
/// move's score via `searchmoves` — the two searches share one
/// `req_id`, and the grade is computed with the Pikafish thresholds
/// (`GradeThresholds::PIKAFISH`), exactly like the desktop worker's
/// `run_grade_on` with a Pikafish engine.
///
/// Known spike trade-off: while the 51.6 MB `.data` file is still
/// downloading the engine is not ready, so requests wait in
/// `pending` (retried every tick) instead of being answered — the
/// first AI move of a session waits for the download.
#[cfg(target_arch = "wasm32")]
pub struct WasmAiWorker {
    pub tx: Sender<AiRequest>,
    pub rx: Receiver<AiResponse>,
    /// Stop handle sharing the shared engine's flag (same contract
    /// as [`AiWorker::stop`]).
    pub stop: StopHandle,
    pub next_req_id: u64,
    /// Taken out by Play/Learn at construction (`take_driver`); the
    /// Practice screen keeps the whole worker and pumps in place.
    driver: Option<WasmEngineDriver>,
}

/// What the WASM driver is waiting on (one search at a time — the
/// worker rejects a second `go` with `engine busy`). The request is
/// kept so a worker error can be answered with the builtin engine
/// instead of stalling the game.
#[cfg(target_arch = "wasm32")]
enum InFlight {
    BestMove {
        req: EngineRequest,
        req_id: u64,
    },
    Analyze {
        req: EngineRequest,
        top_n: usize,
        req_id: u64,
    },
    /// Grading is two searches with one `req_id`: `best` is `None`
    /// while the best move is being searched, `Some((best, score))`
    /// while the played move is being scored.
    Grade {
        req: EngineRequest,
        mv: xq_core::types::Move,
        best: Option<(xq_core::types::Move, i32)>,
        req_id: u64,
    },
}

#[cfg(target_arch = "wasm32")]
impl InFlight {
    /// Rebuild the [`AiRequest`] this in-flight search answers, so
    /// a worker error can fall back to the builtin engine.
    fn into_request(self) -> AiRequest {
        match self {
            InFlight::BestMove { req, req_id } => AiRequest::BestMove { req, req_id },
            InFlight::Analyze { req, top_n, req_id } => AiRequest::Analyze { req, top_n, req_id },
            InFlight::Grade {
                req, mv, req_id, ..
            } => AiRequest::GradeMove {
                start_fen: req.start_fen.clone(),
                moves: req.moves.clone(),
                mv,
                req_id,
            },
        }
    }
}

/// wasm32 only: the pump behind [`WasmAiWorker`]. Owns the
/// [`WasmEngine`] plus the channel ends the screens moved out of the
/// worker. The engine starts LAZILY: `engine` stays `None` until the
/// first AI request arrives (`dispatch` → `ensure_started`), so
/// merely opening a screen — the Play setup dialog, a lesson list —
/// never loads the 51.6 MB `.data` file. On iPhone Safari that load,
/// on top of the 57.6 MB app WASM, killed the tab the moment the
/// user tapped "Đấu với AI" (Hieu, 2026-11-26: the tab reloaded to
/// Home, then "A problem repeatedly occurred"). `None` + `dead`
/// still means the worker failed and every request goes to the
/// builtin; `None` + `!started` just means nothing asked yet.
#[cfg(target_arch = "wasm32")]
pub struct WasmEngineDriver {
    engine: Option<WasmEngine>,
    /// `true` once the worker spawn was attempted (success or
    /// failure — see `ensure_started`).
    started: bool,
    req_rx: Receiver<AiRequest>,
    res_tx: Sender<AiResponse>,
    stop: StopHandle,
    /// One-off builtin for error fallbacks (level synced to the
    /// shared engine at construction).
    fallback: BuiltinEngine,
    notice: std::sync::Arc<Mutex<Option<String>>>,
    pending: Option<AiRequest>,
    in_flight: Option<InFlight>,
    /// The worker died at startup (`Error` with no `req_id`): all
    /// later requests go straight to the builtin.
    dead: bool,
}

#[cfg(target_arch = "wasm32")]
impl WasmEngineDriver {
    /// Whether the worker finished the `uci` + `isready` handshake.
    /// `false` when there is no engine at all.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.engine.as_ref().is_some_and(WasmEngine::is_ready)
    }

    /// The engine's `id name` line once ready (empty before that).
    #[must_use]
    pub fn engine_name(&self) -> String {
        self.engine
            .as_ref()
            .map_or_else(String::new, WasmEngine::name)
    }

    /// Take the fallback notice, if the WASM engine failed and the
    /// builtin took over. The UI shows it as a banner once.
    pub fn take_fallback_notice(&self) -> Option<String> {
        self.notice.lock().expect("notice lock").take()
    }

    fn set_notice_once(&self, message: String) {
        let mut slot = self.notice.lock().expect("notice lock");
        if slot.is_none() {
            *slot = Some(message);
        }
    }

    /// Pump once from the screens' tick handler: forward at most one
    /// queued request to the engine and drain its events into `rx`.
    /// Never blocks — a request the engine is not ready for stays
    /// queued for the next tick.
    pub fn pump(&mut self) {
        // Drain queued requests, keeping only the latest (same rule
        // as `drain_queue`); a Stop also stops the WASM search at
        // once, instead of queueing behind the running search.
        while let Ok(q) = self.req_rx.try_recv() {
            match q {
                AiRequest::Stop => {
                    self.stop.stop();
                    if let Some(engine) = &self.engine {
                        let _ = engine.stop();
                    }
                }
                other => {
                    let pending_is_best = matches!(self.pending, Some(AiRequest::BestMove { .. }));
                    let new_is_best = matches!(other, AiRequest::BestMove { .. });
                    if !pending_is_best || new_is_best {
                        self.pending = Some(other);
                    }
                }
            }
        }
        if self.in_flight.is_none() {
            if let Some(req) = self.pending.take() {
                self.dispatch(req);
            }
        }
        loop {
            let Some(ev) = self.engine.as_ref().and_then(WasmEngine::try_recv) else {
                break;
            };
            self.handle_event(ev);
        }
    }

    /// Spawn the worker on the first AI request (lazy start — see the
    /// struct doc). A no-op after the first attempt; a failed spawn
    /// marks the driver dead and sets the fallback notice once, so
    /// the request falls through to the builtin below.
    fn ensure_started(&mut self) {
        if self.started || self.dead {
            return;
        }
        self.started = true;
        match WasmEngine::new("engine-wasm/pikafish-worker.js") {
            Ok(engine) => self.engine = Some(engine),
            Err(e) => {
                self.dead = true;
                self.set_notice_once(format!(
                    "Không chạy được Pikafish-WASM ({e:?}), đang dùng AI tích hợp"
                ));
            }
        }
    }

    /// Send one request to the engine (or answer it with the builtin
    /// when there is no engine / the worker is dead / the send
    /// fails). A request sent while the engine is not ready yet is
    /// re-queued for the next tick.
    fn dispatch(&mut self, req: AiRequest) {
        self.ensure_started();
        if self.engine.is_none() || self.dead {
            self.answer_with_builtin(req);
            return;
        }
        let engine = self.engine.as_ref().expect("engine present");
        match req {
            AiRequest::BestMove { req, req_id } => match engine.request_best_move(&req, req_id) {
                Ok(()) => {
                    self.in_flight = Some(InFlight::BestMove { req, req_id });
                }
                Err(WasmEngineError::NotReady) => {
                    self.pending = Some(AiRequest::BestMove { req, req_id });
                }
                Err(_) => self.answer_with_builtin(AiRequest::BestMove { req, req_id }),
            },
            AiRequest::Analyze { req, top_n, req_id } => {
                match engine.request_analyze(&req, top_n, req_id) {
                    Ok(()) => {
                        self.in_flight = Some(InFlight::Analyze { req, top_n, req_id });
                    }
                    Err(WasmEngineError::NotReady) => {
                        self.pending = Some(AiRequest::Analyze { req, top_n, req_id });
                    }
                    Err(_) => {
                        self.answer_with_builtin(AiRequest::Analyze { req, top_n, req_id });
                    }
                }
            }
            AiRequest::GradeMove {
                start_fen,
                moves,
                mv,
                req_id,
            } => {
                // Desktop Pikafish grades with `time_ms(1500)`
                // (`PikafishEngine::grade_limits`); the builtin's
                // `depth(6)` does not apply to this engine.
                let req = EngineRequest::new(
                    start_fen.clone(),
                    moves.clone(),
                    SearchLimits::time_ms(1500),
                );
                match engine.request_best_move(&req, req_id) {
                    Ok(()) => {
                        self.in_flight = Some(InFlight::Grade {
                            req,
                            mv,
                            best: None,
                            req_id,
                        });
                    }
                    Err(WasmEngineError::NotReady) => {
                        self.pending = Some(AiRequest::GradeMove {
                            start_fen,
                            moves,
                            mv,
                            req_id,
                        });
                    }
                    Err(_) => self.answer_with_builtin(AiRequest::GradeMove {
                        start_fen,
                        moves,
                        mv,
                        req_id,
                    }),
                }
            }
            AiRequest::Stop => {}
        }
    }

    /// Answer one request synchronously with the builtin engine (no
    /// engine / dead worker / failed send). Same helpers as the
    /// desktop worker's `run_*_on`, minus the §7.3 swap policy —
    /// the builtin IS the fallback here.
    fn answer_with_builtin(&mut self, req: AiRequest) {
        match req {
            AiRequest::BestMove { req, req_id } => {
                let _ = run_best_move_on(&mut self.fallback, &req, req_id, &self.res_tx);
            }
            AiRequest::Analyze { req, top_n, req_id } => {
                let _ = run_analyze_on(&mut self.fallback, &req, top_n, req_id, &self.res_tx);
            }
            AiRequest::GradeMove {
                start_fen,
                moves,
                mv,
                req_id,
            } => {
                let _ = run_grade_on(
                    &mut self.fallback,
                    &start_fen,
                    &moves,
                    mv,
                    req_id,
                    &self.res_tx,
                );
            }
            AiRequest::Stop => {}
        }
    }

    /// Translate one engine event into [`AiResponse`]s.
    fn handle_event(&mut self, ev: WasmEngineEvent) {
        match ev {
            WasmEngineEvent::Ready { .. } => {}
            WasmEngineEvent::Info {
                req_id,
                depth,
                score_cp,
                pv,
            } => {
                let _ = self.res_tx.send(AiResponse::Info {
                    req_id,
                    depth,
                    score: score_cp,
                    pv,
                });
            }
            WasmEngineEvent::BestMove {
                req_id,
                mv,
                score_cp,
            } => {
                match self.in_flight.take() {
                    Some(InFlight::BestMove { .. }) => {
                        let _ = self.res_tx.send(AiResponse::BestMove {
                            req_id,
                            mv,
                            score: score_cp,
                        });
                    }
                    Some(InFlight::Grade {
                        req,
                        mv: played,
                        best: None,
                        req_id,
                    }) => {
                        // Phase 1 done: the best move + its score are
                        // known; now score the played move (same req_id
                        // — the worker freed it when it answered).
                        let best_cp = score_cp;
                        match self
                            .engine
                            .as_ref()
                            .expect("engine present")
                            .request_score_move(&req, played, req_id)
                        {
                            Ok(()) => {
                                self.in_flight = Some(InFlight::Grade {
                                    req,
                                    mv: played,
                                    best: Some((mv, best_cp)),
                                    req_id,
                                });
                            }
                            Err(_) => {
                                self.answer_with_builtin(AiRequest::GradeMove {
                                    start_fen: req.start_fen.clone(),
                                    moves: req.moves.clone(),
                                    mv: played,
                                    req_id,
                                });
                            }
                        }
                    }
                    Some(InFlight::Grade {
                        req,
                        mv: played,
                        best: Some((best_mv, best_cp)),
                        req_id: _,
                    }) => {
                        // Phase 2 done: this `bestmove` IS the played
                        // move, and `score_cp` is its search score.
                        let grade = grade_from_delta_with(
                            best_cp - score_cp,
                            false,
                            GradeThresholds::PIKAFISH,
                        );
                        let explanation = match req.position() {
                            Ok(pos) => explain_tags(&tags_for(&pos, played)),
                            Err(_) => String::new(),
                        };
                        let _ = self.res_tx.send(AiResponse::Graded {
                            req_id,
                            grade,
                            best: Some(best_mv),
                            played_score_cp: score_cp,
                            explanation,
                        });
                    }
                    Some(other) => {
                        // An `Analysis` answers with `Analysis`, not
                        // `BestMove` — a stray bestmove is dropped and
                        // the in-flight request is restored.
                        self.in_flight = Some(other);
                    }
                    None => {}
                }
            }
            WasmEngineEvent::Analysis { req_id, lines } => {
                match self.in_flight.take() {
                    Some(InFlight::Analyze { req, .. }) => {
                        // The bridge leaves `tags` empty on purpose;
                        // fill them like `to_analysis_line` does.
                        let lines = match req.position() {
                            Ok(pos) => lines
                                .iter()
                                .map(|l| AnalysisLine {
                                    mv: l.mv,
                                    score: l.score,
                                    depth: l.depth,
                                    pv: l.pv.clone(),
                                    tags: tags_with_mate(&pos, l.mv, l.score),
                                })
                                .collect(),
                            Err(_) => lines,
                        };
                        let _ = self.res_tx.send(AiResponse::Analysis { req_id, lines });
                    }
                    other => {
                        self.in_flight = other;
                    }
                }
            }
            WasmEngineEvent::Log { .. } => {}
            WasmEngineEvent::Error { req_id, message } => {
                let Some(id) = req_id else {
                    // Startup failure: the worker is dead; the
                    // in-flight request (if any) is answered with
                    // the builtin and everything after it goes
                    // straight there too.
                    if let Some(in_flight) = self.in_flight.take() {
                        let req = in_flight.into_request();
                        self.answer_with_builtin(req);
                    }
                    self.dead = true;
                    self.set_notice_once(format!(
                        "Không chạy được Pikafish-WASM ({message}), đang dùng AI tích hợp"
                    ));
                    return;
                };
                if let Some(in_flight) = self.in_flight.take() {
                    self.set_notice_once(format!(
                        "Pikafish-WASM lỗi ({message}), đang dùng AI tích hợp"
                    ));
                    let _ = id;
                    let req = in_flight.into_request();
                    self.answer_with_builtin(req);
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl WasmAiWorker {
    /// Create the worker: NO engine is spawned here — the Web Worker
    /// (and its 51.6 MB `.data` download) starts only on the first AI
    /// request, inside the driver's `ensure_started` (see
    /// [`WasmEngineDriver`]). Opening a screen must stay light: on
    /// iPhone Safari, spawning at Play entry killed the tab as soon
    /// as the user tapped "Đấu với AI" (Hieu, 2026-11-26). A request
    /// sent before the engine is ready waits in the queue; when the
    /// worker itself fails to spawn, the notice is set and every
    /// request is answered by the builtin.
    #[must_use]
    pub fn new(shared_engine: SharedEngine) -> Self {
        let (req_tx, req_rx) = channel::<AiRequest>();
        let (res_tx, res_rx) = channel::<AiResponse>();
        let level = shared_engine.lock().expect("engine lock").level();
        let stop = shared_engine.stop_handle();
        let notice = std::sync::Arc::new(Mutex::new(None));
        let driver = WasmEngineDriver {
            engine: None,
            started: false,
            req_rx,
            res_tx,
            stop: stop.clone(),
            fallback: BuiltinEngine::with_level(level),
            notice: std::sync::Arc::clone(&notice),
            pending: None,
            in_flight: None,
            dead: false,
        };
        Self {
            tx: req_tx,
            rx: res_rx,
            stop,
            next_req_id: 1,
            driver: Some(driver),
        }
    }

    /// Take the driver out of the worker. Call this before moving
    /// `tx` / `rx` / `stop` out (a partial move afterwards would
    /// make the worker unusable). Play/Learn take it; Practice
    /// keeps the whole worker and pumps in place instead.
    #[must_use]
    pub fn take_driver(&mut self) -> Option<WasmEngineDriver> {
        self.driver.take()
    }

    /// Pump the driver in place (Practice keeps the whole worker).
    /// A no-op once the driver was taken out.
    pub fn pump(&mut self) {
        if let Some(driver) = self.driver.as_mut() {
            driver.pump();
        }
    }

    /// Allocate the next request id.
    pub fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }
}

impl AiWorker {
    /// (On wasm the screens use [`WasmAiWorker`] instead, so this
    /// constructor is uncalled there.)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    #[must_use]
    pub fn new(engine: SharedEngine) -> Self {
        let (req_tx, req_rx) = channel::<AiRequest>();
        let (res_tx, res_rx) = channel::<AiResponse>();
        // The stop handle shares the engine's flag, so setting it
        // interrupts a running search from the UI thread (screens keep
        // their own handle after destructuring the worker). The flag
        // lives in the `SharedEngine`, not in the engine, so it
        // survives swaps.
        let stop = engine.stop_handle();
        let stop_for_thread = stop.clone();
        let fallback_notice = std::sync::Arc::new(Mutex::new(None));
        let notice_for_thread = std::sync::Arc::clone(&fallback_notice);
        // wasm32: the channels feed the desktop worker thread only;
        // the screens use `WasmAiWorker` there instead.
        #[cfg(target_arch = "wasm32")]
        let _ = (&req_rx, &res_tx, &stop_for_thread, &notice_for_thread);
        // Desktop: the worker thread (blocking `recv`, searches run
        // strictly one at a time). wasm32 has no threads — the
        // screens use [`WasmAiWorker`] there instead, so this
        // constructor only builds the (unused) shell on wasm.
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            // One-off builtin for illegal-move fallbacks (this move only);
            // the shared engine is swapped only after repeated failures.
            let mut one_off: Option<BuiltinEngine> = None;
            let mut illegal_count: u32 = 0;
            // Latest request waiting while a search runs; older queued
            // requests are dropped (the UI drops stale responses by
            // req_id anyway). A search already running cannot be
            // interrupted from the queue — only the stop flag can.
            let mut pending: Option<AiRequest> = None;
            loop {
                let req = match pending.take() {
                    Some(r) => r,
                    None => match req_rx.recv() {
                        Ok(r) => r,
                        Err(_) => break,
                    },
                };
                handle_request(
                    req,
                    &engine,
                    &res_tx,
                    &mut one_off,
                    &mut illegal_count,
                    &notice_for_thread,
                    &stop_for_thread,
                );
                drain_queue(&req_rx, &mut pending, &stop_for_thread);
            }
        });
        Self {
            tx: req_tx,
            rx: res_rx,
            stop,
            next_req_id: 1,
            fallback_notice,
        }
    }

    /// Take the fallback notice, if the worker switched engines. The UI
    /// shows it as a banner once.
    // NOTE (integration): see `fallback_notice` — exercised by unit
    // tests; production screens check the engine name instead.
    #[allow(dead_code)]
    pub fn take_fallback_notice(&self) -> Option<String> {
        self.fallback_notice.lock().expect("notice lock").take()
    }

    /// Allocate the next request id.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }
}

impl Default for AiWorker {
    fn default() -> Self {
        Self::new(xq_engine::shared_engine(Box::new(
            xq_engine::BuiltinEngine::new(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::board::Position;
    use xq_engine::Score;
    use xq_engine::SearchLimits;

    fn test_worker() -> AiWorker {
        AiWorker::new(xq_engine::shared_engine(Box::new(
            xq_engine::BuiltinEngine::new(),
        )))
    }

    fn startpos_req(limits: SearchLimits) -> EngineRequest {
        EngineRequest::new(Position::startpos().to_fen(), Vec::new(), limits)
    }

    #[test]
    fn req_ids_increase() {
        let mut w = test_worker();
        let a = w.alloc_req_id();
        let b = w.alloc_req_id();
        let c = w.alloc_req_id();
        assert_eq!((a, b, c), (1, 2, 3));
    }

    #[test]
    fn stale_responses_are_ignored() {
        assert!(response_is_current(7, Some(7)));
        assert!(!response_is_current(6, Some(7)));
        assert!(!response_is_current(7, None));
        assert!(!response_is_current(7, Some(8)));
    }

    #[test]
    fn stop_flag_is_set_immediately() {
        let w = test_worker();
        assert!(!w.stop.is_stopped());
        // What the screens' own `request_stop` does (the worker's
        // method was removed: screens destructure the worker and keep
        // their own stop handle).
        w.stop.stop();
        let _ = w.tx.send(AiRequest::Stop);
        assert!(w.stop.is_stopped());
    }

    #[test]
    fn worker_returns_a_legal_best_move() {
        let mut w = test_worker();
        let pos = Position::startpos();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::depth(1)),
            req_id,
        })
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Ok(AiResponse::BestMove {
                req_id: got, mv, ..
            }) = w.rx.recv_timeout(std::time::Duration::from_millis(50))
            {
                assert_eq!(got, req_id);
                assert!(pos.legal_moves().contains(&mv), "AI move must be legal");
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not answer within 5 s"
            );
        }
    }

    #[test]
    fn stop_returns_quickly_after_a_search_starts() {
        // With a depth-1 search a search finishes immediately; this test
        // pins the contract that setting the stop flag is instant and the
        // worker still answers. The real <=100 ms stop-latency check
        // against a deep search belongs to the M2 search tests
        // (spec §8.6 `stop_flag`).
        let mut w = test_worker();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::time_ms(8000)),
            req_id,
        })
        .unwrap();
        let t0 = std::time::Instant::now();
        // Screens do this via their own stop handle (see above).
        w.stop.stop();
        let _ = w.tx.send(AiRequest::Stop);
        assert!(t0.elapsed() < std::time::Duration::from_millis(100));
        let r =
            w.rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("worker answers");
        assert_eq!(r.req_id(), req_id);
    }

    #[test]
    fn grade_move_returns_a_grade() {
        use xq_core::analysis::Grade;
        use xq_core::notation::parse_cn;
        let mut w = test_worker();
        let pos = Position::startpos();
        // The classic best opening move: the engine's best should match
        // it (or come very close), so the grade is Best/Book.
        let mv = parse_cn(&pos, "炮二平五").expect("parse opening move");
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::GradeMove {
            start_fen: Position::startpos().to_fen(),
            moves: Vec::new(),
            mv,
            req_id,
        })
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(AiResponse::Graded {
                req_id: got,
                grade,
                best,
                ..
            }) = w.rx.recv_timeout(std::time::Duration::from_millis(50))
            {
                assert_eq!(got, req_id);
                assert!(
                    matches!(grade, Grade::Best | Grade::Book),
                    "炮二平五 should grade Best/Book, got {grade:?}"
                );
                let best = best.expect("a best move");
                assert!(pos.legal_moves().contains(&best));
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not answer within 10 s"
            );
        }
    }

    #[test]
    fn grade_sees_the_recapture_after_a_blunder_capture() {
        // Hieu, 2026-11-25: the static eval calls Rook-takes-Horse a
        // clear win, but the Black rook recaptures at once — the
        // engine grade must see that, or the advantage chip (fed by
        // the grade) would keep showing an advantage after a blunder.
        use xq_core::eval;
        use xq_core::types::Move;
        let fen = "4k4/4r4/9/4n4/9/4R4/9/C8/9/4K4 w - - 0 1";
        let pos = Position::from_fen(fen).expect("test fen");
        let mv = Move::new(40, 58);
        assert!(pos.legal_moves().contains(&mv));
        // Static eval after the capture: clearly good for Red (up a
        // horse). `eval` is the side-to-move's view, and Black is to
        // move after the capture, so flip it.
        let after = Position::from_fen("4k4/4r4/9/4R4/9/9/9/C8/9/4K4 b - - 0 1").expect("test fen");
        let static_red = -eval::eval(&after);
        assert!(
            static_red > 150,
            "static eval should call the capture a win, got {static_red}"
        );
        // The engine grade of the same move must see the recapture.
        let mut w = test_worker();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::GradeMove {
            start_fen: pos.to_fen(),
            moves: Vec::new(),
            mv,
            req_id,
        })
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(AiResponse::Graded {
                req_id: got,
                grade,
                played_score_cp,
                ..
            }) = w.rx.recv_timeout(std::time::Duration::from_millis(50))
            {
                assert_eq!(got, req_id);
                assert!(
                    matches!(grade, Grade::Inaccurate | Grade::Blunder),
                    "capture should grade badly, got {grade:?}"
                );
                assert!(
                    played_score_cp < -100,
                    "played move should be losing for Red, got {played_score_cp}"
                );
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not answer within 10 s"
            );
        }
    }

    #[test]
    fn analyze_returns_scored_lines() {
        let mut w = test_worker();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::Analyze {
            req: startpos_req(SearchLimits::depth(2)),
            top_n: 3,
            req_id,
        })
        .unwrap();
        let r =
            w.rx.recv_timeout(std::time::Duration::from_secs(10))
                .expect("worker answers");
        match r {
            AiResponse::Analysis { lines, .. } => {
                assert!(!lines.is_empty());
                assert!(lines.len() <= 3);
                for window in lines.windows(2) {
                    assert!(window[0].score >= window[1].score);
                }
            }
            other => panic!("mong đợi Analysis, gặp {other:?}"),
        }
    }

    #[test]
    fn score_type_is_cp() {
        // Mate scores arrive as clamped centipawns, never as raw MATE.
        let line = Line {
            mv: xq_core::notation::parse_iccs("h2e2").unwrap(),
            score: Score::Mate(3),
            depth: 10,
            pv: Vec::new(),
        };
        assert_eq!(line.score.to_cp_clamped(), 30000 - 3);
    }

    // --- Worker-level fallback tests (§7.3) ---
    //
    // These use a mock engine instead of fake-uci: the worker lives in
    // the binary crate `xq-app` and cannot be imported from an
    // integration test, and the fake-uci binary path is too fragile to
    // reach from here. The fake-engine behaviour itself is covered by
    // `xq-pikafish/tests/fake_engine.rs`.

    #[derive(Clone, Copy, PartialEq)]
    enum FailMode {
        /// Always crash (exit code 1).
        Crash,
        /// Always answer an illegal move.
        Illegal,
        /// Answer a legal move after a 2 s sleep.
        Slow,
    }

    struct MockEngine {
        level: xq_engine::Level,
        fail: FailMode,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl MockEngine {
        fn new(fail: FailMode) -> Self {
            Self {
                level: xq_engine::Level::Medium,
                fail,
                stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn legal_line(req: &EngineRequest) -> Line {
            let pos = req.position().expect("vị trí hợp lệ");
            let mv = pos.legal_moves()[0];
            Line {
                mv,
                score: Score::Cp(35),
                depth: 1,
                pv: vec![mv],
            }
        }
    }

    impl Engine for MockEngine {
        fn name(&self) -> &str {
            "MockPikafish"
        }

        fn new_game(&mut self) -> Result<(), EngineError> {
            Ok(())
        }

        fn set_level(&mut self, level: xq_engine::Level) -> Result<(), EngineError> {
            self.level = level;
            Ok(())
        }

        fn best_move(
            &mut self,
            req: &EngineRequest,
            _on_info: &mut dyn FnMut(&xq_engine::SearchInfo),
        ) -> Result<Line, EngineError> {
            match self.fail {
                FailMode::Crash => Err(EngineError::Crashed(Some(1))),
                FailMode::Illegal => Err(EngineError::IllegalMove("a0a9".to_string())),
                FailMode::Slow => {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    Ok(Self::legal_line(req))
                }
            }
        }

        fn analyze(
            &mut self,
            req: &EngineRequest,
            _top_n: usize,
            _on_info: &mut dyn FnMut(&xq_engine::SearchInfo),
        ) -> Result<Vec<Line>, EngineError> {
            match self.fail {
                FailMode::Crash => Err(EngineError::Crashed(Some(1))),
                FailMode::Illegal => Err(EngineError::IllegalMove("a0a9".to_string())),
                FailMode::Slow => Ok(vec![Self::legal_line(req)]),
            }
        }

        fn score_move(
            &mut self,
            _req: &EngineRequest,
            _mv: xq_core::types::Move,
        ) -> Result<Score, EngineError> {
            Ok(Score::Cp(0))
        }

        fn stop_handle(&self) -> StopHandle {
            StopHandle::from_flag(std::sync::Arc::clone(&self.stop))
        }

        fn level(&self) -> xq_engine::Level {
            self.level
        }
    }

    fn mock_worker(fail: FailMode) -> AiWorker {
        AiWorker::new(xq_engine::shared_engine(Box::new(MockEngine::new(fail))))
    }

    /// Wait for the next `BestMove` response (ignoring `Info`).
    fn wait_best_move(w: &AiWorker, secs: u64) -> AiResponse {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            match w.rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(r @ AiResponse::BestMove { .. }) => return r,
                Ok(_) => {}
                Err(_) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "worker không trả BestMove trong {secs} s"
                    );
                }
            }
        }
    }

    #[test]
    fn worker_falls_back_to_builtin_on_crash() {
        let mut w = mock_worker(FailMode::Crash);
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::depth(1)),
            req_id,
        })
        .unwrap();
        let r = wait_best_move(&w, 10);
        match r {
            AiResponse::BestMove {
                req_id: got, mv, ..
            } => {
                assert_eq!(got, req_id);
                assert!(Position::startpos().legal_moves().contains(&mv));
            }
            other => panic!("mong đợi BestMove, gặp {other:?}"),
        }
        assert!(
            w.take_fallback_notice().is_some(),
            "crash phải để lại banner dự phòng"
        );
    }

    #[test]
    fn illegal_move_uses_one_off_builtin() {
        let mut w = mock_worker(FailMode::Illegal);
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::depth(1)),
            req_id,
        })
        .unwrap();
        let r = wait_best_move(&w, 10);
        match r {
            AiResponse::BestMove {
                req_id: got, mv, ..
            } => {
                assert_eq!(got, req_id);
                assert!(Position::startpos().legal_moves().contains(&mv));
            }
            other => panic!("mong đợi BestMove, gặp {other:?}"),
        }
        assert!(
            w.take_fallback_notice().is_none(),
            "một lần IllegalMove chưa được đổi engine"
        );
    }

    #[test]
    fn illegal_move_swaps_after_three() {
        let mut w = mock_worker(FailMode::Illegal);
        for _ in 0..4 {
            let req_id = w.alloc_req_id();
            w.tx.send(AiRequest::BestMove {
                req: startpos_req(SearchLimits::depth(1)),
                req_id,
            })
            .unwrap();
            let r = wait_best_move(&w, 10);
            match r {
                AiResponse::BestMove {
                    req_id: got, mv, ..
                } => {
                    assert_eq!(got, req_id);
                    assert!(Position::startpos().legal_moves().contains(&mv));
                }
                other => panic!("mong đợi BestMove, gặp {other:?}"),
            }
        }
        assert!(
            w.take_fallback_notice().is_some(),
            "lần IllegalMove thứ 4 phải đổi sang Builtin + banner"
        );
    }

    #[test]
    fn stale_request_dropped() {
        // req 1 is slow; req 2 and req 3 queue behind it. The drain
        // keeps only the latest queued request, so req 2 is never
        // answered and the UI must ignore it by req_id.
        let mut w = mock_worker(FailMode::Slow);
        let id1 = w.alloc_req_id();
        let id2 = w.alloc_req_id();
        let id3 = w.alloc_req_id();
        for id in [id1, id2, id3] {
            w.tx.send(AiRequest::BestMove {
                req: startpos_req(SearchLimits::depth(1)),
                req_id: id,
            })
            .unwrap();
        }
        let mut got = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while got.len() < 2 {
            match w.rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(AiResponse::BestMove { req_id, mv, .. }) => {
                    assert!(Position::startpos().legal_moves().contains(&mv));
                    got.push(req_id);
                }
                Ok(_) => {}
                Err(_) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "worker không trả đủ 2 BestMove trong 10 s (đã có {got:?})"
                    );
                }
            }
        }
        assert_eq!(got, vec![id1, id3], "req giữa phải bị bỏ, chỉ còn mới nhất");
        assert!(!response_is_current(id2, Some(id3)));
        assert!(response_is_current(id3, Some(id3)));
    }

    #[test]
    fn best_move_is_not_dropped_by_a_later_grade() {
        // The "máy đang nghĩ" hang (Hieu, 2026-11-14): a BestMove
        // queued behind a running search used to be dropped when a
        // later hint/grade arrived, leaving `ai_thinking` true
        // forever. A BestMove must survive the drain — only a newer
        // BestMove replaces one.
        let mut w = mock_worker(FailMode::Slow);
        let id1 = w.alloc_req_id();
        let id2 = w.alloc_req_id();
        let id3 = w.alloc_req_id();
        // req 1 is slow (2 s); req 2 (BestMove) and req 3 (grade)
        // queue behind it.
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::depth(1)),
            req_id: id1,
        })
        .unwrap();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(SearchLimits::depth(1)),
            req_id: id2,
        })
        .unwrap();
        let mv = xq_core::notation::parse_iccs("h2e2").expect("h2e2");
        w.tx.send(AiRequest::GradeMove {
            start_fen: Position::startpos().to_fen(),
            moves: Vec::new(),
            mv,
            req_id: id3,
        })
        .unwrap();
        // Both BestMoves must be answered; the grade is dropped.
        let mut got = Vec::new();
        let mut graded = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while got.len() < 2 {
            match w.rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(AiResponse::BestMove { req_id, mv, .. }) => {
                    assert!(Position::startpos().legal_moves().contains(&mv));
                    got.push(req_id);
                }
                Ok(AiResponse::Graded { .. }) => graded = true,
                Ok(_) => {}
                Err(_) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "worker không trả đủ 2 BestMove trong 10 s (đã có {got:?})"
                    );
                }
            }
        }
        assert_eq!(got, vec![id1, id2], "grade sau không được bỏ BestMove");
        assert!(!graded, "grade phải bị bỏ, BestMove được giữ");
    }

    /// Timing probe for the Play-vs-AI "máy đang nghĩ" hang (Hieu,
    /// 2026-11-14). Ignored by default: it needs the real Pikafish
    /// binary (`engine/`) and takes ~10 s. Run with
    /// `cargo test -p xq-iced -- --ignored ai_timing --nocapture`
    /// and read the printed timings: a BestMove at Medium limits
    /// must arrive in ~1 s, a second one just as fast, and one
    /// queued behind a grade in ~2.5 s. Anything far slower (or a
    /// timeout) means the worker/engine path — not the UI — is the
    /// bottleneck.
    #[test]
    #[ignore]
    fn ai_timing_real_pikafish() {
        use std::time::Instant;
        use xq_engine::Level;
        let located = match xq_pikafish::locate(None) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("ai_timing: Pikafish not found ({e}), skipping");
                return;
            }
        };
        let mut options = xq_pikafish::EngineOptions::new(located.nnue.clone());
        options.threads = 1;
        options.hash_mb = 64;
        let mut engine = match xq_pikafish::PikafishEngine::spawn(located, options) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("ai_timing: Pikafish did not spawn ({e}), skipping");
                return;
            }
        };
        eprintln!("ai_timing: engine {}", engine.name());
        let _ = engine.set_level(Level::Medium);
        let w = AiWorker::new(xq_engine::shared_engine(Box::new(engine)));
        let medium = SearchLimits {
            max_depth: Some(6),
            max_time_ms: Some(1000),
            max_nodes: None,
        };
        let time_best_move = |w: &AiWorker, req: EngineRequest, req_id: u64, secs: u64| {
            let start = Instant::now();
            w.tx.send(AiRequest::BestMove { req, req_id }).unwrap();
            let resp = wait_best_move(w, secs);
            eprintln!("ai_timing: BestMove req {req_id} in {:?}", start.elapsed());
            resp
        };
        // 1) First AI move from the start position.
        let _ = time_best_move(&w, startpos_req(medium), 1, 30);
        // 2) Second AI move after a human move (h2e2).
        let mv = xq_core::notation::parse_iccs("h2e2").expect("h2e2");
        let req = EngineRequest::new(Position::startpos().to_fen(), vec![mv], medium);
        let _ = time_best_move(&w, req, 2, 30);
        // 3) A grade (1.5 s) queued ahead of an AI move.
        let start = Instant::now();
        w.tx.send(AiRequest::GradeMove {
            start_fen: Position::startpos().to_fen(),
            moves: Vec::new(),
            mv,
            req_id: 3,
        })
        .unwrap();
        w.tx.send(AiRequest::BestMove {
            req: startpos_req(medium),
            req_id: 4,
        })
        .unwrap();
        let _ = wait_best_move(&w, 30);
        eprintln!(
            "ai_timing: grade + BestMove in {:?} (grade ~1.5 s, move ~1 s)",
            start.elapsed()
        );
    }
}
