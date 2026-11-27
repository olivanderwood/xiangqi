//! Sync mirroring Phase 1 (plan §5, `docs/DECISIONS-sync.md`).
//!
//! Một worker riêng (theo mẫu `ai_worker.rs` /
//! `chessdb_worker.rs`): UI gửi [`SyncRequest`] qua channel, worker
//! gọi Firebase qua [`xq_sync::SyncBackend`] (desktop) hoặc qua
//! fetch + Firebase JS SDK (PWA, xem `sync_wasm.rs`) và trả
//! [`SyncResponse`]. Mạng không bao giờ chặn UI — mất mạng thì ván
//! vẫn chơi được, nước đi đẩy lại sau (xem `PushFailed`).
//!
//! Luật mirroring (plan §5.4) sống ở 2 chỗ:
//! - Đẩy: worker pull trước rồi mới push; nếu bản remote thắng theo
//!   [`xq_sync::conflict::resolve_same_game`] thì trả
//!   [`SyncResponse::PushRejected`] để UI áp bản remote thay vì ghi
//!   đè (nước tới cloud trước thắng).
//! - Kéo: worker poll ván đang xem mỗi 5 giây; bản remote mới hơn
//!   (version cao hơn + nước khác) thì gửi
//!   [`SyncResponse::RemoteUpdate`] để UI áp nước kèm animation.
//!
//! Token: refresh token + email lưu trong keychain hệ thống qua
//! `keyring` (Windows Credential Manager / macOS Keychain / Linux
//! Secret Service), KHÔNG vào settings.json. ID token sống trong bộ
//! nhớ của worker, hết hạn (~1 giờ) thì worker tự refresh bằng
//! refresh token đã lưu.
//!
//! wasm32: không có thread nên worker là async task (`spawn_local`,
//! xem `sync_wasm.rs`); đăng nhập qua Firebase JS SDK
//! (`firebase_auth.rs`), SDK tự giữ phiên trong IndexedDB và tự
//! refresh id_token. Kênh request/response vẫn là `std::sync::mpsc`
//! như desktop — channel không cần thread.

use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(test)]
use std::sync::Mutex;
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
use std::time::{Duration, Instant};

use crate::saved_games::SavedGame;

use xq_sync::SyncEntry;
#[cfg(not(target_arch = "wasm32"))]
use xq_sync::{FirebaseBackend, SyncBackend, XIANGQI_SYNC_CONFIG};

/// Khoảng poll ván đang xem (mirroring qua REST không có listener
/// thật — plan §5.2 cho phép poll; 5 giây là đủ nhanh cho một ván
/// cờ mà không đốt quota đọc của gói Spark).
/// Dùng chung desktop + wasm (worker wasm ở `sync_wasm.rs`).
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Nhịp vòng lặp worker khi không có request (để poll đúng hẹn).
/// Dùng chung desktop + wasm.
pub(crate) const LOOP_TICK: Duration = Duration::from_millis(500);

/// Token sau khi đăng nhập (chỉ dùng trong bộ nhớ + keychain).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncTokens {
    /// Firebase ID token (dùng cho mọi call Firestore).
    pub id_token: String,
    /// Refresh token (đổi lấy id_token mới — lưu keychain).
    pub refresh_token: String,
    /// Firebase uid.
    pub uid: String,
    /// Email Google (hiển thị trong Cài đặt).
    pub email: String,
}

/// Yêu cầu gửi tới worker sync.
///
/// `large_enum_variant` được phép ở đây: `Push` mang cả một ván
/// (`SavedGame` ~280 byte) vì worker cần toàn bộ ván để đẩy — các
/// message chỉ sống ngắn (gửi qua channel rồi xử lý ngay), không ai
/// lưu trữ chúng.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum SyncRequest {
    /// Khôi phục phiên từ refresh token trong keychain (gửi lúc mở
    /// app). Thành công thì worker tự pull toàn bộ ván (`PulledAll`).
    RestoreSession {
        req_id: u64,
    },
    /// Đăng nhập Google (mở trình duyệt). Thành công thì worker lưu
    /// refresh token vào keychain rồi tự pull toàn bộ ván.
    SignIn {
        req_id: u64,
    },
    /// Đăng xuất: xoá keychain + token trong bộ nhớ.
    SignOut {
        req_id: u64,
    },
    /// Đẩy một ván lên cloud (pull-trước-push-sau, xem module docs).
    Push {
        game: SavedGame,
        req_id: u64,
    },
    /// Đọc một ván từ cloud.
    PullOne {
        uuid: String,
        req_id: u64,
    },
    /// Liệt kê mọi ván của người dùng.
    PullAll {
        req_id: u64,
    },
    /// Theo dõi một ván: worker poll nó mỗi [`POLL_INTERVAL`] và gửi
    /// `RemoteUpdate` khi bản remote mới hơn. `version` + `moves`
    /// là trạng thái local hiện tại (để worker biết remote mới hơn
    /// cái gì); `Unwatch` thì dừng theo dõi.
    Watch {
        uuid: String,
        version: u64,
        moves: Vec<String>,
    },
    Unwatch,
}

/// Trả lời từ worker sync. `req_id` khớp với request đã gửi;
/// `RemoteUpdate` là tin không ai hỏi (từ poll) nên không có req_id.
///
/// `large_enum_variant` được phép (như `SyncRequest`): các variant
/// mang ván (`SavedGame`) vì UI cần toàn bộ ván để áp.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum SyncResponse {
    SessionRestored {
        req_id: u64,
        tokens: SyncTokens,
    },
    SessionRestoreFailed {
        req_id: u64,
        reason: String,
    },
    SignedIn {
        req_id: u64,
        tokens: SyncTokens,
    },
    SignInFailed {
        req_id: u64,
        reason: String,
    },
    SignedOut {
        req_id: u64,
    },
    Pushed {
        req_id: u64,
        uuid: String,
        version: u64,
    },
    /// Bản remote thắng (nước tới cloud trước): UI phải áp `remote`
    /// thay cho nước local vừa đẩy hụt.
    PushRejected {
        req_id: u64,
        remote: SavedGame,
    },
    PushFailed {
        req_id: u64,
        reason: String,
    },
    PulledOne {
        req_id: u64,
        game: Option<SavedGame>,
    },
    PulledAll {
        req_id: u64,
        games: Vec<SavedGame>,
    },
    /// Bản remote của ván đang xem mới hơn (từ poll).
    RemoteUpdate {
        uuid: String,
        game: SavedGame,
    },
    SyncFailed {
        req_id: u64,
        reason: String,
    },
}

impl SyncResponse {
    /// `req_id` của response, nếu có (RemoteUpdate thì không).
    #[must_use]
    pub fn req_id(&self) -> Option<u64> {
        match self {
            Self::SessionRestored { req_id, .. }
            | Self::SessionRestoreFailed { req_id, .. }
            | Self::SignedIn { req_id, .. }
            | Self::SignInFailed { req_id, .. }
            | Self::SignedOut { req_id }
            | Self::Pushed { req_id, .. }
            | Self::PushRejected { req_id, .. }
            | Self::PushFailed { req_id, .. }
            | Self::PulledOne { req_id, .. }
            | Self::PulledAll { req_id, .. }
            | Self::SyncFailed { req_id, .. } => Some(*req_id),
            Self::RemoteUpdate { .. } => None,
        }
    }
}

/// Trạng thái đăng nhập sync (hiển thị trong Cài đặt).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncStatus {
    /// Chưa đăng nhập.
    Off,
    /// Đang chờ đăng nhập Google trong trình duyệt / đang khôi phục.
    SigningIn,
    /// Đã đăng nhập (email hiển thị).
    SignedIn { email: String },
    /// Lỗi lần cuối (mất mạng, token hết hạn không refresh được...).
    Error(String),
}

/// Đề nghị tiếp tục ván dở từ máy khác (mới hơn ván dở trên máy
/// này): Home hiện hộp thoại, người dùng chọn Tiếp tục / Để sau.
#[derive(Clone, Debug)]
pub struct ResumeOffer {
    pub game: SavedGame,
    /// Nhãn nguồn, vd "iPhone của Hieu" / email / device id ngắn.
    pub from_label: String,
}

/// Trạng thái sync sống trong `Shared`: worker + trạng thái hiển thị
/// + đề nghị tiếp tục ván (nếu có).
pub struct SyncState {
    pub worker: SyncWorker,
    pub status: SyncStatus,
    pub resume_offer: Option<ResumeOffer>,
}

impl SyncState {
    #[must_use]
    pub fn new(device_id: String) -> Self {
        Self {
            worker: SyncWorker::new(device_id),
            status: SyncStatus::Off,
            resume_offer: None,
        }
    }

    /// Đã đăng nhập chưa.
    #[must_use]
    pub fn signed_in(&self) -> bool {
        matches!(self.status, SyncStatus::SignedIn { .. })
    }

    /// Email đang đăng nhập (hiển thị trong Cài đặt).
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        match &self.status {
            SyncStatus::SignedIn { email } => Some(email.as_str()),
            _ => None,
        }
    }

    /// Lấy một response (không chặn) nếu worker đã trả lời.
    #[must_use]
    pub fn try_recv(&self) -> Option<SyncResponse> {
        self.worker.rx.try_recv().ok()
    }
}

/// Worker sync: kênh tới thread + `req_id` tăng dần (như `AiWorker`).
pub struct SyncWorker {
    pub tx: Sender<SyncRequest>,
    pub rx: Receiver<SyncResponse>,
    pub next_req_id: u64,
    /// ID máy này (`device-<uuid>` trong settings) — worker ghi vào
    /// `updated_by_device` mỗi lần đẩy.
    pub device_id: String,
}

impl SyncWorker {
    /// Tạo worker với backend Firebase thật. Desktop: spawn thread.
    /// wasm32: spawn async task (`sync_wasm::run` — không thread).
    #[must_use]
    pub fn new(device_id: String) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // OAuth client khong nam trong repo (GitHub push protection
            // chan): nap luc chay tu bien moi truong hoac file
            // sync-oauth.json canh settings.json. Thieu thi backend van
            // chay (khoi phuc phien bang refresh token khong can OAuth),
            // chi dang nhap Google moi bao loi huong dan cach tao.
            let oauth_path = crate::settings::settings_path().with_file_name("sync-oauth.json");
            let mut backend = FirebaseBackend::new(XIANGQI_SYNC_CONFIG);
            if let Ok(creds) = xq_sync::OAuthCredentials::load(Some(&oauth_path)) {
                backend = backend.with_oauth(creds);
            }
            Self::with_backend(device_id, Box::new(backend), Box::new(KeyringStore))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (req_tx, req_rx) = channel::<SyncRequest>();
            let (res_tx, res_rx) = channel::<SyncResponse>();
            wasm_bindgen_futures::spawn_local(crate::sync_wasm::run(
                req_rx,
                res_tx,
                device_id.clone(),
            ));
            Self {
                tx: req_tx,
                rx: res_rx,
                next_req_id: 1,
                device_id,
            }
        }
    }

    /// Tạo worker với backend + token store tuỳ chọn (test dùng
    /// `MockBackend` + `MemoryStore` để không chạm mạng/keychain).
    /// Desktop only: wasm không có backend.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn with_backend(
        device_id: String,
        backend: Box<dyn SyncBackend>,
        store: Box<dyn TokenStore>,
    ) -> Self {
        let (req_tx, req_rx) = channel::<SyncRequest>();
        let (res_tx, res_rx) = channel::<SyncResponse>();
        let device_id_for_thread = device_id.clone();
        std::thread::spawn(move || {
            worker_loop(req_rx, res_tx, backend, store, device_id_for_thread);
        });
        Self {
            tx: req_tx,
            rx: res_rx,
            next_req_id: 1,
            device_id,
        }
    }

    /// Cấp `req_id` mới.
    pub fn alloc_req_id(&mut self) -> u64 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }

    /// Gửi request (bỏ qua lỗi khi worker đã chết — app vẫn chạy).
    pub fn send(&self, req: SyncRequest) {
        let _ = self.tx.send(req);
    }
}

/// Nơi lưu refresh token + email (keychain thật hoặc bộ nhớ cho test).
/// `Send` vì worker thread sở hữu nó.
#[cfg(not(target_arch = "wasm32"))]
pub trait TokenStore: Send {
    fn load_session(&self) -> Option<(String, String)>;
    fn save_session(&self, refresh_token: &str, email: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

/// Lưu vào keychain hệ thống (service `xiangqi-sync`).
/// Desktop only.
#[cfg(not(target_arch = "wasm32"))]
pub struct KeyringStore;

#[cfg(not(target_arch = "wasm32"))]
impl TokenStore for KeyringStore {
    fn load_session(&self) -> Option<(String, String)> {
        let refresh = keyring::Entry::new("xiangqi-sync", "refresh-token")
            .ok()?
            .get_password()
            .ok()?;
        let email = keyring::Entry::new("xiangqi-sync", "email")
            .ok()?
            .get_password()
            .unwrap_or_default();
        Some((refresh, email))
    }

    fn save_session(&self, refresh_token: &str, email: &str) -> Result<(), String> {
        // `set_password` tạo mới nếu chưa có, ghi đè nếu đã có.
        keyring::Entry::new("xiangqi-sync", "refresh-token")
            .map_err(|e| format!("keychain: {e}"))?
            .set_password(refresh_token)
            .map_err(|e| format!("keychain: {e}"))?;
        keyring::Entry::new("xiangqi-sync", "email")
            .map_err(|e| format!("keychain: {e}"))?
            .set_password(email)
            .map_err(|e| format!("keychain: {e}"))?;
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        for user in ["refresh-token", "email"] {
            if let Ok(entry) = keyring::Entry::new("xiangqi-sync", user) {
                // Xoá entry chưa có cũng coi như xong.
                let _ = entry.delete_credential();
            }
        }
        Ok(())
    }
}

/// Lưu trong bộ nhớ (test — không chạm keychain/mạng).
/// Chỉ dùng trong test nên gate `cfg(test)`.
#[cfg(test)]
pub struct MemoryStore {
    session: Mutex<Option<(String, String)>>,
}

#[cfg(test)]
impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }
}

#[cfg(test)]
impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl TokenStore for MemoryStore {
    fn load_session(&self) -> Option<(String, String)> {
        self.session.lock().ok()?.clone()
    }

    fn save_session(&self, refresh_token: &str, email: &str) -> Result<(), String> {
        let mut slot = self
            .session
            .lock()
            .map_err(|_| "memory store bi khoa".to_string())?;
        *slot = Some((refresh_token.to_string(), email.to_string()));
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        let mut slot = self
            .session
            .lock()
            .map_err(|_| "memory store bi khoa".to_string())?;
        *slot = None;
        Ok(())
    }
}

/// Trạng thái worker thread: token hiện tại + ván đang theo dõi.
#[cfg(not(target_arch = "wasm32"))]
struct WorkerState {
    tokens: Option<SyncTokens>,
    watched: Option<Watched>,
    last_poll: Instant,
}

/// Một ván đang được poll: uuid + trạng thái cuối cùng máy này biết
/// (từ lần đẩy thành công / lần RemoteUpdate gần nhất).
#[cfg(not(target_arch = "wasm32"))]
struct Watched {
    uuid: String,
    version: u64,
    moves: Vec<String>,
}

/// Vòng lặp worker thread (desktop only).
#[cfg(not(target_arch = "wasm32"))]
fn worker_loop(
    req_rx: Receiver<SyncRequest>,
    res_tx: Sender<SyncResponse>,
    backend: Box<dyn SyncBackend>,
    store: Box<dyn TokenStore>,
    device_id: String,
) {
    let mut state = WorkerState {
        tokens: None,
        watched: None,
        last_poll: Instant::now(),
    };
    loop {
        match req_rx.recv_timeout(LOOP_TICK) {
            Ok(req) => {
                if !handle_request(
                    req,
                    backend.as_ref(),
                    store.as_ref(),
                    &device_id,
                    &mut state,
                    &res_tx,
                ) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Không có request mới: tới giờ poll ván đang xem chưa?
                poll_watched(backend.as_ref(), &mut state, &res_tx);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Xử lý một request. Trả `false` khi worker nên dừng (không xảy ra
/// trong thực tế — kênh chỉ đóng khi app thoát, lúc đó `recv_timeout`
/// đã trả Disconnected).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn handle_request(
    req: SyncRequest,
    backend: &dyn SyncBackend,
    store: &dyn TokenStore,
    device_id: &str,
    state: &mut WorkerState,
    res_tx: &Sender<SyncResponse>,
) -> bool {
    match req {
        SyncRequest::RestoreSession { req_id } => match restore_session(backend, store) {
            Ok(tokens) => {
                state.tokens = Some(tokens.clone());
                let _ = res_tx.send(SyncResponse::SessionRestored { req_id, tokens });
                pull_all_into(backend, state, req_id, res_tx);
            }
            Err(reason) => {
                let _ = res_tx.send(SyncResponse::SessionRestoreFailed { req_id, reason });
            }
        },
        SyncRequest::SignIn { req_id } => match backend.sign_in_google() {
            Ok(t) => {
                let tokens = SyncTokens {
                    id_token: t.id_token,
                    refresh_token: t.refresh_token,
                    uid: t.uid,
                    email: t.email,
                };
                if let Err(e) = store.save_session(&tokens.refresh_token, &tokens.email) {
                    let _ = res_tx.send(SyncResponse::SignInFailed {
                        req_id,
                        reason: format!("khong luu duoc token vao keychain: {e}"),
                    });
                    return true;
                }
                state.tokens = Some(tokens.clone());
                let _ = res_tx.send(SyncResponse::SignedIn { req_id, tokens });
                pull_all_into(backend, state, req_id, res_tx);
            }
            Err(e) => {
                let _ = res_tx.send(SyncResponse::SignInFailed {
                    req_id,
                    reason: e.to_string(),
                });
            }
        },
        SyncRequest::SignOut { req_id } => {
            let _ = store.clear();
            state.tokens = None;
            state.watched = None;
            let _ = res_tx.send(SyncResponse::SignedOut { req_id });
        }
        SyncRequest::Push { game, req_id } => {
            let Some(tokens) = state.tokens.clone() else {
                let _ = res_tx.send(SyncResponse::PushFailed {
                    req_id,
                    reason: "chua dang nhap".to_string(),
                });
                return true;
            };
            push_game(backend, &tokens, &game, device_id, req_id, state, res_tx);
        }
        SyncRequest::PullOne { uuid, req_id } => {
            let Some(tokens) = state.tokens.clone() else {
                let _ = res_tx.send(SyncResponse::SyncFailed {
                    req_id,
                    reason: "chua dang nhap".to_string(),
                });
                return true;
            };
            match backend.pull_one(&tokens.id_token, &tokens.uid, &uuid) {
                Ok(found) => {
                    let _ = res_tx.send(SyncResponse::PulledOne {
                        req_id,
                        game: found.map(|e| SavedGame::from_sync_entry(&e)),
                    });
                }
                Err(e) => {
                    let _ = res_tx.send(SyncResponse::SyncFailed {
                        req_id,
                        reason: e.to_string(),
                    });
                }
            }
        }
        SyncRequest::PullAll { req_id } => {
            pull_all_into(backend, state, req_id, res_tx);
        }
        SyncRequest::Watch {
            uuid,
            version,
            moves,
        } => {
            state.watched = Some(Watched {
                uuid,
                version,
                moves,
            });
            // Poll ngay ở vòng lặp tới (không chờ đủ 5 giây).
            state.last_poll = Instant::now() - POLL_INTERVAL;
        }
        SyncRequest::Unwatch => {
            state.watched = None;
        }
    }
    true
}

/// Khôi phục phiên: refresh token trong keychain → id_token mới.
/// Trả về tokens đầy đủ (email lấy từ keychain, vì response refresh
/// của Firebase không có email).
#[cfg(not(target_arch = "wasm32"))]
fn restore_session(
    backend: &dyn SyncBackend,
    store: &dyn TokenStore,
) -> Result<SyncTokens, String> {
    let (refresh_token, email) = store
        .load_session()
        .ok_or_else(|| "chua dang nhap".to_string())?;
    let id_token = backend.refresh(&refresh_token).map_err(|e| {
        // Refresh token hỏng/hết hạn: xoá luôn để lần sau đăng nhập
        // lại từ đầu thay vì thử mãi.
        let _ = store.clear();
        e.to_string()
    })?;
    // uid không có trong response refresh: lấy từ lần pull đầu tiên
    // thì muộn — nhưng mọi call Firestore đều cần uid. Giải mã uid từ
    // chính id_token (JWT payload có `user_id`) là đủ cho mục đích
    // này (token vừa lấy qua TLS từ Firebase).
    let uid = decode_jwt_user_id(&id_token).unwrap_or_default();
    Ok(SyncTokens {
        id_token,
        refresh_token,
        uid,
        email,
    })
}

/// Giải mã `user_id` từ payload JWT (base64url, không verify chữ ký
/// — chỉ để lấy uid hiển thị / làm key Firestore; token vừa lấy qua
/// TLS từ Firebase nên nội dung đáng tin cho mục đích này).
#[cfg(not(target_arch = "wasm32"))]
fn decode_jwt_user_id(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let decoded = base64url_decode(payload)?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json["user_id"].as_str().map(str::to_string)
}

/// Giải mã base64url (không padding) thành bytes.
#[cfg(not(target_arch = "wasm32"))]
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let bytes = input.as_bytes();
    let (chunks, rem) = bytes.as_chunks::<4>();
    for chunk in chunks {
        let n = (u32::from(val(chunk[0])?) << 18)
            | (u32::from(val(chunk[1])?) << 12)
            | (u32::from(val(chunk[2])?) << 6)
            | u32::from(val(chunk[3])?);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
    match rem.len() {
        2 => {
            let n = (u32::from(val(rem[0])?) << 18) | (u32::from(val(rem[1])?) << 12);
            out.push((n >> 16) as u8);
        }
        3 => {
            let n = (u32::from(val(rem[0])?) << 18)
                | (u32::from(val(rem[1])?) << 12)
                | (u32::from(val(rem[2])?) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => {}
    }
    Some(out)
}

/// Pull toàn bộ ván rồi gửi `PulledAll` (dùng sau Restore/SignIn và
/// khi UI hỏi trực tiếp). Chưa đăng nhập thì báo `SyncFailed`.
#[cfg(not(target_arch = "wasm32"))]
fn pull_all_into(
    backend: &dyn SyncBackend,
    state: &WorkerState,
    req_id: u64,
    res_tx: &Sender<SyncResponse>,
) {
    let Some(tokens) = &state.tokens else {
        let _ = res_tx.send(SyncResponse::SyncFailed {
            req_id,
            reason: "chua dang nhap".to_string(),
        });
        return;
    };
    match backend.pull_all(&tokens.id_token, &tokens.uid) {
        Ok(entries) => {
            let games = entries
                .iter()
                .map(SavedGame::from_sync_entry)
                .collect::<Vec<_>>();
            let _ = res_tx.send(SyncResponse::PulledAll { req_id, games });
        }
        Err(e) => {
            let _ = res_tx.send(SyncResponse::SyncFailed {
                req_id,
                reason: e.to_string(),
            });
        }
    }
}

/// Đẩy một ván: pull trước để kiểm tra version, rồi mới upsert.
/// Remote thắng → `PushRejected`; lỗi mạng → `PushFailed` (UI đẩy
/// lại sau).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn push_game(
    backend: &dyn SyncBackend,
    tokens: &SyncTokens,
    game: &SavedGame,
    device_id: &str,
    req_id: u64,
    state: &mut WorkerState,
    res_tx: &Sender<SyncResponse>,
) {
    let mut entry = game.to_sync_entry();
    entry.updated_by_device = device_id.to_string();
    match backend.pull_one(&tokens.id_token, &tokens.uid, &entry.uuid) {
        Ok(Some(remote)) => {
            // Cùng một ván: luật xung đột quyết định ai thắng.
            if xq_sync::conflict::resolve_same_game(&entry, &remote)
                == xq_sync::conflict::SameGameOutcome::UseRemote
            {
                let _ = res_tx.send(SyncResponse::PushRejected {
                    req_id,
                    remote: SavedGame::from_sync_entry(&remote),
                });
                return;
            }
        }
        Ok(None) => {}
        Err(e) => {
            let _ = res_tx.send(SyncResponse::PushFailed {
                req_id,
                reason: e.to_string(),
            });
            return;
        }
    }
    match backend.push(&tokens.id_token, &tokens.uid, &entry) {
        Ok(()) => {
            if let Some(w) = state.watched.as_mut() {
                if w.uuid == entry.uuid {
                    w.version = entry.version;
                    w.moves = entry.moves.clone();
                }
            }
            let _ = res_tx.send(SyncResponse::Pushed {
                req_id,
                uuid: entry.uuid,
                version: entry.version,
            });
        }
        Err(e) => {
            let _ = res_tx.send(SyncResponse::PushFailed {
                req_id,
                reason: e.to_string(),
            });
        }
    }
}

/// Poll ván đang xem: remote mới hơn trạng thái máy này biết thì gửi
/// `RemoteUpdate`. Lỗi mạng thì bỏ qua lặng lẽ — poll sau thử lại
/// (mất mạng là bình thường, không phải lỗi hiện cho người dùng).
#[cfg(not(target_arch = "wasm32"))]
fn poll_watched(backend: &dyn SyncBackend, state: &mut WorkerState, res_tx: &Sender<SyncResponse>) {
    // Lấy bản sao các trường cần so sánh rồi mới mượn `state` để
    // ghi (không giữ borrow qua chỗ gán `state.watched`).
    let Some((uuid, version, moves)) = state
        .watched
        .as_ref()
        .map(|w| (w.uuid.clone(), w.version, w.moves.clone()))
    else {
        return;
    };
    if state.last_poll.elapsed() < POLL_INTERVAL {
        return;
    }
    state.last_poll = Instant::now();
    let Some((id_token, uid)) = state
        .tokens
        .as_ref()
        .map(|t| (t.id_token.clone(), t.uid.clone()))
    else {
        return;
    };
    let Ok(Some(remote)) = backend.pull_one(&id_token, &uid, &uuid) else {
        return;
    };
    if remote.deleted {
        // Ván bị xoá ở máy kia: báo một lần để UI xoá theo, rồi
        // dừng theo dõi (không gửi lại mỗi lần poll).
        let _ = res_tx.send(SyncResponse::RemoteUpdate {
            uuid: uuid.clone(),
            game: SavedGame::from_sync_entry(&remote),
        });
        state.watched = None;
        return;
    }
    if remote_is_newer(version, &moves, &remote) {
        let game = SavedGame::from_sync_entry(&remote);
        state.watched = Some(Watched {
            uuid: uuid.clone(),
            version: remote.version,
            moves: remote.moves.clone(),
        });
        let _ = res_tx.send(SyncResponse::RemoteUpdate { uuid, game });
    }
}

/// Đối chiếu 2 danh sách nước (dùng trong test + poll): remote mới
/// hơn khi version cao hơn, hoặc cùng version nhưng nước khác.
/// Dùng chung desktop + wasm (worker wasm ở `sync_wasm.rs`).
#[must_use]
pub fn remote_is_newer(local_version: u64, local_moves: &[String], remote: &SyncEntry) -> bool {
    remote.version > local_version
        || (remote.version == local_version && remote.moves != local_moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    use xq_sync::MockBackend;

    #[test]
    fn response_req_id() {
        let r = SyncResponse::Pushed {
            req_id: 7,
            uuid: "u".to_string(),
            version: 3,
        };
        assert_eq!(r.req_id(), Some(7));
        assert_eq!(
            SyncResponse::RemoteUpdate {
                uuid: "u".to_string(),
                game: SavedGame {
                    id: "u".to_string(),
                    mode: crate::saved_games::SavedMode::Play,
                    label: String::new(),
                    created_unix: 0,
                    finished: false,
                    start_fen: String::new(),
                    moves: Vec::new(),
                    difficulty: 2,
                    human_red: Some(true),
                    result: None,
                    lesson_id: None,
                    node_path: Vec::new(),
                    percent: None,
                    uuid: "u".to_string(),
                    client_id: None,
                    version: 1,
                    updated_unix: 0,
                    updated_by_device: String::new(),
                    deleted: false,
                },
            }
            .req_id(),
            None
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn base64url_decode_roundtrip() {
        // "hello" -> aGVsbG8; "user_id":"abc" payload mẫu.
        assert_eq!(base64url_decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64url_decode("YWI").unwrap(), b"ab");
        assert_eq!(base64url_decode("YQ").unwrap(), b"a");
        assert!(base64url_decode("!!!").is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn jwt_user_id_decodes() {
        // header.payload.signature — payload {"user_id":"uid-1"}.
        let payload = "eyJ1c2VyX2lkIjoidWlkLTEifQ";
        let token = format!("e30.{payload}.sig");
        assert_eq!(decode_jwt_user_id(&token).as_deref(), Some("uid-1"));
        assert_eq!(decode_jwt_user_id("khong-phai-jwt"), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn remote_is_newer_cases() {
        let mut remote = SyncEntry::new(
            "u".to_string(),
            "g".to_string(),
            "play".to_string(),
            "fen".to_string(),
            2,
            Some(true),
            "d1".to_string(),
        );
        remote.moves = vec!["h2e2".to_string()];
        // Version cao hơn -> mới hơn.
        assert!(remote_is_newer(1, &[], &remote));
        // Cùng version, cùng nước -> không mới hơn.
        assert!(!remote_is_newer(1, &["h2e2".to_string()], &remote));
        // Cùng version, nước khác -> mới hơn (hoà version, khác nước).
        assert!(remote_is_newer(1, &["h7e7".to_string()], &remote));
        // Version thấp hơn -> không mới hơn.
        remote.version = 1;
        let mut older = remote.clone();
        older.version = 1;
        assert!(!remote_is_newer(2, &[], &remote));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sample_game(uuid: &str, version: u64, moves: Vec<&str>) -> SavedGame {
        SavedGame {
            id: uuid.to_string(),
            mode: crate::saved_games::SavedMode::Play,
            label: "Đấu với AI".to_string(),
            created_unix: 1_763_000_000,
            finished: false,
            start_fen: xq_core::Position::startpos().to_fen(),
            moves: moves.into_iter().map(str::to_string).collect(),
            difficulty: 2,
            human_red: Some(true),
            result: None,
            lesson_id: None,
            node_path: Vec::new(),
            percent: None,
            uuid: uuid.to_string(),
            client_id: None,
            version,
            updated_unix: 1_763_000_100,
            updated_by_device: "device-test".to_string(),
            deleted: false,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn recv_timeout(rx: &Receiver<SyncResponse>, secs: u64) -> Option<SyncResponse> {
        rx.recv_timeout(Duration::from_secs(secs)).ok()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn worker_push_pull_roundtrip_with_mock() {
        let backend = Box::new(MockBackend::new());
        let store = Box::new(MemoryStore::new());
        let worker = SyncWorker::with_backend("device-test".to_string(), backend, store);
        // Chưa đăng nhập: push bị từ chối (không crash).
        let req_id = 1;
        worker.send(SyncRequest::Push {
            game: sample_game("u1", 1, vec![]),
            req_id,
        });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::PushFailed { req_id: got, .. } => assert_eq!(got, req_id),
            other => panic!("mong PushFailed, gap {other:?}"),
        }
        // Đăng nhập mock -> SignedIn + PulledAll (rong).
        let req_id = 2;
        worker.send(SyncRequest::SignIn { req_id });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::SignedIn {
                req_id: got,
                tokens,
            } => {
                assert_eq!(got, req_id);
                assert_eq!(tokens.uid, "mock-uid");
            }
            other => panic!("mong SignedIn, gap {other:?}"),
        }
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::PulledAll { games, .. } => assert!(games.is_empty()),
            other => panic!("mong PulledAll rong, gap {other:?}"),
        }
        // Push 1 van -> Pushed.
        let req_id = 3;
        worker.send(SyncRequest::Push {
            game: sample_game("u1", 1, vec!["h2e2"]),
            req_id,
        });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::Pushed {
                req_id: got,
                uuid,
                version,
            } => {
                assert_eq!(got, req_id);
                assert_eq!(uuid, "u1");
                assert_eq!(version, 1);
            }
            other => panic!("mong Pushed, gap {other:?}"),
        }
        // Pull lai -> thay van vua day.
        let req_id = 4;
        worker.send(SyncRequest::PullOne {
            uuid: "u1".to_string(),
            req_id,
        });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::PulledOne { req_id: got, game } => {
                assert_eq!(got, req_id);
                assert_eq!(game.map(|g| g.moves), Some(vec!["h2e2".to_string()]));
            }
            other => panic!("mong PulledOne, gap {other:?}"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn worker_push_rejected_when_remote_wins() {
        let backend = Box::new(MockBackend::new());
        let store = Box::new(MemoryStore::new());
        let worker = SyncWorker::with_backend("device-test".to_string(), backend, store);
        worker.send(SyncRequest::SignIn { req_id: 1 });
        let _ = recv_timeout(&worker.rx, 5);
        let _ = recv_timeout(&worker.rx, 5);
        // May A day version 3 len cloud (gia lap bang push truc tiep).
        worker.send(SyncRequest::Push {
            game: sample_game("u1", 3, vec!["h2e2", "h9g7", "h0g2"]),
            req_id: 2,
        });
        let _ = recv_timeout(&worker.rx, 5);
        // May nay day ban version 2 (cu hon) -> bi tu choi, nhan ban remote.
        worker.send(SyncRequest::Push {
            game: sample_game("u1", 2, vec!["h2e2", "h9g7"]),
            req_id: 3,
        });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::PushRejected {
                req_id: got,
                remote,
            } => {
                assert_eq!(got, 3);
                assert_eq!(remote.version, 3);
                assert_eq!(remote.moves.len(), 3);
            }
            other => panic!("mong PushRejected, gap {other:?}"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn worker_restore_from_memory_store() {
        let backend = Box::new(MockBackend::new());
        let store = Box::new(MemoryStore::new());
        store.save_session("mock-refresh-token", "a@b.c").unwrap();
        let worker = SyncWorker::with_backend("device-test".to_string(), backend, store);
        worker.send(SyncRequest::RestoreSession { req_id: 1 });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::SessionRestored {
                req_id: got,
                tokens,
            } => {
                assert_eq!(got, 1);
                assert_eq!(tokens.email, "a@b.c");
            }
            other => panic!("mong SessionRestored, gap {other:?}"),
        }
        // Sau restore worker tu pull all.
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::PulledAll { .. } => {}
            other => panic!("mong PulledAll, gap {other:?}"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn worker_sign_out_clears_session() {
        let backend = Box::new(MockBackend::new());
        let store = Box::new(MemoryStore::new());
        let worker = SyncWorker::with_backend("device-test".to_string(), backend, store);
        worker.send(SyncRequest::SignIn { req_id: 1 });
        let _ = recv_timeout(&worker.rx, 5);
        let _ = recv_timeout(&worker.rx, 5);
        worker.send(SyncRequest::SignOut { req_id: 2 });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::SignedOut { req_id: got } => assert_eq!(got, 2),
            other => panic!("mong SignedOut, gap {other:?}"),
        }
        // Sau khi dang xuat, restore that bai (store da xoa).
        worker.send(SyncRequest::RestoreSession { req_id: 3 });
        match recv_timeout(&worker.rx, 5).expect("worker tra loi") {
            SyncResponse::SessionRestoreFailed { req_id: got, .. } => assert_eq!(got, 3),
            other => panic!("mong SessionRestoreFailed, gap {other:?}"),
        }
    }
}
