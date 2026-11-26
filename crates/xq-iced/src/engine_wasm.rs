#![cfg(target_arch = "wasm32")]

//! Pikafish (WASM) engine bridge for the PWA spike.
//!
//! The desktop app drives Pikafish as a child process over UCI
//! (`xq-pikafish`); WASM has no child processes, so the engine runs in a
//! dedicated Web Worker (`assets/engine-wasm/pikafish-worker.js`, which
//! loads `pikafish.js` / `pikafish.wasm` / `pikafish.data`) and this module
//! talks to it with `postMessage`.
//!
//! Design (why it does NOT implement the sync [`xq_engine::Engine`]
//! trait): on `wasm32` there is no blocking wait — `best_move` cannot
//! return a [`xq_core::search::SearchResult`] synchronously. Instead this
//! module mirrors the [`crate::ai_worker`] channel shape: the caller sends
//! a request with a `req_id` and drains [`WasmEngineEvent`]s out of a
//! queue (poll it from the Iced update loop, e.g. on a timer tick — see
//! `INTEGRATION.md` in `assets/engine-wasm/`). The `req_id` discipline is
//! the same as [`crate::ai_worker::response_is_current`]: the UI keeps the
//! `req_id` it is waiting for and drops anything else, which is what makes
//! Undo-while-thinking safe.
//!
//! The worker enforces UCI discipline (one search at a time; `stop`
//! answers the running search with `bestmove` in ~1 ms, measured in Node
//! 24 — see `INTEGRATION.md`), and this module enforces the Rust side:
//! a request is only sent once the worker has reported `ready`, and a
//! `bestmove` is only accepted when it is legal in the requested position
//! (same rule as `ai_worker::run_best_move_on`).
//!
//! This file is gated to `wasm32` as a whole; on native targets the module
//! is empty. The coordinator still has to add the declaration (one line)
//! to `main.rs` — see `INTEGRATION.md` §3 — because this file may not
//! touch existing sources.
//!
//! Required dependencies (in `Cargo.toml`, wasm32-gated):
//! `wasm-bindgen = "0.2"`, `web-sys = "0.3"` with features
//! `Worker`, `MessageEvent`, `console`. Everything else used here
//! (`serde`, `serde_json`, `xq-core`, `xq-engine`) is already a
//! dependency of `xq-iced`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker};
use xq_core::eval::MATE;
use xq_core::notation::{iccs, parse_iccs};
use xq_core::search::AnalysisLine;
use xq_core::types::Move;
use xq_engine::EngineRequest;

/// Convert a UCI `score mate <n>` to centipawns, exactly like
/// [`xq_engine::Score::Mate`] via `to_cp_clamped`: mate in `n` moves is
/// `MATE - n`; being mated in `n` is `-(MATE + n)`.
fn mate_to_cp(mate: i32) -> i32 {
    if mate >= 0 {
        MATE - mate
    } else {
        -(MATE + mate)
    }
}

/// One message from the worker (always a JSON string; see
/// `pikafish-worker.js`). Unknown fields are ignored by serde.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerMsg {
    Ready {
        name: String,
    },
    Info {
        req_id: u64,
        depth: u8,
        #[serde(default = "default_multipv")]
        multipv: u8,
        score_cp: Option<i32>,
        mate: Option<i32>,
        #[serde(default)]
        pv: Vec<String>,
    },
    /// The worker posts `type: 'bestmove'` (no underscore); the
    /// enum's `rename_all = "snake_case"` would otherwise expect
    /// `"best_move"` and serde would drop every bestmove silently
    /// (2026-11-26: the AI never moved — info flowed, bestmove did
    /// not — because this rename was missing).
    #[serde(rename = "bestmove")]
    BestMove {
        req_id: u64,
        #[serde(rename = "move")]
        mv: String,
    },
    Log {
        stream: String,
        line: String,
    },
    /// Download progress for the 51.6 MB `.data` file (sent while the
    /// worker is still loading). The bridge ignores it — the app
    /// only needs `Ready` — but it must parse, or the message would
    /// warn and drop.
    Progress {
        #[allow(dead_code)]
        text: String,
    },
    Error {
        req_id: Option<u64>,
        message: String,
    },
}

fn default_multipv() -> u8 {
    1
}

/// Events the app drains out of [`WasmEngine::try_recv`] /
/// [`WasmEngine::drain_events`]. Fields mirror [`crate::ai_worker::AiResponse`]
/// (the `Ready` name and `Log` lines are public API for the engine
/// log / banner — not every field is read by the screens yet).
#[allow(dead_code)]
/// where the concepts overlap (`Info` / `BestMove` / `Analysis`), so a
/// future `WasmAiWorker` can translate them one-to-one.
#[derive(Debug)]
pub enum WasmEngineEvent {
    /// The worker finished the `uci` + `isready` handshake. `name` is the
    /// engine's `id name` line (verified: `Pikafish dev-20261103-nogit`).
    Ready { name: String },
    /// One `info depth ...` line for a running search. `score_cp` is the
    /// side-to-move's score (mate already converted with [`mate_to_cp`]).
    Info {
        req_id: u64,
        depth: u8,
        score_cp: i32,
        pv: Vec<Move>,
    },
    /// The search finished. `score_cp` is the score of the last `info`
    /// line (0 if the engine never sent one).
    BestMove {
        req_id: u64,
        mv: Move,
        score_cp: i32,
    },
    /// A finished MultiPV analysis: one line per `multipv` slot, best
    /// first. `tags` is empty — the consumer fills it the way
    /// `ai_worker::to_analysis_line` does (it needs the position, which
    /// this module does not keep after the request is sent).
    Analysis {
        req_id: u64,
        lines: Vec<AnalysisLine>,
    },
    /// An engine output line that is not a search result (`id`/`option`,
    /// `info string`, `bench` summary on stderr, ...). Surfaced so the
    /// engine-log panel keeps working on the PWA.
    Log { stream: String, line: String },
    /// The worker or the engine rejected something. Carries the `req_id`
    /// when the failure belongs to one request (`engine busy`,
    /// `engine not ready`, illegal bestmove); `None` for startup failures.
    Error {
        req_id: Option<u64>,
        message: String,
    },
}

/// What went wrong sending a request to the worker.
#[derive(Debug)]
pub enum WasmEngineError {
    /// The worker has not reported `ready` yet (still downloading the
    /// 51.6 MB `.data` file or handshaking). Wait for
    /// [`WasmEngineEvent::Ready`] and retry.
    NotReady,
    /// A request with this `req_id` is already in flight.
    DuplicateReqId(u64),
    /// `postMessage` to the worker failed (worker gone).
    PostFailed(String),
}

impl std::fmt::Display for WasmEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => write!(f, "engine WASM chưa sẵn sàng"),
            Self::DuplicateReqId(id) => write!(f, "req_id {id} đang chạy"),
            Self::PostFailed(e) => write!(f, "không gửi được tới worker: {e}"),
        }
    }
}

impl std::error::Error for WasmEngineError {}

/// State kept per in-flight request, keyed by `req_id`.
struct Pending {
    /// The request, kept so a `bestmove` can be checked for legality
    /// against the position it was searched from.
    req: EngineRequest,
    /// `true` for [`WasmEngine::request_analyze`]: the `bestmove` then
    /// answers with [`WasmEngineEvent::Analysis`] instead of `BestMove`.
    analyze: bool,
    /// Score of the last `info` line (0 until the first one arrives).
    last_score_cp: i32,
    /// Collected analysis lines, one slot per MultiPV index (0-based).
    lines: Vec<Option<AnalysisLine>>,
}

/// The Pikafish WASM engine, running in a Web Worker.
///
/// Single-threaded by construction (`Rc`, no `Send`): hold it on the main
/// thread only. There is exactly one search at a time — the worker rejects
/// a second `go` with `engine busy`, and so does [`Self::request_best_move`]
/// (via the duplicate-`req_id` / busy check below is per-request; the
/// caller must not overlap requests, exactly like the desktop worker
/// thread serialises them behind the shared-engine mutex).
pub struct WasmEngine {
    worker: Worker,
    /// Keeps the `onmessage` closure alive for the engine's lifetime;
    /// dropping it would unregister the handler.
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    pending: Rc<RefCell<HashMap<u64, Pending>>>,
    events: Rc<RefCell<VecDeque<WasmEngineEvent>>>,
    ready: Rc<Cell<bool>>,
    name: Rc<RefCell<String>>,
}

impl WasmEngine {
    /// Spawn the worker at `worker_url` (relative to the page: Trunk
    /// serves `dist/engine-wasm/pikafish-worker.js`, so pass
    /// `"engine-wasm/pikafish-worker.js"` — see `INTEGRATION.md` §4).
    /// The engine is NOT usable until [`WasmEngineEvent::Ready`] is
    /// drained; requests sent before that fail with
    /// [`WasmEngineError::NotReady`].
    pub fn new(worker_url: &str) -> Result<Self, JsValue> {
        let worker = Worker::new(worker_url)?;
        let pending: Rc<RefCell<HashMap<u64, Pending>>> = Rc::new(RefCell::new(HashMap::new()));
        let events: Rc<RefCell<VecDeque<WasmEngineEvent>>> = Rc::new(RefCell::new(VecDeque::new()));
        let ready = Rc::new(Cell::new(false));
        let name = Rc::new(RefCell::new(String::new()));

        let p = Rc::clone(&pending);
        let ev = Rc::clone(&events);
        let r = Rc::clone(&ready);
        let n = Rc::clone(&name);
        // No user code runs inside this closure — it only parses the
        // worker message and queues an event (or updates per-request
        // state), so a callback can never re-enter the engine mid-dispatch
        // and trip a `RefCell` borrow.
        let onmessage = Closure::new(move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };
            let msg: WorkerMsg = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "pikafish-worker: bad message {e}: {text}"
                    )));
                    return;
                }
            };
            let mut events = ev.borrow_mut();
            match msg {
                WorkerMsg::Ready { name: engine_name } => {
                    r.set(true);
                    *n.borrow_mut() = engine_name.clone();
                    events.push_back(WasmEngineEvent::Ready { name: engine_name });
                }
                WorkerMsg::Info {
                    req_id,
                    depth,
                    multipv,
                    score_cp,
                    mate,
                    pv,
                } => {
                    let score = score_cp.or_else(|| mate.map(mate_to_cp)).unwrap_or(0);
                    let pv_moves: Vec<Move> =
                        pv.iter().filter_map(|s| parse_iccs(s).ok()).collect();
                    let mut pending = p.borrow_mut();
                    if let Some(entry) = pending.get_mut(&req_id) {
                        entry.last_score_cp = score;
                        if entry.analyze {
                            let idx = multipv.saturating_sub(1) as usize;
                            if idx < entry.lines.len() {
                                if let Some(&mv) = pv_moves.first() {
                                    entry.lines[idx] = Some(AnalysisLine {
                                        mv,
                                        score,
                                        depth,
                                        pv: pv_moves.clone(),
                                        tags: Vec::new(),
                                    });
                                }
                            }
                        }
                    } else {
                        return; // stale info (request already answered)
                    }
                    events.push_back(WasmEngineEvent::Info {
                        req_id,
                        depth,
                        score_cp: score,
                        pv: pv_moves,
                    });
                }
                WorkerMsg::BestMove { req_id, mv } => {
                    let mut pending = p.borrow_mut();
                    let Some(entry) = pending.remove(&req_id) else {
                        return; // stale bestmove
                    };
                    let mv = match parse_iccs(&mv) {
                        Ok(m) => m,
                        Err(_) => {
                            events.push_back(WasmEngineEvent::Error {
                                req_id: Some(req_id),
                                message: format!("bestmove không hợp lệ: {mv}"),
                            });
                            return;
                        }
                    };
                    // Legality check, like `ai_worker::run_best_move_on`:
                    // never play garbage, even from the real engine.
                    match entry.req.position() {
                        Ok(pos) if pos.legal_moves().contains(&mv) => {}
                        Ok(_) => {
                            events.push_back(WasmEngineEvent::Error {
                                req_id: Some(req_id),
                                message: format!(
                                    "nước không hợp lệ: {} ở thế {}",
                                    iccs(mv),
                                    entry.req.start_fen
                                ),
                            });
                            return;
                        }
                        Err(e) => {
                            events.push_back(WasmEngineEvent::Error {
                                req_id: Some(req_id),
                                message: format!("FEN không hợp lệ: {e}"),
                            });
                            return;
                        }
                    }
                    if entry.analyze {
                        let mut lines: Vec<AnalysisLine> =
                            entry.lines.into_iter().flatten().collect();
                        lines.sort_by_key(|l| std::cmp::Reverse(l.score));
                        events.push_back(WasmEngineEvent::Analysis { req_id, lines });
                    } else {
                        events.push_back(WasmEngineEvent::BestMove {
                            req_id,
                            mv,
                            score_cp: entry.last_score_cp,
                        });
                    }
                }
                WorkerMsg::Log { stream, line } => {
                    events.push_back(WasmEngineEvent::Log { stream, line });
                }
                WorkerMsg::Progress { .. } => {
                    // Loading progress for the 51.6 MB `.data` file:
                    // no event — the app only waits for `Ready`.
                }
                WorkerMsg::Error { req_id, message } => {
                    if let Some(id) = req_id {
                        p.borrow_mut().remove(&id);
                    }
                    events.push_back(WasmEngineEvent::Error { req_id, message });
                }
            }
        });
        worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        Ok(Self {
            worker,
            _onmessage: onmessage,
            pending,
            events,
            ready,
            name,
        })
    }

    /// Whether the worker finished the `uci` + `isready` handshake.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.get()
    }

    /// The engine's `id name` line once ready (empty before that).
    #[must_use]
    pub fn name(&self) -> String {
        self.name.borrow().clone()
    }

    /// Next event, if any. Drain in the Iced update loop until it
    /// returns `None`.
    #[must_use]
    pub fn try_recv(&self) -> Option<WasmEngineEvent> {
        self.events.borrow_mut().pop_front()
    }

    /// Drain all queued events into `f`.
    /// (Convenience API, not wired to a screen yet.)
    #[allow(dead_code)]
    pub fn drain_events(&self, mut f: impl FnMut(WasmEngineEvent)) {
        while let Some(ev) = self.try_recv() {
            f(ev);
        }
    }

    /// Send one `go` for `req` (best move). Limits come from
    /// `req.limits` (`max_depth` / `max_time_ms`, exactly like the
    /// desktop `go_command` in `xq-pikafish/src/uci.rs` — both caps are
    /// sent when both are set, and a 1000 ms movetime is the default when
    /// neither is set). Use [`xq_pikafish::levels::pikafish_limits`] to
    /// build the limits for a [`xq_engine::Level`], the same table the
    /// desktop engine uses.
    pub fn request_best_move(
        &self,
        req: &EngineRequest,
        req_id: u64,
    ) -> Result<(), WasmEngineError> {
        self.request_go(req, req_id, None, None)
    }

    /// Send one MultiPV analysis (`top_n` lines, best first, answered as
    /// [`WasmEngineEvent::Analysis`]). Same limits as
    /// [`Self::request_best_move`].
    pub fn request_analyze(
        &self,
        req: &EngineRequest,
        top_n: usize,
        req_id: u64,
    ) -> Result<(), WasmEngineError> {
        self.request_go(req, req_id, Some(top_n.max(1)), None)
    }

    /// Score one specific move (the `GradeMove` building block): a `go`
    /// restricted to `mv` with `searchmoves`. The answer is a
    /// [`WasmEngineEvent::BestMove`] whose `mv` is the requested move and
    /// whose `score_cp` is that move's search score.
    pub fn request_score_move(
        &self,
        req: &EngineRequest,
        mv: Move,
        req_id: u64,
    ) -> Result<(), WasmEngineError> {
        self.request_go(req, req_id, None, Some(mv))
    }

    fn request_go(
        &self,
        req: &EngineRequest,
        req_id: u64,
        multipv: Option<usize>,
        searchmoves: Option<Move>,
    ) -> Result<(), WasmEngineError> {
        if !self.ready.get() {
            return Err(WasmEngineError::NotReady);
        }
        if self.pending.borrow().contains_key(&req_id) {
            return Err(WasmEngineError::DuplicateReqId(req_id));
        }
        let mut msg = json!({
            "type": "go",
            "req_id": req_id,
            "fen": req.start_fen,
            "moves": req.moves.iter().map(|m| iccs(*m)).collect::<Vec<_>>(),
        });
        if let Some(d) = req.limits.max_depth {
            msg["depth"] = json!(d);
        }
        if let Some(ms) = req.limits.max_time_ms {
            msg["movetime"] = json!(ms);
        }
        if let Some(n) = multipv {
            msg["multipv"] = json!(n);
        }
        if let Some(mv) = searchmoves {
            msg["searchmoves"] = json!(iccs(mv));
        }
        self.post(&msg)?;
        let slots = multipv.unwrap_or(1);
        self.pending.borrow_mut().insert(
            req_id,
            Pending {
                req: req.clone(),
                analyze: multipv.is_some(),
                last_score_cp: 0,
                lines: vec![None; slots],
            },
        );
        Ok(())
    }

    /// Stop the running search. The engine still answers with
    /// `bestmove` (~1 ms measured), so the pending request is answered
    /// normally — `stop` does not cancel the event. A no-op when idle.
    pub fn stop(&self) -> Result<(), WasmEngineError> {
        if !self.ready.get() {
            return Ok(());
        }
        self.post(&json!({ "type": "stop" }))
    }

    /// Reset the engine for a new game (`ucinewgame`: clears the
    /// transposition table and the repetition history).
    /// (Not wired to a screen yet.)
    #[allow(dead_code)]
    pub fn new_game(&self) -> Result<(), WasmEngineError> {
        if !self.ready.get() {
            return Ok(());
        }
        self.post(&json!({ "type": "ucinewgame" }))
    }

    /// Shut the engine down: send `quit` to the worker, then terminate
    /// it. Infallible — called on app exit.
    /// (Not wired to a screen yet.)
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.post(&json!({ "type": "quit" }));
        self.worker.terminate();
    }

    fn post(&self, msg: &serde_json::Value) -> Result<(), WasmEngineError> {
        self.worker
            .post_message(&JsValue::from_str(&msg.to_string()))
            .map_err(|e| WasmEngineError::PostFailed(format!("{e:?}")))
    }
}
