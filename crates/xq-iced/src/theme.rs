//! Dark theme for the Iced UI (`docs/DESIGN-iced.md` §1).
//!
//! The palette, font sizes and widget styles are the single source
//! of truth for how the app looks; screens use these constants and
//! style functions instead of inventing their own colours. Values
//! are copied from the design doc and proven in the spike
//! (`~/workspace/iced-spike`, `docs/SPIKE-iced.md`): the button
//! styles below are the spike's `primary_button_style` /
//! `secondary_button_style`, with `danger_button_style` added for
//! destructive actions (e.g. Resign).

use iced::font::{self, Weight};
use iced::theme::Palette;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Font, Theme};

// ---------- Palette (DESIGN-iced.md §1, theme TỐI) ----------

/// Window background.
pub const BG: Color = Color::from_rgb8(0x17, 0x19, 0x1D);
/// Surface (cards, panels).
pub const SURFACE: Color = Color::from_rgb8(0x20, 0x23, 0x2A);
/// Surface 2 (buttons, selected rows).
pub const SURFACE2: Color = Color::from_rgb8(0x2A, 0x2E, 0x36);
/// Borders.
pub const BORDER: Color = Color::from_rgb8(0x3A, 0x3F, 0x4A);
/// Primary text.
pub const TEXT: Color = Color::from_rgb8(0xEC, 0xE7, 0xDC);
/// Secondary text.
pub const TEXT_DIM: Color = Color::from_rgb8(0xA9, 0xA5, 0x99);
/// Accent (gold).
pub const GOLD: Color = Color::from_rgb8(0xD4, 0xA2, 0x4C);
/// Warning / Red side.
pub const RED: Color = Color::from_rgb8(0xEA, 0x6A, 0x5F);
/// Success / ready.
pub const GREEN: Color = Color::from_rgb8(0x5D, 0xB0, 0x8A);
/// Your suggestion (opening book).
pub const BLUE: Color = Color::from_rgb8(0x2F, 0x6F, 0xD0);
/// Opponent suggestion.
pub const PURPLE: Color = Color::from_rgb8(0x7B, 0x3F, 0xC4);
/// Board wood.
pub const WOOD: Color = Color::from_rgb8(0xE2, 0xBC, 0x82);
/// Board lines.
pub const BOARD_LINE: Color = Color::from_rgb8(0x4A, 0x33, 0x20);
/// Piece background.
pub const PIECE_BG: Color = Color::from_rgb8(0xF6, 0xEA, 0xD0);
/// Red pieces.
pub const RED_PIECE: Color = Color::from_rgb8(0xB3, 0x26, 0x1E);
/// Black pieces.
pub const BLACK_PIECE: Color = Color::from_rgb8(0x2B, 0x2B, 0x2B);

// ---------- Font sizes (DESIGN-iced.md §1) ----------

/// Large title (home screen).
/// Large title (Home h1). The design-system page says 36, but the
/// sample's rendered Home title is 52 px — Hieu (2026-11-13) chose to
/// match the rendered sample, so this is 52.
pub const FONT_TITLE_LARGE: f32 = 52.0;
/// Screen title.
pub const FONT_SCREEN_TITLE: f32 = 20.0;
/// Card title.
pub const FONT_CARD_TITLE: f32 = 16.0;
/// Body text.
pub const FONT_BODY: f32 = 15.0;
/// Caption / hint text.
pub const FONT_CAPTION: f32 = 13.0;
/// Chinese character on a piece.
pub const FONT_PIECE: f32 = 30.0;

// ---------- Spacing & corner radii (DESIGN-iced.md §1) ----------
//
// Design tokens for the layout pass: the spacing scale
// 4 / 8 / 12 / 16 / 24 / 32 and the corner radii 6 / 10 / 14.
// Named constants (one per step) so call sites read as `SPACE_16`
// instead of a bare number. Palette / font sizes above are unchanged.

/// Spacing step: 4 px.
pub const SPACE_4: f32 = 4.0;
/// Spacing step: 8 px.
pub const SPACE_8: f32 = 8.0;
/// Spacing step: 12 px.
pub const SPACE_12: f32 = 12.0;
/// Spacing step: 16 px.
pub const SPACE_16: f32 = 16.0;
/// Spacing step: 24 px.
pub const SPACE_24: f32 = 24.0;
/// Spacing step: 32 px.
pub const SPACE_32: f32 = 32.0;

/// Corner radius: 6 px.
pub const RADIUS_6: f32 = 6.0;
/// Corner radius: 10 px.
pub const RADIUS_10: f32 = 10.0;
/// Corner radius: 14 px.
pub const RADIUS_14: f32 = 14.0;

// ---------- Board geometry (DESIGN-iced.md §1) ----------

/// One grid cell, in px.
pub const BOARD_CELL: f32 = 64.0;
/// Board margin, in px.
pub const BOARD_MARGIN: f32 = 40.0;
/// Board size (592 × 656 px at scale 1).
pub const BOARD_WIDTH: f32 = 592.0;
/// Board size (592 × 656 px at scale 1).
pub const BOARD_HEIGHT: f32 = 656.0;
/// Piece radius (inner ring 22).
pub const PIECE_RADIUS: f32 = 27.0;
/// Piece inner-ring radius.
pub const PIECE_INNER_RADIUS: f32 = 22.0;

/// The app's custom dark theme, for `.theme(|_| custom_theme())`.
#[must_use]
pub fn custom_theme() -> Theme {
    Theme::custom(
        "XiangqiDark",
        Palette {
            background: BG,
            text: TEXT,
            primary: GOLD,
            success: GREEN,
            warning: GOLD,
            danger: RED,
        },
    )
}

/// CJK font for piece glyphs / river text: the embedded subset
/// (`assets/NotoSerifSC-Bold-subset.ttf`, family
/// "Noto Serif CJK SC", Bold), loaded at boot via `font::load`.
/// NOTE: the family name is "Noto Serif CJK SC" — not "Noto Serif
/// SC" as the design doc abbreviates it (see `docs/SPIKE-iced.md`).
#[must_use]
pub fn cjk_font() -> Font {
    Font {
        weight: Weight::Bold,
        ..Font::with_name("Noto Serif CJK SC")
    }
}

/// Load the embedded CJK subset font. Call once at startup (the
/// boot `Task` in `main.rs`); the result message only reports
/// failure, the app works without it (pieces fall back to the
/// default font).
pub fn load_cjk_font() -> iced::Task<Result<(), font::Error>> {
    font::load(include_bytes!("../assets/NotoSerifSC-Bold-subset.ttf"))
}

/// Inter UI font, Regular (weight 400) — the design's UI typeface
/// (`docs/DESIGN-iced.md` §1; the mẫu uses Inter 400/500/600/700
/// for all UI text). Embedded static TTFs (`assets/Inter-*.ttf`,
/// family "Inter", OFL), loaded at boot via [`load_inter_fonts`].
#[must_use]
pub fn inter_font() -> Font {
    Font::with_name("Inter")
}

/// Inter UI font, Medium (weight 500).
#[must_use]
pub fn inter_font_medium() -> Font {
    Font {
        weight: Weight::Medium,
        ..Font::with_name("Inter")
    }
}

/// Inter UI font, Semibold (weight 600).
#[must_use]
pub fn inter_font_semibold() -> Font {
    Font {
        weight: Weight::Semibold,
        ..Font::with_name("Inter")
    }
}

/// Inter UI font, Bold (weight 700).
#[must_use]
pub fn inter_font_bold() -> Font {
    Font {
        weight: Weight::Bold,
        ..Font::with_name("Inter")
    }
}

/// Load the four embedded Inter weights (400/500/600/700). Call
/// once at startup, batched with [`load_cjk_font`] (the boot `Task`
/// in `App::new`); like the CJK load, each result only reports
/// failure — the app works without them (text falls back to the
/// default font).
pub fn load_inter_fonts() -> iced::Task<Result<(), font::Error>> {
    iced::Task::batch([
        font::load(include_bytes!("../assets/Inter-Regular.ttf")),
        font::load(include_bytes!("../assets/Inter-Medium.ttf")),
        font::load(include_bytes!("../assets/Inter-SemiBold.ttf")),
        font::load(include_bytes!("../assets/Inter-Bold.ttf")),
    ])
}

/// Primary button: gold fill, dark text (spike-proven).
#[must_use]
pub fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered => Color::from_rgb8(0xE0, 0xB2, 0x5C),
        button::Status::Pressed => Color::from_rgb8(0xB8, 0x8A, 0x3A),
        _ => GOLD,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: BG,
        border: Border {
            color: GOLD,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

/// Secondary button: surface-2 fill, light text, border
/// (spike-proven).
#[must_use]
pub fn secondary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered => SURFACE2,
        button::Status::Pressed => BORDER,
        _ => SURFACE2,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: TEXT,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

/// Danger button (e.g. Resign): red fill, light text.
#[must_use]
pub fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered => Color::from_rgb8(0xF0, 0x7D, 0x72),
        button::Status::Pressed => Color::from_rgb8(0xC4, 0x4E, 0x44),
        _ => RED,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: TEXT,
        border: Border {
            color: RED,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

/// Card / panel container: surface background, border, radius 10
/// (spike-proven `card` helper).
#[must_use]
pub fn card_container_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

/// Dim backdrop behind a centred dialog overlay (60% opaque
/// background — same as the Play screen's private helper).
#[must_use]
pub fn dim_background(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(iced::Color { a: 0.6, ..BG })),
        ..Default::default()
    }
}
