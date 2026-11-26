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

use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Size, Subscription, Task, Theme,
};

use crate::app::{Screen, Shared};
use crate::i18n::{self, Key};
use crate::icons::{icon_tinted, Icon, ICON_SIZE_LARGE};
use crate::theme::{
    self, BLACK_PIECE, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_TITLE_LARGE, GOLD, PIECE_BG,
    RED_PIECE, SPACE_16, SPACE_24, SPACE_32, SPACE_8, TEXT_DIM,
};
use crate::widgets::{
    card_style, is_narrow, primary_button_with_icon, secondary_button, secondary_button_with_icon,
};

/// Home screen state: the screen is stateless (everything it shows
/// comes from `Shared`) except the window size, tracked so the
/// three cards can stack vertically on a narrow (mobile) window
/// instead of squeezing into one row.
pub struct State {
    /// Current window size (forwarded from `App` via
    /// `Msg::WindowResized`, seeded 1280 × 800 like the other
    /// screens — see `App::window`). Visible inside the crate for
    /// the App forwarding test.
    pub(crate) window: Size,
}

impl Default for State {
    fn default() -> Self {
        Self {
            window: Size::new(1280.0, 800.0),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// The window was resized: the cards follow it.
    WindowResized(Size),
}

/// One 80 px piece disc from the mockup title block (piece
/// background, double ring in the piece's colour, 44 px CJK glyph).
/// NOTE: the centring here must be `align_x`/`align_y`, NOT
/// `center_x(Length::Fill)`/`center_y(Length::Fill)` — those set the
/// size to Fill, overwriting the Fixed 80/72 px sizes (Hieu,
/// 2026-11-26: at 390 px the discs vanished entirely — Fill height
/// inside the scrolling home collapses to 0 — and at 1280 px they
/// blew up into giant panels).
fn piece_disc(glyph: &str, color: Color) -> Element<'_, Msg> {
    let inner = container(
        text(glyph)
            .size(44.0)
            .font(theme::cjk_font())
            .style(move |_: &Theme| iced::widget::text::Style { color: Some(color) }),
    )
    .width(Length::Fixed(72.0))
    .height(Length::Fixed(72.0))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
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
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
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
    /// Create the home screen (window seeded 1280 × 800, like the
    /// other screens — the real size arrives via `WindowResized`).
    #[must_use]
    pub fn new(_shared: &Shared) -> Self {
        Self {
            window: Size::new(1280.0, 800.0),
        }
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
            Msg::WindowResized(size) => {
                self.window = size;
                Task::none()
            }
        }
    }

    /// The home screen has no background work: the window size
    /// arrives from `App` (`Message::WindowResized`), not from a
    /// subscription of its own — see `App::window`.
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::none()
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

        let card_play = home_card(
            Icon::Maximize,
            i18n::t(Key::PlayVsAi),
            i18n::t(Key::HomePlayDesc),
            primary_button_with_icon(
                i18n::t(Key::NewGame),
                Icon::Plus,
                Msg::Navigate(Screen::Play),
            ),
        );
        let card_learn = home_card(
            Icon::BookOpen,
            i18n::t(Key::LearnOpenings),
            i18n::t(Key::HomeLearnDesc),
            secondary_button_with_icon(
                i18n::t(Key::HomeLearnButton),
                Icon::BookOpen,
                Msg::Navigate(Screen::LearnSelect),
            ),
        );
        let card_practice = home_card(
            Icon::Target,
            i18n::t(Key::PracticeTitle),
            i18n::t(Key::HomePracticeDesc),
            secondary_button_with_icon(
                i18n::t(Key::HomePracticeButton),
                Icon::Target,
                Msg::Navigate(Screen::Practice),
            ),
        );
        // Narrow (mobile): the three cards stack vertically —
        // side by side each would get ~110 px at a 390 px phone
        // width (Hieu, 2026-11-26: responsive vỡ trên mobile).
        let cards: Element<'_, Msg> = if is_narrow(self.window) {
            column![card_play, card_learn, card_practice]
                .spacing(SPACE_24)
                .into()
        } else {
            row![card_play, card_learn, card_practice]
                .spacing(SPACE_24)
                .into()
        };

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

        if is_narrow(self.window) {
            // Stacked cards + title + footer are taller than a
            // phone viewport, so the whole home scrolls.
            return scrollable(container(content).width(Length::Fill).padding(SPACE_16))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(SPACE_32)
            .into()
    }
}
