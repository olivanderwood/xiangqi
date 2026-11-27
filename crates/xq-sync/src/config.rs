//! Cấu hình Firebase cho sync.
//!
//! Project `xiangqi-sync` (tài khoản hieuvd1412@gmail.com, gói Spark
//! $0, tạo 2026-11-27).
//!
//! Chỉ 2 giá trị public được nhúng vào code:
//! - API key của Firebase Web là public by-design (Google ghi rõ trong
//!   tài liệu Firebase: key chỉ định danh project, không phải bí mật).
//!   Bảo mật thật nằm ở Firestore rule chỉ-owner (plan §5.1).
//! - Project ID cũng public (nằm trong URL Firestore).
//!
//! OAuth client ID + client secret (loại Desktop) KHÔNG nằm trong
//! repo: GitHub push protection chặn cả hai (quét được pattern
//! `GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET`), và blob
//! wasm trên gh-pages cũng bị chặn theo. Nạp lúc chạy từ biến môi
//! trường (giống spike Phase 0) hoặc file JSON cạnh `settings.json`:
//! - `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET`, hoặc
//! - file `sync-oauth.json`: `{"client_id": "...", "client_secret": "..."}`
//!   đặt cạnh `settings.json` (Windows: `%APPDATA%\xiangqi\`,
//!   macOS: `~/Library/Application Support/xiangqi/`).
//!   Tạo file xong khởi động lại app. PWA không cần file này (web
//!   SDK chỉ dùng apiKey + projectId).
//! - Đổi project thì sửa 2 hằng dưới + tạo lại file OAuth.

use std::path::Path;

/// Cấu hình Firebase cho sync (chỉ giá trị public).
#[derive(Clone, Copy, Debug)]
pub struct FirebaseConfig {
    /// Web API key (public by-design).
    pub api_key: &'static str,
    /// Project ID.
    pub project_id: &'static str,
}

/// Cấu hình project `xiangqi-sync`.
pub const XIANGQI_SYNC_CONFIG: FirebaseConfig = FirebaseConfig {
    api_key: "AIzaSyApLltjDd2MK--J2WBsCGomshJA5wqGKoM",
    project_id: "xiangqi-sync",
};

/// OAuth client (loại Desktop) cho đăng nhập Google trên desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthCredentials {
    /// OAuth client ID (loại Desktop).
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
}

impl OAuthCredentials {
    /// Nạp client ID + secret: biến môi trường trước, rồi file JSON.
    ///
    /// `file` là đường dẫn `sync-oauth.json` (thường cạnh
    /// `settings.json`); `None` thì chỉ thử biến môi trường. Cả hai
    /// đều thiếu thì trả lỗi hướng dẫn người dùng cách tạo.
    pub fn load(file: Option<&Path>) -> Result<Self, String> {
        if let Some(creds) = Self::from_env() {
            return Ok(creds);
        }
        if let Some(path) = file {
            let text = std::fs::read_to_string(path).map_err(|_| {
                format!(
                    "chua co OAuth client: dat bien moi truong GOOGLE_CLIENT_ID + GOOGLE_CLIENT_SECRET, hoac tao file {} voi {{\"client_id\": \"...\", \"client_secret\": \"...\"}} roi khoi dong lai app",
                    path.display()
                )
            })?;
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| format!("file {} khong phai JSON hop le: {e}", path.display()))?;
            let client_id = value["client_id"].as_str().unwrap_or("").to_string();
            let client_secret = value["client_secret"].as_str().unwrap_or("").to_string();
            if client_id.is_empty() || client_secret.is_empty() {
                return Err(format!(
                    "file {} thieu client_id hoac client_secret",
                    path.display()
                ));
            }
            return Ok(Self {
                client_id,
                client_secret,
            });
        }
        Err(
            "chua co OAuth client: dat bien moi truong GOOGLE_CLIENT_ID + GOOGLE_CLIENT_SECRET, hoac tao file sync-oauth.json canh settings.json roi khoi dong lai app"
                .to_string(),
        )
    }

    /// Nạp từ biến môi trường (giống spike Phase 0). Cả hai đều phải
    /// có và không rỗng; thiếu một trong hai thì coi như chưa có.
    fn from_env() -> Option<Self> {
        let client_id = std::env::var("GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok()?;
        if client_id.is_empty() || client_secret.is_empty() {
            return None;
        }
        Some(Self {
            client_id,
            client_secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_oauth_json(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("sync-oauth.json");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn load_reads_json_file() {
        let dir = std::env::temp_dir().join("xq-sync-oauth-test-ok");
        let _ = std::fs::create_dir_all(&dir);
        let path = write_oauth_json(
            &dir,
            r#"{"client_id": "id-123", "client_secret": "secret-456"}"#,
        );
        let creds = OAuthCredentials::load(Some(&path)).unwrap();
        assert_eq!(creds.client_id, "id-123");
        assert_eq!(creds.client_secret, "secret-456");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_json_missing_a_field() {
        let dir = std::env::temp_dir().join("xq-sync-oauth-test-bad");
        let _ = std::fs::create_dir_all(&dir);
        let path = write_oauth_json(&dir, r#"{"client_id": "id-123"}"#);
        let err = OAuthCredentials::load(Some(&path)).unwrap_err();
        assert!(err.contains("thieu client_id hoac client_secret"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_without_env_or_file_explains_how_to_create_it() {
        // Đảm bảo không có biến môi trường (test khác không đụng 2 tên này).
        std::env::remove_var("GOOGLE_CLIENT_ID");
        std::env::remove_var("GOOGLE_CLIENT_SECRET");
        let err = OAuthCredentials::load(None).unwrap_err();
        assert!(err.contains("GOOGLE_CLIENT_ID"), "{err}");
        assert!(err.contains("sync-oauth.json"), "{err}");
    }
}
