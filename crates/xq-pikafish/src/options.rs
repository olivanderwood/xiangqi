//! Engine startup options (integration spec §3).
//!
//! Pure configuration: which NNUE file, how many threads, how much hash.
//! The RAM-based hash sizing is a pure function of total RAM so it can be
//! unit-tested; reading the actual RAM total is wired up in the app
//! (settings), which passes the value in.

use std::path::PathBuf;

/// Startup options for one Pikafish process.
#[derive(Clone, Debug)]
pub struct EngineOptions {
    /// Absolute path to `pikafish.nnue`.
    pub eval_file: PathBuf,
    /// Worker threads for the engine (UI keeps one core for itself).
    pub threads: u32,
    /// Transposition-table size in MB.
    pub hash_mb: u32,
}

impl EngineOptions {
    #[must_use]
    pub fn new(eval_file: PathBuf) -> Self {
        Self {
            eval_file,
            threads: default_threads(),
            hash_mb: 256,
        }
    }
}

/// `clamp(logical_cpus − 1, 1, 8)`: leave one core for the UI.
#[must_use]
pub fn default_threads() -> u32 {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    cpus.saturating_sub(1).clamp(1, 8) as u32
}

/// Hash size from total RAM (spec §3): 256 MB normally, 128 MB under
/// 8 GB, 64 MB under 4 GB.
#[must_use]
pub fn hash_mb_for_total_ram_gb(total_gb: f64) -> u32 {
    if total_gb < 4.0 {
        64
    } else if total_gb < 8.0 {
        128
    } else {
        256
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_in_range() {
        let t = default_threads();
        assert!((1..=8).contains(&t));
    }

    #[test]
    fn hash_by_ram() {
        assert_eq!(hash_mb_for_total_ram_gb(2.0), 64);
        assert_eq!(hash_mb_for_total_ram_gb(6.0), 128);
        assert_eq!(hash_mb_for_total_ram_gb(16.0), 256);
    }
}
