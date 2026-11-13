//! Home screen (DESIGN-iced.md §2.1, `xq-app/src/screens/menu.rs`,
//! mockup page 1).
//!
//! Title block (two piece discs + "Cờ Tướng" 36 Bold + subtitle),
//! three cards in a row — Play vs AI, Learn openings, Self practice
//! — each 264 px tall with a 36 px gold icon, a Semibold 16 title, a
//! description and a full-width 48 px button, then the footer row
//! (Settings + About, 44 px, then the version + Pikafish line).
//! Settings + About stay stacked vertically in the egui menu
//! (Hieu, 2026-11-13, commit e224a40); the Iced home keeps both
//! buttons, side by side in the footer row next to the version line
//! (the mockup shows only "Cài đặt" — About is kept for parity).

use iced::widget::{column, container, row, text, Space};
use iced::{Background, Border, Color, Element, Length, Subscription, Task, Theme};

use crate::app::{Screen, Shared};
use crate::i18n::{self, Key};
use crate::icons::{icon_tinted, Icon, ICON_SIZE_LARGE};
use crate::theme::{
    self, BLACK_PIECE, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_TITLE_LARGE, GOLD, PIECE_BG,
    RED_PIECE, SPACE_16, SPACE_24, SPACE_32, SPACE_8, TEXT_DIM,
};
use crate::widgets::{
    card_style, primary_button_with_icon, secondary_button, secondary_button_with_icon,
};

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

/// One 80 px piece disc from the mockup title block (piece
/// background, double ring in the piece's colour, 44 px CJK glyph).
fn piece_disc(glyph: &str, color: Color) -> Element<'_, Msg> {
    let inner = container(
        text(glyph)
            .size(44.0)
            .font(theme::cjk_font())
            .style(move |_: &Theme| iced::widget::text::Style { color: Some(color) }),
    )
    .width(Length::Fixed(72.0))
    .height(Length::Fixed(72.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(PIECE_BG)),
        border: Border {
            color,
            width: 1.0,
            radius: 36.0.into(),
        },
        ..Default::default()
    });
    container(inner)
        .width(Length::Fixed(80.0))
        .height(Length::Fixed(80.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(PIECE_BG)),
            border: Border {
                color,
                width: 2.0,
                radius: 40.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// One home card: 36 px gold icon, Semibold 16 title, description,
/// full-width 48 px button at the bottom (mockup page 1: 264 px
/// tall, padding 24 — the mockup's 28 px is not on the spacing
/// scale, so the nearest token [`SPACE_24`] is used).
fn home_card<'a>(
    icn: Icon,
    title: &'a str,
    desc: &'a str,
    button: iced::widget::Button<'a, Msg>,
) -> Element<'a, Msg> {
    let content = column![
        icon_tinted(icn, GOLD)
            .width(ICON_SIZE_LARGE)
            .height(ICON_SIZE_LARGE),
        Space::new().height(Length::Fixed(SPACE_16)),
        text(title)
            .size(FONT_CARD_TITLE)
            .font(theme::inter_font_semibold()),
        Space::new().height(Length::Fixed(SPACE_8)),
        text(desc)
            .size(FONT_BODY)
            .style(|_: &Theme| iced::widget::text::Style {
                color: Some(TEXT_DIM),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        button.width(Length::Fill).height(Length::Fixed(48.0)),
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    container(content)
        .padding(SPACE_24)
        .width(Length::Fill)
        .height(Length::Fixed(264.0))
        .style(card_style)
        .into()
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

    /// The home screen: title block, three cards in a row, footer.
    pub fn view(&self, _shared: &Shared) -> Element<'_, Msg> {
        let title_block = column![
            row![piece_disc("帥", RED_PIECE), piece_disc("將", BLACK_PIECE),].spacing(SPACE_16),
            Space::new().height(Length::Fixed(20.0)),
            text(i18n::t(Key::AppTitle))
                .size(FONT_TITLE_LARGE)
                .font(theme::inter_font_bold()),
            Space::new().height(Length::Fixed(SPACE_8)),
            text(i18n::t(Key::AppSubtitle))
                .size(FONT_BODY)
                .style(|_: &Theme| iced::widget::text::Style {
                    color: Some(TEXT_DIM),
                }),
        ]
        .align_x(iced::Alignment::Center);

        let cards = row![
            home_card(
                Icon::Maximize,
                i18n::t(Key::PlayVsAi),
                i18n::t(Key::HomePlayDesc),
                primary_button_with_icon(
                    i18n::t(Key::NewGame),
                    Icon::Plus,
                    Msg::Navigate(Screen::Play)
                ),
            ),
            home_card(
                Icon::BookOpen,
                i18n::t(Key::LearnOpenings),
                i18n::t(Key::HomeLearnDesc),
                secondary_button_with_icon(
                    i18n::t(Key::HomeLearnButton),
                    Icon::BookOpen,
                    Msg::Navigate(Screen::LearnSelect)
                ),
            ),
            home_card(
                Icon::Target,
                i18n::t(Key::PracticeTitle),
                i18n::t(Key::HomePracticeDesc),
                secondary_button_with_icon(
                    i18n::t(Key::HomePracticeButton),
                    Icon::Target,
                    Msg::Navigate(Screen::Practice)
                ),
            ),
        ]
        .spacing(SPACE_24);

        let footer = row![
            secondary_button_with_icon(
                i18n::t(Key::Settings),
                Icon::SlidersHorizontal,
                Msg::ShowSettings
            )
            .height(Length::Fixed(44.0)),
            secondary_button(i18n::t(Key::About), Msg::ShowAbout).height(Length::Fixed(44.0)),
            text(i18n::t(Key::HomeVersion).replace("{n}", env!("CARGO_PKG_VERSION")))
                .size(FONT_CAPTION)
                .style(|_: &Theme| iced::widget::text::Style {
                    color: Some(TEXT_DIM),
                }),
        ]
        .spacing(SPACE_24)
        .align_y(iced::Alignment::Center);

        let content = column![title_block, cards, footer]
            .spacing(SPACE_32)
            .align_x(iced::Alignment::Center)
            .width(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(SPACE_32)
            .into()
    }

    /// The home screen has no background work.
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::none()
    }
}
