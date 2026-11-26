//! Wasm-safe clock (`Instant` / `SystemTime`).
//!
//! `std::time::Instant::now()` and `SystemTime::now()` PANIC on
//! `wasm32-unknown-unknown` ("time not implemented on this
//! platform") — the target has no OS clock, so std's implementation
//! is a stub. The app called them at startup (`Play::new_seed`,
//! `Learn::new`, the tick subscriptions) and the PWA died before
//! its first frame (2026-11-26: `RuntimeError: unreachable` in the
//! browser, no message until `console_error_panic_hook` was added).
//!
//! [`wasmtimer`] (already a transitive dependency via iced 0.14 —
//! it is also what `iced::time::every` yields, so the tick instant
//! passes straight through to the screens) backs `Instant` with
//! `performance.now()` and `SystemTime` with `Date.now()` on wasm,
//! and the module re-exports `std::time` verbatim on native
//! targets — so this alias is zero-cost on desktop and the screens
//! keep one clock type everywhere.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::{Instant, SystemTime, UNIX_EPOCH};
