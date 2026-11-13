//! Home screen (DESIGN-iced.md §2.1, `xq-app/src/screens/menu.rs`).
//!
//! Title "Cờ Tướng" (36), subtitle, three cards — Play vs AI, Learn
//! openings, Self practice — each with its description and button,
//! then Settings + About stacked vertically, each on its own centred
//! row (Hieu, 2026-11-13, commit e224a40), and the version +
//! Pikafish line at the bottom.

use iced::widget::{column, container, text};
use iced::{Element, Length, Subscription, Task};

use crate::app::{Screen, Shared};
use crate::i18n::{self, Key};
use crate::theme::{FONT_BODY, FONT_CAPTION, FONT_TITLE_LARGE};
use crate::widgets::{card, hero_button, hero_half_button};

/// Home screen state: the screen is stateless (everything it shows
/// comes from `Shared`); the struct exists so the module follows the
/// same contract as the other screens.
#[derive(Default)]
pub struct State;

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
}

impl State {
    /// Create the (stateless) home screen.
    #[must_use]
    pub fn new(_shared: &Shared) -> Self {
        Self
    }

    /// `Navigate` / `ShowSettings` / `ShowAbout` are intercepted by
    /// `App` before they reach here, so this never has anything to
    /// do; the match stays exhaustive for future variants. Kept for
    /// the screen contract (`State::update` on every screen) even
    /// though no caller reaches it yet.
    #[allow(dead_code)]
    pub fn update(&mut self, msg: Msg, _shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout => Task::none(),
        }
    }

    /// The home screen: centred column, 340 px hero buttons.
    pub fn view(&self, _shared: &Shared) -> Element<'_, Msg> {
        let play_card = card(
            i18n::t(Key::PlayVsAi),
            column![
                text(i18n::t(Key::HomePlayDesc)).size(FONT_BODY),
                hero_button(i18n::t(Key::NewGame), Msg::Navigate(Screen::Play)),
            ]
            .spacing(12),
        );
        let learn_card = card(
            i18n::t(Key::LearnOpenings),
            column![
                text(i18n::t(Key::HomeLearnDesc)).size(FONT_BODY),
                hero_button(
                    i18n::t(Key::HomeLearnButton),
                    Msg::Navigate(Screen::LearnSelect)
                ),
            ]
            .spacing(12),
        );
        let practice_card = card(
            i18n::t(Key::PracticeTitle),
            column![
                text(i18n::t(Key::HomePracticeDesc)).size(FONT_BODY),
                hero_button(
                    i18n::t(Key::HomePracticeButton),
                    Msg::Navigate(Screen::Practice)
                ),
            ]
            .spacing(12),
        );

        let content = column![
            text(i18n::t(Key::AppTitle)).size(FONT_TITLE_LARGE),
            text(i18n::t(Key::AppSubtitle)).size(FONT_BODY),
            play_card,
            learn_card,
            practice_card,
            hero_half_button(i18n::t(Key::Settings), Msg::ShowSettings),
            hero_half_button(i18n::t(Key::About), Msg::ShowAbout),
            text(i18n::t(Key::HomeVersion).replace("{n}", env!("CARGO_PKG_VERSION")))
                .size(FONT_CAPTION),
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center)
        .width(Length::Fixed(340.0));

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(16)
            .into()
    }

    /// The home screen has no background work.
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::none()
    }
}
