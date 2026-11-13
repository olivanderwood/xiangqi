//! ChessDB worker thread — a separate live scoring system (mirrors
//! `ai_worker.rs`).
//!
//! After every move the app asks ChessDB for the new position's
//! evaluation and shows it in its own UI section, in both Play-vs-AI
//! and Learn mode (Hieu, 2026-11-11). The worker owns one
//! [`ChessDbClient`] and a per-FEN cache, so a position asked twice
//! (e.g. after Undo, or when switching screens) is answered from
//! memory without another HTTP call.
//!
//! The worker never blocks the UI: it runs on its own thread and the
//! screens drain its responses once per frame. A failed request (no
//! network, timeout, bad status) is delivered as an `Err` and the UI
//! shows the section as unavailable — the game itself never depends
//! on ChessDB.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use xq_chessdb::{ChessDbClient, ChessDbError, ChessDbEval};

/// Requests to the ChessDB worker.
#[derive(Debug)]
pub enum ChessDbRequest {
    /// Evaluate one position (FEN, project N/B convention — the same
    /// FEN ChessDB expects).
    Evaluate { fen: String, req_id: u64 },
}

/// Responses from the ChessDB worker.
#[derive(Debug)]
pub enum ChessDbResponse {
    Evaluated {
        req_id: u64,
        result: Result<ChessDbEval, ChessDbError>,
    },
}

/// A response is current only if its `req_id` is the one the UI is
/// still waiting for. Anything else (e.g. an evaluation overtaken by
/// the next move) is stale and must be ignored.
#[must_use]
pub fn response_is_current(resp_req_id: u64, current: Option<u64>) -> bool {
    Some(resp_req_id) == current
}

/// What the ChessDB section shows right now.
#[derive(Clone, Debug)]
pub enum ChessDbView {
    /// No position has been asked about yet.
    Idle,
    /// Waiting for the worker's answer for this FEN.
    Loading,
    /// ChessDB's evaluation for the current position.
    Ready(ChessDbEval),
    /// ChessDB could not be reached (or had no data) for the current
    /// position. The message is shown in the section; the game is
    /// unaffected. `Ready` data for an earlier position is NOT kept:
    /// showing a stale score for a different position would mislead.
    Unavailable(String),
}

/// A fixed worker thread driving one ChessDB client + cache.
pub struct ChessDbWorker {
    pub tx: Sender<ChessDbRequest>,
    pub rx: Receiver<ChessDbResponse>,
}

impl ChessDbWorker {
    /// Spawn the worker thread.
    #[must_use]
    pub fn new() -> Self {
        let (req_tx, req_rx) = channel::<ChessDbRequest>();
        let (res_tx, res_rx) = channel::<ChessDbResponse>();
        std::thread::spawn(move || {
            let client = ChessDbClient::new();
            let mut cache: HashMap<String, ChessDbEval> = HashMap::new();
            while let Ok(req) = req_rx.recv() {
                // Cancel queued work (Hieu, 2026-11-12): when the user
                // moves again while a request is in flight, newer
                // requests queue up behind it. Only the latest position
                // matters, so drop every older queued request without
                // evaluating it — otherwise the worker falls behind and
                // the section keeps showing stale "loading" for positions
                // the user has already left. (The request already in
                // flight still finishes; its response is then ignored
                // by `req_id` on the UI side.)
                let mut req = req;
                while let Ok(newer) = req_rx.try_recv() {
                    req = newer;
                }
                let ChessDbRequest::Evaluate { fen, req_id } = req;
                let result = match cache.get(&fen) {
                    Some(hit) => Ok(hit.clone()),
                    None => match client.query_all(&fen) {
                        Ok(eval) => {
                            cache.insert(fen.clone(), eval.clone());
                            Ok(eval)
                        }
                        Err(err) => Err(err),
                    },
                };
                let _ = res_tx.send(ChessDbResponse::Evaluated { req_id, result });
            }
        });
        Self {
            tx: req_tx,
            rx: res_rx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_responses_are_ignored() {
        assert!(response_is_current(3, Some(3)));
        assert!(!response_is_current(2, Some(3)));
        assert!(!response_is_current(3, None));
    }

    #[test]
    fn view_variants_are_constructible() {
        let _ = ChessDbView::Idle;
        let _ = ChessDbView::Loading;
        let _ = ChessDbView::Unavailable("không có mạng".to_string());
        let eval = ChessDbEval {
            fen: "startpos".to_string(),
            score: Some(1),
            top: Vec::new(),
        };
        let _ = ChessDbView::Ready(eval);
    }
}
