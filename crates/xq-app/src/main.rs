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

#[cfg(test)]
mod smoke_tests;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        i18n::t(i18n::Key::AppTitle),
        options,
        Box::new(|cc| Ok(Box::new(app::XiangqiApp::new(cc)))),
    )
}
