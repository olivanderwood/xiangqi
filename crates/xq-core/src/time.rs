//! Wasm-safe clock (`Instant`).
//!
//! `std::time::Instant::now()` PANICS on `wasm32-unknown-unknown`
//! ("time not implemented on this platform") — the target has no OS
//! clock, so std's implementation is a stub. The searcher calls it
//! at construction (`Searcher::new` sets `start: Instant::now()`),
//! so the PWA died at startup inside `BuiltinEngine::new()` before
//! its first frame (2026-11-26, found via boot breadcrumbs in the
//! browser console).
//!
//! [`wasmtimer`] backs `Instant` with `performance.now()` on wasm,
//! and the module re-exports `std::time` verbatim on native targets
//! — so this alias is zero-cost on desktop and the searcher keeps
//! one clock type everywhere. (Same pattern as `xq-iced`'s
//! `time.rs`, which also covers `SystemTime` for the UI.)

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::Instant;
