//! Rotating engine log (integration spec §7.3): `logs/engine.log`,
//! 1 MB × 3 files, in the app's data directory. Warn-level events
//! (fallbacks, restarts) are always recorded; raw UCI lines (trace)
//! only when the user enables them in Settings.
//!
//! The logger fails open: if the directory cannot be created, logging
//! is silently disabled instead of breaking the app.

// NOTE (P4b): wired into `Shared` (`app.rs`) — the Settings
// dialog's engine-trace toggle (`set_trace_enabled`) and the
// About dialog's "Open engine.log" button (`path`) are its
// consumers, and the Pikafish log sink calls `write`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use xq_pikafish::LogLevel;

/// Max size of one log file before rotation.
const MAX_BYTES: u64 = 1024 * 1024;
/// Number of rotated files kept (engine.log.1, .2, .3).
const KEEP: usize = 3;

pub struct EngineLog {
    dir: PathBuf,
    file: Mutex<Option<File>>,
    trace_enabled: AtomicBool,
}

impl EngineLog {
    /// Create the log in `<data_dir>/logs`. Returns `None` when the
    /// directory cannot be created (logging disabled).
    #[must_use]
    pub fn new(data_dir: &std::path::Path) -> Option<std::sync::Arc<Self>> {
        let dir = data_dir.join("logs");
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("engine.log"))
            .ok();
        Some(std::sync::Arc::new(Self {
            dir,
            file: Mutex::new(file),
            trace_enabled: AtomicBool::new(false),
        }))
    }

    /// Enable/disable trace (raw UCI lines).
    pub fn set_trace_enabled(&self, enabled: bool) {
        self.trace_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Current log file path (for the About screen).
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.dir.join("engine.log")
    }

    /// Write one line. Trace lines are dropped unless enabled.
    pub fn write(&self, level: LogLevel, msg: &str) {
        if level == LogLevel::Trace && !self.trace_enabled.load(Ordering::Relaxed) {
            return;
        }
        let mut guard = self.file.lock().expect("engine log lock");
        let Some(file) = guard.as_mut() else {
            return;
        };
        let tag = match level {
            LogLevel::Warn => "WARN",
            LogLevel::Trace => "TRACE",
        };
        let line = format!("{} [{tag}] {msg}\n", timestamp());
        if file.write_all(line.as_bytes()).is_err() {
            return;
        }
        let _ = file.flush();
        if file.metadata().map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
            drop(guard);
            self.rotate();
        }
    }

    fn rotate(&self) {
        let mut guard = self.file.lock().expect("engine log lock");
        *guard = None;
        drop(guard);
        for i in (1..=KEEP).rev() {
            let from = if i == 1 {
                self.dir.join("engine.log")
            } else {
                self.dir.join(format!("engine.log.{}", i - 1))
            };
            let to = self.dir.join(format!("engine.log.{i}"));
            let _ = std::fs::rename(&from, &to);
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("engine.log"))
            .ok();
        *self.file.lock().expect("engine log lock") = file;
    }
}

fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // Seconds since epoch + millis: simple, no chrono dependency.
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}
