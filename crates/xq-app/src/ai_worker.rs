//! AI worker thread (§2): requests/responses with `req_id` (M3/T3.3).
//!
//! Design notes (see `docs/DECISIONS-m3.md`):
//! - The worker owns one `Searcher`. The `Searcher`'s own stop flag is the
//!   one the search loop polls, so the worker hands that exact `Arc` back
//!   to the constructor and `AiWorker::stop` *is* the searcher's flag.
//!   `request_stop()` therefore takes effect immediately even while a
//!   search is running — sending `AiRequest::Stop` alone would queue
//!   behind the running search and could not interrupt it.
//! - The UI keeps the `req_id` of the request it is currently waiting for
//!   and drops every response whose `req_id` does not match
//!   (`response_is_current`). This is what makes Undo-while-thinking safe.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver, Sender},
    Arc,
};
use xq_core::board::Position;
use xq_core::search::{AnalysisLine, SearchLimits};
use xq_core::types::Move;

#[derive(Debug)]
pub enum AiRequest {
    BestMove {
        pos: Position,
        history: Vec<u64>,
        limits: SearchLimits,
        req_id: u64,
    },
    Analyze {
        pos: Position,
        history: Vec<u64>,
        top_n: usize,
        limits: SearchLimits,
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
}

impl AiResponse {
    /// `req_id` carried by this response.
    #[allow(dead_code)] // used by tests; the UI matches on the field directly
    #[must_use]
    pub fn req_id(&self) -> u64 {
        match self {
            AiResponse::Info { req_id, .. }
            | AiResponse::BestMove { req_id, .. }
            | AiResponse::Analysis { req_id, .. } => *req_id,
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

/// A fixed worker thread owning a `Searcher`.
pub struct AiWorker {
    pub tx: Sender<AiRequest>,
    pub rx: Receiver<AiResponse>,
    /// The `Searcher`'s own stop flag (shared with the worker thread).
    pub stop: Arc<AtomicBool>,
    pub next_req_id: u64,
}

impl AiWorker {
    #[must_use]
    pub fn new() -> Self {
        let (req_tx, req_rx) = channel::<AiRequest>();
        let (res_tx, res_rx) = channel::<AiResponse>();
        // The worker sends the Searcher's stop handle back once, so the
        // main thread can set the exact flag the search loop polls.
        let (stop_tx, stop_rx) = channel::<Arc<AtomicBool>>();
        std::thread::spawn(move || {
            let mut searcher = xq_core::search::Searcher::new(32);
            let searcher_stop = searcher.stop_handle();
            let _ = stop_tx.send(Arc::clone(&searcher_stop));
            while let Ok(req) = req_rx.recv() {
                match req {
                    AiRequest::BestMove {
                        pos,
                        history,
                        limits,
                        req_id,
                    } => {
                        let tx = res_tx.clone();
                        let result = searcher.search(&pos, &history, limits, &mut |info| {
                            let _ = tx.send(AiResponse::Info {
                                req_id,
                                depth: info.depth,
                                score: info.score,
                                pv: info.pv.clone(),
                            });
                        });
                        let _ = res_tx.send(AiResponse::BestMove {
                            req_id,
                            mv: result.best,
                            score: result.score,
                        });
                    }
                    AiRequest::Analyze {
                        pos,
                        history,
                        top_n,
                        limits,
                        req_id,
                    } => {
                        let lines = searcher.analyze(&pos, &history, top_n, limits);
                        let _ = res_tx.send(AiResponse::Analysis { req_id, lines });
                    }
                    AiRequest::Stop => {
                        searcher_stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        });
        let stop = stop_rx.recv().expect("AI worker thread started");
        Self {
            tx: req_tx,
            rx: res_rx,
            stop,
            next_req_id: 1,
        }
    }

    /// Allocate the next request id.
    pub fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }

    /// Ask the running search to stop as soon as possible. Sets the flag
    /// the search loop polls directly (no queueing), and also sends the
    /// `Stop` request so the worker's flag stays set between searches.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
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
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::board::Position;

    #[test]
    fn req_ids_increase() {
        let mut w = AiWorker::new();
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
        let w = AiWorker::new();
        assert!(!w.stop.load(Ordering::Relaxed));
        w.request_stop();
        assert!(w.stop.load(Ordering::Relaxed));
    }

    #[test]
    fn worker_returns_a_legal_best_move() {
        let mut w = AiWorker::new();
        let pos = Position::startpos();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            pos: pos.clone(),
            history: vec![pos.hash()],
            limits: SearchLimits::depth(1),
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
        // With the placeholder depth-1 search (M2 not landed yet) a search
        // finishes immediately; this test pins the contract that setting
        // the stop flag is instant and the worker still answers. The real
        // <=100 ms stop-latency check against a deep search belongs to
        // the M2 search tests (spec §8.6 `stop_flag`).
        let mut w = AiWorker::new();
        let pos = Position::startpos();
        let req_id = w.alloc_req_id();
        w.tx.send(AiRequest::BestMove {
            pos: pos.clone(),
            history: vec![pos.hash()],
            limits: SearchLimits::time_ms(8000),
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
}
