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
use xq_engine::{Engine, Level, SharedEngine};
use xq_pikafish::{hash_mb_for_total_ram_gb, locate, EngineOptions, PikafishEngine};

/// Outcome of one install attempt, delivered to the UI thread.
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
fn total_ram_gb() -> f64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.total_memory() as f64 / 1_000_000_000.0
}

/// Resolve the hash size: the setting, or the RAM-based default (§3)
/// when it is 0 (auto).
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
#[must_use]
pub fn spawn_install(
    shared: SharedEngine,
    user_path: Option<PathBuf>,
    threads: u32,
    hash_mb: u32,
    level: Level,
) -> Receiver<EngineEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let event = install(&shared, user_path.as_deref(), threads, hash_mb, level);
        let _ = tx.send(event);
    });
    rx
}

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
pub fn open_folder(dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}
