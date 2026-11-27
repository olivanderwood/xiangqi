//! Xiangqi AI — Iced 0.14 UI (rewritten from the egui app).
//!
//! Boot: `iced::application` with [`App`](app::App), the custom dark
//! theme ([`theme::custom_theme`]), a 1280 × 800 window (min
//! 1100 × 720), and the embedded CJK subset font loaded by the boot
//! task in [`App::new`](app::App::new).
//!
//! The egui app (`crates/xq-app`) is untouched; this crate only
//! reuses its core crates and the copied worker / i18n / sound /
//! settings modules.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_worker;
mod app;
mod board;
mod bridge;
mod chessdb_panel;
mod chessdb_worker;
mod engine_log;
mod engine_manager;
#[cfg(target_arch = "wasm32")]
mod engine_wasm;
#[cfg(target_arch = "wasm32")]
mod firebase_auth;
mod helpers;
mod i18n;
mod icons;
mod saved_games;
mod screens;
mod settings;
mod sound;
#[cfg(target_arch = "wasm32")]
mod storage;
mod sync;
#[cfg(target_arch = "wasm32")]
mod sync_fetch;
#[cfg(target_arch = "wasm32")]
mod sync_wasm;
mod theme;
mod time;
mod widgets;

pub use app::{App, Message, Screen, Shared};

fn main() -> iced::Result {
    // PWA only: log Rust panics to the browser console (see
    // Cargo.toml — the release profile aborts on panic, which would
    // otherwise surface only as `RuntimeError: unreachable`).
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .theme(|_: &App| theme::custom_theme())
        // Window title follows the app name ("Cờ Tướng" / "Xiangqi",
        // Hieu 2026-11-13) and the current language setting.
        .title(|_: &App| i18n::t(i18n::Key::AppTitle).to_string())
        // Inter is the UI typeface (DESIGN-iced.md §1): the default
        // font, so every `text` without an explicit weight renders
        // in Inter Regular; Semibold / Bold / Medium are set
        // per-element with the `theme::inter_font_*` helpers. The
        // four weights are loaded at boot (`App::new`).
        .default_font(theme::inter_font())
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            // Desktop only: the 1100 px minimum is a real window
            // constraint there. On the web (PWA) winit applies it
            // to the canvas, so a 390 px phone would render a
            // 1100 px-wide app and overflow horizontally (Hieu,
            // 2026-11-26: responsive vỡ trên mobile) — no minimum
            // there; the screens stack below 760 px instead.
            #[cfg(not(target_arch = "wasm32"))]
            min_size: Some(iced::Size::new(1100.0, 720.0)),
            #[cfg(target_arch = "wasm32")]
            min_size: None,
            ..Default::default()
        })
        .run()
}
