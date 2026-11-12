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
//!   polls to send `stop` (Pikafish). `request_stop()` therefore takes
//!   effect immediately even while a search is running — sending
//!   `AiRequest::Stop` alone would queue behind the running search and
//!   could not interrupt it.
//! - The UI keeps the `req_id` of the request it is currently waiting for
//!   and drops every response whose `req_id` does not match
//!   (`response_is_current`). This is what makes Undo-while-thinking safe.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use xq_core::analysis::tags_with_mate;
use xq_core::analysis::{explain_tags, grade_from_delta_with, tags_for, Grade};
use xq_core::search::AnalysisLine;
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
    /// engine's best move at that position (for "nước đúng"), and the
    /// explanation.
    Graded {
        req_id: u64,
        grade: Grade,
        best: Option<xq_core::types::Move>,
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
pub struct AiWorker {
    pub tx: Sender<AiRequest>,
    pub rx: Receiver<AiResponse>,
    /// Stop handle of the shared engine (see module docs).
    pub stop: StopHandle,
    pub next_req_id: u64,
    /// Set once when the worker falls back from Pikafish to the builtin
    /// engine; the UI shows it as a banner and clears it by taking it.
    fallback_notice: std::sync::Arc<Mutex<Option<String>>>,
}

/// Swap the shared engine for the builtin one (once) and record why, so
/// the UI can show a banner. A no-op when the engine is already builtin.
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
        explanation,
    });
    Ok(())
}

/// Run one grade request against the shared engine.
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

impl AiWorker {
    #[must_use]
    pub fn new(engine: SharedEngine) -> Self {
        let (req_tx, req_rx) = channel::<AiRequest>();
        let (res_tx, res_rx) = channel::<AiResponse>();
        // The stop handle shares the engine's flag, so `request_stop()`
        // interrupts a running search from the UI thread. The flag lives
        // in the `SharedEngine`, not in the engine, so it survives swaps.
        let stop = engine.stop_handle();
        let stop_for_thread = stop.clone();
        let fallback_notice = std::sync::Arc::new(Mutex::new(None));
        let notice_for_thread = std::sync::Arc::clone(&fallback_notice);
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
                match req {
                    AiRequest::BestMove { req, req_id } => {
                        let result = run_best_move(&engine, &req, req_id, &res_tx);
                        if let Err(e) = result {
                            let action = fallback_action(
                                &e,
                                &engine,
                                &notice_for_thread,
                                &mut illegal_count,
                            );
                            match action {
                                Fallback::None => {}
                                Fallback::RetryShared => {
                                    let _ = run_best_move(&engine, &req, req_id, &res_tx);
                                }
                                Fallback::RetryOneOff => {
                                    retry_one_off(&engine, &mut one_off, |fb| {
                                        let _ = run_best_move_on(fb, &req, req_id, &res_tx);
                                    });
                                }
                            }
                        }
                    }
                    AiRequest::Analyze { req, top_n, req_id } => {
                        let result = run_analyze(&engine, &req, top_n, req_id, &res_tx);
                        if let Err(e) = result {
                            let action = fallback_action(
                                &e,
                                &engine,
                                &notice_for_thread,
                                &mut illegal_count,
                            );
                            match action {
                                Fallback::None => {}
                                Fallback::RetryShared => {
                                    let _ = run_analyze(&engine, &req, top_n, req_id, &res_tx);
                                }
                                Fallback::RetryOneOff => {
                                    retry_one_off(&engine, &mut one_off, |fb| {
                                        let _ = run_analyze_on(fb, &req, top_n, req_id, &res_tx);
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
                        let result = run_grade(&engine, &start_fen, &moves, mv, req_id, &res_tx);
                        if let Err(e) = result {
                            let action = fallback_action(
                                &e,
                                &engine,
                                &notice_for_thread,
                                &mut illegal_count,
                            );
                            match action {
                                Fallback::None => {}
                                Fallback::RetryShared => {
                                    let _ =
                                        run_grade(&engine, &start_fen, &moves, mv, req_id, &res_tx);
                                }
                                Fallback::RetryOneOff => {
                                    retry_one_off(&engine, &mut one_off, |fb| {
                                        let _ = run_grade_on(
                                            fb, &start_fen, &moves, mv, req_id, &res_tx,
                                        );
                                    });
                                }
                            }
                        }
                    }
                    AiRequest::Stop => {
                        stop_for_thread.stop();
                    }
                }
                // Drain queued requests, keeping only the latest; a Stop
                // queued between searches still takes effect.
                while let Ok(q) = req_rx.try_recv() {
                    match q {
                        AiRequest::Stop => stop_for_thread.stop(),
                        other => pending = Some(other),
                    }
                }
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
    pub fn take_fallback_notice(&self) -> Option<String> {
        self.fallback_notice.lock().expect("notice lock").take()
    }

    /// Allocate the next request id.
    pub fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }

    /// Ask the running search to stop as soon as possible. Sets the flag
    /// the search loop polls directly (no queueing), and also sends the
    /// `Stop` request so the flag stays set between searches.
    pub fn request_stop(&self) {
        self.stop.stop();
        let _ = self.tx.send(AiRequest::Stop);
    }

    /// Non-blocking drain of all responses currently queued.
    pub fn drain_responses(&self) -> Vec<AiResponse> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            out.push(r);
        }
        out
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
        w.request_stop();
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
        w.request_stop();
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
}
