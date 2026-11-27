//! Persisted settings (§6.6), stored as JSON in the user's config
//! directory.
//!
//! Same fields and defaults as the egui app
//! (`crates/xq-app/src/settings.rs`); only the storage backend
//! changed: `eframe::Storage` does not exist in Iced, so settings
//! live in a plain file instead. The app saves automatically
//! whenever a setting changes (call [`Settings::save`] after a
//! mutation) — there is no Save button, and no "restore defaults"
//! button, matching the egui app (see `docs/SPIKE-iced.md`).
//!
//! File location: `<config_dir>/xiangqi/settings.json` —
//! `~/.config/xiangqi/settings.json` on Linux,
//! `%APPDATA%\xiangqi\settings.json` on Windows,
//! `~/Library/Application Support/xiangqi/settings.json` on macOS
//! (via the `dirs` crate). Loading fails open: a missing or
//! corrupt file yields the defaults.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// How move names are shown in the UI. Chinese characters appear only
/// on the board pieces (Hieu, 2026-11-12); move names are Vietnamese
/// or English.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum NotationChoice {
    Vietnamese,
    English,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LessonProgress {
    pub completed: bool,
    pub best_book_ratio: f32,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub difficulty: u8,
    /// `true` once the difficulty scale has been migrated to the
    /// 8-level scheme (Hieu, 2026-11-19): old values 0–4 meant
    /// Easy/Medium/Hard/Master/Custom; the new scale is 0–7
    /// (Beginner/Easy/Medium/Fairly strong/Strong/Expert/Master/
    /// Custom). Old files lack this key, so they load as `false`
    /// and migrate on load (see [`Settings::migrate_difficulty`]).
    #[serde(default)]
    pub difficulty_v2: bool,
    pub human_color: Option<String>,
    pub notation: NotationChoice,
    /// UI language (`"vi"` / `"en"`), persisted; applied at startup.
    #[serde(default)]
    pub language: String,
    pub sound: bool,
    /// §5: varied Master moves (default on).
    #[serde(default = "default_true")]
    pub diversity: bool,
    /// §5: tournament mode — diversity fully off (default off).
    #[serde(default)]
    pub tournament: bool,
    /// §4: custom Elo for difficulty 4 (slider, step 50).
    #[serde(default = "default_custom_elo")]
    pub custom_elo: u16,
    /// §7: Pikafish threads (1–8).
    #[serde(default = "default_engine_threads")]
    pub engine_threads: u32,
    /// §7: Pikafish hash in MB (0 = auto, by RAM).
    #[serde(default)]
    pub engine_hash_mb: u32,
    /// §7: record raw UCI lines in engine.log (default off).
    #[serde(default)]
    pub engine_trace_log: bool,
    /// §7: user-set engine path (empty = auto-locate).
    #[serde(default)]
    pub pikafish_path: String,
    pub progress: HashMap<String, LessonProgress>,
    /// Sync mirroring Phase 1 (2026-11-27): ID máy này, dạng
    /// `device-<uuid>`, ghi vào `updated_by_device` mỗi lần đẩy.
    /// Cấp một lần lúc load nếu chưa có (desktop only — uuid v4 cần
    /// OS RNG; PWA chưa sync nên để trống).
    #[serde(default)]
    pub device_id: String,
    /// Tên thân thiện của máy này (hiện trong "bản lưu trữ từ ...").
    /// Mặc định "Máy này", đổi được trong Cài đặt → Tài khoản.
    #[serde(default = "default_device_name")]
    pub device_name: String,
    /// Email Google đang đăng nhập sync (chỉ để hiển thị; token ở
    /// keychain, KHÔNG ở đây). None = chưa đăng nhập.
    #[serde(default)]
    pub sync_email: Option<String>,
}

fn default_device_name() -> String {
    "Máy này".to_string()
}

fn default_true() -> bool {
    true
}

fn default_custom_elo() -> u16 {
    1900
}

fn default_engine_threads() -> u32 {
    xq_pikafish::default_threads()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            difficulty: 2,
            difficulty_v2: true,
            human_color: None,
            notation: NotationChoice::Vietnamese,
            language: "vi".to_string(),
            sound: true,
            diversity: true,
            tournament: false,
            custom_elo: default_custom_elo(),
            engine_threads: default_engine_threads(),
            engine_hash_mb: 0,
            engine_trace_log: false,
            pikafish_path: String::new(),
            progress: HashMap::new(),
            device_id: String::new(),
            device_name: default_device_name(),
            sync_email: None,
        }
    }
}

/// File name of the settings file, under the config dir.
const SETTINGS_FILE: &str = "settings.json";

/// Path of the settings file:
/// `<config_dir>/xiangqi/settings.json`. Falls back to
/// `~/.config/xiangqi/settings.json` when the OS reports no config
/// dir, and to `./xiangqi/settings.json` when even the home dir is
/// unknown (both should never happen on a real desktop).
#[must_use]
pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("xiangqi").join(SETTINGS_FILE)
}

/// Difficulty index (0..=7) to its engine level: fixed points on the
/// Pikafish Elo table (`xq_pikafish::levels` anchors) — 0 → 1000,
/// 1 → 1400, 2 → 1900, 3 → 2200, 4 → 2400, 5 → 2600, 6 → Master,
/// 7 → the custom Elo from Settings. Shared by the Play screen, the
/// Practice screen and [`Settings::level`] so the three never drift.
#[must_use]
pub fn difficulty_level(difficulty: u8, custom_elo: u16) -> xq_engine::Level {
    match difficulty {
        0 => xq_engine::Level::Elo(1000),
        1 => xq_engine::Level::Elo(1400),
        2 => xq_engine::Level::Elo(1900),
        3 => xq_engine::Level::Elo(2200),
        4 => xq_engine::Level::Elo(2400),
        5 => xq_engine::Level::Elo(2600),
        6 => xq_engine::Level::Master,
        7 => xq_engine::Level::Elo(custom_elo),
        _ => xq_engine::Level::Elo(1900),
    }
}

impl Settings {
    /// Load from the default path ([`settings_path`]); defaults on
    /// a missing or corrupt file. Cấp `device_id` rồi ghi lại ngay
    /// nếu file chưa có nó — id máy phải ổn định giữa các lần mở
    /// (không cấp mới mỗi lần chạy).
    #[must_use]
    pub fn load() -> Self {
        let path = settings_path();
        let mut s = Self::load_from(&path);
        if s.ensure_device_id() {
            s.save_to(&path);
        }
        s
    }

    /// Load from an explicit path (used by tests, so they never
    /// touch the user's real config). Không cấp `device_id` — caller
    /// nào cần thì gọi [`Settings::ensure_device_id`] (xem
    /// [`Settings::load`]).
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Self {
        // wasm: không có filesystem — đọc từ localStorage (xem
        // `crate::storage`); `path` chỉ dùng cho test trên desktop.
        #[cfg(target_arch = "wasm32")]
        let text = {
            let _ = path;
            crate::storage::load(crate::storage::SETTINGS_KEY)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let text = std::fs::read_to_string(path).ok();
        let mut s: Self = text
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.migrate_difficulty();
        s
    }

    /// Cấp `device_id` (`device-<uuid>`) nếu file cũ chưa có. Trả
    /// `true` khi vừa cấp (để `load` ghi lại ngay).
    pub fn ensure_device_id(&mut self) -> bool {
        if self.device_id.is_empty() {
            self.device_id = format!("device-{}", uuid::Uuid::new_v4());
            return true;
        }
        false
    }

    /// Migrate an old-scheme difficulty (0–4) to the 8-level scale:
    /// 0 → 1 (Dễ), 1 → 2 (Trung bình), 2 → 4 (Mạnh), 3 → 6 (Cao thủ),
    /// 4 → 7 (Tùy chỉnh). New-scheme files are untouched. Called by
    /// [`Settings::load_from`]; a fresh default is already new-scheme.
    pub fn migrate_difficulty(&mut self) {
        if self.difficulty_v2 {
            return;
        }
        self.difficulty = match self.difficulty {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 6,
            4 => 7,
            other => other,
        };
        self.difficulty_v2 = true;
    }

    /// Save to the default path ([`settings_path`]). Creates the
    /// parent directory as needed. Fails silently — a settings
    /// write must never break the game.
    pub fn save(&self) {
        self.save_to(&settings_path());
    }

    /// Save to an explicit path (used by tests). Trên wasm ghi vào
    /// localStorage thay vì file (xem `crate::storage`).
    pub fn save_to(&self, path: &std::path::Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = path;
                crate::storage::save(crate::storage::SETTINGS_KEY, &s);
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(parent) = path.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        return;
                    }
                }
                let _ = std::fs::write(path, s);
            }
        }
    }

    /// The level the settings select (§4): difficulties 0–6 are fixed
    /// points on the Pikafish Elo table, 7 is the custom Elo slider.
    #[must_use]
    pub fn level(&self) -> xq_engine::Level {
        difficulty_level(self.difficulty, self.custom_elo)
    }

    /// User-set engine path, if any.
    #[must_use]
    pub fn pikafish_path(&self) -> Option<std::path::PathBuf> {
        let p = self.pikafish_path.trim();
        if p.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(p))
        }
    }

    /// Record a lesson result (§6.6 progress map).
    pub fn record_lesson(&mut self, lesson_id: &str, completed: bool, book_ratio: f32) {
        let e = self
            .progress
            .entry(lesson_id.to_string())
            .or_insert(LessonProgress {
                completed: false,
                best_book_ratio: 0.0,
                attempts: 0,
            });
        e.attempts += 1;
        if completed {
            e.completed = true;
        }
        if book_ratio > e.best_book_ratio {
            e.best_book_ratio = book_ratio;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_settings() -> Settings {
        let mut s = Settings {
            difficulty: 7,
            custom_elo: 2100,
            language: "en".to_string(),
            notation: NotationChoice::English,
            sound: false,
            engine_threads: 4,
            engine_hash_mb: 512,
            engine_trace_log: true,
            pikafish_path: "/opt/pikafish/pikafish".to_string(),
            device_id: "device-test-1".to_string(),
            device_name: "Máy test".to_string(),
            sync_email: Some("a@b.c".to_string()),
            ..Default::default()
        };
        s.record_lesson("c64", true, 0.8);
        s
    }

    #[test]
    fn json_round_trip() {
        let s = sample_settings();
        let json = serde_json::to_string(&s).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn file_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("xq-iced-settings-test-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = sample_settings();
        s.save_to(&path);
        let back = Settings::load_from(&path);
        assert_eq!(s, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let path = std::env::temp_dir().join(format!(
            "xq-iced-settings-missing-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut back = Settings::load_from(&path);
        // `ensure_device_id` cấp id mới cho file chưa có — không tính.
        back.device_id = String::new();
        assert_eq!(back, Settings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir =
            std::env::temp_dir().join(format!("xq-iced-settings-corrupt-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(&path, "{not json").expect("write");
        let mut back = Settings::load_from(&path);
        // `ensure_device_id` cấp id mới cho file hỏng — không tính.
        back.device_id = String::new();
        assert_eq!(back, Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_path_ends_with_xiangqi_settings_json() {
        let p = settings_path();
        assert!(p.ends_with(std::path::Path::new("xiangqi/settings.json")));
    }

    #[test]
    fn level_mapping() {
        let mut s = Settings {
            custom_elo: 1900,
            ..Settings::default()
        };
        for (d, level) in [
            (0, xq_engine::Level::Elo(1000)),
            (1, xq_engine::Level::Elo(1400)),
            (2, xq_engine::Level::Elo(1900)),
            (3, xq_engine::Level::Elo(2200)),
            (4, xq_engine::Level::Elo(2400)),
            (5, xq_engine::Level::Elo(2600)),
            (6, xq_engine::Level::Master),
            (7, xq_engine::Level::Elo(1900)),
        ] {
            s.difficulty = d;
            assert_eq!(s.level(), level);
        }
    }

    #[test]
    fn old_difficulty_scheme_migrates_on_load() {
        // A settings file written before the 8-level scale (no
        // `difficulty_v2` flag) migrates: 0→1, 1→2, 2→4, 3→6, 4→7.
        let dir =
            std::env::temp_dir().join(format!("xq-iced-settings-migrate-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).expect("mkdir");
        for (old, new) in [(0u8, 1u8), (1, 2), (2, 4), (3, 6), (4, 7)] {
            let s = Settings {
                difficulty: old,
                difficulty_v2: false,
                ..Settings::default()
            };
            let json = serde_json::to_string(&s).expect("serialize");
            std::fs::write(&path, json).expect("write");
            let back = Settings::load_from(&path);
            assert_eq!(
                back.difficulty, new,
                "old difficulty {old} must migrate to {new}"
            );
            assert!(back.difficulty_v2);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn device_id_is_generated_once_and_stable() {
        // File cũ chưa có device_id: load cấp một cái, lưu lại rồi
        // load nữa vẫn là cái đó (không cấp mới mỗi lần).
        let dir =
            std::env::temp_dir().join(format!("xq-iced-settings-device-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let s = Settings {
            device_id: String::new(),
            ..Settings::default()
        };
        // Ghi tay không qua ensure_device_id (giả file cũ).
        let json = serde_json::to_string(&s).expect("serialize");
        std::fs::write(&path, json).expect("write");
        let mut back = Settings::load_from(&path);
        assert!(back.ensure_device_id(), "file cu chua co id thi phai cap");
        assert!(
            back.device_id.starts_with("device-"),
            "device_id phai duoc cap: {}",
            back.device_id
        );
        assert_eq!(back.device_id.len(), 43); // "device-" + uuid 36 ký tự
        back.save_to(&path);
        let mut back2 = Settings::load_from(&path);
        assert!(!back2.ensure_device_id(), "file da co id thi khong cap moi");
        assert_eq!(back2.device_id, back.device_id);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
