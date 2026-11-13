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
    pub human_color: Option<String>,
    pub notation: NotationChoice,
    /// UI language (`"vi"` / `"en"`), persisted; applied at startup.
    #[serde(default)]
    pub language: String,
    pub sound: bool,
    pub animate: bool,
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
            difficulty: 1,
            human_color: None,
            notation: NotationChoice::Vietnamese,
            language: "vi".to_string(),
            sound: true,
            animate: true,
            diversity: true,
            tournament: false,
            custom_elo: default_custom_elo(),
            engine_threads: default_engine_threads(),
            engine_hash_mb: 0,
            engine_trace_log: false,
            pikafish_path: String::new(),
            progress: HashMap::new(),
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

impl Settings {
    /// Load from the default path ([`settings_path`]); defaults on
    /// a missing or corrupt file.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    /// Load from an explicit path (used by tests, so they never
    /// touch the user's real config).
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save to the default path ([`settings_path`]). Creates the
    /// parent directory as needed. Fails silently — a settings
    /// write must never break the game.
    pub fn save(&self) {
        self.save_to(&settings_path());
    }

    /// Save to an explicit path (used by tests).
    pub fn save_to(&self, path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    /// The level the settings select (§4): difficulties 0–3 are the
    /// classic levels, 4 is the custom Elo slider.
    #[must_use]
    pub fn level(&self) -> xq_engine::Level {
        match self.difficulty {
            0 => xq_engine::Level::Easy,
            2 => xq_engine::Level::Hard,
            3 => xq_engine::Level::Master,
            4 => xq_engine::Level::Elo(self.custom_elo),
            _ => xq_engine::Level::Medium,
        }
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
            difficulty: 4,
            custom_elo: 2100,
            language: "en".to_string(),
            notation: NotationChoice::English,
            sound: false,
            engine_threads: 4,
            engine_hash_mb: 512,
            engine_trace_log: true,
            pikafish_path: "/opt/pikafish/pikafish".to_string(),
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
        assert_eq!(Settings::load_from(&path), Settings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir =
            std::env::temp_dir().join(format!("xq-iced-settings-corrupt-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(&path, "{not json").expect("write");
        assert_eq!(Settings::load_from(&path), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_path_ends_with_xiangqi_settings_json() {
        let p = settings_path();
        assert!(p.ends_with(std::path::Path::new("xiangqi/settings.json")));
    }

    #[test]
    fn level_mapping_matches_egui_app() {
        let mut s = Settings::default();
        for (d, level) in [
            (0, xq_engine::Level::Easy),
            (1, xq_engine::Level::Medium),
            (2, xq_engine::Level::Hard),
            (3, xq_engine::Level::Master),
            (4, xq_engine::Level::Elo(1900)),
        ] {
            s.difficulty = d;
            assert_eq!(s.level(), level);
        }
    }
}
