//! App shell: `Screen` enum + top-level `eframe::App` (M3/M5).
//!
//! Screen bodies live in `screens/` as `impl XiangqiApp` methods; this
//! file holds shared state, font setup (§7.5), settings persistence
//! (§6.6) and small shared widgets (move list, eval bar).

use crate::ai_worker::{AiResponse, AiWorker};
use crate::board_view::eval_bar_fraction;
use crate::chessdb_worker::{ChessDbResponse, ChessDbView, ChessDbWorker};
use crate::engine_log::EngineLog;
use crate::engine_manager::{open_folder, spawn_install, EngineEvent};
use crate::i18n::{self, Key};
use crate::screens::learn::LearnState;
use crate::screens::learn_select::LearnSelectState;
use crate::screens::summary::SummaryState;
use crate::screens::vs_ai::VsAiState;
use crate::settings::{NotationChoice, Settings};
use egui::{FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use xq_book::Book;
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::Game;
use xq_core::notation;
use xq_core::types::Move;
use xq_engine::SharedEngine;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Menu,
    VsAi,
    LearnSelect,
    Learn,
    Summary,
}

pub struct XiangqiApp {
    pub screen: Screen,
    pub settings: Settings,
    pub settings_dirty: bool,
    pub show_settings: bool,
    pub show_about: bool,
    pub book: Book,
    pub worker: AiWorker,
    /// Worker responses not yet consumed by the screen waiting for them.
    /// Buffered centrally so switching screens mid-search cannot lose a
    /// response another screen is still waiting for.
    pub pending_ai: Vec<AiResponse>,
    /// ChessDB worker (a separate live scoring system, §ChessDB): one
    /// request per position change, answered on its own thread.
    pub chessdb: ChessDbWorker,
    /// What the ChessDB section shows for the current position.
    pub chessdb_view: ChessDbView,
    /// `req_id` of the ChessDB evaluation the UI is waiting for.
    pub chessdb_req_id: Option<u64>,
    /// FEN of the last position a ChessDB evaluation was requested for
    /// (avoids re-asking for the same position every frame).
    pub chessdb_last_fen: Option<String>,
    pub vs_ai: VsAiState,
    pub learn_select: LearnSelectState,
    pub learn: LearnState,
    pub summary: SummaryState,
    /// The shared engine (also held by the worker and the screens).
    pub engine: SharedEngine,
    /// Display name of the active engine ("AI tích hợp" until Pikafish
    /// is installed).
    pub engine_name: String,
    /// Directory holding the engine binary + NNUE, when Pikafish runs.
    pub engine_dir: Option<PathBuf>,
    /// One-time fallback banner (startup failure or mid-game fallback).
    pub engine_notice: Option<String>,
    /// Pending Pikafish install (startup pre-warm / retry button).
    pub engine_install_rx: Option<Receiver<EngineEvent>>,
    /// Rotating engine log (`None` when the log dir is unavailable).
    pub engine_log: Option<Arc<EngineLog>>,
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sans".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSans-Regular-subset.ttf"
        ))),
    );
    fonts.font_data.insert(
        "noto-sans-bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSans-Bold-subset.ttf"
        ))),
    );
    fonts.font_data.insert(
        "cjk".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSerifCJKSC-Regular-subset.ttf"
        ))),
    );
    fonts.font_data.insert(
        "cjk-bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSerifCJKSC-Bold-subset.ttf"
        ))),
    );
    // UI text: Noto Sans (Vietnamese) first, CJK as fallback for Han chars
    // inside notation strings.
    fonts.families.insert(
        FontFamily::Proportional,
        vec!["noto-sans".into(), "cjk".into()],
    );
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["noto-sans".into()]);
    fonts.families.insert(
        FontFamily::Name("cjk".into()),
        vec!["cjk".into(), "noto-sans".into()],
    );
    fonts.families.insert(
        FontFamily::Name("cjk-bold".into()),
        vec!["cjk-bold".into(), "noto-sans-bold".into()],
    );
    ctx.set_fonts(fonts);
    // One type scale for the whole app (§7.5): every screen reads from
    // these styles, so text and buttons stay big and uniform. The egui
    // defaults (Body 14, Heading 22) read small on a game-board UI.
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(32.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(17.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(18.0, egui::FontFamily::Monospace),
    );
    // Generous button padding + spacing, so every button and row has
    // the same substantial feel on every screen.
    style.spacing.button_padding = egui::Vec2::new(18.0, 12.0);
    style.spacing.item_spacing = egui::Vec2::new(10.0, 8.0);
    ctx.set_style(style);
}

/// A button with the app's uniform minimum size (§7.5): every action
/// button on every screen is at least 120 x 46 px, so buttons look
/// equally big and are easy to tap. Use for all panel/dialog buttons;
/// the Menu and lesson-picker cards use their own larger `add_sized`.
#[must_use]
pub fn big_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(egui::Vec2::new(120.0, 46.0)))
}

/// A section title, the same on every screen: strong 24 px, then a
/// small gap. Sections themselves are separated by `ui.separator()`.
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().size(24.0));
    ui.add_space(4.0);
}

/// Advantage of a hint score (centipawns, side-to-move's perspective)
/// as a short pawn string: `+0.8`, `-0.3`, `M3` for mate in 3.
#[must_use]
pub fn format_advantage(cp: i32) -> String {
    if cp >= eval::MATE - 500 {
        return format!("M{}", (eval::MATE - cp + 1) / 2);
    }
    if cp <= -eval::MATE + 500 {
        return format!("-M{}", (eval::MATE + cp + 1) / 2);
    }
    format!("{:+.1}", cp as f32 / 100.0)
}

/// Cell colour for an advantage score: green when the learner is
/// better, red when worse, grey when level.
#[must_use]
pub fn advantage_color(cp: i32) -> egui::Color32 {
    if cp > 0 {
        egui::Color32::from_rgb(0x2E, 0x7D, 0x32)
    } else if cp < 0 {
        egui::Color32::from_rgb(0xC0, 0x39, 0x2B)
    } else {
        egui::Color32::GRAY
    }
}

/// A big, clear score cell (§7.5): a tinted rounded chip with the
/// advantage number in bold 28 px, at least 110 px wide so the column
/// lines up across hint rows. The number is the point — it must read
/// at a glance, unlike a plain 18 px label.
pub fn advantage_chip_ui(ui: &mut egui::Ui, cp: i32) -> egui::Response {
    let color = advantage_color(cp);
    let bg = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 36);
    egui::Frame::default()
        .fill(bg)
        .stroke(egui::Stroke::new(1.5_f32, color))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.set_min_width(96.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format_advantage(cp))
                        .strong()
                        .size(28.0)
                        .color(color),
                );
            });
        })
        .response
}

impl XiangqiApp {
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let settings = cc.storage.map(Settings::load).unwrap_or_default();
        let book = Book::load_embedded().unwrap_or(Book {
            lessons: Vec::new(),
        });
        // One engine shared by the worker thread and the UI's synchronous
        // grading paths (Pikafish integration spec §1.3). Builtin first;
        // Pikafish is pre-warmed on a background thread below and swaps
        // in when it is ready.
        let engine = xq_engine::shared_engine(Box::new(xq_engine::BuiltinEngine::new()));
        let level = settings.level();
        // Rotating engine log next to eframe's storage; a missing log
        // dir disables logging instead of breaking startup.
        let app_title = i18n::t(Key::AppTitle).to_string();
        let engine_log = eframe::storage_dir(&app_title).and_then(|d| EngineLog::new(&d));
        if let Some(log) = &engine_log {
            log.set_trace_enabled(settings.engine_trace_log);
            let sink = Arc::clone(log);
            xq_pikafish::set_log_sink(Some(std::sync::Arc::new(move |level, msg| {
                sink.write(level, msg);
            })));
        }
        // Pre-warm Pikafish in the background (§7.2): the first game
        // does not wait for the NNUE to load.
        let engine_install_rx = Some(spawn_install(
            engine.clone(),
            settings.pikafish_path(),
            settings.engine_threads,
            settings.engine_hash_mb,
            level,
        ));
        Self {
            screen: Screen::Menu,
            settings,
            settings_dirty: false,
            show_settings: false,
            show_about: false,
            book,
            worker: AiWorker::new(engine.clone()),
            pending_ai: Vec::new(),
            chessdb: ChessDbWorker::new(),
            chessdb_view: ChessDbView::Idle,
            chessdb_req_id: None,
            chessdb_last_fen: None,
            vs_ai: VsAiState::new(engine.clone()),
            learn_select: LearnSelectState::new(),
            learn: LearnState::new(engine.clone()),
            summary: SummaryState::new(),
            engine: engine.clone(),
            engine_name: "AI tích hợp".to_string(),
            engine_dir: None,
            engine_notice: None,
            engine_install_rx,
            engine_log,
        }
    }

    /// Render one move with the user's chosen notation (§6.6).
    #[must_use]
    pub fn render_move(&self, pos: &Position, mv: Move) -> String {
        match self.settings.notation {
            NotationChoice::Chinese => notation::render_cn(pos, mv),
            NotationChoice::Vietnamese => notation::render_vn(pos, mv),
        }
    }

    /// Drain ChessDB responses and (re)request when the position
    /// changed. Called once per frame by the active screen with its
    /// current FEN. A request goes out only when the FEN differs from
    /// the last one asked about, so a frame costs one string compare
    /// and a position change costs one HTTP call (cached per FEN in
    /// the worker). Stale responses (overtaken by the next move) are
    /// dropped by `req_id`.
    ///
    /// DIAG (crash 0xc0000409, 2026-11-12): worker đã bật lại để
    /// khoanh vùng — chỉ có section vẽ là còn tắt. Nếu bản này vẫn
    /// crash -> lỗi ở worker (HTTP/parse); nếu hết crash -> lỗi ở
    /// phần vẽ section.
    pub fn poll_chessdb(&mut self, fen: &str, ctx: &egui::Context) {
        while let Ok(resp) = self.chessdb.rx.try_recv() {
            let ChessDbResponse::Evaluated { req_id, result } = resp;
            if !crate::chessdb_worker::response_is_current(req_id, self.chessdb_req_id) {
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
            let req_id = self.chessdb.request(fen.to_string());
            self.chessdb_req_id = Some(req_id);
            self.chessdb_view = ChessDbView::Loading;
        }
    }

    /// The ChessDB section (a separate live scoring system): the
    /// position score, Red's win/draw/loss, and the best move, for the
    /// current position. The score and the rates are from Red's
    /// perspective (ChessDB's own convention), matching the eval bar;
    /// the section is a second opinion next to the app's own advantage
    /// chip, not a replacement for it. Shown in both the Play-vs-AI
    /// and Learn panels. `pos` is the position the section is about
    /// (used to render the best move in the user's notation).
    ///
    /// DIAG (crash 0xc0000409 vẫn còn sau khi bỏ ureq, 2026-11-12):
    /// section đang tắt hẳn để khoanh vùng. Bỏ `#[allow]` + `return`
    /// để bật lại.
    #[allow(unreachable_code)]
    pub fn chessdb_section_ui(&self, ui: &mut egui::Ui, pos: &Position) {
        let _ = (ui, pos);
        return;
        section_title(ui, i18n::t(Key::ChessDbTitle));
        match &self.chessdb_view {
            ChessDbView::Idle | ChessDbView::Loading => {
                ui.label(i18n::t(Key::ChessDbLoading));
            }
            ChessDbView::Ready(eval) => {
                if let Some(score) = eval.score {
                    ui.horizontal(|ui| {
                        advantage_chip_ui(ui, score);
                        if let Some(best) = eval.top.first() {
                            ui.label(
                                i18n::t(Key::ChessDbWinRate)
                                    .replace("{n}", &format!("{:.0}", best.winrate))
                                    .replace("{m}", &format!("{:.0}", best.drawrate))
                                    .replace("{k}", &format!("{:.0}", best.lossrate)),
                            );
                        }
                    });
                }
                if let Some(best) = eval.top.first() {
                    // The best move in the user's notation; fall back
                    // to the raw ICCS when it does not parse (it always
                    // should — the FEN and the move come from ChessDB
                    // for this same position).
                    let name = notation::parse_iccs(&best.mv)
                        .ok()
                        .map(|mv| self.render_move(pos, mv))
                        .unwrap_or_else(|| best.mv.clone());
                    ui.label(i18n::t(Key::ChessDbBestMove).replace("{name}", &name));
                }
            }
            ChessDbView::Unavailable(msg) => {
                ui.label(msg);
            }
        }
    }

    /// Position after `ply` moves of `game` (0 = start position).
    #[must_use]
    pub fn position_at(game: &Game, ply: usize) -> Position {
        let mut p = game.start.clone();
        for m in game.moves.iter().take(ply) {
            p.make_move(*m);
        }
        p
    }

    /// All moves of `game` rendered in the chosen notation, in order.
    #[must_use]
    pub fn move_strings(&self, game: &Game) -> Vec<String> {
        let mut out = Vec::with_capacity(game.moves.len());
        let mut p = game.start.clone();
        for m in &game.moves {
            out.push(self.render_move(&p, *m));
            p.make_move(*m);
        }
        out
    }

    /// Go to a screen, resetting per-screen transient dialogs.
    pub fn goto(&mut self, screen: Screen) {
        self.screen = screen;
    }

    /// One-time engine fallback banner (§7.3), shown at the top of
    /// every screen until dismissed.
    fn engine_banner(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = self.engine_notice.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(0xC0, 0x39, 0x2B), "⚠");
            ui.label(&notice);
            if ui.button("✕").clicked() {
                self.engine_notice = None;
            }
        });
        ui.separator();
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new(i18n::t(Key::Settings))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                changed |= ui
                    .checkbox(&mut self.settings.sound, i18n::t(Key::SoundOn))
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.animate, i18n::t(Key::AnimateLabel))
                    .changed();
                ui.label(i18n::t(Key::NotationLabel));
                changed |= ui
                    .radio_value(
                        &mut self.settings.notation,
                        NotationChoice::Vietnamese,
                        i18n::t(Key::NotationVn),
                    )
                    .changed();
                changed |= ui
                    .radio_value(
                        &mut self.settings.notation,
                        NotationChoice::Chinese,
                        i18n::t(Key::NotationCn),
                    )
                    .changed();
                section_title(ui, i18n::t(Key::Difficulty));
                for d in 0..=4u8 {
                    changed |= ui
                        .radio_value(&mut self.settings.difficulty, d, i18n::difficulty_name(d))
                        .changed();
                }
                if self.settings.difficulty == 4 {
                    ui.horizontal(|ui| {
                        ui.label(i18n::t(Key::EngineCustomElo));
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut self.settings.custom_elo, 1200..=3000)
                                    .step_by(50.0),
                            )
                            .changed();
                        ui.label(format!("Elo {}", self.settings.custom_elo));
                    });
                }
                ui.separator();
                ui.heading(i18n::t(Key::EngineSection));
                ui.label(i18n::t(Key::EngineInUse).replace("{n}", &self.engine_name));
                ui.horizontal(|ui| {
                    ui.label(i18n::t(Key::EngineThreads));
                    changed |= ui
                        .add(egui::Slider::new(&mut self.settings.engine_threads, 1..=8))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(i18n::t(Key::EngineHash));
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.settings.engine_hash_mb, 0..=1024)
                                .step_by(64.0),
                        )
                        .changed();
                    if self.settings.engine_hash_mb == 0 {
                        ui.label(i18n::t(Key::EngineHashAuto));
                    } else {
                        ui.label(format!("{} MB", self.settings.engine_hash_mb));
                    }
                });
                changed |= ui
                    .checkbox(&mut self.settings.diversity, i18n::t(Key::EngineDiversity))
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.settings.tournament,
                        i18n::t(Key::EngineTournament),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.settings.engine_trace_log,
                        i18n::t(Key::EngineTraceLog),
                    )
                    .changed();
                ui.label(i18n::t(Key::EnginePath));
                changed |= ui
                    .text_edit_singleline(&mut self.settings.pikafish_path)
                    .changed();
                if big_button(ui, i18n::t(Key::EngineRetry)).clicked() {
                    // Re-run the install with the current settings; the
                    // outcome arrives via `engine_install_rx` like the
                    // startup pre-warm. Disabled while one is running.
                    if self.engine_install_rx.is_none() {
                        let level = self.settings.level();
                        self.engine_install_rx = Some(spawn_install(
                            self.engine.clone(),
                            self.settings.pikafish_path(),
                            self.settings.engine_threads,
                            self.settings.engine_hash_mb,
                            level,
                        ));
                    }
                }
                ui.label(i18n::t(Key::AboutNoSound));
            });
        if !open {
            self.show_settings = false;
        }
        if changed {
            self.settings_dirty = true;
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        let version = env!("CARGO_PKG_VERSION");
        egui::Window::new(i18n::t(Key::About))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(i18n::t(Key::AboutBody).replace("{n}", version));
                ui.label(i18n::t(Key::AboutEngineBody));
                ui.label(i18n::t(Key::AboutEngine).replace("{n}", &self.engine_name));
                ui.label(i18n::t(Key::AboutNnue));
                ui.horizontal(|ui| {
                    if let Some(dir) = &self.engine_dir {
                        if big_button(ui, i18n::t(Key::OpenEngineFolder)).clicked() {
                            open_folder(dir);
                        }
                    }
                    if let Some(log) = &self.engine_log {
                        if big_button(ui, i18n::t(Key::OpenEngineLog)).clicked() {
                            if let Some(parent) = log.path().parent() {
                                open_folder(parent);
                            }
                        }
                    }
                });
                ui.label(i18n::t(Key::AboutFonts));
            });
        if !open {
            self.show_about = false;
        }
    }
}

impl eframe::App for XiangqiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Keep the log's trace switch in sync with Settings.
        if let Some(log) = &self.engine_log {
            log.set_trace_enabled(self.settings.engine_trace_log);
        }
        // Pikafish install outcome (startup pre-warm / retry button).
        if let Some(rx) = &self.engine_install_rx {
            if let Ok(event) = rx.try_recv() {
                self.engine_install_rx = None;
                match event {
                    EngineEvent::Installed { name, dir } => {
                        self.engine_name = name;
                        self.engine_dir = Some(dir);
                        self.engine_notice = None;
                    }
                    EngineEvent::Unavailable(reason) => {
                        self.engine_name = "AI tích hợp".to_string();
                        self.engine_dir = None;
                        self.engine_notice = Some(reason);
                    }
                }
            }
        }
        // Mid-game fallback (worker swapped to Builtin): show once.
        if let Some(notice) = self.worker.take_fallback_notice() {
            xq_pikafish::log::log_warn(&notice);
            self.engine_notice = Some(notice);
            self.engine_name = "AI tích hợp".to_string();
            self.engine_dir = None;
        }
        // Drain the worker once per frame; screens consume from the buffer
        // (unmatched responses stay buffered for the screen waiting for them).
        let fresh = self.worker.drain_responses();
        self.pending_ai.extend(fresh);
        // Drop responses nobody is waiting for anymore (stale req_ids).
        let wanted = [
            self.vs_ai.ai_req_id,
            self.vs_ai.hint_req_id,
            self.vs_ai.diverse_req_id,
            self.learn.opponent_ai_req,
        ];
        self.pending_ai
            .retain(|r| wanted.contains(&Some(r.req_id())));
        egui::CentralPanel::default().show(ctx, |ui| {
            self.engine_banner(ui);
            match self.screen {
                Screen::Menu => self.show_menu(ui),
                Screen::VsAi => self.show_vs_ai(ctx, ui),
                Screen::LearnSelect => self.show_learn_select(ctx, ui),
                Screen::Learn => self.show_learn(ctx, ui),
                Screen::Summary => self.show_summary(ctx, ui),
            }
        });
        self.settings_window(ctx);
        self.about_window(ctx);
        if self.settings_dirty {
            if let Some(storage) = frame.storage_mut() {
                self.settings.save(storage);
            }
            self.settings_dirty = false;
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.save(storage);
    }

    /// Quit the engine subprocess on exit (§7.2): stop any running
    /// search first, then `quit` (waits up to 2 s, then kills).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.engine.stop_handle().stop();
        if let Ok(mut guard) = self.engine.lock() {
            guard.shutdown();
        }
    }
}

/// Two-column (Red / Black) scrollable move list (§7.4).
/// Clicking a row enters review mode at that ply (`review` = ply count).
pub fn move_list_ui(ui: &mut egui::Ui, moves: &[String], review: &mut Option<usize>) {
    section_title(ui, i18n::t(Key::MovesTitle));
    let height = 200.0;
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            egui::Grid::new("move_list_grid")
                .num_columns(3)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    let mut i = 0;
                    while i < moves.len() {
                        let no = i / 2 + 1;
                        ui.label(format!("{no}."));
                        for (j, mv) in moves.iter().skip(i).take(2).enumerate() {
                            let ply = i + j + 1;
                            let selected = *review == Some(ply);
                            if ui.selectable_label(selected, mv).clicked() {
                                *review = Some(ply);
                            }
                        }
                        if moves.len() % 2 == 1 && i + 1 == moves.len() {
                            ui.label("");
                        }
                        ui.end_row();
                        i += 2;
                    }
                });
        });
}

/// Vertical eval bar (§6.4): red share from below, black from above.
/// `score_red` is the engine score from Red's perspective.
/// `height` is passed in explicitly: inside `ui.horizontal` egui only
/// offers the current row height as available height (~one line), so
/// reading it there shrinks the bar to ~16 px.
pub fn eval_bar_ui(ui: &mut egui::Ui, score_red: i32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(20.0, height), egui::Sense::hover());
    let frac = eval_bar_fraction(score_red);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::BLACK);
    let red_h = rect.height() * frac;
    let red_rect =
        egui::Rect::from_min_max(egui::Pos2::new(rect.min.x, rect.max.y - red_h), rect.max);
    painter.rect_filled(
        red_rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_rgb(0xC0, 0x39, 0x2B),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
        egui::StrokeKind::Inside,
    );
}

/// Whether the window is narrow enough that the side panel goes below
/// the board (§7: < 1100 px).
#[must_use]
pub fn is_narrow(ctx: &egui::Context) -> bool {
    ctx.screen_rect().width() < 1100.0
}

#[cfg(test)]
mod tests {
    use super::{advantage_color, format_advantage};

    #[test]
    fn advantage_formats_as_pawns_with_sign() {
        assert_eq!(format_advantage(80), "+0.8");
        assert_eq!(format_advantage(-30), "-0.3");
        assert_eq!(format_advantage(0), "+0.0");
        assert_eq!(format_advantage(150), "+1.5");
    }

    #[test]
    fn advantage_formats_mate() {
        assert_eq!(format_advantage(29_999), "M1");
        assert_eq!(format_advantage(-29_999), "-M1");
        assert_eq!(format_advantage(29_901), "M50");
    }

    #[test]
    fn advantage_colours_follow_the_learner() {
        assert_eq!(
            advantage_color(10),
            egui::Color32::from_rgb(0x2E, 0x7D, 0x32)
        );
        assert_eq!(
            advantage_color(-10),
            egui::Color32::from_rgb(0xC0, 0x39, 0x2B)
        );
        assert_eq!(advantage_color(0), egui::Color32::GRAY);
    }
}
