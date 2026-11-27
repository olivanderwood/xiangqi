//! Bản ghi ván cờ đồng bộ: một ván = một document Firestore tại
//! `users/{uid}/games/{uuid}` (plan §5.3).
//!
//! Field ván cờ (FEN + nước ICCS) giống `SavedGame` của xq-iced; field
//! sync (`uuid`, `version`, `updated_unix`, `updated_by_device`,
//! `deleted`) là metadata mirroring. `xq-iced` map qua lại giữa
//! `SavedGame` và `SyncEntry` (xem `crates/xq-iced/src/saved_games.rs`).

use serde::{Deserialize, Serialize};

/// Một ván cờ trên cloud.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEntry {
    /// ID ổn định của ván, cũng là document ID trên Firestore.
    /// Ván mới: UUID v4. Ván cũ migrate: dùng lại id cũ.
    pub uuid: String,
    /// Nhãn hiển thị (vd "Đấu với AI — Đỏ đi trước").
    pub label: String,
    /// Chế độ: phase 1 chỉ "play".
    pub mode: String,
    /// FEN lúc bắt đầu ván.
    pub start_fen: String,
    /// Các nước đã đi, dạng ICCS (vd "h2e2").
    pub moves: Vec<String>,
    /// Mức khó 0–7 (sau migration difficulty_v2).
    pub difficulty: u8,
    /// `Some(true)` = người chơi cầm Đỏ; `None` = ván 2 người (không sync
    /// ở phase 1, nhưng field vẫn lưu để tương thích sau).
    pub human_red: Option<bool>,
    /// Ván đã kết thúc chưa.
    pub finished: bool,
    /// Kết quả: "red" | "black" | "draw" (None khi chưa kết thúc).
    pub result: Option<String>,
    /// Số phiên bản, tăng 1 mỗi lần máy nào đó đẩy bản mới.
    /// Luật xung đột (plan §5.4): `version` cao hơn thắng.
    pub version: u64,
    /// Unix seconds lúc đẩy bản này (đồng hồ máy đẩy; cả 2 máy của Hieu
    /// đều bật đồng bộ giờ tự động nên lệch không đáng kể).
    pub updated_unix: u64,
    /// ID máy đẩy bản này (`device-<uuid>` trong settings.json).
    pub updated_by_device: String,
    /// Tombstone: true = ván đã bị xoá (giữ document để máy kia biết mà
    /// xoá theo, thay vì xoá hẳn).
    pub deleted: bool,
}

impl SyncEntry {
    /// Tạo entry mới cho một ván (version = 1).
    #[must_use]
    pub fn new(
        uuid: String,
        label: String,
        mode: String,
        start_fen: String,
        difficulty: u8,
        human_red: Option<bool>,
        updated_by_device: String,
    ) -> Self {
        Self {
            uuid,
            label,
            mode,
            start_fen,
            moves: Vec::new(),
            difficulty,
            human_red,
            finished: false,
            result: None,
            version: 1,
            updated_unix: now_unix(),
            updated_by_device,
            deleted: false,
        }
    }

    /// Kiểm tra entry hợp lệ tối thiểu trước khi đẩy lên cloud.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.uuid.is_empty() && self.version >= 1 && !self.start_fen.is_empty()
    }

    /// Chuẩn bị bản mới sau một nước đi (hoặc sau khi ván kết thúc):
    /// tăng version, cập nhật thời gian + máy đẩy.
    pub fn bump(&mut self, device_id: &str) {
        self.version = self.version.saturating_add(1);
        self.updated_unix = now_unix();
        self.updated_by_device = device_id.to_string();
    }
}

/// Unix seconds hiện tại (đồng hồ hệ thống).
#[must_use]
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SyncEntry {
        SyncEntry::new(
            "a0b2d496-7ddb-44e5-a4e2-be5a3f61aa62".to_string(),
            "Đấu với AI".to_string(),
            "play".to_string(),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1".to_string(),
            2,
            Some(true),
            "device-1".to_string(),
        )
    }

    #[test]
    fn new_entry_is_valid_and_version_1() {
        let e = sample();
        assert!(e.is_valid());
        assert_eq!(e.version, 1);
        assert!(e.moves.is_empty());
        assert!(!e.finished);
        assert!(!e.deleted);
    }

    #[test]
    fn invalid_when_uuid_or_fen_empty_or_version_zero() {
        let mut e = sample();
        e.uuid.clear();
        assert!(!e.is_valid());
        let mut e = sample();
        e.start_fen.clear();
        assert!(!e.is_valid());
        let mut e = sample();
        e.version = 0;
        assert!(!e.is_valid());
    }

    #[test]
    fn bump_increments_version_and_device() {
        let mut e = sample();
        e.bump("device-2");
        assert_eq!(e.version, 2);
        assert_eq!(e.updated_by_device, "device-2");
    }
}
