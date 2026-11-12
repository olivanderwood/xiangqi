//! Xiangqi theme (Hieu, 2026-11-12): the app's colours and visuals
//! are drawn from the game itself — Chinese red, gold, rice paper
//! and warm wood — instead of the default blue/grey egui theme.
//!
//! Palette:
//! - **Chinese red** (`RED`) — the primary accent: hero/primary
//!   buttons, selection, the Red side. A deep vermilion, not a bright
//!   alarm red.
//! - **Gold** (`GOLD`) — borders, highlights and the selection
//!   outline: the colour of the palace lines and of a seal.
//! - **Rice paper** (`PAPER`) — the panel/window background: a warm
//!   off-white, like the paper of a Xiangqi manual.
//! - **Dark brown** (`INK`) — the text colour: warm, not pure black.
//! - **Wood** (`WOOD`) — the board colour (kept in `board_view`);
//!   the secondary button outline is a darker wood brown.
//!
//! [`apply_theme`] installs the visuals once at startup
//! (`XiangqiApp::new`). The button widgets in `widgets.rs` use the
//! same constants, so buttons and panels can never drift apart.

use egui::{Color32, Stroke, Visuals};

/// Chinese red — primary accent (hero/primary buttons, selection).
pub const RED: Color32 = Color32::from_rgb(0xA6, 0x1E, 0x1E);
/// Brighter red for a hovered primary button.
pub const RED_HOVER: Color32 = Color32::from_rgb(0xC0, 0x27, 0x27);
/// Darker red for a pressed primary button.
pub const RED_ACTIVE: Color32 = Color32::from_rgb(0x8B, 0x1A, 0x1A);
/// Gold — borders, highlights, selection outline.
pub const GOLD: Color32 = Color32::from_rgb(0xC9, 0xA2, 0x27);
/// Pale gold tint for a hovered outlined button. A function, not a
/// constant, because `from_rgba_unmultiplied` is not `const`.
#[must_use]
pub fn gold_tint() -> Color32 {
    Color32::from_rgba_unmultiplied(201, 162, 39, 24)
}
/// Pale gold tint for a pressed outlined button.
#[must_use]
pub fn gold_tint_active() -> Color32 {
    Color32::from_rgba_unmultiplied(201, 162, 39, 48)
}
/// Rice paper — panel/window background.
pub const PAPER: Color32 = Color32::from_rgb(0xF5, 0xEF, 0xE0);
/// Cream — the fill of a plain widget (text fields, radio rows).
pub const CREAM: Color32 = Color32::from_rgb(0xFF, 0xF8, 0xE7);
/// Dark brown ink — the text colour.
pub const INK: Color32 = Color32::from_rgb(0x3E, 0x27, 0x23);
/// Dark wood brown — the outline of secondary buttons.
pub const WOOD_DARK: Color32 = Color32::from_rgb(0x6B, 0x4F, 0x2A);

/// Install the Xiangqi visuals on `ctx`: a warm light theme (rice
/// paper background, dark brown text, red selection with a gold
/// outline). Call once at startup; the button widgets pick up the
/// same palette from the constants above.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::light();
    visuals.panel_fill = PAPER;
    visuals.window_fill = PAPER;
    visuals.extreme_bg_color = CREAM;
    visuals.faint_bg_color = CREAM;
    visuals.override_text_color = Some(INK);
    visuals.selection.bg_fill = RED;
    visuals.selection.stroke = Stroke::new(1.0_f32, GOLD);
    visuals.widgets.noninteractive.bg_fill = CREAM;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.inactive.bg_fill = CREAM;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.hovered.bg_fill = gold_tint();
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.active.bg_fill = gold_tint_active();
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.open.bg_fill = gold_tint();
    ctx.set_visuals(visuals);
}
