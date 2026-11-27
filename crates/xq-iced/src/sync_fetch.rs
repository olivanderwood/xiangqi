//! Firestore qua REST trên PWA (wasm only) — bản `fetch` của
//! `xq-sync/src/firestore.rs`.
//!
//! Cùng URL, cùng body typed values, cùng cách xử lý status (404 →
//! `None` / `[]`, lỗi khác → `SyncError::Http` với message Firebase)
//! như bản desktop — chỉ transport khác: `fetch` qua gloo-net thay
//! vì ureq (không có socket trên wasm). CORS đã kiểm chứng: cả 3
//! endpoint Google đều trả `Access-Control-Allow-Origin` cho origin
//! của PWA, nên không cần proxy.
//!
//! Id token lấy từ Firebase JS SDK (`firebase_auth::get_id_token`,
//! SDK tự refresh) — không có refresh token tay như desktop.

use gloo_net::http::Request;
use serde_json::{json, Value};

use xq_sync::error::firebase_message;
use xq_sync::firestore::{collection_url, doc_url, entry_to_fields, parse_entry};
use xq_sync::{SyncEntry, SyncError, XIANGQI_SYNC_CONFIG};

fn auth_header(id_token: &str) -> String {
    format!("Bearer {id_token}")
}

/// Gọi API rồi parse body JSON. Trả `(status, body)` — body rỗng
/// (vd 404 không có JSON) thì về `Value::Null`.
async fn call(req: gloo_net::http::Request) -> Result<(u16, Value), SyncError> {
    let resp = req
        .send()
        .await
        .map_err(|e| SyncError::Http(format!("fetch loi: {e}")))?;
    let status = resp.status();
    let parsed: Value = resp.json().await.unwrap_or(Value::Null);
    Ok((status, parsed))
}

/// Build một GET request (gloo-net tách builder và request đã build).
fn get_req(url: &str, id_token: &str) -> Result<gloo_net::http::Request, SyncError> {
    Request::get(url)
        .header("Authorization", &auth_header(id_token))
        .build()
        .map_err(|e| SyncError::Http(format!("GET {url} loi: {e}")))
}

/// Đẩy (upsert) một ván lên cloud. PATCH không updateMask.
pub async fn fetch_upsert(id_token: &str, uid: &str, entry: &SyncEntry) -> Result<(), SyncError> {
    if !entry.is_valid() {
        return Err(SyncError::Parse(
            "entry khong hop le (thieu uuid/fen/version)".to_string(),
        ));
    }
    let url = doc_url(&XIANGQI_SYNC_CONFIG, uid, &entry.uuid);
    let body = json!({ "fields": entry_to_fields(entry) });
    let (status, parsed) = call(
        Request::patch(&url)
            .header("Authorization", &auth_header(id_token))
            .json(&body)
            .map_err(|e| SyncError::Http(format!("PATCH {url} loi: {e}")))?,
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(SyncError::Http(format!(
            "PATCH loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    Ok(())
}

/// Đọc một ván từ cloud. Trả `None` nếu document chưa có (404).
pub async fn fetch_get(
    id_token: &str,
    uid: &str,
    uuid: &str,
) -> Result<Option<SyncEntry>, SyncError> {
    let url = doc_url(&XIANGQI_SYNC_CONFIG, uid, uuid);
    let (status, parsed) = call(get_req(&url, id_token)?).await?;
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(SyncError::Http(format!(
            "GET loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    parse_entry(&parsed).map(Some)
}

/// Liệt kê mọi ván của người dùng (kể cả tombstone `deleted=true` để
/// máy kia biết mà xoá theo). Collection trống → về `[]`.
pub async fn fetch_list(id_token: &str, uid: &str) -> Result<Vec<SyncEntry>, SyncError> {
    let url = collection_url(&XIANGQI_SYNC_CONFIG, uid);
    let (status, parsed) = call(get_req(&url, id_token)?).await?;
    if status == 404 {
        // Collection chưa tồn tại = chưa có ván nào.
        return Ok(Vec::new());
    }
    if !(200..300).contains(&status) {
        return Err(SyncError::Http(format!(
            "GET loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    let mut out = Vec::new();
    if let Some(docs) = parsed["documents"].as_array() {
        for doc in docs {
            out.push(parse_entry(doc)?);
        }
    }
    Ok(out)
}
