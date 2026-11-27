//! Firestore qua REST: upsert / get / list document ván cờ.
//!
//! Endpoint: `https://firestore.googleapis.com/v1/projects/{project}
//! /databases/(default)/documents/users/{uid}/games/{uuid}`.
//! Auth: header `Authorization: Bearer <Firebase id_token>`.
//! Rule chỉ-owner ở Firestore chặn đọc/ghi ngoài `users/{uid}` của mình.
//!
//! Firestore REST dùng typed values: `stringValue`, `integerValue`
//! (dạng chuỗi!), `booleanValue`, `arrayValue`, `nullValue`. PATCH
//! không kèm updateMask = upsert toàn bộ document (đã kiểm chứng bằng
//! spike trên máy Windows, 2026-11-27).

use serde_json::{json, Value};

use crate::config::FirebaseConfig;
#[cfg(not(target_arch = "wasm32"))]
use crate::error::firebase_message;
use crate::error::SyncError;
use crate::model::SyncEntry;

/// Agent ureq dùng chung. Desktop only (ureq không có trên wasm —
/// transport ở đó là fetch, xem `xq-iced/src/sync_fetch.rs`).
#[cfg(not(target_arch = "wasm32"))]
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent()
}

/// URL của một document ván cờ. Pub vì worker wasm (fetch) dùng
/// chung — chỉ transport khác, URL và body giống hệt desktop.
#[must_use]
pub fn doc_url(cfg: &FirebaseConfig, uid: &str, uuid: &str) -> String {
    format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/users/{}/games/{}",
        cfg.project_id, uid, uuid
    )
}

/// URL của collection ván cờ. Pub (như [`doc_url`]).
#[must_use]
pub fn collection_url(cfg: &FirebaseConfig, uid: &str) -> String {
    format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/users/{}/games",
        cfg.project_id, uid
    )
}

/// Chuyển `SyncEntry` thành object `fields` của Firestore.
#[must_use]
pub fn entry_to_fields(e: &SyncEntry) -> Value {
    json!({
        "uuid": { "stringValue": e.uuid },
        "label": { "stringValue": e.label },
        "mode": { "stringValue": e.mode },
        "start_fen": { "stringValue": e.start_fen },
        "moves": { "arrayValue": { "values": e.moves.iter().map(|m| json!({ "stringValue": m })).collect::<Vec<_>>() } },
        "difficulty": { "integerValue": e.difficulty.to_string() },
        "human_red": match e.human_red {
            Some(v) => json!({ "booleanValue": v }),
            None => json!({ "nullValue": null }),
        },
        "finished": { "booleanValue": e.finished },
        "result": match &e.result {
            Some(r) => json!({ "stringValue": r }),
            None => json!({ "nullValue": null }),
        },
        "version": { "integerValue": e.version.to_string() },
        "updated_unix": { "integerValue": e.updated_unix.to_string() },
        "updated_by_device": { "stringValue": e.updated_by_device },
        "deleted": { "booleanValue": e.deleted },
    })
}

fn str_field(fields: &Value, name: &str) -> Result<String, SyncError> {
    fields[name]["stringValue"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| SyncError::Parse(format!("thieu field string '{name}'")))
}

fn int_field(fields: &Value, name: &str) -> Result<u64, SyncError> {
    let raw = fields[name]["integerValue"]
        .as_str()
        .ok_or_else(|| SyncError::Parse(format!("thieu field integer '{name}'")))?;
    raw.parse::<u64>()
        .map_err(|_| SyncError::Parse(format!("field '{name}' khong phai so: {raw}")))
}

fn bool_field(fields: &Value, name: &str) -> Result<bool, SyncError> {
    fields[name]["booleanValue"]
        .as_bool()
        .ok_or_else(|| SyncError::Parse(format!("thieu field boolean '{name}'")))
}

/// Chuyển document Firestore (object có `name` + `fields`) thành entry.
pub fn parse_entry(doc: &Value) -> Result<SyncEntry, SyncError> {
    let fields = &doc["fields"];
    let uuid = str_field(fields, "uuid")?;
    let moves = fields["moves"]["arrayValue"]["values"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["stringValue"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let human_red = fields["human_red"]["booleanValue"].as_bool();
    let result = fields["result"]["stringValue"].as_str().map(str::to_string);
    Ok(SyncEntry {
        uuid,
        label: str_field(fields, "label")?,
        mode: str_field(fields, "mode")?,
        start_fen: str_field(fields, "start_fen")?,
        moves,
        difficulty: int_field(fields, "difficulty")? as u8,
        human_red,
        finished: bool_field(fields, "finished")?,
        result,
        version: int_field(fields, "version")?,
        updated_unix: int_field(fields, "updated_unix")?,
        updated_by_device: str_field(fields, "updated_by_device")?,
        deleted: bool_field(fields, "deleted")?,
    })
}

/// Đẩy (upsert) một ván lên cloud. PATCH không updateMask.
/// Desktop only (bản wasm ở `xq-iced/src/sync_fetch.rs`).
#[cfg(not(target_arch = "wasm32"))]
pub fn upsert_game(
    cfg: &FirebaseConfig,
    id_token: &str,
    uid: &str,
    entry: &SyncEntry,
) -> Result<(), SyncError> {
    if !entry.is_valid() {
        return Err(SyncError::Parse(
            "entry khong hop le (thieu uuid/fen/version)".to_string(),
        ));
    }
    let url = doc_url(cfg, uid, &entry.uuid);
    let body = json!({ "fields": entry_to_fields(entry) });
    let resp = agent()
        .patch(&url)
        .header("Authorization", &format!("Bearer {id_token}"))
        .send_json(&body)
        .map_err(|e| SyncError::Http(format!("PATCH {url} loi: {e}")))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let parsed: Value = resp.into_body().read_json().unwrap_or(Value::Null);
        return Err(SyncError::Http(format!(
            "PATCH loi ({}): {}",
            status,
            firebase_message(&parsed)
        )));
    }
    Ok(())
}

/// Đọc một ván từ cloud. Trả `None` nếu document chưa có (404).
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub fn get_game(
    cfg: &FirebaseConfig,
    id_token: &str,
    uid: &str,
    uuid: &str,
) -> Result<Option<SyncEntry>, SyncError> {
    let url = doc_url(cfg, uid, uuid);
    let resp = agent()
        .get(&url)
        .header("Authorization", &format!("Bearer {id_token}"))
        .call()
        .map_err(|e| SyncError::Http(format!("GET {url} loi: {e}")))?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    let parsed: Value = resp
        .into_body()
        .read_json()
        .map_err(|e| SyncError::Parse(format!("doc JSON loi tu {url}: {e}")))?;
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
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub fn list_games(
    cfg: &FirebaseConfig,
    id_token: &str,
    uid: &str,
) -> Result<Vec<SyncEntry>, SyncError> {
    let url = collection_url(cfg, uid);
    let resp = agent()
        .get(&url)
        .header("Authorization", &format!("Bearer {id_token}"))
        .call()
        .map_err(|e| SyncError::Http(format!("GET {url} loi: {e}")))?;
    let status = resp.status().as_u16();
    let parsed: Value = resp
        .into_body()
        .read_json()
        .map_err(|e| SyncError::Parse(format!("doc JSON loi tu {url}: {e}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now_unix;

    fn sample() -> SyncEntry {
        let mut e = SyncEntry::new(
            "a0b2d496-7ddb-44e5-a4e2-be5a3f61aa62".to_string(),
            "Đấu với AI".to_string(),
            "play".to_string(),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1".to_string(),
            2,
            Some(true),
            "device-1".to_string(),
        );
        e.moves = vec!["h2e2".to_string(), "h9g7".to_string()];
        e.version = 3;
        e.updated_unix = now_unix();
        e
    }

    #[test]
    fn fields_roundtrip() {
        let e = sample();
        let doc = json!({ "fields": entry_to_fields(&e) });
        let back = parse_entry(&doc).expect("parse lai phai duoc");
        assert_eq!(back, e);
    }

    #[test]
    fn roundtrip_with_nulls_and_finished() {
        let mut e = sample();
        e.human_red = None;
        e.finished = true;
        e.result = Some("red".to_string());
        let doc = json!({ "fields": entry_to_fields(&e) });
        let back = parse_entry(&doc).expect("parse lai phai duoc");
        assert_eq!(back, e);
    }

    #[test]
    fn roundtrip_empty_moves() {
        let e = sample();
        let mut e2 = e.clone();
        e2.moves.clear();
        let doc = json!({ "fields": entry_to_fields(&e2) });
        let back = parse_entry(&doc).expect("parse lai phai duoc");
        assert_eq!(back, e2);
    }

    #[test]
    fn parse_missing_field_errors() {
        let doc = json!({ "fields": { "label": { "stringValue": "x" } } });
        assert!(parse_entry(&doc).is_err());
    }

    #[test]
    fn doc_url_shape() {
        let url = doc_url(&crate::config::XIANGQI_SYNC_CONFIG, "uid1", "uuid1");
        assert_eq!(
            url,
            "https://firestore.googleapis.com/v1/projects/xiangqi-sync/databases/(default)/documents/users/uid1/games/uuid1"
        );
    }
}
