//! Spike Phase 0 cho sync mirroring (plan `research_notes/sync-plan-20261125/plan.md`).
//!
//! Chay DUY NHAT tren may co trinh duyet (Windows/Mac cua Hieu) — can mo
//! trinh duyet de dang nhap Google. VM Linux khong co trinh duyet nen chi
//! build duoc, khong chay het flow.
//!
//! Luong chay:
//!   1. Google OAuth loopback (Desktop app): mo trinh duyet, bat `code` o
//!      http://127.0.0.1:<port>, doi `code` lay Google `id_token`.
//!   2. Firebase Auth REST `accounts:signInWithIdp`: doi Google `id_token`
//!      lay Firebase `idToken` + `refreshToken` + `localId` (uid).
//!   3. Firestore REST: PATCH 1 document van co tai
//!      `users/{uid}/games/{uuid}` (upsert), GET lai de kiem tra roundtrip.
//!   4. Firebase Auth REST `securetoken.googleapis.com/v1/token`: doi
//!      `refreshToken` lay `idToken` moi.
//!
//! Cau hinh qua bien moi truong (khong hardcode secret vao code):
//!   FIREBASE_API_KEY        Web API key (Firebase console > Project settings)
//!   FIREBASE_PROJECT_ID     Project ID Firebase
//!   GOOGLE_CLIENT_ID        OAuth client ID loai "Desktop app"
//!   GOOGLE_CLIENT_SECRET    Client secret cua OAuth client do
//!
//! Xem README.md cung thu muc de biet cac buoc tao project Firebase.

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;

use serde_json::{json, Value};
use url::Url;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const FIREBASE_SIGN_IN_WITH_IDP: &str =
    "https://identitytoolkit.googleapis.com/v1/accounts:signInWithIdp";
const FIREBASE_REFRESH_TOKEN: &str = "https://securetoken.googleapis.com/v1/token";

struct Config {
    api_key: String,
    project_id: String,
    google_client_id: String,
    google_client_secret: String,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let get =
            |k: &str| env::var(k).map_err(|_| format!("thieu bien moi truong {k} (xem README.md)"));
        Ok(Self {
            api_key: get("FIREBASE_API_KEY")?,
            project_id: get("FIREBASE_PROJECT_ID")?,
            google_client_id: get("GOOGLE_CLIENT_ID")?,
            google_client_secret: get("GOOGLE_CLIENT_SECRET")?,
        })
    }
}

/// HTTP client: ureq 3 + rustls (khong OpenSSL), khong coi status loi
/// la Err de doc duoc body loi JSON cua Firebase.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// POST JSON, tra ve (status, body). Loi HTTP van tra body de in ra.
fn post_json(agent: &ureq::Agent, url: &str, body: Value) -> Result<(u16, Value), String> {
    let mut resp = agent
        .post(url)
        .send_json(body)
        .map_err(|e| format!("POST {url} that bai: {e}"))?;
    let status = resp.status().as_u16();
    let v: Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("doc body JSON tu {url} that bai: {e}"))?;
    Ok((status, v))
}

fn get_json(agent: &ureq::Agent, url: &str, id_token: &str) -> Result<(u16, Value), String> {
    let mut resp = agent
        .get(url)
        .header("Authorization", &format!("Bearer {id_token}"))
        .call()
        .map_err(|e| format!("GET {url} that bai: {e}"))?;
    let status = resp.status().as_u16();
    let v: Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("doc body JSON tu {url} that bai: {e}"))?;
    Ok((status, v))
}

fn patch_json(
    agent: &ureq::Agent,
    url: &str,
    id_token: &str,
    body: Value,
) -> Result<(u16, Value), String> {
    let mut resp = agent
        .patch(url)
        .header("Authorization", &format!("Bearer {id_token}"))
        .send_json(body)
        .map_err(|e| format!("PATCH {url} that bai: {e}"))?;
    let status = resp.status().as_u16();
    let v: Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("doc body JSON tu {url} that bai: {e}"))?;
    Ok((status, v))
}

/// Mo URL bang trinh duyet mac dinh (std only, khong them dep).
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let r = Command::new("cmd").args(["/C", "start", "", url]).status();
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(url).status();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = Command::new("xdg-open").arg(url).status();
    match r {
        Ok(_) => println!("Da mo trinh duyet. Neu khong tu mo, copy URL tren vao trinh duyet."),
        Err(e) => {
            println!("Khong mo duoc trinh duyet tu dong ({e}) — copy URL tren vao trinh duyet.")
        }
    }
}

/// Buoc 1: Google OAuth loopback — tra ve Google `id_token`.
fn google_sign_in(cfg: &Config) -> Result<String, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("khong bind duoc 127.0.0.1: {e}"))?;
    let port = listener.local_addr().unwrap().port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let auth_url = Url::parse_with_params(
        GOOGLE_AUTH_URL,
        [
            ("client_id", cfg.google_client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "openid email profile"),
            ("access_type", "offline"),
        ],
    )
    .unwrap();
    println!("\n[1/4] Dang nhap Google — mo URL nay trong trinh duyet:");
    println!("{auth_url}");
    open_browser(auth_url.as_str());
    println!("Dang doi ban dang nhap xong o trinh duyet...");

    // Chi nhan 1 request duy nhat (redirect tu Google ve).
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| format!("khong nhan duoc redirect: {e}"))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    // GET /?code=...&scope=... HTTP/1.1
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("request line khong co path")?;
    let full = format!("http://127.0.0.1{path}");
    let url = Url::parse(&full).map_err(|e| format!("parse redirect URL that bai: {e}"))?;
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| {
            let err = url
                .query_pairs()
                .find(|(k, _)| k == "error")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            format!("Google tra ve loi thay vi code: {err}")
        })?;
    let html = "Dang nhap xong — co the dong tab nay va quay lai cua so dong lenh.";
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        html.len(),
        html
    );
    println!("Da nhan duoc code tu Google, doi code lay id_token...");

    let agent = agent();
    let (status, body) = post_json(
        &agent,
        GOOGLE_TOKEN_URL,
        json!({
            "code": code,
            "client_id": cfg.google_client_id,
            "client_secret": cfg.google_client_secret,
            "redirect_uri": redirect_uri,
            "grant_type": "authorization_code",
        }),
    )?;
    if status != 200 {
        return Err(format!(
            "doi code that bai (HTTP {status}): {}",
            body["error_description"].as_str().unwrap_or("?")
        ));
    }
    body["id_token"]
        .as_str()
        .map(str::to_string)
        .ok_or("Google khong tra id_token".to_string())
}

/// Buoc 2: doi Google id_token lay Firebase idToken + refreshToken.
fn firebase_sign_in_with_google(
    cfg: &Config,
    google_id_token: &str,
) -> Result<(String, String, String, String), String> {
    let agent = agent();
    let url = format!("{FIREBASE_SIGN_IN_WITH_IDP}?key={}", cfg.api_key);
    let (status, body) = post_json(
        &agent,
        &url,
        json!({
            "requestUri": "http://localhost",
            "postBody": format!("id_token={google_id_token}&providerId=google.com"),
            "returnSecureToken": true,
        }),
    )?;
    if status != 200 {
        return Err(format!(
            "signInWithIdp that bai (HTTP {status}): {}",
            body["error"]["message"].as_str().unwrap_or("?")
        ));
    }
    let id_token = body["idToken"].as_str().unwrap_or("").to_string();
    let refresh = body["refreshToken"].as_str().unwrap_or("").to_string();
    let uid = body["localId"].as_str().unwrap_or("").to_string();
    let email = body["email"].as_str().unwrap_or("").to_string();
    if id_token.is_empty() || uid.is_empty() {
        return Err(format!("Firebase tra ve thieu idToken/uid: {body}"));
    }
    Ok((id_token, refresh, uid, email))
}

/// Buoc 4: doi Firebase refreshToken lay idToken moi.
fn firebase_refresh(cfg: &Config, refresh_token: &str) -> Result<String, String> {
    let agent = agent();
    let url = format!("{FIREBASE_REFRESH_TOKEN}?key={}", cfg.api_key);
    let (status, body) = post_json(
        &agent,
        &url,
        json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }),
    )?;
    if status != 200 {
        return Err(format!(
            "refresh token that bai (HTTP {status}): {}",
            body["error"]["message"].as_str().unwrap_or("?")
        ));
    }
    body["id_token"]
        .as_str()
        .map(str::to_string)
        .ok_or("refresh khong tra id_token".to_string())
}

/// Firestore typed value helpers (`integerValue` la string trong JSON).
fn s(v: &str) -> Value {
    json!({"stringValue": v})
}
fn n(v: i64) -> Value {
    json!({"integerValue": v.to_string()})
}
fn b(v: bool) -> Value {
    json!({"booleanValue": v})
}
fn arr(vs: &[String]) -> Value {
    json!({"arrayValue": {"values": vs.iter().map(|x| s(x)).collect::<Vec<_>>()}})
}

fn run() -> Result<(), String> {
    let cfg = Config::from_env()?;
    let agent = agent();

    // Buoc 1: Google OAuth.
    let google_id_token = google_sign_in(&cfg)?;
    println!("[1/4] OK — da co Google id_token.");

    // Buoc 2: Firebase Auth.
    let (mut id_token, refresh_token, uid, email) =
        firebase_sign_in_with_google(&cfg, &google_id_token)?;
    println!("[2/4] OK — Firebase uid={uid} email={email}");

    // Buoc 3: Firestore — ghi 1 van co mau roi doc lai.
    let game_uuid = uuid::Uuid::new_v4().to_string();
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let doc_url = format!(
        "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/users/{}/games/{}",
        cfg.project_id, uid, game_uuid
    );
    let moves = vec!["h2e2".to_string(), "h7e7".to_string()];
    let fields = json!({
        "fields": {
            "uuid": s(&game_uuid),
            "label": s("Spike — Đấu với AI — 2 nước"),
            "start_fen": s("rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1"),
            "moves": arr(&moves),
            "difficulty": n(2),
            "human_red": b(true),
            "finished": b(false),
            "version": n(1),
            "updated_unix": n(now_unix),
            "updated_by_device": s("sync-spike"),
            "deleted": b(false),
        }
    });
    let (status, body) = patch_json(&agent, &doc_url, &id_token, fields)?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "ghi Firestore that bai (HTTP {status}): {}",
            body["error"]["message"].as_str().unwrap_or("?")
        ));
    }
    println!("[3/4] OK — da ghi document users/{uid}/games/{game_uuid}");

    let (status, body) = get_json(&agent, &doc_url, &id_token)?;
    if status != 200 {
        return Err(format!(
            "doc Firestore that bai (HTTP {status}): {}",
            body["error"]["message"].as_str().unwrap_or("?")
        ));
    }
    let got_moves = body["fields"]["moves"]["arrayValue"]["values"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v["stringValue"].as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if got_moves != moves {
        return Err(format!(
            "roundtrip SAI: ghi {moves:?}, doc ve {got_moves:?}"
        ));
    }
    println!("[3/4] OK — doc lai dung, roundtrip khop (moves = {got_moves:?}).");

    // Buoc 4: refresh token.
    id_token = firebase_refresh(&cfg, &refresh_token)?;
    let (status, _) = get_json(&agent, &doc_url, &id_token)?;
    if status != 200 {
        return Err(format!("doc lai bang token moi that bai (HTTP {status})"));
    }
    println!("[4/4] OK — refresh token doi duoc idToken moi, doc lai van duoc.");

    println!("\nSPIKE QUA: dang nhap Google + Firebase Auth + Firestore ghi/doc + refresh token deu chay.");
    println!("uid={uid}  doc=users/{uid}/games/{game_uuid}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("\nSPIKE THAT BAI: {e}");
        std::process::exit(1);
    }
}
