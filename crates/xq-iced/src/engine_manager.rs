//! Engine manager (integration spec §7): locate + spawn Pikafish on a
//! background thread at startup (pre-warm, so the first game does not
//! wait for the NNUE to load), install it behind the shared engine,
//! and fall back to the builtin engine with a one-time banner when it
//! is unavailable. The "Thử lại Pikafish" button re-runs the same
//! attempt.
//!
//! Installing swaps the engine behind the `SharedEngine`; the worker
//! thread keeps working because it locks the same handle, and stopping
//! keeps working because the stop flag lives in the handle (P7).

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
/// `Engine` (trait methods on the install path) is desktop-only.
#[cfg(not(target_arch = "wasm32"))]
use xq_engine::Engine;
use xq_engine::{Level, SharedEngine};
#[cfg(not(target_arch = "wasm32"))]
use xq_pikafish::{hash_mb_for_total_ram_gb, locate, EngineOptions, PikafishEngine};

/// Outcome of one install attempt, delivered to the UI thread.
/// (On wasm no variant is ever constructed — the install never runs
/// — but the type is part of `App`'s message flow.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug)]
pub enum EngineEvent {
    /// Pikafish is now the active engine.
    Installed {
        /// Display name from `id name` (e.g. `Pikafish 2026-09-06`).
        name: String,
        /// Directory holding the binary + NNUE (About screen).
        dir: PathBuf,
    },
    /// Pikafish is unavailable; the app stays on the builtin engine.
    /// The string is the one-time banner text.
    Unavailable(String),
}

/// Total RAM in GB, via sysinfo (0.0 when it cannot be read).
/// Desktop only: on wasm the install never runs, so this is unused.
#[cfg(not(target_arch = "wasm32"))]
fn total_ram_gb() -> f64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.total_memory() as f64 / 1_000_000_000.0
}

/// Resolve the hash size: the setting, or the RAM-based default (§3)
/// when it is 0 (auto). Desktop only (see `total_ram_gb`).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn resolve_hash_mb(setting_mb: u32) -> u32 {
    if setting_mb > 0 {
        setting_mb
    } else {
        hash_mb_for_total_ram_gb(total_ram_gb())
    }
}

/// Try to install Pikafish on a background thread. The returned channel
/// delivers exactly one [`EngineEvent`]. `user_path` is the path from
/// Settings (empty = auto-locate); `hash_mb` 0 means auto (by RAM).
///
/// wasm32: no thread — `std::thread::spawn` panics on wasm. The
/// install attempt runs synchronously on the calling thread instead:
/// `locate` finds nothing on wasm (no filesystem / subprocess), so
/// this always answers `Unavailable` at once and the app stays on
/// the builtin engine, exactly like the desktop fallback.
/// (On wasm this is never called — `App` has no install receiver
/// there — but the function is kept so the install API is the same
/// on both targets.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[must_use]
pub fn spawn_install(
    shared: SharedEngine,
    user_path: Option<PathBuf>,
    threads: u32,
    hash_mb: u32,
    level: Level,
) -> Receiver<EngineEvent> {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let event = install(&shared, user_path.as_deref(), threads, hash_mb, level);
        let _ = tx.send(event);
    });
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (threads, hash_mb, level);
        let _ = shared;
        let _ = user_path;
        // Never called on wasm (see the module docs), but kept so
        // the install API is the same on both targets. The web build
        // runs Pikafish-WASM instead — there is nothing to install.
        let event =
            EngineEvent::Unavailable("Bản web dùng Pikafish-WASM, không cần cài đặt".to_string());
        let _ = tx.send(event);
    }
    rx
}

/// Desktop only: the real install attempt (locate + spawn). On wasm
/// `spawn_install` answers `Unavailable` directly instead.
#[cfg(not(target_arch = "wasm32"))]
fn install(
    shared: &SharedEngine,
    user_path: Option<&std::path::Path>,
    threads: u32,
    hash_mb: u32,
    level: Level,
) -> EngineEvent {
    let located = match locate(user_path) {
        Ok(l) => l,
        Err(_) => {
            return EngineEvent::Unavailable(
                "Không tìm thấy Pikafish, đang dùng AI tích hợp".to_string(),
            );
        }
    };
    let mut options = EngineOptions::new(located.nnue.clone());
    options.threads = threads.max(1);
    options.hash_mb = resolve_hash_mb(hash_mb);
    let mut engine = match PikafishEngine::spawn(located.clone(), options) {
        Ok(e) => e,
        Err(err) => {
            xq_pikafish::log::log_warn(&format!("không khởi chạy được Pikafish: {err}"));
            return EngineEvent::Unavailable(format!(
                "Không chạy được Pikafish ({err}), đang dùng AI tích hợp"
            ));
        }
    };
    let name = engine.name().to_string();
    let _ = engine.set_level(level);
    {
        let mut guard = shared.lock().expect("engine lock");
        *guard = Box::new(engine);
        guard.set_stop_flag(&shared.stop_flag());
    }
    EngineEvent::Installed {
        name,
        dir: located.engine_dir,
    }
}

/// Open a folder in the system file manager (About screen). Best
/// effort; failures are ignored.
///
/// wasm32: no-op — there is no file manager to open, and the buttons
/// that call this are hidden on wasm (no engine dir / log exist).
pub fn open_folder(dir: &std::path::Path) {
    #[cfg(target_arch = "wasm32")]
    let _ = dir;
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(target_os = "windows", target_os = "macos"))
    ))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}
