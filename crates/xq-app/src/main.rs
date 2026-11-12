#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_worker;
mod app;
mod board_view;
mod chessdb_worker;
mod engine_log;
mod engine_manager;
mod i18n;
mod screens;
mod settings;
mod sound;

#[cfg(test)]
mod smoke_tests;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            // Mở tối đa theo mặc định (Hieu, 2026-11-12): cửa sổ
            // 1280x800 trông nhỏ trên màn hình lớn; mở tối đa thì
            // bàn cờ và giao diện dùng hết màn hình. Người dùng vẫn
            // thu nhỏ lại được như cửa sổ thường.
            .with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        i18n::t(i18n::Key::AppTitle),
        options,
        Box::new(|cc| Ok(Box::new(app::XiangqiApp::new(cc)))),
    )
}
