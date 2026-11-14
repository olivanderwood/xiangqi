//! Shared widgets: one style for every button / card / panel in the
//! app, written once and reused by every screen.
//!
//! Ported from the egui app (`crates/xq-app/src/app.rs` —
//! `advantage_chip_ui`, `eval_bar_ui`, `move_list_ui`,
//! `section_title`, `format_advantage` — and `widgets.rs` for the
//! button sizes). All helpers are generic over the screen's `Message`
//! type so any screen can use them.
//!
//! Layout rule carried over from egui: every side panel is at
//! least [`PANEL_WIDTH`] (380 px) wide and fills whatever width
//! the window leaves after the board and the chrome — see
//! [`board_size_for_window`] and [`panel_width_for_window`]. The
//! board never flexes when a panel's content changes, because the
//! board size is computed by reserving that 380 px minimum, not
//! by measuring the panel.
//!
//! Look (DESIGN-iced.md §1 + the mockup pages): Inter for all UI
//! text (the app default font is Inter Regular, set in `main.rs`;
//! weights are set explicitly here — buttons Semibold 15, card and
//! section titles Semibold 16, chips Medium 13, the advantage number
//! Bold 28); buttons radius 10, cards radius 14 ([`RADIUS_14`]);
//! paddings and gaps use the [`SPACE_*`](crate::theme) scale, never
//! bare numbers. Icons sit before the label with an 8 px gap
//! ([`icon`](crate::icons::icon) is 20 × 20, the mockup's button-icon
//! size); on a gold primary button the icon is tinted dark
//! ([`BG`](crate::theme::BG)) so it reads against the gold fill.

use crate::helpers::format_advantage;
use crate::icons::{icon_tinted, Icon};
use crate::theme::{
    self, BG, BORDER, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, GOLD, GREEN, PIECE_BG, RADIUS_10,
    RADIUS_14, RED, SPACE_12, SPACE_16, SPACE_24, SPACE_4, SPACE_8, SURFACE, SURFACE2, TEXT,
    TEXT_DIM,
};
use crate::theme::{BOARD_HEIGHT, BOARD_WIDTH};
use iced::widget::{button, column, container, row, text, Button, Row};
use iced::{Alignment, Color, Element, Length, Size, Theme};
use xq_core::game::GameResult;

/// Text color for a game result (Hieu, 2026-11-14): the winner's
/// color. Red wins → bright red; Black wins → light text (true
/// black is unreadable on the dark theme, so Black's result uses
/// the light text color); a draw → dim text.
#[must_use]
pub fn result_text_color(result: GameResult) -> Color {
    match result {
        GameResult::RedWins(_) => RED,
        GameResult::BlackWins(_) => TEXT,
        GameResult::Draw(_) => TEXT_DIM,
        GameResult::Ongoing => TEXT,
    }
}

/// The minimum width of every side panel (egui
/// `SIDE_PANEL_WIDTH`). Screens lay the panel out with
/// `Length::Fill`, so on a wide window it grows past this; it
/// never shrinks below it — see [`panel_width_for_window`].
pub const PANEL_WIDTH: f32 = 380.0;

/// Height of the screen header bar (mockup pages 2/4/5/6: 64 px,
/// surface background, 1 px divider underneath).
pub const HEADER_HEIGHT: f32 = 64.0;

/// The board's display size for a window, from the mockup's
/// responsive rule (`scale = min(w / 592, h / 656)`, DESIGN-iced.md
/// §1): the board takes whatever the window leaves after the
/// 64 px header, 1 px divider and 16 px content padding (vertical),
/// and after the 24 px content padding, 24 px gap and the 380 px
/// panel minimum (horizontal). (The 14 px eval bar used to sit
/// between the padding and the board; Hieu 2026-11-13 asked to
/// remove it, so the board gets that width back.) The
/// horizontal budget reserves [`PANEL_WIDTH`] even though the
/// panel itself is `Length::Fill` — that reservation is what
/// guarantees the Fill panel is never narrower than 380 px (see
/// [`panel_width_for_window`]). The returned size keeps the design
/// 592 × 656 aspect ratio, so [`Geom`](crate::board::Geom)
/// draws it with no letterboxing inside its own bounds. Screens
/// size their board container with this AND convert drop points
/// with the same size, so clicks keep mapping to the right square
/// at any window size (see the play / learn / practice screens).
#[must_use]
pub fn board_size_for_window(window: Size) -> Size {
    let avail_w = (window.width - SPACE_24 * 2.0 - SPACE_24 - PANEL_WIDTH).max(1.0);
    let avail_h = (window.height - HEADER_HEIGHT - 1.0 - SPACE_16 * 2.0).max(1.0);
    let scale = (avail_w / BOARD_WIDTH).min(avail_h / BOARD_HEIGHT);
    Size::new(BOARD_WIDTH * scale, BOARD_HEIGHT * scale)
}

/// The width a `Length::Fill` side panel gets in a window of this
/// size: the window width minus the 24 px content padding on each
/// side, the 24 px gap before the panel, and the board from
/// [`board_size_for_window`]. Because that function already
/// reserves [`PANEL_WIDTH`] in the board's horizontal budget, this
/// is at least [`PANEL_WIDTH`] for any window at or above the
/// 1100 × 720 minimum — the board is height-bound there, so the
/// panel is wider than the minimum (about 466 px at 1100 × 720,
/// about 574 px at 1280 × 800, about 961 px at 1920 × 1080).
#[allow(dead_code)] // used by tests; documents the Fill panel's layout width
#[must_use]
pub fn panel_width_for_window(window: Size) -> f32 {
    let board = board_size_for_window(window);
    (window.width - SPACE_24 * 2.0 - SPACE_24 - board.width).max(PANEL_WIDTH)
}

/// Button content: icon first, then the label (mockup: icon + text,
/// 8 px gap, Semibold 15). `tint` is the icon colour — dark
/// ([`BG`](crate::theme::BG)) on a gold primary button, light
/// ([`TEXT`](crate::theme::TEXT)) elsewhere.
fn labeled_icon<'a, Message: 'a>(icn: Icon, tint: Color, label: &'a str) -> Row<'a, Message> {
    row![
        icon_tinted(icn, tint),
        text(label)
            .size(FONT_BODY)
            .font(theme::inter_font_semibold()),
    ]
    .spacing(SPACE_8)
    .align_y(Alignment::Center)
}

/// Primary button: gold fill, dark text (`theme::primary_button_style`).
pub fn primary_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(
        text(label)
            .size(FONT_BODY)
            .font(theme::inter_font_semibold()),
    )
    .on_press(on_press)
    .style(theme::primary_button_style)
    .padding(SPACE_12)
}

/// Primary button with a leading icon, tinted dark so it reads on
/// the gold fill (mockup: "Bắt đầu ván mới", "Ván mới", "Bắt đầu học").
pub fn primary_button_with_icon<'a, Message>(
    label: &'a str,
    icn: Icon,
    on_press: Message,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(labeled_icon(icn, BG, label))
        .on_press(on_press)
        .style(theme::primary_button_style)
        .padding(SPACE_12)
}

/// Secondary button: surface-2 fill, light text, border
/// (`theme::secondary_button_style`).
pub fn secondary_button<'a, Message>(label: &'a str, on_press: Message) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(
        text(label)
            .size(FONT_BODY)
            .font(theme::inter_font_semibold()),
    )
    .on_press(on_press)
    .style(theme::secondary_button_style)
    .padding(SPACE_12)
}

/// Secondary button with a leading icon (mockup: "Hoàn nước",
/// "Gợi ý", "Đổi khai cuộc", "Ẩn gợi ý", "Đặt lại").
pub fn secondary_button_with_icon<'a, Message>(
    label: &'a str,
    icn: Icon,
    on_press: Message,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(labeled_icon(icn, TEXT, label))
        .on_press(on_press)
        .style(theme::secondary_button_style)
        .padding(SPACE_12)
}

/// Danger button with a leading icon (mockup: "Xin thua" with the
/// flag icon, light on the red fill).
pub fn danger_button_with_icon<'a, Message>(
    label: &'a str,
    icn: Icon,
    on_press: Message,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(labeled_icon(icn, TEXT, label))
        .on_press(on_press)
        .style(theme::danger_button_style)
        .padding(SPACE_12)
}

/// Ghost button style (mockup headers: transparent fill, light text,
/// surface-2 on hover — "Quay lại", "Cài đặt"). Defined here because
/// `theme.rs` has no ghost style and must not be edited.
#[must_use]
pub fn ghost_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered => Some(SURFACE2),
        button::Status::Pressed => Some(BORDER),
        _ => None,
    };
    button::Style {
        background: fill.map(iced::Background::Color),
        text_color: TEXT,
        border: iced::Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: RADIUS_10.into(),
        },
        ..Default::default()
    }
}

/// Ghost button with a leading icon (mockup headers).
pub fn ghost_button_with_icon<'a, Message>(
    label: &'a str,
    icn: Icon,
    on_press: Message,
) -> Button<'a, Message>
where
    Message: Clone + 'a,
{
    Button::new(labeled_icon(icn, TEXT, label))
        .on_press(on_press)
        .style(ghost_button_style)
        .padding(SPACE_12)
}

/// A button with no `on_press` (rendered disabled by Iced), with a
/// leading icon — e.g. "Hoàn nước" when there is nothing to undo.
pub fn disabled_button_with_icon<'a, Message>(label: &'a str, icn: Icon) -> Button<'a, Message>
where
    Message: 'a,
{
    Button::new(labeled_icon(icn, TEXT_DIM, label))
        .style(theme::secondary_button_style)
        .padding(SPACE_12)
}

// The menu's big call-to-action: a primary button filling its card,
// 48 px tall (mockup page 1: card buttons are full-width, 48 px).
// (Removed: the old egui `hero_button` / `hero_half_button` — the
// home cards now use the icon buttons below with explicit sizes.)

/// A dark card: surface background, border, radius 14
/// (mockup pages 2/4/5: cards are `radius: 14px`). Defined here —
/// not `theme::card_container_style` (radius 10) — because
/// `theme.rs` must not be edited; the colours are the theme's own.
#[must_use]
pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(SURFACE)),
        border: iced::Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_14.into(),
        },
        ..Default::default()
    }
}

/// A dark card: surface background, border, radius 14, padding
/// 12 px vertical / 16 px horizontal (mockup opponent / player cards).
pub fn card<'a, Message>(
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        column![
            text(title)
                .size(FONT_CARD_TITLE)
                .font(theme::inter_font_semibold()),
            content.into()
        ]
        .spacing(SPACE_8)
        .width(Length::Fill),
    )
    .padding(iced::Padding {
        top: SPACE_12,
        right: SPACE_16,
        bottom: SPACE_12,
        left: SPACE_16,
    })
    .width(Length::Fill)
    .style(card_style)
    .into()
}

/// A section title, the same on every screen (mockup: 16 px Semibold
/// — "Nước đi", "Gợi ý nước tiếp theo cho bạn", "Điểm ChessDB").
pub fn section_title<'a, Message: 'a>(title: &'a str) -> Element<'a, Message> {
    column![text(title)
        .size(FONT_CARD_TITLE)
        .font(theme::inter_font_semibold()),]
    .spacing(SPACE_4)
    .into()
}

/// One side card (mockup pages 2/4/5): a 44 px piece disc, Semibold
/// 16 name, 13 px Medium subtitle, status chip on the right; a gold
/// border marks the side to move (the player card on your turn).
/// Shared by the Play and Learn panels.
pub fn side_card<'a, Message: 'a>(
    glyph: &'a str,
    glyph_color: Color,
    name: &'a str,
    subtitle: String,
    chip: Element<'a, Message>,
    active: bool,
) -> Element<'a, Message> {
    let disc = container(
        text(glyph)
            .size(24.0)
            .font(theme::cjk_font())
            .style(move |_: &Theme| text::Style {
                color: Some(glyph_color),
            }),
    )
    .width(Length::Fixed(44.0))
    .height(Length::Fixed(44.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(iced::Background::Color(PIECE_BG)),
        border: iced::Border {
            color: glyph_color,
            width: 2.0,
            radius: 22.0.into(),
        },
        ..Default::default()
    });
    let body = column![
        text(name)
            .size(FONT_CARD_TITLE)
            .font(theme::inter_font_semibold()),
        text(subtitle)
            .size(FONT_CAPTION)
            .font(theme::inter_font_medium())
            .style(move |_: &Theme| text::Style {
                color: Some(TEXT_DIM),
            }),
    ]
    .spacing(2.0)
    .width(Length::Fill);
    container(
        row![disc, body, chip]
            .spacing(14.0)
            .align_y(Alignment::Center),
    )
    .padding(iced::Padding {
        top: SPACE_12,
        right: SPACE_16,
        bottom: SPACE_12,
        left: SPACE_16,
    })
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(iced::Background::Color(SURFACE)),
        border: iced::Border {
            color: if active { GOLD } else { BORDER },
            width: 1.0,
            radius: RADIUS_14.into(),
        },
        ..Default::default()
    })
    .into()
}

/// A status chip (mockup header chips: 30 px tall, 13 px Medium,
/// surface-2 fill, border, radius 15; an optional 8 px coloured dot
/// in front — green for "Pikafish sẵn sàng", gold for "Đến lượt bạn").
/// `text_color` is dim for informational chips, gold for the active
/// player's turn chip.
pub fn status_chip<'a, Message: 'a>(
    label: String,
    dot: Option<Color>,
    text_color: Color,
) -> Element<'a, Message> {
    let mut inner = row![].spacing(SPACE_8).align_y(Alignment::Center);
    if let Some(dot_color) = dot {
        inner = inner.push(
            container(text(""))
                .width(Length::Fixed(8.0))
                .height(Length::Fixed(8.0))
                .style(move |_: &Theme| container::Style {
                    background: Some(iced::Background::Color(dot_color)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }
    inner = inner.push(
        text(label)
            .size(FONT_CAPTION)
            .font(theme::inter_font_medium())
            .style(move |_: &Theme| text::Style {
                color: Some(text_color),
            }),
    );
    container(inner)
        .padding(iced::Padding {
            top: 0.0,
            right: SPACE_12,
            bottom: 0.0,
            left: SPACE_12,
        })
        .height(Length::Fixed(30.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(SURFACE2)),
            border: iced::Border {
                color: BORDER,
                width: 1.0,
                radius: 15.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// The 1 px divider under a header bar (mockup: `border-bottom: 1px
/// solid #3A3F4A` on the 64 px header).
pub fn header_divider<'a, Message: 'a>() -> Element<'a, Message> {
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_: &Theme| container::Style {
            background: Some(iced::Background::Color(BORDER)),
            ..Default::default()
        })
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
            .font(theme::inter_font_bold())
            .style(move |_: &Theme| text::Style { color: Some(color) }),
    )
    .padding(SPACE_8)
    .width(Length::Fixed(110.0))
    .style(move |_: &Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: iced::Border {
            color,
            width: 1.5,
            radius: RADIUS_10.into(),
        },
        ..Default::default()
    })
    .into()
}

// (The move list itself is built per-screen — Play/Learn/Summary
// each need review-click rows — so there is no shared `move_list`
// widget here.)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_size_for_window_matches_the_mockup_at_1280x800() {
        // Default window: width leaves room (798 px), height binds:
        // 800 - 64 - 1 - 32 = 703 px -> scale 703/656.
        let size = board_size_for_window(Size::new(1280.0, 800.0));
        assert!((size.height - 703.0).abs() < 0.01, "height {size:?}");
        assert!(
            (size.width - 592.0 * 703.0 / 656.0).abs() < 0.1,
            "width {size:?}"
        );
        // Aspect ratio preserved.
        assert!((size.width / size.height - 592.0 / 656.0).abs() < 1e-4);
    }

    #[test]
    fn board_size_for_window_shrinks_to_the_minimum_window() {
        // 1100 x 720 (the window minimum): height still binds,
        // 720 - 64 - 1 - 32 = 623 px, and the board + chrome must
        // fit the window exactly — this is the case the old fixed
        // 592 x 656 board overflowed (656 > 623).
        let size = board_size_for_window(Size::new(1100.0, 720.0));
        assert!((size.height - 623.0).abs() < 0.01, "height {size:?}");
        assert!(
            (size.width - 592.0 * 623.0 / 656.0).abs() < 0.1,
            "width {size:?}"
        );
        assert!(size.width < 618.0, "board fits the width budget: {size:?}");
        assert!(
            size.height + HEADER_HEIGHT + 1.0 + SPACE_16 * 2.0 <= 720.0,
            "board + header + padding fits the window height: {size:?}"
        );
        assert!(
            size.width + SPACE_24 + PANEL_WIDTH + SPACE_24 * 2.0 <= 1100.0,
            "board + gap + minimum panel fits the window width: {size:?}"
        );
    }

    #[test]
    fn fill_panel_is_at_least_380_and_leaves_no_gap() {
        // The Fill panel takes exactly the width left after the
        // chrome and the board, so board + chrome + panel == the
        // window width (no dead space on the right) and the panel
        // is never below the 380 px minimum — at the sizes below
        // the board is height-bound, so the panel is wider still.
        // (The values grew by 30 px vs. UI-FIX-2: the removed eval
        // bar freed 14 px + its 16 px gap.)
        for (w, h, expected_panel) in [
            (1100.0, 720.0, 465.8),
            (1280.0, 800.0, 573.6),
            (1920.0, 1080.0, 960.9),
        ] {
            let window = Size::new(w, h);
            let board = board_size_for_window(window);
            let panel = panel_width_for_window(window);
            assert!(
                panel >= PANEL_WIDTH,
                "panel {panel} < minimum {PANEL_WIDTH} at {w}x{h}"
            );
            assert!(
                (panel - expected_panel).abs() < 1.0,
                "panel {panel} at {w}x{h}, expected ~{expected_panel}"
            );
            let total = SPACE_24 * 2.0 + SPACE_24 + board.width + panel;
            assert!(
                (total - w).abs() < 0.01,
                "board + chrome + panel {total} != window width {w} at {w}x{h}"
            );
        }
    }
}
