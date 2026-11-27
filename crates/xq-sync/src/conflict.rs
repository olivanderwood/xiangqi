//! Luật xung đột mirroring (plan §5.4).
//!
//! - Cùng một ván (cùng `uuid`): `version` cao hơn thắng. Hoà version
//!   thì `updated_unix` mới hơn thắng; vẫn hoà thì bản remote thắng
//!   (quy tắc cố định để 2 máy luôn ra cùng kết quả).
//! - Ván dở KHÁC hẳn (uuid khác, cả hai đều chưa xong): giữ cả hai —
//!   ván mới hơn (theo `updated_unix`, hoà thì theo `version`) làm ván
//!   đang dở, ván cũ vào Lịch sử dạng "bản lưu trữ từ <tên máy>".
//! - Tombstone (`deleted=true`): máy kia xoá ván local tương ứng.
//!
//! Luật này là pure function, không chạm mạng — test được trực tiếp.

use crate::model::SyncEntry;

/// Kết quả khi so 2 bản của CÙNG một ván.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SameGameOutcome {
    /// Dùng bản local.
    UseLocal,
    /// Dùng bản remote.
    UseRemote,
}

/// So 2 bản của cùng một ván (cùng `uuid`).
#[must_use]
pub fn resolve_same_game(local: &SyncEntry, remote: &SyncEntry) -> SameGameOutcome {
    debug_assert_eq!(local.uuid, remote.uuid);
    if remote.version > local.version {
        return SameGameOutcome::UseRemote;
    }
    if remote.version < local.version {
        return SameGameOutcome::UseLocal;
    }
    if remote.updated_unix > local.updated_unix {
        return SameGameOutcome::UseRemote;
    }
    if remote.updated_unix < local.updated_unix {
        return SameGameOutcome::UseLocal;
    }
    // Hoà cả hai: remote thắng (quy tắc cố định, 2 máy ra cùng kết quả).
    SameGameOutcome::UseRemote
}

/// Kết quả khi có 2 ván dở KHÁC hẳn (uuid khác nhau).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DifferentGamesOutcome {
    /// Ván làm ván đang dở (mới hơn).
    pub current: SyncEntry,
    /// Ván vào Lịch sử dạng bản lưu trữ (cũ hơn).
    pub archived: SyncEntry,
}

/// So 2 ván dở khác nhau: mới hơn làm ván đang dở, cũ hơn lưu trữ.
#[must_use]
pub fn resolve_different_unfinished(a: &SyncEntry, b: &SyncEntry) -> DifferentGamesOutcome {
    let a_newer = (a.updated_unix, a.version) > (b.updated_unix, b.version);
    if a_newer {
        DifferentGamesOutcome {
            current: a.clone(),
            archived: b.clone(),
        }
    } else {
        DifferentGamesOutcome {
            current: b.clone(),
            archived: a.clone(),
        }
    }
}

/// Một entry có phải tombstone (đã xoá) không.
#[must_use]
pub fn is_tombstone(entry: &SyncEntry) -> bool {
    entry.deleted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(uuid: &str, version: u64, updated_unix: u64) -> SyncEntry {
        let mut e = SyncEntry::new(
            uuid.to_string(),
            "Đấu với AI".to_string(),
            "play".to_string(),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1".to_string(),
            2,
            Some(true),
            "device-1".to_string(),
        );
        e.version = version;
        e.updated_unix = updated_unix;
        e
    }

    #[test]
    fn same_game_higher_version_wins() {
        let local = entry("u", 2, 100);
        let remote = entry("u", 3, 50);
        assert_eq!(
            resolve_same_game(&local, &remote),
            SameGameOutcome::UseRemote
        );
        assert_eq!(
            resolve_same_game(&remote, &local),
            SameGameOutcome::UseLocal
        );
    }

    #[test]
    fn same_game_tie_version_newer_time_wins() {
        let local = entry("u", 2, 100);
        let remote = entry("u", 2, 200);
        assert_eq!(
            resolve_same_game(&local, &remote),
            SameGameOutcome::UseRemote
        );
        assert_eq!(
            resolve_same_game(&remote, &local),
            SameGameOutcome::UseLocal
        );
    }

    #[test]
    fn same_game_full_tie_remote_wins() {
        let local = entry("u", 2, 100);
        let remote = entry("u", 2, 100);
        assert_eq!(
            resolve_same_game(&local, &remote),
            SameGameOutcome::UseRemote
        );
    }

    #[test]
    fn different_games_newer_is_current() {
        let a = entry("u-a", 5, 100);
        let b = entry("u-b", 2, 200);
        let out = resolve_different_unfinished(&a, &b);
        assert_eq!(out.current.uuid, "u-b");
        assert_eq!(out.archived.uuid, "u-a");
    }

    #[test]
    fn different_games_tie_time_higher_version_is_current() {
        let a = entry("u-a", 9, 100);
        let b = entry("u-b", 2, 100);
        let out = resolve_different_unfinished(&a, &b);
        assert_eq!(out.current.uuid, "u-a");
        assert_eq!(out.archived.uuid, "u-b");
    }

    #[test]
    fn tombstone_detected() {
        let mut e = entry("u", 1, 1);
        assert!(!is_tombstone(&e));
        e.deleted = true;
        assert!(is_tombstone(&e));
    }
}
