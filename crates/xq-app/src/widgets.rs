//! Shared button + panel widgets (Hieu, 2026-11-12): one style for
//! every button in the app, written once and reused everywhere.
//!
//! Buttons (all 20 px bold labels, 10 px corner radius, consistent
//! hover/pressed feedback):
//! - [`hero_button`] — the menu's big call-to-action (340 x 64, filled).
//! - [`primary_button`] — the main action in a dialog/panel (160 x 52, filled).
//! - [`secondary_button`] — a normal action (120 x 46, outlined).
//! - [`big_button`] — the standard action button (120 x 46), upgraded
//!   from `app::big_button` (same minimum size, rounded corners).
//!   Kept so existing call sites keep working.
//! - [`icon_button`] — a small square button for glyphs (✕ / ↺ / ⇄).
//! - [`big_radio`] — a radio option with a big (20 px) label, easy to tap.
//!
//! Filled buttons do NOT use `Button::fill` (egui documents that it
//! kills the hover effect); instead the button is drawn inside a scope
//! whose widget visuals carry the palette, so hover/pressed still
//! lighten/darken the fill.
//!
//! Panels:
//! - [`SIDE_PANEL_WIDTH`] — the one fixed width of every side panel.
//! - [`side_panel`] — a fixed-width, vertically scrolling side panel,
//!   so long content scrolls instead of flexing the board/panel.

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};

/// Minimum size of a standard action button (§7.5).
pub const BTN_MIN: Vec2 = Vec2::new(120.0, 46.0);
/// Minimum size of a primary action button.
pub const BTN_PRIMARY_MIN: Vec2 = Vec2::new(160.0, 52.0);
/// Minimum size of a menu hero button.
pub const BTN_HERO_MIN: Vec2 = Vec2::new(340.0, 64.0);
/// Size of a square icon button.
pub const BTN_ICON: Vec2 = Vec2::new(46.0, 46.0);
/// Corner radius of every button.
pub const BTN_RADIUS: u8 = 10;
/// The one fixed width of every side panel (board + panel). The board
/// budget and the panel width both use this constant, so they can never
/// disagree and flex the layout.
pub const SIDE_PANEL_WIDTH: f32 = 380.0;

/// Accent fill of primary/hero buttons (dark blue).
const ACCENT: Color32 = Color32::from_rgb(0x1E, 0x5A, 0xC8);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x2B, 0x6E, 0xE0);
const ACCENT_ACTIVE: Color32 = Color32::from_rgb(0x14, 0x46, 0x9E);
/// Colours of a button in its three states (inactive / hovered /
/// pressed). `text: None` keeps the theme's default text colour.
struct BtnPalette {
    fill: Color32,
    hover: Color32,
    active: Color32,
    stroke: Color32,
    text: Option<Color32>,
}

/// Outline of a secondary button.
const OUTLINE: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x8A);

/// The filled accent palette (primary / hero buttons).
fn accent_palette() -> BtnPalette {
    BtnPalette {
        fill: ACCENT,
        hover: ACCENT_HOVER,
        active: ACCENT_ACTIVE,
        stroke: ACCENT_ACTIVE,
        text: Some(Color32::WHITE),
    }
}

/// The outlined palette (secondary / icon buttons). The hover/active
/// tints use `from_rgba_unmultiplied`, which is not `const`, so this
/// is a function, not a constant.
fn outline_palette() -> BtnPalette {
    BtnPalette {
        fill: Color32::TRANSPARENT,
        hover: Color32::from_rgba_unmultiplied(128, 128, 128, 24),
        active: Color32::from_rgba_unmultiplied(128, 128, 128, 48),
        stroke: OUTLINE,
        text: None,
    }
}

/// Draw a button whose background comes from the scope's widget
/// visuals (so hover/pressed keep working), with a strong label.
fn styled_button(
    ui: &mut egui::Ui,
    text: &str,
    min_size: Vec2,
    font_size: f32,
    palette: BtnPalette,
) -> egui::Response {
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_fill = palette.fill;
        widgets.hovered.bg_fill = palette.hover;
        widgets.active.bg_fill = palette.active;
        widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.stroke);
        widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.stroke);
        widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.stroke);
        if let Some(c) = palette.text {
            let fg = Stroke::new(1.0_f32, c);
            widgets.inactive.fg_stroke = fg;
            widgets.hovered.fg_stroke = fg;
            widgets.active.fg_stroke = fg;
        }
        ui.add(
            egui::Button::new(RichText::new(text).strong().size(font_size))
                .min_size(min_size)
                .corner_radius(CornerRadius::same(BTN_RADIUS)),
        )
    })
    .inner
}

fn as_str(text: impl Into<egui::WidgetText>) -> String {
    text.into().text().to_string()
}

/// The menu's big call-to-action: filled accent, white 24 px label.
#[must_use]
pub fn hero_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let text = as_str(text);
    styled_button(ui, &text, BTN_HERO_MIN, 24.0, accent_palette())
}

/// The main action in a dialog/panel: filled accent, white 20 px label.
#[must_use]
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let text = as_str(text);
    styled_button(ui, &text, BTN_PRIMARY_MIN, 20.0, accent_palette())
}

/// A normal action: outlined, theme text colour, 20 px label.
#[must_use]
pub fn secondary_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let text = as_str(text);
    styled_button(ui, &text, BTN_MIN, 20.0, outline_palette())
}

/// A button with the app's uniform minimum size (§7.5): every action
/// button on every screen is at least 120 x 46 px, so buttons look
/// equally big and are easy to tap. Use for all panel/dialog buttons;
/// the Menu and lesson-picker cards use their own larger `add_sized`.
/// Upgraded from `app::big_button` (same size, rounded corners).
#[must_use]
pub fn big_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .min_size(BTN_MIN)
            .corner_radius(CornerRadius::same(BTN_RADIUS)),
    )
}

/// A small square button for a glyph (✕ / ↺ / ⇄), with a tooltip.
#[must_use]
pub fn icon_button(ui: &mut egui::Ui, glyph: &str, tooltip: &str) -> egui::Response {
    styled_button(ui, glyph, BTN_ICON, 22.0, outline_palette()).on_hover_text(tooltip)
}

/// A radio option with a big (20 px) label, easy to tap. Returns the
/// same `Response` as `ui.radio_value` (use `.changed()` on it).
pub fn big_radio<T: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    text: &str,
) -> egui::Response {
    ui.radio_value(current, value, RichText::new(text).size(20.0))
}

/// A side panel of fixed `width`, vertically scrolling: long content
/// scrolls instead of flexing the board/panel. Use for the panel next
/// to the board in both wide and narrow layouts (pass the available
/// width in narrow mode).
pub fn side_panel(ui: &mut egui::Ui, width: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        egui::ScrollArea::vertical().show(ui, add_contents);
    });
}
