//! Bridge tới Firebase Auth JS SDK trên PWA (wasm only).
//!
//! Glue JS ở `assets/pwa/firebase-auth.js` expose
//! `window.__xiangqiAuth` (SDK v11.10.0 từ CDN gstatic). Mọi hàm glue
//! trả Promise<string> là JSON `{ok, ...}` và không throw qua bridge
//! — nhưng nếu glue chưa kịp load (hoặc SDK không tải được) thì gọi
//! hàm sẽ throw, nên mọi extern đều có `catch` và Rust dịch lỗi
//! thành câu báo cho người dùng.
//!
//! Cố tình KHÔNG dùng `async fn` trong extern: với `catch` +
//! `async fn`, khi hàm JS throw đồng bộ (glue chưa có) thì wrapper
//! trả `undefined` thay vì Err, rồi machinery await gọi `.then` lên
//! `undefined` và ném lỗi không bắt được — app trắng màn hình lúc
//! mở (bắt được bằng headless Chromium 2026-11-27, khi gstatic bị
//! chặn). Extern đồng bộ trả `js_sys::Promise` + `JsFuture` thì lỗi
//! đồng bộ về đúng đường Err, app vẫn chạy, sync chỉ ở trạng thái
//! Off.
//!
//! SDK tự giữ phiên trong IndexedDB và tự refresh id_token, nên
//! Rust chỉ cần: chờ phiên settle lúc mở app, lấy id_token tươi
//! trước mỗi đợt gọi Firestore, đăng nhập (popup → redirect fallback
//! trong glue) và đăng xuất. Không có refresh token như desktop.

use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// User Firebase tối thiểu mà UI cần (uid làm key Firestore, email
/// hiển thị trong Cài đặt).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirebaseUser {
    pub uid: String,
    pub email: String,
}

/// Kết quả đăng nhập: xong ngay (popup) hoặc trang đang chuyển hướng
/// (redirect fallback — phiên sẽ khôi phục khi tải lại).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInOutcome {
    SignedIn {
        uid: String,
        email: String,
        id_token: String,
    },
    Redirecting,
}

#[derive(Deserialize)]
struct GlueReply {
    ok: bool,
    error: Option<String>,
    user: Option<GlueUser>,
    #[serde(rename = "idToken")]
    id_token: Option<String>,
    uid: Option<String>,
    email: Option<String>,
    redirecting: Option<bool>,
}

#[derive(Deserialize)]
struct GlueUser {
    uid: String,
    email: String,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        catch,
        js_namespace = ["window", "__xiangqiAuth"],
        js_name = waitForAuth
    )]
    fn wait_for_auth_js() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(
        catch,
        js_namespace = ["window", "__xiangqiAuth"],
        js_name = getIdToken
    )]
    fn get_id_token_js() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(
        catch,
        js_namespace = ["window", "__xiangqiAuth"],
        js_name = signIn
    )]
    fn sign_in_js() -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(
        catch,
        js_namespace = ["window", "__xiangqiAuth"],
        js_name = signOut
    )]
    fn sign_out_js() -> Result<js_sys::Promise, JsValue>;
}

/// Gọi một hàm glue và chờ Promise của nó resolve thành chuỗi JSON.
/// Lỗi đồng bộ (glue chưa có) về Err ngay; Promise reject cũng về
/// Err; resolve ra giá trị không phải chuỗi thì báo lỗi đọc.
async fn call_glue(promise: Result<js_sys::Promise, JsValue>) -> Result<String, String> {
    let value = JsFuture::from(promise.map_err(js_error)?)
        .await
        .map_err(js_error)?;
    value
        .as_string()
        .ok_or_else(|| "tra loi Firebase Auth khong phai chuoi".to_string())
}

fn js_error(err: JsValue) -> String {
    // Glue chưa load (hoặc SDK lỗi): JsValue thường là TypeError;
    // `format!("{err:?}")` đủ để ghi log, còn câu báo cho người
    // dùng thì viết rõ.
    let _ = err;
    "Firebase Auth chua san sang — kiem tra mang roi thu lai".to_string()
}

fn parse_reply(json: &str) -> Result<GlueReply, String> {
    serde_json::from_str::<GlueReply>(json)
        .map_err(|e| format!("tra loi Firebase Auth khong doc duoc: {e}"))
}

fn check_reply(reply: GlueReply) -> Result<GlueReply, String> {
    if reply.ok {
        Ok(reply)
    } else {
        Err(reply.error.unwrap_or_else(|| "loi khong ro".to_string()))
    }
}

/// Chờ SDK khôi phục phiên xong rồi trả user hiện tại (`None` nếu
/// chưa đăng nhập).
pub async fn wait_for_auth() -> Result<Option<FirebaseUser>, String> {
    let json = call_glue(wait_for_auth_js()).await?;
    let reply = check_reply(parse_reply(&json)?)?;
    Ok(reply.user.map(|u| FirebaseUser {
        uid: u.uid,
        email: u.email,
    }))
}

/// Id token còn hạn (SDK tự refresh nếu hết — không gọi mạng khi
/// token vẫn còn hạn). Lỗi khi chưa đăng nhập.
pub async fn get_id_token() -> Result<String, String> {
    let json = call_glue(get_id_token_js()).await?;
    let reply = check_reply(parse_reply(&json)?)?;
    reply
        .id_token
        .ok_or_else(|| "Firebase khong tra ve id_token".to_string())
}

/// Đăng nhập Google (popup, rớt thì redirect — xem glue JS).
pub async fn sign_in() -> Result<SignInOutcome, String> {
    let json = call_glue(sign_in_js()).await?;
    let reply = check_reply(parse_reply(&json)?)?;
    if reply.redirecting.unwrap_or(false) {
        return Ok(SignInOutcome::Redirecting);
    }
    let uid = reply.uid.unwrap_or_default();
    let id_token = reply.id_token.unwrap_or_default();
    if uid.is_empty() || id_token.is_empty() {
        return Err("Firebase khong tra ve du token dang nhap".to_string());
    }
    Ok(SignInOutcome::SignedIn {
        uid,
        email: reply.email.unwrap_or_default(),
        id_token,
    })
}

/// Đăng xuất khỏi Firebase (xoá phiên trong IndexedDB).
pub async fn sign_out() -> Result<(), String> {
    let json = call_glue(sign_out_js()).await?;
    check_reply(parse_reply(&json)?)?;
    Ok(())
}
