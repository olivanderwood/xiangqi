//! Saved games and lesson progress (Hieu, 2026-11-14).
//!
//! Three modes, one store — a JSON file next to the settings
//! (`<config_dir>/xiangqi/saved_games.json`, same pattern as
//! `settings.rs`: `load_from` / `save_to` with explicit paths so
//! tests never touch the user's real config).
//!
//! - **Play vs AI**: a finished game that the user chose to save
//!   goes to the "Lịch sử đấu" (history); an unfinished game that
//!   the user chose to save is the resumable game — re-entering
//!   Play asks whether to continue it (yes loads it, no deletes
//!   it).
//! - **Learn**: only an unfinished lesson is saved (a finished
//!   lesson is 100% and needs no history). The entry holds the
//!   lesson id, the node path through the lesson tree and the %
//!   learned, so the lesson list can show the % outside and
//!   resuming lands back at the same state.
//! - **Practice**: finished and unfinished games alike can be
//!   saved; the setup screen has a "ván cờ đã lưu" button listing
//!   them.
//!
//! A saved game is a start FEN plus ICCS moves — the board is
//! rebuilt by replaying the moves, so no engine or book state is
//! persisted. A save write must never break the game: a missing or
//! corrupt file loads as empty, and a failed write is silent.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SAVED_GAMES_FILE: &str = "saved_games.json";

/// Which mode a saved entry belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SavedMode {
    Play,
    Learn,
    Practice,
}

impl SavedMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SavedMode::Play => "play",
            SavedMode::Learn => "learn",
            SavedMode::Practice => "practice",
        }
    }
}

/// One saved game (Play / Practice) or one saved lesson progress
/// (Learn). `moves` are ICCS strings (`"h2e2"`); the board is
/// rebuilt by replaying them from `start_fen`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedGame {
    /// Unique id (`"<mode>-<unix-millis>"`).
    pub id: String,
    pub mode: SavedMode,
    /// Display label, built at save time (e.g. `"Đấu với AI — Đỏ
    /// thắng — 24 nước"`).
    pub label: String,
    /// Seconds since the Unix epoch, when the entry was saved.
    pub created_unix: u64,
    /// The game ended (Play / Practice) or the lesson is complete
    /// (Learn — never stored, a finished lesson needs no entry).
    pub finished: bool,
    /// Starting position (FEN).
    pub start_fen: String,
    /// Moves played, ICCS (`"h2e2"`).
    pub moves: Vec<String>,
    /// Play / Practice difficulty (0–4, same scale as Settings).
    #[serde(default)]
    pub difficulty: u8,
    /// Play only: the human plays Red (`true`) or Black (`false`).
    #[serde(default)]
    pub human_red: Option<bool>,
    /// Play / Practice only: `"red"`, `"black"` or `"draw"` (None
    /// while the game is unfinished).
    #[serde(default)]
    pub result: Option<String>,
    /// Learn only: the lesson id.
    #[serde(default)]
    pub lesson_id: Option<String>,
    /// Learn only: the path through the lesson tree (`Tutor`
    /// `node_path`) at save time.
    #[serde(default)]
    pub node_path: Vec<usize>,
    /// Learn only: % learned at save time.
    #[serde(default)]
    pub percent: Option<u8>,
}

impl SavedGame {
    /// A fresh id: mode + current unix time in milliseconds. Two
    /// saves in the same millisecond would collide, but the store
    /// replaces an entry with the same id rather than duplicating.
    #[must_use]
    pub fn new_id(mode: SavedMode) -> String {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{}-{ms}", mode.as_str())
    }

    /// Current unix time in seconds.
    #[must_use]
    pub fn now_unix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// The store: all saved entries, newest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedGames {
    #[serde(default)]
    pub entries: Vec<SavedGame>,
}

pub fn saved_games_path() -> PathBuf {
    let base = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("xiangqi").join(SAVED_GAMES_FILE)
}

impl SavedGames {
    /// Load from the default path; empty on a missing or corrupt file.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&saved_games_path())
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

    /// Save to the default path. Creates the parent directory as
    /// needed. Fails silently — a save must never break the game.
    pub fn save(&self) {
        self.save_to(&saved_games_path());
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

    /// Insert (or replace by id) at the front, newest first.
    pub fn add(&mut self, game: SavedGame) {
        self.entries.retain(|e| e.id != game.id);
        self.entries.insert(0, game);
    }

    /// Remove an entry by id. Returns whether one was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// Entries of one mode, newest first.
    #[must_use]
    pub fn by_mode(&self, mode: SavedMode) -> Vec<&SavedGame> {
        self.entries.iter().filter(|e| e.mode == mode).collect()
    }

    /// The latest unfinished Play game (the resumable one), if any.
    /// At most one unfinished Play game is kept: saving a new one
    /// replaces the old.
    #[must_use]
    pub fn unfinished_play(&self) -> Option<&SavedGame> {
        self.entries
            .iter()
            .find(|e| e.mode == SavedMode::Play && !e.finished)
    }

    /// The saved progress for one lesson, if any.
    #[must_use]
    pub fn lesson_progress(&self, lesson_id: &str) -> Option<&SavedGame> {
        self.entries
            .iter()
            .find(|e| e.mode == SavedMode::Learn && e.lesson_id.as_deref() == Some(lesson_id))
    }
}

/// Format a unix timestamp as `DD/MM/YYYY HH:MM` (local time is
/// not available without a date library, so this is UTC — the
/// history list only needs a rough "when").
/// Uses the days-from-civil algorithm (Howard Hinnant, public
/// domain).
#[must_use]
pub fn format_date(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{d:02}/{m:02}/{y} {:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_play() -> SavedGame {
        SavedGame {
            id: "play-1".to_string(),
            mode: SavedMode::Play,
            label: "Đấu với AI — Đỏ thắng — 24 nước".to_string(),
            created_unix: 1_763_000_000,
            finished: true,
            start_fen: xq_core::Position::startpos().to_fen(),
            moves: vec!["h2e2".to_string(), "h7e7".to_string()],
            difficulty: 1,
            human_red: Some(true),
            result: Some("red".to_string()),
            lesson_id: None,
            node_path: Vec::new(),
            percent: None,
        }
    }

    #[test]
    fn roundtrip_through_json() {
        let mut store = SavedGames::default();
        store.add(sample_play());
        let dir = std::env::temp_dir().join("xiangqi-saved-games-test");
        let path = dir.join("saved_games.json");
        store.save_to(&path);
        let back = SavedGames::load_from(&path);
        assert_eq!(back, store);
        assert_eq!(back.by_mode(SavedMode::Play).len(), 1);
        assert_eq!(back.by_mode(SavedMode::Learn).len(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let dir = std::env::temp_dir().join("xiangqi-saved-games-corrupt");
        let path = dir.join("saved_games.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(SavedGames::load_from(&path), SavedGames::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unfinished_play_and_lesson_progress() {
        let mut store = SavedGames::default();
        let mut game = sample_play();
        game.finished = false;
        game.result = None;
        store.add(game);
        assert!(store.unfinished_play().is_some());
        assert!(store.lesson_progress("c45").is_none());
        let mut lesson = sample_play();
        lesson.id = "learn-1".to_string();
        lesson.mode = SavedMode::Learn;
        lesson.finished = false;
        lesson.lesson_id = Some("c45".to_string());
        lesson.percent = Some(60);
        store.add(lesson);
        assert_eq!(
            store.lesson_progress("c45").and_then(|e| e.percent),
            Some(60)
        );
        // Removing the unfinished game clears the resume prompt.
        let id = store.unfinished_play().unwrap().id.clone();
        assert!(store.remove(&id));
        assert!(store.unfinished_play().is_none());
    }

    #[test]
    fn format_date_is_sane() {
        // 2026-11-14 20:00:00 UTC.
        assert_eq!(format_date(1_794_686_400), "14/11/2026 20:00");
        assert_eq!(format_date(0), "01/01/1970 00:00");
    }
}
