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
    pub progress: HashMap<String, LessonProgress>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            difficulty: 1,
            human_color: None,
            notation: NotationChoice::Vietnamese,
            sound: true,
            animate: true,
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
