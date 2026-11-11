//! App shell: `Screen` enum + top-level `eframe::App` (M3/M5).
//!
//! Screen bodies live in `screens/` as `impl XiangqiApp` methods; this
//! file holds shared state, font setup (§7.5), settings persistence
//! (§6.6) and small shared widgets (move list, eval bar).

use crate::ai_worker::{AiResponse, AiWorker};
use crate::board_view::eval_bar_fraction;
use crate::i18n::{self, Key};
use crate::screens::learn::LearnState;
use crate::screens::learn_select::LearnSelectState;
use crate::screens::summary::SummaryState;
use crate::screens::vs_ai::VsAiState;
use crate::settings::{NotationChoice, Settings};
use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;
use xq_book::Book;
use xq_core::board::Position;
use xq_core::game::Game;
use xq_core::notation;
use xq_core::types::Move;

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
    pub vs_ai: VsAiState,
    pub learn_select: LearnSelectState,
    pub learn: LearnState,
    pub summary: SummaryState,
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
}

impl XiangqiApp {
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let settings = cc.storage.map(Settings::load).unwrap_or_default();
        let book = Book::load_embedded().unwrap_or(Book {
            lessons: Vec::new(),
        });
        Self {
            screen: Screen::Menu,
            settings,
            settings_dirty: false,
            show_settings: false,
            show_about: false,
            book,
            worker: AiWorker::new(),
            pending_ai: Vec::new(),
            vs_ai: VsAiState::new(),
            learn_select: LearnSelectState::new(),
            learn: LearnState::new(),
            summary: SummaryState::new(),
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
                ui.label(i18n::t(Key::Difficulty));
                for d in 0..=3u8 {
                    changed |= ui
                        .radio_value(&mut self.settings.difficulty, d, i18n::difficulty_name(d))
                        .changed();
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
                ui.label(i18n::t(Key::AboutFonts));
            });
        if !open {
            self.show_about = false;
        }
    }
}

impl eframe::App for XiangqiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Drain the worker once per frame; screens consume from the buffer
        // (unmatched responses stay buffered for the screen waiting for them).
        let fresh = self.worker.drain_responses();
        self.pending_ai.extend(fresh);
        // Drop responses nobody is waiting for anymore (stale req_ids).
        let wanted = [
            self.vs_ai.ai_req_id,
            self.vs_ai.hint_req_id,
            self.learn.opponent_ai_req,
        ];
        self.pending_ai
            .retain(|r| wanted.contains(&Some(r.req_id())));
        egui::CentralPanel::default().show(ctx, |ui| match self.screen {
            Screen::Menu => self.show_menu(ui),
            Screen::VsAi => self.show_vs_ai(ctx, ui),
            Screen::LearnSelect => self.show_learn_select(ctx, ui),
            Screen::Learn => self.show_learn(ctx, ui),
            Screen::Summary => self.show_summary(ctx, ui),
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
}

/// Two-column (Red / Black) scrollable move list (§7.4).
/// Clicking a row enters review mode at that ply (`review` = ply count).
pub fn move_list_ui(ui: &mut egui::Ui, moves: &[String], review: &mut Option<usize>) {
    ui.label(i18n::t(Key::MovesTitle));
    let height = 180.0;
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            egui::Grid::new("move_list_grid")
                .num_columns(3)
                .spacing([8.0, 2.0])
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
pub fn eval_bar_ui(ui: &mut egui::Ui, score_red: i32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::new(16.0, ui.available_height()),
        egui::Sense::hover(),
    );
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
