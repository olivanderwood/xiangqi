//! Lưu trữ bền trên PWA (wasm only): localStorage của trình duyệt.
//!
//! Desktop dùng file JSON (`settings.json` / `saved_games.json`).
//! Trên wasm không có filesystem nên `std::fs` luôn lỗi (được nuốt
//! lặng lẽ trong `save_to` / `load_from`) — settings, tên máy,
//! `device_id` và ván lưu sẽ mất mỗi lần tải lại trang nếu không có
//! module này. Sync cần id máy + ván lưu ổn định giữa các lần mở
//! (không thì mỗi lần mở app lại là một "máy" mới và Lịch sử trống),
//! nên 2 file đó đi qua localStorage trên wasm.
//!
//! Key cố định theo file desktop để dễ đối chiếu:
//! - `xiangqi-settings` ↔ `settings.json`
//! - `xiangqi-saved-games` ↔ `saved_games.json`
//!
//! localStorage là per-origin, dùng được cả trong PWA standalone
//! trên iOS (app đã cài không bị xoá sau 7 ngày như web thường).
//! Quota ~5 MB — file settings + ván lưu (JSON: FEN + nước ICCS)
//! chỉ vài chục KB nên dư sức.

/// Key localStorage cho settings.
pub const SETTINGS_KEY: &str = "xiangqi-settings";
/// Key localStorage cho ván đã lưu.
pub const SAVED_GAMES_KEY: &str = "xiangqi-saved-games";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Đọc một giá trị đã lưu (`None` nếu chưa có / storage không dùng
/// được — vd chế độ ẩn danh chặn localStorage).
#[must_use]
pub fn load(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}

/// Ghi một giá trị (lỗi thì bỏ qua lặng lẽ như `save_to` trên
/// desktop — app vẫn chạy, chỉ không bền).
pub fn save(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}
