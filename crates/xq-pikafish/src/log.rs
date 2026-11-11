//! Engine logging (integration spec §7.3).
//!
//! The app installs one sink (a rotating file, see `xq-app`'s
//! `engine_log.rs`); without a sink every call is a no-op, so the
//! library stays usable standalone and tests stay quiet. Two levels:
//! `Warn` for fallbacks (engine died, handshake failed, restart
//! exhausted) and `Trace` for raw UCI lines (off by default in the app).

use std::sync::{Arc, Mutex, OnceLock};

/// Log level for engine events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Warn,
    Trace,
}

type Sink = Arc<dyn Fn(LogLevel, &str) + Send + Sync>;

static SINK: OnceLock<Mutex<Option<Sink>>> = OnceLock::new();

fn sink_slot() -> &'static Mutex<Option<Sink>> {
    SINK.get_or_init(|| Mutex::new(None))
}

/// Install (or clear with `None`) the log sink. Called once by the app.
pub fn set_log_sink(sink: Option<Sink>) {
    *sink_slot().lock().expect("log sink lock") = sink;
}

fn emit(level: LogLevel, msg: &str) {
    if let Some(sink) = sink_slot().lock().expect("log sink lock").clone() {
        sink(level, msg);
    }
}

/// Log a fallback / failure (always recorded when a sink is installed).
pub fn log_warn(msg: &str) {
    emit(LogLevel::Warn, msg);
}

/// Log a raw UCI line (only recorded when the app enables tracing).
pub fn log_trace(msg: &str) {
    emit(LogLevel::Trace, msg);
}
