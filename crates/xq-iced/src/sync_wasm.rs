//! Worker sync cho PWA (wasm only) — bản async của worker thread
//! desktop (`sync.rs`).
//!
//! Không có thread trên wasm nên worker là một async task
//! (`spawn_local`), chạy vòng lặp: rút hết request đang chờ trong
//! channel (vẫn `std::sync::mpsc` như desktop — channel không cần
//! thread), poll ván đang xem mỗi 5 giây, rồi ngủ 500 ms. Logic
//! mirroring giống hệt desktop (push pull-trước, luật xung đột,
//! tombstone báo một lần) — chỉ transport khác: Firestore qua fetch
//! (`sync_fetch.rs`), đăng nhập qua Firebase JS SDK
//! (`firebase_auth.rs`).
//!
//! Token: SDK tự giữ phiên trong IndexedDB và tự refresh id_token,
//! nên worker gọi `get_id_token()` tươi trước mỗi đợt gọi Firestore
//! và không giữ refresh token (trường `refresh_token` của
//! `SyncTokens` để rỗng trên wasm).

use std::sync::mpsc::{Receiver, Sender};

use crate::firebase_auth::{self, SignInOutcome};
use crate::saved_games::SavedGame;
use crate::sync::{
    remote_is_newer, SyncRequest, SyncResponse, SyncTokens, LOOP_TICK, POLL_INTERVAL,
};
use crate::sync_fetch::{fetch_get, fetch_list, fetch_upsert};
use crate::time::Instant;

/// Trạng thái worker: token hiện tại + ván đang theo dõi.
struct WorkerState {
    tokens: Option<SyncTokens>,
    watched: Option<Watched>,
    last_poll: Instant,
}

/// Một ván đang được poll: uuid + trạng thái cuối cùng máy này biết
/// (từ lần đẩy thành công / lần RemoteUpdate gần nhất).
struct Watched {
    uuid: String,
    version: u64,
    moves: Vec<String>,
}

/// Vòng lặp worker (spawn_local từ `SyncWorker::new` trên wasm).
pub async fn run(req_rx: Receiver<SyncRequest>, res_tx: Sender<SyncResponse>, device_id: String) {
    let mut state = WorkerState {
        tokens: None,
        watched: None,
        last_poll: Instant::now(),
    };
    loop {
        while let Ok(req) = req_rx.try_recv() {
            handle_request(req, &device_id, &mut state, &res_tx).await;
        }
        poll_watched(&mut state, &res_tx).await;
        gloo_timers::future::TimeoutFuture::new(LOOP_TICK.as_millis() as u32).await;
    }
}

/// Lấy id_token tươi (SDK tự refresh nếu hết) và cập nhật vào state.
/// Lỗi khi chưa đăng nhập / SDK chưa sẵn sàng.
async fn fresh_id_token(state: &mut WorkerState) -> Result<String, String> {
    let tokens = state
        .tokens
        .as_mut()
        .ok_or_else(|| "chua dang nhap".to_string())?;
    let id_token = firebase_auth::get_id_token().await?;
    tokens.id_token = id_token.clone();
    Ok(id_token)
}

async fn handle_request(
    req: SyncRequest,
    device_id: &str,
    state: &mut WorkerState,
    res_tx: &Sender<SyncResponse>,
) {
    match req {
        SyncRequest::RestoreSession { req_id } => match restore_session().await {
            Ok(tokens) => {
                state.tokens = Some(tokens.clone());
                let _ = res_tx.send(SyncResponse::SessionRestored { req_id, tokens });
                pull_all_into(state, req_id, res_tx).await;
            }
            Err(reason) => {
                let _ = res_tx.send(SyncResponse::SessionRestoreFailed { req_id, reason });
            }
        },
        SyncRequest::SignIn { req_id } => match firebase_auth::sign_in().await {
            Ok(SignInOutcome::SignedIn {
                uid,
                email,
                id_token,
            }) => {
                let tokens = SyncTokens {
                    id_token,
                    refresh_token: String::new(),
                    uid,
                    email,
                };
                state.tokens = Some(tokens.clone());
                let _ = res_tx.send(SyncResponse::SignedIn { req_id, tokens });
                pull_all_into(state, req_id, res_tx).await;
            }
            Ok(SignInOutcome::Redirecting) => {
                // Trang đang chuyển hướng sang Google rồi tải lại —
                // không trả lời gì, phiên sẽ khôi phục lúc mở lại.
            }
            Err(e) => {
                let _ = res_tx.send(SyncResponse::SignInFailed { req_id, reason: e });
            }
        },
        SyncRequest::SignOut { req_id } => {
            // Đăng xuất khỏi SDK (lỗi thì thôi — trạng thái local
            // vẫn xoá để UI về Off; phiên SDK hỏng sẽ khôi phục
            // lại ở lần mở app sau nếu còn).
            let _ = firebase_auth::sign_out().await;
            state.tokens = None;
            state.watched = None;
            let _ = res_tx.send(SyncResponse::SignedOut { req_id });
        }
        SyncRequest::Push { game, req_id } => {
            if state.tokens.is_none() {
                let _ = res_tx.send(SyncResponse::PushFailed {
                    req_id,
                    reason: "chua dang nhap".to_string(),
                });
                return;
            }
            push_game(&game, device_id, req_id, state, res_tx).await;
        }
        SyncRequest::PullOne { uuid, req_id } => {
            let Ok(id_token) = fresh_id_token(state).await else {
                let _ = res_tx.send(SyncResponse::SyncFailed {
                    req_id,
                    reason: "chua dang nhap".to_string(),
                });
                return;
            };
            let uid = state
                .tokens
                .as_ref()
                .map(|t| t.uid.clone())
                .unwrap_or_default();
            match fetch_get(&id_token, &uid, &uuid).await {
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
            pull_all_into(state, req_id, res_tx).await;
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
}

/// Khôi phục phiên: chờ SDK settle rồi lấy user + id_token.
/// SDK giữ phiên trong IndexedDB nên mở lại app là có ngay.
async fn restore_session() -> Result<SyncTokens, String> {
    let user = firebase_auth::wait_for_auth()
        .await?
        .ok_or_else(|| "chua dang nhap".to_string())?;
    let id_token = firebase_auth::get_id_token().await?;
    Ok(SyncTokens {
        id_token,
        refresh_token: String::new(),
        uid: user.uid,
        email: user.email,
    })
}

/// Pull toàn bộ ván rồi gửi `PulledAll` (dùng sau Restore/SignIn và
/// khi UI hỏi trực tiếp). Chưa đăng nhập thì báo `SyncFailed`.
async fn pull_all_into(state: &mut WorkerState, req_id: u64, res_tx: &Sender<SyncResponse>) {
    let Ok(id_token) = fresh_id_token(state).await else {
        let _ = res_tx.send(SyncResponse::SyncFailed {
            req_id,
            reason: "chua dang nhap".to_string(),
        });
        return;
    };
    let uid = state
        .tokens
        .as_ref()
        .map(|t| t.uid.clone())
        .unwrap_or_default();
    match fetch_list(&id_token, &uid).await {
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
async fn push_game(
    game: &SavedGame,
    device_id: &str,
    req_id: u64,
    state: &mut WorkerState,
    res_tx: &Sender<SyncResponse>,
) {
    let Ok(id_token) = fresh_id_token(state).await else {
        let _ = res_tx.send(SyncResponse::PushFailed {
            req_id,
            reason: "chua dang nhap".to_string(),
        });
        return;
    };
    let uid = state
        .tokens
        .as_ref()
        .map(|t| t.uid.clone())
        .unwrap_or_default();
    let mut entry = game.to_sync_entry();
    entry.updated_by_device = device_id.to_string();
    match fetch_get(&id_token, &uid, &entry.uuid).await {
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
    match fetch_upsert(&id_token, &uid, &entry).await {
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
async fn poll_watched(state: &mut WorkerState, res_tx: &Sender<SyncResponse>) {
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
    let Ok(id_token) = fresh_id_token(state).await else {
        return;
    };
    let uid = state
        .tokens
        .as_ref()
        .map(|t| t.uid.clone())
        .unwrap_or_default();
    let Ok(Some(remote)) = fetch_get(&id_token, &uid, &uuid).await else {
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
