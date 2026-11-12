//! Menu screen (§6.1): two big buttons + Settings + About.

use crate::app::widgets::{hero_button, hero_half_button};
use crate::app::{Screen, XiangqiApp};
use crate::i18n::{self, Key};

impl XiangqiApp {
    pub(crate) fn show_menu(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(i18n::t(Key::AppTitle));
            ui.label(i18n::t(Key::AppSubtitle));
            ui.add_space(40.0);
            if hero_button(ui, i18n::t(Key::PlayVsAi)).clicked() {
                if !self.vs_ai.started {
                    self.vs_ai.show_start_dialog = true;
                }
                self.goto(Screen::VsAi);
            }
            ui.add_space(12.0);
            if hero_button(ui, i18n::t(Key::LearnOpenings)).clicked() {
                self.goto(Screen::LearnSelect);
            }
            ui.add_space(12.0);
            if hero_button(ui, i18n::t(Key::PracticeTitle)).clicked() {
                self.goto(Screen::Practice);
            }
            ui.add_space(24.0);
            // Settings + About, centred and lined up evenly with the
            // mode buttons (Hieu, 2026-11-12): two half-hero buttons
            // side by side span exactly one hero button (340 px), so
            // the row has the same width and centre as the modes
            // above it. A plain `ui.horizontal` inside
            // `vertical_centered` lays its buttons out from the row's
            // left edge, so `horizontal_centered` centres the pair.
            ui.horizontal_centered(|ui| {
                if hero_half_button(ui, i18n::t(Key::Settings)).clicked() {
                    self.show_settings = true;
                }
                ui.add_space(12.0);
                if hero_half_button(ui, i18n::t(Key::About)).clicked() {
                    self.show_about = true;
                }
            });
        });
    }
}
