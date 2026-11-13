//! Shared widgets: one style for every button / card / panel in the
//! app, written once and reused by every screen.
//!
//! Ported from the egui app (`crates/xq-app/src/app.rs` —
//! `advantage_chip_ui`, `eval_bar_ui`, `move_list_ui`,
//! `section_title`, `format_advantage` — and `widgets.rs` for the
//! button sizes). All helpers are generic over the screen's `Message`
//! type so any screen can use them.
//!
//! Layout rule carried over from egui: every side panel is exactly
//! [`PANEL_WIDTH`] (380 px) wide, so the board never flexes when a
//! panel's content changes.

use crate::helpers::format_advantage;
use crate::theme::{self, FONT_BODY, FONT_CARD_TITLE, FONT_SCREEN_TITLE, GREEN, RED, TEXT_DIM};
use iced::widget::{column, container, text, Button};
use iced::{Color, Element, Length, Theme};

/// The one fixed width of every side panel (egui `SIDE_PANEL_WIDTH`).
pub const PANEL_WIDTH: f32 = 380.0;

/// Minimum size of a menu hero button (egui `BTN_HERO_MIN`): 340 × 64.
pub const HERO_WIDTH: f32 = 340.0;
/// Minimum size of a half-hero button (egui `BTN_HERO_HALF_MIN`):
/// 164 × 56 — two of them side by side, with a 12 px gap, span
/// exactly one hero button.
pub const HERO_HALF_WIDTH: f32 = 164.0;

/// Primary button: gold fill, dark text (`theme::primary_button_style`).
pub fn primary_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(text(label).size(FONT_BODY))
        .on_press(on_press)
        .style(theme::primary_button_style)
        .padding(12)
}

/// Secondary button: surface-2 fill, light text, border
/// (`theme::secondary_button_style`).
pub fn secondary_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(text(label).size(FONT_BODY))
        .on_press(on_press)
        .style(theme::secondary_button_style)
        .padding(12)
}

/// Danger button (e.g. Resign): red fill (`theme::danger_button_style`).
pub fn danger_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(text(label).size(FONT_BODY))
        .on_press(on_press)
        .style(theme::danger_button_style)
        .padding(12)
}

/// The menu's big call-to-action: a primary button 340 × 64
/// (egui `hero_button`).
pub fn hero_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    primary_button(label, on_press)
        .width(Length::Fixed(HERO_WIDTH))
        .height(Length::Fixed(64.0))
}

/// Half a hero button: a secondary button 164 × 56
/// (egui `hero_half_button`). Two of them, stacked or side by side,
/// line up evenly with one hero button above.
pub fn hero_half_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    secondary_button(label, on_press)
        .width(Length::Fixed(HERO_HALF_WIDTH))
        .height(Length::Fixed(56.0))
}

/// A dark card: surface background, border, radius 10
/// (`theme::card_container_style`; spike-proven `card` helper).
pub fn card<'a, Message>(
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        column![text(title).size(FONT_CARD_TITLE), content.into()]
            .spacing(8)
            .width(Length::Fill),
    )
    .padding(12)
    .width(Length::Fill)
    .style(theme::card_container_style)
    .into()
}

/// A section title, the same on every screen (egui `section_title`:
/// strong, then a small gap).
pub fn section_title<'a, Message: 'a>(title: &'a str) -> Element<'a, Message> {
    column![text(title).size(FONT_SCREEN_TITLE),]
        .spacing(4)
        .into()
}

/// Colour of an advantage chip: green when the side is better, red
/// when worse, dim when level (egui `advantage_color`, adapted to the
/// dark theme — green/red are the theme's own colours).
#[must_use]
pub fn advantage_color(cp: i32) -> Color {
    if cp > 0 {
        GREEN
    } else if cp < 0 {
        RED
    } else {
        TEXT_DIM
    }
}

/// A big, clear score cell (egui `advantage_chip_ui`): a tinted
/// rounded chip with the advantage number in bold 28 px, at least
/// 110 px wide so the column lines up across hint rows. The number
/// is the point — it must read at a glance.
pub fn advantage_chip<'a, Message: 'a>(cp: i32) -> Element<'a, Message> {
    let color = advantage_color(cp);
    let bg = Color { a: 0.14, ..color };
    container(
        text(format_advantage(cp))
            .size(28.0)
            .style(move |_: &Theme| text::Style { color: Some(color) }),
    )
    .padding(6)
    .width(Length::Fixed(110.0))
    .style(move |_: &Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: iced::Border {
            color,
            width: 1.5,
            radius: 10.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Red's share of the vertical eval bar (0..1), from a score in Red's
/// perspective (§6.4): `tanh(score / 400)` mapped to 0..1.
/// Ported from `xq-app/src/board_view.rs::eval_bar_fraction`.
#[must_use]
pub fn eval_bar_fraction(score_red: i32) -> f32 {
    0.5 * (1.0 + (f64::from(score_red) / 400.0).tanh() as f32)
}

/// Vertical eval bar (egui `eval_bar_ui`): red share from below,
/// black from above. `score_red` is the engine score from Red's
/// perspective; `height` is passed in explicitly because the caller
/// knows the panel height.
pub fn eval_bar<'a, Message: 'a>(score_red: i32, height: f32) -> Element<'a, Message> {
    let frac = eval_bar_fraction(score_red);
    let red_h = (height * frac).max(0.0);
    let black_h = (height - red_h).max(0.0);
    let black = container(text(""))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(black_h))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(Color::BLACK)),
            ..Default::default()
        });
    let red = container(text(""))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(red_h))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb8(0xC0, 0x39, 0x2B))),
            ..Default::default()
        });
    container(
        column![black, red]
            .spacing(0)
            .width(Length::Fixed(20.0))
            .height(Length::Fixed(height)),
    )
    .width(Length::Fixed(20.0))
    .height(Length::Fixed(height))
    .style(|_: &Theme| container::Style {
        border: iced::Border {
            color: TEXT_DIM,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .into()
}

// (The move list itself is built per-screen — Play/Learn/Summary
// each need review-click rows — so there is no shared `move_list`
// widget here.)
