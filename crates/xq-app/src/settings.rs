//! Persisted settings (§6.6), stored via `eframe::Storage`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum NotationChoice {
    Chinese,
    Vietnamese,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LessonProgress {
    pub completed: bool,
    pub best_book_ratio: f32,
    pub attempts: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub difficulty: u8,
    pub human_color: Option<String>,
    pub notation: NotationChoice,
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

impl Settings {
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        storage
            .get_string("xiangqi-settings")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(self) {
            storage.set_string("xiangqi-settings", s);
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
