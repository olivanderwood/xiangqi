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
    /// Sync (Phase 1, plan §5.3): ID ổn định của ván trên cloud —
    /// cũng là document ID trên Firestore (`users/{uid}/games/{uuid}`).
    /// Ván mới: UUID v4. Ván cũ migrate: cấp UUID v4 lúc load, giữ id
    /// cũ trong `client_id`.
    #[serde(default)]
    pub uuid: String,
    /// ID cũ (`"<mode>-<unix-millis>"`) của ván đã migrate; None với
    /// ván tạo sau khi có sync.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Số phiên bản sync, tăng 1 mỗi lần đẩy bản mới (luật xung đột:
    /// version cao hơn thắng).
    #[serde(default)]
    pub version: u64,
    /// Unix seconds lúc đẩy bản này lên cloud.
    #[serde(default)]
    pub updated_unix: u64,
    /// ID máy đẩy bản này (`device-<uuid>` trong settings.json).
    /// Rỗng cho tới khi sync worker đẩy lần đầu (worker lấy device id
    /// từ settings, màn hình không biết).
    #[serde(default)]
    pub updated_by_device: String,
    /// Tombstone: true = ván đã bị xoá, giữ document để máy kia
    /// biết mà xoá theo.
    #[serde(default)]
    pub deleted: bool,
}

impl SavedGame {
    /// A fresh id: mode + current unix time in milliseconds. Two
    /// saves in the same millisecond would collide, but the store
    /// replaces an entry with the same id rather than duplicating.
    #[must_use]
    pub fn new_id(mode: SavedMode) -> String {
        let ms = crate::time::SystemTime::now()
            .duration_since(crate::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{}-{ms}", mode.as_str())
    }

    /// Chuyển sang entry đồng bộ (Phase 1 chỉ sync ván Play; Learn /
    /// Practice map được nhưng worker không đẩy).
    #[must_use]
    pub fn to_sync_entry(&self) -> xq_sync::SyncEntry {
        xq_sync::SyncEntry {
            uuid: self.uuid.clone(),
            label: self.label.clone(),
            mode: self.mode.as_str().to_string(),
            start_fen: self.start_fen.clone(),
            moves: self.moves.clone(),
            difficulty: self.difficulty,
            human_red: self.human_red,
            finished: self.finished,
            result: self.result.clone(),
            version: self.version,
            updated_unix: self.updated_unix,
            updated_by_device: self.updated_by_device.clone(),
            deleted: self.deleted,
        }
    }

    /// Tạo SavedGame từ entry đồng bộ (ván kéo về từ cloud). `id` lấy
    /// luôn `uuid` để 2 máy ra cùng id và dedup đúng khi lưu.
    #[must_use]
    pub fn from_sync_entry(e: &xq_sync::SyncEntry) -> Self {
        let mode = match e.mode.as_str() {
            "learn" => SavedMode::Learn,
            "practice" => SavedMode::Practice,
            _ => SavedMode::Play,
        };
        Self {
            id: e.uuid.clone(),
            mode,
            label: e.label.clone(),
            created_unix: e.updated_unix,
            finished: e.finished,
            start_fen: e.start_fen.clone(),
            moves: e.moves.clone(),
            difficulty: e.difficulty,
            human_red: e.human_red,
            result: e.result.clone(),
            lesson_id: None,
            node_path: Vec::new(),
            percent: None,
            uuid: e.uuid.clone(),
            client_id: None,
            version: e.version,
            updated_unix: e.updated_unix,
            updated_by_device: e.updated_by_device.clone(),
            deleted: e.deleted,
        }
    }

    /// Current unix time in seconds.
    #[must_use]
    pub fn now_unix() -> u64 {
        crate::time::SystemTime::now()
            .duration_since(crate::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// ID sync cho một ván mới tạo: UUID v4 (desktop: OS RNG; wasm:
    /// `crypto.getRandomValues` qua feature `js` của getrandom).
    #[must_use]
    pub fn fresh_sync_uuid() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// The store: all saved entries, newest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedGames {
    #[serde(default)]
    pub entries: Vec<SavedGame>,
    /// `true` once entry difficulties have been migrated to the
    /// 8-level scale (Hieu, 2026-11-19): old 0–4 → 1/2/4/6/7.
    /// Old files lack this key, so they load as `false` and migrate
    /// on load. A fresh store is already new-scheme (`true`).
    #[serde(default)]
    pub difficulty_v2: bool,
}

impl Default for SavedGames {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            difficulty_v2: true,
        }
    }
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
    /// touch the user's real config). Trên wasm đọc từ localStorage
    /// (xem `crate::storage`), `path` chỉ dùng cho test trên desktop.
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Self {
        // wasm: không có filesystem — đọc từ localStorage.
        #[cfg(target_arch = "wasm32")]
        let text = {
            let _ = path;
            crate::storage::load(crate::storage::SAVED_GAMES_KEY)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let text = std::fs::read_to_string(path).ok();
        let mut s: Self = text
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.migrate_difficulty();
        s.migrate_sync();
        s
    }

    /// Migrate sync fields cho ván cũ (trước Phase 1): cấp UUID v4,
    /// giữ id cũ trong `client_id`, version = 1, updated_unix =
    /// created_unix (nếu chưa có). Idempotent: ván đã có uuid thì
    /// không đụng vào. Lưu ý: uuid cấp trong bộ nhớ — lần lưu file
    /// kế tiếp mới ghi xuống đĩa; sync worker lưu ngay sau khi
    /// migrate (xem `sync.rs`).
    fn migrate_sync(&mut self) {
        for e in &mut self.entries {
            if e.uuid.is_empty() {
                e.client_id = Some(e.id.clone());
                e.uuid = uuid::Uuid::new_v4().to_string();
                if e.version == 0 {
                    e.version = 1;
                }
                if e.updated_unix == 0 {
                    e.updated_unix = e.created_unix;
                }
            }
        }
    }

    /// Migrate entry difficulties from the old 0–4 scale to the
    /// 8-level scale (0→1, 1→2, 2→4, 3→6, 4→7) so an unfinished
    /// game resumes at the same strength it was saved with.
    fn migrate_difficulty(&mut self) {
        if self.difficulty_v2 {
            return;
        }
        for e in &mut self.entries {
            e.difficulty = match e.difficulty {
                0 => 1,
                1 => 2,
                2 => 4,
                3 => 6,
                4 => 7,
                other => other,
            };
        }
        self.difficulty_v2 = true;
    }

    /// Save to the default path. Creates the parent directory as
    /// needed. Fails silently — a save must never break the game.
    pub fn save(&self) {
        self.save_to(&saved_games_path());
    }

    /// Save to an explicit path (used by tests). Trên wasm ghi vào
    /// localStorage thay vì file (xem `crate::storage`).
    pub fn save_to(&self, path: &std::path::Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = path;
                crate::storage::save(crate::storage::SAVED_GAMES_KEY, &s);
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
            uuid: "a0b2d496-7ddb-44e5-a4e2-be5a3f61aa62".to_string(),
            client_id: None,
            version: 3,
            updated_unix: 1_763_000_100,
            updated_by_device: "device-1".to_string(),
            deleted: false,
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
    fn old_difficulty_scheme_migrates_on_load() {
        // A store written before the 8-level scale (flag false)
        // migrates its entries: old Hard (2) becomes new Mạnh (4).
        let dir = std::env::temp_dir().join("xiangqi-saved-games-migrate");
        let path = dir.join("saved_games.json");
        let mut store = SavedGames::default();
        let mut game = sample_play();
        game.difficulty = 2;
        store.add(game);
        store.difficulty_v2 = false;
        store.save_to(&path);
        let back = SavedGames::load_from(&path);
        assert_eq!(back.entries[0].difficulty, 4);
        assert!(back.difficulty_v2);
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

    #[test]
    fn old_entries_get_sync_ids_on_load() {
        // File viết trước Phase 1: không có field sync nào.
        let dir = std::env::temp_dir().join("xiangqi-saved-games-sync-migrate");
        let path = dir.join("saved_games.json");
        std::fs::create_dir_all(&dir).unwrap();
        let old_json = r#"{
            "entries": [{
                "id": "play-1763000000000",
                "mode": "Play",
                "label": "Đấu với AI",
                "created_unix": 1763000000,
                "finished": false,
                "start_fen": "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
                "moves": ["h2e2"],
                "difficulty": 2
            }],
            "difficulty_v2": true
        }"#;
        std::fs::write(&path, old_json).unwrap();
        let back = SavedGames::load_from(&path);
        let e = &back.entries[0];
        // UUID v4: 36 ký tự, dạng 8-4-4-4-12.
        assert_eq!(e.uuid.len(), 36);
        assert_eq!(e.uuid.matches('-').count(), 4);
        assert_eq!(e.client_id.as_deref(), Some("play-1763000000000"));
        assert_eq!(e.version, 1);
        assert_eq!(e.updated_unix, 1763000000);
        assert!(!e.deleted);
        // Load lại lần nữa (file chưa lưu): uuid mới khác uuid cũ
        // (migrate trong bộ nhớ), nhưng không crash và vẫn hợp lệ.
        let back2 = SavedGames::load_from(&path);
        assert_ne!(back2.entries[0].uuid, e.uuid);
        assert!(back2.entries[0].uuid.len() == 36);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sync_entry_roundtrip() {
        let game = sample_play();
        let entry = game.to_sync_entry();
        assert_eq!(entry.uuid, game.uuid);
        assert_eq!(entry.mode, "play");
        assert_eq!(entry.moves, game.moves);
        assert_eq!(entry.version, 3);
        let back = SavedGame::from_sync_entry(&entry);
        assert_eq!(back.id, entry.uuid);
        assert_eq!(back.uuid, entry.uuid);
        assert_eq!(back.mode, SavedMode::Play);
        assert_eq!(back.moves, game.moves);
        assert_eq!(back.version, 3);
        assert_eq!(back.updated_by_device, "device-1");
    }
}
