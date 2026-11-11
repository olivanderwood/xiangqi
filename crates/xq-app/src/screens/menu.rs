//! Menu screen (§6.1): two big buttons + Settings + About.

use crate::app::{big_button, Screen, XiangqiApp};
use crate::i18n::{self, Key};

impl XiangqiApp {
    pub(crate) fn show_menu(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(i18n::t(Key::AppTitle));
            ui.label(i18n::t(Key::AppSubtitle));
            ui.add_space(40.0);
            let btn = egui::Vec2::new(340.0, 64.0);
            if ui
                .add_sized(btn, egui::Button::new(i18n::t(Key::PlayVsAi)))
                .clicked()
            {
                if !self.vs_ai.started {
                    self.vs_ai.show_start_dialog = true;
                }
                self.goto(Screen::VsAi);
            }
            ui.add_space(12.0);
            if ui
                .add_sized(btn, egui::Button::new(i18n::t(Key::LearnOpenings)))
                .clicked()
            {
                self.goto(Screen::LearnSelect);
            }
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                if big_button(ui, i18n::t(Key::Settings)).clicked() {
                    self.show_settings = true;
                }
                if big_button(ui, i18n::t(Key::About)).clicked() {
                    self.show_about = true;
                }
            });
        });
    }
}
