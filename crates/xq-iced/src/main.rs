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
mod helpers;
mod i18n;
mod screens;
mod settings;
mod sound;
mod theme;
mod widgets;

pub use app::{App, Message, Screen, Shared};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .theme(|_: &App| theme::custom_theme())
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            min_size: Some(iced::Size::new(1100.0, 720.0)),
            ..Default::default()
        })
        .run()
}
