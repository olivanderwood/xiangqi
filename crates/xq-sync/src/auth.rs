//! Đăng nhập: Google OAuth loopback (desktop) → Firebase Auth REST.
//!
//! Luồng desktop (plan §5.2, đã kiểm chứng bằng spike
//! `tools/sync-spike/` trên máy Windows của Hieu, 2026-11-27):
//! 1. `google_sign_in` mở trình duyệt tới accounts.google.com, Google
//!    trả `code` về `http://127.0.0.1:<port>/` (TcpListener cục bộ).
//! 2. Đổi `code` lấy Google `id_token` tại oauth2.googleapis.com/token.
//! 3. `firebase_sign_in_with_google` đổi Google `id_token` lấy Firebase
//!    `id_token` + `refresh_token` + `uid` (signInWithIdp).
//! 4. `firebase_refresh` đổi `refresh_token` lấy `id_token` mới khi hết
//!    hạn (id_token Firebase sống ~1 giờ).
//!
//! Trên wasm (PWA sync, 2026-11-27) không có socket/loopback: đăng
//! nhập đi qua Firebase JS SDK (`signInWithPopup`, xem
//! `crates/xq-iced/src/firebase_auth.rs`), SDK tự giữ phiên và tự
//! refresh id_token — các hàm REST dưới đây chỉ dùng trên desktop.
//!
//! Token lưu trong keychain qua `keyring` (phía xq-iced), KHÔNG vào
//! settings.json. Crate này chỉ trả token về cho caller.

#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::TcpListener;

#[cfg(not(target_arch = "wasm32"))]
use serde_json::Value;

#[cfg(not(target_arch = "wasm32"))]
use crate::config::{FirebaseConfig, OAuthCredentials};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::{firebase_message, SyncError};

/// Token sau khi đăng nhập Firebase.
#[derive(Clone, Debug)]
pub struct Tokens {
    /// Firebase ID token (dùng cho mọi call Firestore, sống ~1 giờ).
    pub id_token: String,
    /// Refresh token (đổi lấy id_token mới, sống lâu — lưu keychain).
    pub refresh_token: String,
    /// Firebase uid của người dùng.
    pub uid: String,
    /// Email Google (hiển thị trong Cài đặt).
    pub email: String,
}

/// Tạo ureq agent dùng chung: không coi HTTP error status là lỗi Rust
/// (để đọc được body lỗi JSON của Firebase), và mọi call chết sau 30
/// giây thay vì treo mãi (worker thread không được kẹt).
/// Desktop only (ureq không có trên wasm — transport ở đó là fetch).
#[cfg(not(target_arch = "wasm32"))]
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent()
}

/// Gọi POST JSON, trả về `(status, body)`.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
fn post_json(url: &str, body: &Value) -> Result<(u16, Value), SyncError> {
    let resp = agent()
        .post(url)
        .header("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| SyncError::Http(format!("POST {url} loi: {e}")))?;
    let status = resp.status().as_u16();
    let parsed: Value = resp
        .into_body()
        .read_json()
        .map_err(|e| SyncError::Parse(format!("doc JSON loi tu {url}: {e}")))?;
    Ok((status, parsed))
}

/// Bước 1–2: đăng nhập Google qua trình duyệt, trả về Google id_token.
/// Mở trình duyệt hệ thống; người dùng đăng nhập rồi Google redirect về
/// `http://127.0.0.1:<port>/` mà spike đang listen. Chờ tối đa 5 phút
/// rồi báo lỗi (trình duyệt đóng mà không đăng nhập thì worker không
/// bị kẹt mãi ở `accept`).
/// Desktop only (loopback không có trên wasm — PWA dùng JS SDK).
#[cfg(not(target_arch = "wasm32"))]
pub fn google_sign_in(oauth: &OAuthCredentials) -> Result<String, SyncError> {
    google_sign_in_with_timeout(oauth, std::time::Duration::from_secs(300))
}

/// Như [`google_sign_in`] nhưng với deadline riêng (test dùng deadline
/// ngắn để kiểm tra đường timeout mà không chờ 5 phút).
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub fn google_sign_in_with_timeout(
    oauth: &OAuthCredentials,
    timeout: std::time::Duration,
) -> Result<String, SyncError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| SyncError::Io(format!("khong mo duoc cong loopback: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| SyncError::Io(format!("khong dat nonblocking cho loopback: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| SyncError::Io(format!("khong lay duoc cong loopback: {e}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/");

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&access_type=offline&prompt=consent",
        url::form_urlencoded::byte_serialize(oauth.client_id.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(redirect_uri.as_bytes()).collect::<String>(),
    );
    open_browser(&auth_url)?;

    // Chờ 1 request duy nhất từ Google (redirect) rồi đóng listener.
    let code = wait_for_code(&listener, timeout)?;

    // Đổi code lấy Google id_token.
    let token_url = "https://oauth2.googleapis.com/token";
    let body = serde_json::json!({
        "code": code,
        "client_id": oauth.client_id,
        "client_secret": oauth.client_secret,
        "redirect_uri": redirect_uri,
        "grant_type": "authorization_code",
    });
    let (status, parsed) = post_json(token_url, &body)?;
    if status != 200 {
        return Err(SyncError::Auth(format!(
            "doi code Google loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    parsed["id_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| SyncError::Auth("Google khong tra ve id_token".to_string()))
}

/// Chờ redirect của Google trên listener đã mở: đọc request đầu
/// tiên, trả về `code` (hoặc lỗi khi Google từ chối / hết deadline).
/// Non-blocking + ngủ ngắn để còn kịp hết deadline khi người dùng
/// đóng trình duyệt mà không đăng nhập.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
fn wait_for_code(
    listener: &TcpListener,
    timeout: std::time::Duration,
) -> Result<String, SyncError> {
    let deadline = std::time::Instant::now() + timeout;
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(SyncError::Auth(
                        "het thoi gian cho dang nhap Google (5 phut)".to_string(),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                return Err(SyncError::Io(format!(
                    "khong nhan duoc redirect tu Google: {e}"
                )));
            }
        }
    };
    let mut buf = [0u8; 8192];
    let n = stream
        .read(&mut buf)
        .map_err(|e| SyncError::Io(format!("doc redirect tu Google loi: {e}")))?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    // Trang trả về cho người dùng (không cần đẹp, chỉ để họ biết xong).
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>Dang nhap xong, co the dong tab nay.</h1></body></html>");
    let _ = stream.flush();

    let path = first_line.split_whitespace().nth(1).unwrap_or("");
    let query = path.split('?').nth(1).unwrap_or("");
    // `form_urlencoded::parse` giải mã đúng cả ký tự đặc biệt trong code.
    let mut code: Option<String> = None;
    let mut err: Option<String> = None;
    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        if k == "code" {
            code = Some(v.into_owned());
        } else if k == "error" {
            err = Some(v.into_owned());
        }
    }
    if let Some(e) = err {
        return Err(SyncError::Auth(format!("Google tu choi dang nhap: {e}")));
    }
    code.ok_or_else(|| SyncError::Auth("Google khong tra ve code".to_string()))
}

/// Bước 3: đổi Google id_token lấy Firebase token.
/// Desktop only (PWA lấy token Firebase trực tiếp từ JS SDK).
#[cfg(not(target_arch = "wasm32"))]
pub fn firebase_sign_in_with_google(
    cfg: &FirebaseConfig,
    google_id_token: &str,
) -> Result<Tokens, SyncError> {
    let url = format!(
        "https://identitytoolkit.googleapis.com/v1/accounts:signInWithIdp?key={}",
        cfg.api_key
    );
    let body = serde_json::json!({
        "postBody": format!("id_token={google_id_token}&providerId=google.com"),
        "requestUri": "http://localhost",
        "returnSecureToken": true,
    });
    let (status, parsed) = post_json(&url, &body)?;
    if status != 200 {
        return Err(SyncError::Auth(format!(
            "Firebase signInWithIdp loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    Ok(Tokens {
        id_token: parsed["idToken"].as_str().unwrap_or("").to_string(),
        refresh_token: parsed["refreshToken"].as_str().unwrap_or("").to_string(),
        uid: parsed["localId"].as_str().unwrap_or("").to_string(),
        email: parsed["email"].as_str().unwrap_or("").to_string(),
    })
}

/// Bước 4: đổi refresh_token lấy Firebase id_token mới.
/// Desktop only (PWA: JS SDK tự refresh, Rust gọi `getIdToken()`).
#[cfg(not(target_arch = "wasm32"))]
pub fn firebase_refresh(cfg: &FirebaseConfig, refresh_token: &str) -> Result<String, SyncError> {
    let url = format!(
        "https://securetoken.googleapis.com/v1/token?key={}",
        cfg.api_key
    );
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let (status, parsed) = post_json(&url, &body)?;
    if status != 200 {
        return Err(SyncError::Auth(format!(
            "refresh token loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    parsed["id_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| SyncError::Auth("Firebase khong tra ve id_token moi".to_string()))
}

/// Mở URL bằng trình duyệt mặc định của hệ thống.
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
fn open_browser(url: &str) -> Result<(), SyncError> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    result.map(|_| ()).map_err(|e| {
        SyncError::Io(format!(
            "khong mo duoc trinh duyet ({e}); mo tay URL dang nhap Google"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_for_code_times_out_when_nobody_connects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let t0 = std::time::Instant::now();
        let r = wait_for_code(&listener, std::time::Duration::from_millis(150));
        assert!(matches!(r, Err(SyncError::Auth(_))));
        assert!(t0.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn wait_for_code_returns_code_from_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            let _ = stream.write_all(b"GET /?code=abc%2F123&scope=x HTTP/1.1\r\nHost: x\r\n\r\n");
            // Đọc trang "đăng nhập xong" mà wait_for_code trả về.
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
        });
        let code = wait_for_code(&listener, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(code, "abc/123");
    }
}
