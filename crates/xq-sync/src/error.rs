//! Lỗi của crate sync: một enum duy nhất, message đủ để UI hiện cho
//! người dùng mà không lộ token hay secret.

use std::fmt;

/// Lỗi sync. `message` không bao giờ chứa token, secret hay mật khẩu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    /// Lỗi HTTP (kèm status + message ngắn từ server, nếu có).
    Http(String),
    /// Đăng nhập / token không hợp lệ, hết hạn, hoặc bị từ chối.
    Auth(String),
    /// Body trả về không đúng dạng chờ đợi.
    Parse(String),
    /// Lỗi I/O cục bộ (mở trình duyệt, bind cổng loopback...).
    Io(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(m) | Self::Auth(m) | Self::Parse(m) | Self::Io(m) => {
                write!(f, "{m}")
            }
        }
    }
}

impl std::error::Error for SyncError {}

/// Lấy message lỗi ngắn từ body JSON của Firebase (`error.message`),
/// hoặc toàn bộ body nếu không có field đó.
#[must_use]
pub fn firebase_message(body: &serde_json::Value) -> String {
    body["error"]["message"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| body.to_string())
}
