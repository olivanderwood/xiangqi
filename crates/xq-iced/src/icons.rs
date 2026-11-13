//! UI icons extracted from the Iced design mockup
//! (`~/workspace/user/files/C__t__ng___Thi_t_k__giao_di_n__iced.html`,
//! 7 pages; see the inventory in the task report).
//!
//! Every icon is a Lucide-style 24 × 24 stroke icon (viewBox
//! `0 0 24 24`, `fill="none"`, `stroke-width` 2, round caps/joins),
//! stored as a standalone `.svg` under `assets/icons/` and embedded
//! with `include_bytes!`. The mockup draws each icon with
//! `stroke="currentColor"` (inheriting the button/card text colour);
//! the files bake that stroke to `#ECE7DC` (theme `TEXT`) instead,
//! because iced's `Svg` renderer has no inherited `currentColor` —
//! `usvg` would resolve it to black on the dark theme. The baked
//! colour is only the default: pass a colour to [`icon_tinted`]
//! (or chain `.style(...)` with `svg::Style { color: Some(..) }`)
//! and iced repaints the whole icon with it, ignoring the baked
//! colour (`iced_core::svg::Svg::color` docs).
//!
//! The board drawing on mockup pages 2/4/5 (592 × 656 SVG: board
//! lines, palace diagonals, piece circles + Chinese glyphs) is NOT
//! an icon and is not extracted — the board is already drawn by the
//! canvas in `board.rs`.
//!
//! Layout agent: use [`icon`] (20 × 20, the mockup's button-icon
//! size) and chain `.width(..)` / `.height(..)` for other sizes
//! (36 px card icons on the home screen, 18 px dropdown chevrons).
//! Screens must NOT be edited by this module's author; wiring the
//! icons into the screens is a later step.

use iced::widget::svg::{Handle, Svg};

/// The default icon size (px): the mockup's button-icon size.
pub const ICON_SIZE: f32 = 20.0;
/// The home-screen card icon size (px) in the mockup.
pub const ICON_SIZE_LARGE: f32 = 36.0;
/// The dropdown chevron size (px) in the mockup. No dropdown exists
/// in the app yet (Settings uses radio buttons), so this is currently
/// the small-icon size, e.g. the 18 px completed-lesson check.
pub const ICON_SIZE_SMALL: f32 = 18.0;

/// One UI icon from the design mockup (one variant per extracted
/// `.svg` file in `assets/icons/`).
///
/// Note: `chevron_down.svg` was extracted from the mockup too, but
/// its variant was removed — the app has no dropdown / `pick_list`
/// anywhere (Settings uses radio buttons; there is no board-theme
/// picker), so the icon has no UI to attach to and the variant would
/// only be dead code. The `.svg` file stays in `assets/icons/`; re-add
/// the variant when a dropdown appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// `maximize` — "Chơi với máy" card icon (home, 36 px) and the
    /// "Chơi tiếp với máy" button (Learn screens).
    Maximize,
    /// `plus` — "Bắt đầu ván mới" (home) and "Ván mới" (Play).
    Plus,
    /// `book-open` — "Học khai cuộc" card + "Vào học" (home),
    /// "Bắt đầu học" (Learn select), "Đổi khai cuộc" (Learn).
    BookOpen,
    /// `target` — "Tự luyện tập" card + "Vào luyện tập" (home).
    Target,
    /// `sliders-horizontal` — "Cài đặt" (home footer, Play/Learn
    /// headers).
    SlidersHorizontal,
    /// `arrow-left` — "Quay lại" (Play, Learn, Settings headers).
    ArrowLeft,
    /// `undo-2` — "Hoàn nước" (Play/Learn) and "Đặt lại" (Learn).
    Undo,
    /// `lightbulb` — "Gợi ý" (Play) and "Ẩn gợi ý" (Learn).
    Lightbulb,
    /// `flag` — "Xin thua" (Play).
    Flag,
    /// `check` — green completed-lesson check (Learn select).
    ///
    /// The mockup also draws this icon on its "Lưu" (Save) button,
    /// but the app has no Save button (settings auto-save), so the
    /// check is only used for completed lessons.
    Check,
}

impl Icon {
    /// All icons, for tests. Test-only: no production code iterates
    /// the whole set, so this stays out of normal builds (otherwise
    /// it would be dead code there).
    #[cfg(test)]
    #[must_use]
    pub const fn all() -> [Icon; 10] {
        [
            Icon::Maximize,
            Icon::Plus,
            Icon::BookOpen,
            Icon::Target,
            Icon::SlidersHorizontal,
            Icon::ArrowLeft,
            Icon::Undo,
            Icon::Lightbulb,
            Icon::Flag,
            Icon::Check,
        ]
    }

    /// The embedded SVG bytes for this icon.
    #[must_use]
    pub const fn svg_bytes(self) -> &'static [u8] {
        match self {
            Icon::Maximize => include_bytes!("../assets/icons/maximize.svg"),
            Icon::Plus => include_bytes!("../assets/icons/plus.svg"),
            Icon::BookOpen => include_bytes!("../assets/icons/book_open.svg"),
            Icon::Target => include_bytes!("../assets/icons/target.svg"),
            Icon::SlidersHorizontal => {
                include_bytes!("../assets/icons/sliders_horizontal.svg")
            }
            Icon::ArrowLeft => include_bytes!("../assets/icons/arrow_left.svg"),
            Icon::Undo => include_bytes!("../assets/icons/undo.svg"),
            Icon::Lightbulb => include_bytes!("../assets/icons/lightbulb.svg"),
            Icon::Flag => include_bytes!("../assets/icons/flag.svg"),
            Icon::Check => include_bytes!("../assets/icons/check.svg"),
        }
    }

    /// The [`Handle`] for this icon's embedded SVG bytes.
    #[must_use]
    pub fn handle(self) -> Handle {
        Handle::from_memory(self.svg_bytes())
    }
}

/// Build the [`Svg`] widget for an icon, at the default 20 × 20 px
/// (the mockup's button-icon size), in the baked `#ECE7DC` stroke.
/// Chain `.width(..)` / `.height(..)` for other sizes
/// ([`ICON_SIZE_LARGE`] / [`ICON_SIZE_SMALL`]) and use
/// [`icon_tinted`] for a different colour.
#[must_use]
pub fn icon(name: Icon) -> Svg<'static> {
    Svg::new(name.handle()).width(ICON_SIZE).height(ICON_SIZE)
}

/// Build the [`Svg`] widget for an icon, repainted entirely in
/// `color` (e.g. `theme::BG` for an icon on a gold primary button,
/// whose text is dark). The style colour overrides the baked
/// `#ECE7DC` stroke.
#[must_use]
pub fn icon_tinted(name: Icon, color: iced::Color) -> Svg<'static> {
    icon(name).style(move |_, _| iced::widget::svg::Style { color: Some(color) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_are_distinct_valid_svgs() {
        let mut handles = Vec::new();
        for ic in Icon::all() {
            let bytes = ic.svg_bytes();
            let text = std::str::from_utf8(bytes).expect("icon svg is utf-8");
            assert!(
                text.contains("viewBox=\"0 0 24 24\""),
                "{ic:?}: missing 24x24 viewBox"
            );
            assert!(
                text.contains("stroke=\"#ECE7DC\""),
                "{ic:?}: missing baked TEXT stroke"
            );
            assert!(
                text.contains("stroke-width=\"2\""),
                "{ic:?}: missing stroke-width 2"
            );
            let handle = ic.handle();
            assert!(
                !handles.contains(&handle.id()),
                "{ic:?}: duplicate svg bytes"
            );
            handles.push(handle.id());
            // The widget itself must construct for every variant.
            let _widget = icon(ic);
            let _tinted = icon_tinted(ic, iced::Color::WHITE);
        }
        assert_eq!(handles.len(), 10, "expected 10 distinct icons");
    }
}
