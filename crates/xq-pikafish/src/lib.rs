//! UCI client for the Pikafish Xiangqi engine (integration spec §1.1).
//!
//! `xq-pikafish` has no UI dependencies. The engine runs as a subprocess
//! and is driven over UCI (`uci.rs`); `parse.rs` parses its output;
//! `process.rs` owns the child process; `options.rs` holds startup
//! options. `locate.rs` (P4) finds the binary + NNUE file; `levels.rs`
//! (P6) maps difficulty levels to `go` limits (the real binary has no
//! strength options); `diversity.rs` (P6) picks a varied move at Master.

pub mod diversity;
pub mod engine;
pub mod levels;
pub mod locate;
pub mod log;
pub mod options;
pub mod parse;
pub mod process;
pub mod uci;

pub use diversity::{pick_diverse, pick_weighted, GameRng, DIVERSITY_MARGIN_CP, DIVERSITY_PLIES};
pub use engine::PikafishEngine;
pub use levels::{pikafish_level, pikafish_limits, PikafishLevel};
pub use locate::{locate, LocatedEngine};
pub use log::{set_log_sink, LogLevel};
pub use options::{default_threads, hash_mb_for_total_ram_gb, EngineOptions};
pub use uci::{SessionTimeouts, UciSession};
