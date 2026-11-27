//! Sync mirroring cho game Cờ Tướng (plan
//! `~/workspace/research_notes/sync-plan-20261125/plan.md`, quyết định ở
//! `docs/DECISIONS-sync.md`).
//!
//! Crate pure logic, không dính Iced: Firebase Auth + Firestore gọi qua
//! REST bằng ureq 3 + rustls trên desktop (đã chốt 2026-11-27 sau khi
//! spike chạy qua trên máy Windows của Hieu). Trên wasm (PWA sync,
//! 2026-11-27) không có socket: transport là `fetch` ở phía xq-iced,
//! đăng nhập qua Firebase JS SDK — crate này chỉ đóng góp phần pure
//! (model / conflict / firestore URL + parse) cho cả hai target.
//! App (xq-iced) dùng crate này qua trait [`SyncBackend`] trên desktop;
//! đổi backend sau chỉ cần implement trait mới.
//!
//! Thiết kế:
//! - [`SyncEntry`] là bản ghi ván cờ đồng bộ (FEN + nước ICCS + metadata
//!   sync: `uuid`, `version`, `updated_unix`, `updated_by_device`,
//!   `deleted`).
//! - [`FirebaseBackend`] implement [`SyncBackend`] bằng REST.
//! - [`MockBackend`] (in-memory) dùng cho test, không chạm mạng.
//! - [`conflict`] chứa luật xung đột mirroring (plan §5.4): cùng một ván
//!   thì `version` cao hơn thắng; ván dở khác hẳn thì giữ cả hai.

pub mod auth;
pub mod backend;
pub mod config;
pub mod conflict;
pub mod error;
pub mod firestore;
pub mod model;

pub use auth::Tokens;
#[cfg(not(target_arch = "wasm32"))]
pub use backend::{FirebaseBackend, MockBackend, SyncBackend};
pub use config::{FirebaseConfig, OAuthCredentials, XIANGQI_SYNC_CONFIG};
pub use error::SyncError;
pub use model::SyncEntry;
