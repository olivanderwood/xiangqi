//! The "Điểm ChessDB" panel, shared by the Play and Learn screens.
//!
//! Ported from the egui app (`crates/xq-app/src/app.rs`:
//! `poll_chessdb` + `chessdb_section_ui`, plus
//! `crates/xq-iced/src/chessdb_worker.rs`). ChessDB is a separate
//! live scoring system (Hieu, 2026-11-11): after every move the app
//! asks ChessDB for the new position's evaluation and shows it in its
//! own section. The game never depends on it — no network means the
//! section shows "không khả dụng", never a crash.
//!
//! Wiring (Elm): the panel owns its worker thread. The worker's
//! receiver is handed to the bridge ([`crate::bridge::from_mpsc`],
//! one bridge per receiver, each panel with its own `bridge_id`)
//! and answers arrive as [`ChessDbPanelMsg::Response`] through
//! [`ChessDbPanel::subscription`]. Stale responses are dropped by
//! `req_id`, exactly as in the egui app.
//!
//! # Sharing rule (read before adding a second panel)
//!
//! The bridge recipe's identity is its `id`, so two panels sharing
//! one would collide: iced deduplicates subscriptions by hash, and
//! the hash is only the recipe type + the `id` — NOT the receiver.
//! Switching screens would then keep the first panel's forwarder
//! thread (still reading the first panel's receiver) and the new
//! screen's panel would never be read, even though only one panel
//! is subscribed at a time (2026-11-14: Practice never got a
//! ChessDB score or a hint after visiting Play). Every panel gets
//! its own `bridge_id` (see [`ChessDbPanel::new`]). If a future
//! screen needs ChessDB at the same time as another, move the
//! single panel into `Shared` instead of creating a second one.
//!
//! # Invariant (see `board.rs`)
//!
//! Call [`ChessDbPanel::request`] only after the piece has landed —
//! i.e. when the board is NOT animating. Requesting for a position
//! whose animation is still running would score a position the user
//! has not finished seeing.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use iced::widget::{column, container, text};
use iced::{Element, Length, Subscription, Task};
use xq_chessdb::ChessDbEval;
use xq_core::board::Position;
use xq_core::notation;

use crate::bridge;
use crate::chessdb_worker::{
    response_is_current, ChessDbRequest, ChessDbResponse, ChessDbView, ChessDbWorker,
};
use crate::helpers::render_move;
use crate::i18n::{self, Key};
use crate::settings::Settings;
use crate::theme::{FONT_BODY, SPACE_12, SPACE_16, SPACE_8};
use crate::widgets::{advantage_chip, card_style, section_title};

/// Messages for the ChessDB panel.
#[derive(Debug)]
pub enum ChessDbPanelMsg {
    /// An evaluation arrived from the worker (may be stale — the
    /// panel drops it unless its `req_id` is the one waited for).
    Response(ChessDbResponse),
}

/// The ChessDB section: one worker + per-FEN cache (in the worker) +
/// the `req_id` of the evaluation currently waited for.
pub struct ChessDbPanel {
    /// Whether the win/draw/loss rates are shown. Learn shows only
    /// the score chip + best move, no "%" (Hieu, 2026-11-12); Play
    /// shows the rates.
    pub show_win_rate: bool,
    /// The bridge subscription's identity (see `subscription`).
    bridge_id: &'static str,
    tx: Sender<ChessDbRequest>,
    rx: Arc<Mutex<Receiver<ChessDbResponse>>>,
    next_req_id: u64,
    view: ChessDbView,
    req_id: Option<u64>,
    last_fen: Option<String>,
}

impl ChessDbPanel {
    /// Create a panel (spawns its worker thread). `show_win_rate`
    /// is `true` for Play, `false` for Learn. `bridge_id` must be
    /// DISTINCT per panel (e.g. `"chessdb-play"`,
    /// `"chessdb-practice"`, `"chessdb-learn"`): the bridge
    /// recipe's identity is its `id`, so two panels sharing one
    /// would collide and iced would keep only the first panel's
    /// forwarder thread — the other panel's receiver would never
    /// be read (2026-11-14: Practice never got a ChessDB score
    /// or a hint after visiting Play, because its view stayed
    /// `Loading` and `maybe_suggest` never fell back to Pikafish).
    #[must_use]
    pub fn new(show_win_rate: bool, bridge_id: &'static str) -> Self {
        // Destructure the worker to take its receiver out for the
        // bridge (see `bridge.rs`: the forwarder thread is the
        // receiver's only consumer while bridged).
        let ChessDbWorker { tx, rx } = ChessDbWorker::new();
        Self {
            show_win_rate,
            bridge_id,
            tx,
            rx: Arc::new(Mutex::new(rx)),
            next_req_id: 0,
            view: ChessDbView::Idle,
            req_id: None,
            last_fen: None,
        }
    }

    /// What the panel currently shows (for tests / screens that need
    /// the raw evaluation).
    #[must_use]
    pub fn view_state(&self) -> &ChessDbView {
        &self.view
    }

    /// Ask for the evaluation of `fen` — but only when it differs
    /// from the last position asked about, so a screen re-render
    /// costs one string compare and a position change costs one HTTP
    /// call (cached per FEN in the worker). Only call this when the
    /// board is NOT animating (see the module docs).
    pub fn request(&mut self, fen: &str) {
        if self.last_fen.as_deref() == Some(fen) {
            return;
        }
        self.last_fen = Some(fen.to_string());
        self.next_req_id += 1;
        let req_id = self.next_req_id;
        let _ = self.tx.send(ChessDbRequest::Evaluate {
            fen: fen.to_string(),
            req_id,
        });
        self.req_id = Some(req_id);
        self.view = ChessDbView::Loading;
    }

    /// Handle a worker response. Stale responses (overtaken by the
    /// next move) are dropped by `req_id`.
    pub fn update(&mut self, msg: ChessDbPanelMsg) -> Task<ChessDbPanelMsg> {
        match msg {
            ChessDbPanelMsg::Response(ChessDbResponse::Evaluated { req_id, result }) => {
                if !response_is_current(req_id, self.req_id) {
                    return Task::none();
                }
                self.req_id = None;
                self.view = match result {
                    Ok(eval) => {
                        if eval.top.is_empty() {
                            ChessDbView::Unavailable(i18n::t(Key::ChessDbNoData).to_string())
                        } else {
                            ChessDbView::Ready(eval)
                        }
                    }
                    Err(err) => ChessDbView::Unavailable(err.to_string()),
                };
            }
        }
        Task::none()
    }

    /// The panel's subscription: worker answers arrive as
    /// [`ChessDbPanelMsg::Response`]. One bridge per receiver, with
    /// this panel's own `bridge_id` — two panels must never share
    /// an id, or iced keeps only the first forwarder thread (see
    /// [`ChessDbPanel::new`]).
    pub fn subscription(&self) -> Subscription<ChessDbPanelMsg> {
        bridge::from_mpsc(self.bridge_id, Arc::clone(&self.rx)).map(ChessDbPanelMsg::Response)
    }

    /// Render the section for `pos` (the position the section is
    /// about — used to render the best move in the user's notation).
    ///
    /// The best move is only rendered in the user's notation when it
    /// is actually legal in this position: ChessDB's answer can be
    /// for a different position than the one on screen (review ply,
    /// or a move made while the reply was in flight), and the
    /// notation renderers panic on an empty source square — that
    /// stale best move used to crash the app (0xc0000409, release
    /// `panic = "abort"`, 2026-11-12). An unparseable or illegal best
    /// move falls back to the raw ICCS string, never a panic.
    ///
    /// The section is its own card (radius 14, padding 16) so it
    /// reads as one separated block inside the side panel.
    pub fn view<'a>(&'a self, pos: &Position, settings: &Settings) -> Element<'a, ChessDbPanelMsg> {
        let mut col = column![section_title(i18n::t(Key::ChessDbTitle))].spacing(SPACE_8);
        match &self.view {
            ChessDbView::Idle | ChessDbView::Loading => {
                col = col.push(text(i18n::t(Key::ChessDbLoading)).size(FONT_BODY));
            }
            ChessDbView::Ready(eval) => {
                col = col.push(self.ready_view(pos, settings, eval));
            }
            ChessDbView::Unavailable(msg) => {
                col = col.push(text(msg).size(FONT_BODY));
            }
        }
        container(col.width(Length::Fill))
            .padding(SPACE_16)
            .width(Length::Fill)
            .style(card_style)
            .into()
    }

    /// The `Ready` branch of [`ChessDbPanel::view`].
    fn ready_view<'a>(
        &'a self,
        pos: &Position,
        settings: &Settings,
        eval: &'a ChessDbEval,
    ) -> Element<'a, ChessDbPanelMsg> {
        let mut col = column![].spacing(SPACE_8);
        if let Some(score) = eval.score {
            let mut score_row = iced::widget::row![advantage_chip(score)].spacing(SPACE_12);
            if self.show_win_rate {
                if let Some(best) = eval.top.first() {
                    score_row = score_row.push(
                        text(
                            i18n::t(Key::ChessDbWinRate)
                                .replace("{n}", &format!("{:.0}", best.winrate))
                                .replace("{m}", &format!("{:.0}", best.drawrate))
                                .replace("{k}", &format!("{:.0}", best.lossrate)),
                        )
                        .size(FONT_BODY),
                    );
                }
            }
            col = col.push(score_row);
        }
        if let Some(best) = eval.top.first() {
            let legal = pos.legal_moves();
            let name = notation::parse_iccs(&best.mv)
                .ok()
                .filter(|mv| legal.contains(mv))
                .map(|mv| render_move(pos, mv, settings.notation))
                .unwrap_or_else(|| best.mv.clone());
            col = col
                .push(text(i18n::t(Key::ChessDbBestMove).replace("{name}", &name)).size(FONT_BODY));
        }
        col.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three screens' panels must use distinct bridge ids:
    /// iced deduplicates subscriptions by recipe hash (type + id),
    /// so sharing one id makes only the first-visited screen's
    /// ChessDB work — the others' views stay `Loading` forever
    /// (2026-11-14: Practice got no hint and no ChessDB score
    /// after visiting Play).
    #[test]
    fn panels_use_distinct_bridge_ids() {
        let play = ChessDbPanel::new(true, "chessdb-play");
        let practice = ChessDbPanel::new(true, "chessdb-practice");
        let learn = ChessDbPanel::new(false, "chessdb-learn");
        assert_ne!(play.bridge_id, practice.bridge_id);
        assert_ne!(play.bridge_id, learn.bridge_id);
        assert_ne!(practice.bridge_id, learn.bridge_id);
    }
}
