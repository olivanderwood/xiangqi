//! Saved-games list ("Lịch sử đấu" for Play, "Ván cờ đã lưu"
//! for Practice — Hieu, 2026-11-14).
//!
//! One screen for both: the App sets the mode with [`State::open`]
//! before navigating here. Entries are newest first; each row has
//! the label, the save date and "Mở ván" / "Xoá". Loading and
//! deleting are intercepted by `App::update` (loading resumes the
//! game on its own screen, deleting removes it from the store).

use iced::widget::{column, container, row, scrollable, text, Column, Space};
use iced::{Element, Length, Theme};

use crate::app::{Screen, Shared};
use crate::i18n::{self, Key};
use crate::saved_games::{format_date, SavedMode};
use crate::theme::{FONT_BODY, FONT_SCREEN_TITLE, SPACE_12, SPACE_16, SPACE_24, SPACE_8};
use crate::widgets::{
    card_style, ghost_button_with_icon, header_divider, primary_button, secondary_button,
    section_title, HEADER_HEIGHT,
};
use crate::{icons::Icon, theme};

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// Open a saved game (intercepted by `App::update`: resumes it
    /// on its own screen).
    LoadGame(String),
    /// Delete a saved game (intercepted by `App::update`).
    DeleteGame(String),
    Noop,
}

pub struct State {
    mode: SavedMode,
}

impl State {
    #[must_use]
    pub fn new(_shared: &Shared) -> Self {
        Self {
            mode: SavedMode::Play,
        }
    }

    /// Which list this screen shows (set by the App before
    /// navigating here).
    pub fn open(&mut self, mode: SavedMode) {
        self.mode = mode;
    }

    /// Iced `view`: header + a centred list card.
    pub fn view<'a>(&self, shared: &'a Shared) -> Element<'a, Msg> {
        let title = match self.mode {
            SavedMode::Play => i18n::t(Key::GameHistoryTitle),
            SavedMode::Learn => i18n::t(Key::GameHistoryTitle),
            SavedMode::Practice => i18n::t(Key::SavedGamesTitle),
        };
        let header = container(
            row![
                ghost_button_with_icon(
                    i18n::t(Key::BackToMenu),
                    Icon::ArrowLeft,
                    Msg::Navigate(Screen::Home)
                ),
                text(title)
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold())
                    .width(Length::Fill),
                ghost_button_with_icon(
                    i18n::t(Key::Settings),
                    Icon::SlidersHorizontal,
                    Msg::ShowSettings
                ),
            ]
            .spacing(SPACE_16)
            .align_y(iced::Alignment::Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: SPACE_24,
            bottom: 0.0,
            left: SPACE_24,
        })
        .width(Length::Fill)
        .height(Length::Fixed(HEADER_HEIGHT))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(theme::SURFACE)),
            ..Default::default()
        });
        // Finished Play games only (the history); every Practice
        // game, finished or not (the saved-games list).
        let entries: Vec<_> = shared
            .saved_games
            .by_mode(self.mode)
            .into_iter()
            .filter(|e| self.mode != SavedMode::Play || e.finished)
            .collect();
        let mut list = Column::new().spacing(SPACE_12).width(Length::Fill);
        if entries.is_empty() {
            list = list.push(text(i18n::t(Key::NoSavedGames)).size(FONT_BODY));
        }
        for entry in entries {
            let row_el = row![
                column![
                    text(&entry.label).size(FONT_BODY),
                    text(format_date(entry.created_unix))
                        .size(FONT_BODY)
                        .style(|_: &Theme| iced::widget::text::Style {
                            color: Some(theme::TEXT_DIM),
                        }),
                ]
                .spacing(SPACE_8)
                .width(Length::Fill),
                primary_button(i18n::t(Key::LoadGame), Msg::LoadGame(entry.id.clone())),
                secondary_button(i18n::t(Key::DeleteGame), Msg::DeleteGame(entry.id.clone())),
            ]
            .spacing(SPACE_12)
            .align_y(iced::Alignment::Center);
            list = list.push(row_el);
        }
        let card = container(
            column![
                section_title(title),
                Space::new().height(Length::Fixed(SPACE_8)),
                scrollable(list).width(Length::Fill).height(Length::Fill),
            ]
            .spacing(SPACE_12)
            .width(Length::Fill),
        )
        .padding(SPACE_24)
        .width(Length::Fixed(880.0))
        .style(card_style);
        iced::widget::column![
            header,
            header_divider(),
            container(container(card).width(Length::Fill).center_x(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(SPACE_24),
        ]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
