//! Trait backend sync + 2 implement: Firebase qua REST và mock in-memory.
//!
//! App (xq-iced) chỉ nói chuyện với [`SyncBackend`], nên test UI/worker
//! không cần mạng, và đổi backend sau (vd Firestore listen streaming)
//! không đụng code app.
//!
//! Desktop only: các method đồng bộ (ureq chặn thread worker). Trên
//! wasm, worker async gọi thẳng REST qua fetch (xem
//! `xq-iced/src/sync_fetch.rs`) và dùng chung phần pure của crate này
//! (model / conflict / firestore URL + parse) thay vì trait này.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use crate::auth::{firebase_refresh, firebase_sign_in_with_google, google_sign_in, Tokens};
#[cfg(not(target_arch = "wasm32"))]
use crate::config::{FirebaseConfig, OAuthCredentials};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::SyncError;
#[cfg(not(target_arch = "wasm32"))]
use crate::firestore::{get_game, list_games, upsert_game};
#[cfg(not(target_arch = "wasm32"))]
use crate::model::SyncEntry;

/// Backend sync: đăng nhập + đẩy/kéo ván cờ.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub trait SyncBackend: Send {
    /// Đăng nhập Google rồi đổi lấy token Firebase (mở trình duyệt).
    fn sign_in_google(&self) -> Result<Tokens, SyncError>;
    /// Đổi refresh_token lấy id_token mới.
    fn refresh(&self, refresh_token: &str) -> Result<String, SyncError>;
    /// Đẩy một ván lên cloud (upsert).
    fn push(&self, id_token: &str, uid: &str, entry: &SyncEntry) -> Result<(), SyncError>;
    /// Đọc một ván từ cloud (`None` nếu chưa có).
    fn pull_one(
        &self,
        id_token: &str,
        uid: &str,
        uuid: &str,
    ) -> Result<Option<SyncEntry>, SyncError>;
    /// Liệt kê mọi ván của người dùng (kể cả tombstone).
    fn pull_all(&self, id_token: &str, uid: &str) -> Result<Vec<SyncEntry>, SyncError>;
}

/// Backend Firebase thật qua REST.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub struct FirebaseBackend {
    config: FirebaseConfig,
    /// OAuth client (Desktop) — nạp lúc chạy, không nằm trong repo.
    /// `None` thì đăng nhập Google báo lỗi hướng dẫn cách tạo.
    oauth: Option<OAuthCredentials>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FirebaseBackend {
    #[must_use]
    pub fn new(config: FirebaseConfig) -> Self {
        Self {
            config,
            oauth: None,
        }
    }

    /// Gắn OAuth client đã nạp lúc chạy (biến môi trường / file).
    #[must_use]
    pub fn with_oauth(mut self, oauth: OAuthCredentials) -> Self {
        self.oauth = Some(oauth);
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SyncBackend for FirebaseBackend {
    fn sign_in_google(&self) -> Result<Tokens, SyncError> {
        let Some(oauth) = &self.oauth else {
            return Err(SyncError::Auth(
                "chua co OAuth client: dat bien moi truong GOOGLE_CLIENT_ID + GOOGLE_CLIENT_SECRET, hoac tao file sync-oauth.json canh settings.json roi khoi dong lai app"
                    .to_string(),
            ));
        };
        let google_id_token = google_sign_in(oauth)?;
        firebase_sign_in_with_google(&self.config, &google_id_token)
    }

    fn refresh(&self, refresh_token: &str) -> Result<String, SyncError> {
        firebase_refresh(&self.config, refresh_token)
    }

    fn push(&self, id_token: &str, uid: &str, entry: &SyncEntry) -> Result<(), SyncError> {
        upsert_game(&self.config, id_token, uid, entry)
    }

    fn pull_one(
        &self,
        id_token: &str,
        uid: &str,
        uuid: &str,
    ) -> Result<Option<SyncEntry>, SyncError> {
        get_game(&self.config, id_token, uid, uuid)
    }

    fn pull_all(&self, id_token: &str, uid: &str) -> Result<Vec<SyncEntry>, SyncError> {
        list_games(&self.config, id_token, uid)
    }
}

/// Backend mock in-memory (test + chạy offline): lưu entry trong HashMap,
/// không chạm mạng. `sign_in_google` trả token giả cố định.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub struct MockBackend {
    store: Mutex<HashMap<String, SyncEntry>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl MockBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SyncBackend for MockBackend {
    fn sign_in_google(&self) -> Result<Tokens, SyncError> {
        Ok(Tokens {
            id_token: "mock-id-token".to_string(),
            refresh_token: "mock-refresh-token".to_string(),
            uid: "mock-uid".to_string(),
            email: "mock@example.com".to_string(),
        })
    }

    fn refresh(&self, _refresh_token: &str) -> Result<String, SyncError> {
        Ok("mock-id-token-refreshed".to_string())
    }

    fn push(&self, _id_token: &str, _uid: &str, entry: &SyncEntry) -> Result<(), SyncError> {
        if !entry.is_valid() {
            return Err(SyncError::Parse("entry khong hop le".to_string()));
        }
        let mut store = self
            .store
            .lock()
            .map_err(|_| SyncError::Io("mock store bi khoa".to_string()))?;
        store.insert(entry.uuid.clone(), entry.clone());
        Ok(())
    }

    fn pull_one(
        &self,
        _id_token: &str,
        _uid: &str,
        uuid: &str,
    ) -> Result<Option<SyncEntry>, SyncError> {
        let store = self
            .store
            .lock()
            .map_err(|_| SyncError::Io("mock store bi khoa".to_string()))?;
        Ok(store.get(uuid).cloned())
    }

    fn pull_all(&self, _id_token: &str, _uid: &str) -> Result<Vec<SyncEntry>, SyncError> {
        let store = self
            .store
            .lock()
            .map_err(|_| SyncError::Io("mock store bi khoa".to_string()))?;
        Ok(store.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(uuid: &str) -> SyncEntry {
        SyncEntry::new(
            uuid.to_string(),
            "Đấu với AI".to_string(),
            "play".to_string(),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1".to_string(),
            2,
            Some(true),
            "device-1".to_string(),
        )
    }

    #[test]
    fn mock_push_pull_roundtrip() {
        let b = MockBackend::new();
        let e = sample("uuid-1");
        b.push("tok", "uid", &e).expect("push phai duoc");
        let got = b.pull_one("tok", "uid", "uuid-1").expect("pull phai duoc");
        assert_eq!(got, Some(e));
        assert_eq!(b.pull_one("tok", "uid", "khong-co").unwrap(), None);
    }

    #[test]
    fn mock_pull_all_lists_everything() {
        let b = MockBackend::new();
        b.push("tok", "uid", &sample("uuid-1")).unwrap();
        b.push("tok", "uid", &sample("uuid-2")).unwrap();
        let mut all = b.pull_all("tok", "uid").unwrap();
        all.sort_by(|a, c| a.uuid.cmp(&c.uuid));
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].uuid, "uuid-1");
        assert_eq!(all[1].uuid, "uuid-2");
    }

    #[test]
    fn mock_sign_in_returns_fixed_tokens() {
        let b = MockBackend::new();
        let t = b.sign_in_google().unwrap();
        assert_eq!(t.uid, "mock-uid");
        assert!(!t.id_token.is_empty());
    }
}
