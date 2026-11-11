//! Locating the Pikafish binary + NNUE file (integration spec §7.1).
//!
//! Search order: `XQ_PIKAFISH_PATH` env (dev/test) → user-configured
//! path (Settings) → directory bundled with the app → repo `engine/`
//! (Linux dev). Missing everything → [`EngineError::NotFound`].

use std::path::{Path, PathBuf};
use xq_engine::EngineError;

/// A located engine: executable, its directory, and the NNUE weights.
#[derive(Clone, Debug)]
pub struct LocatedEngine {
    pub exe: PathBuf,
    pub engine_dir: PathBuf,
    pub nnue: PathBuf,
}

/// Executable file name for this platform inside `engine/`.
///
/// The pinned release (2026-09-06) ships only universal binaries —
/// there are no separate avx2/sse41 builds to choose between, so no
/// CPU-feature detection is needed here. See
/// `docs/research-pikafish-2026-09-06.md` §3.
#[must_use]
pub fn exe_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Pikafish-Windows-x86-64-universal.exe"
    }
    #[cfg(target_os = "macos")]
    {
        "Pikafish-MacOS-universal"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "Pikafish-Linux-x86-64-universal"
    }
}

/// Candidate engine directories, in search order (after env + user path).
fn bundled_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Windows: <dir with xiangqi.exe>/engine/
            dirs.push(dir.join("engine"));
            // macOS: <App>.app/Contents/MacOS/xiangqi ->
            //        <App>.app/Contents/Resources/engine/
            if let Some(contents) = dir.parent() {
                dirs.push(contents.join("Resources").join("engine"));
            }
        }
    }
    // Linux dev: <repo>/engine/
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("engine"));
        if let Some(parent) = cwd.parent() {
            dirs.push(parent.join("engine"));
        }
    }
    dirs
}

/// Locate the engine. `user_path` is the path from Settings, if the user
/// set one (a directory containing the engine files, or the executable
/// itself).
pub fn locate(user_path: Option<&Path>) -> Result<LocatedEngine, EngineError> {
    // 1. Dev/test override: the executable itself.
    if let Ok(p) = std::env::var("XQ_PIKAFISH_PATH") {
        if !p.is_empty() {
            let exe = PathBuf::from(&p);
            if exe.is_file() {
                let engine_dir = exe
                    .parent()
                    .map(|d| d.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                return Ok(LocatedEngine {
                    nnue: engine_dir.join("pikafish.nnue"),
                    exe,
                    engine_dir,
                });
            }
        }
    }
    // 2. User-configured path.
    if let Some(p) = user_path {
        if let Some(located) = from_dir_or_exe(p) {
            return Ok(located);
        }
    }
    // 3. Bundled directories.
    for dir in bundled_dirs() {
        if let Some(located) = from_dir_or_exe(&dir) {
            return Ok(located);
        }
    }
    Err(EngineError::NotFound)
}

fn from_dir_or_exe(p: &Path) -> Option<LocatedEngine> {
    if p.is_file() {
        let engine_dir = p
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        return Some(LocatedEngine {
            nnue: engine_dir.join("pikafish.nnue"),
            exe: p.to_path_buf(),
            engine_dir,
        });
    }
    let exe = p.join(exe_name());
    if exe.is_file() {
        return Some(LocatedEngine {
            nnue: p.join("pikafish.nnue"),
            exe,
            engine_dir: p.to_path_buf(),
        });
    }
    // Dev convenience: a plain `pikafish` (or `pikafish.exe`) copied
    // or renamed by hand.
    let plain = if cfg!(target_os = "windows") {
        "pikafish.exe"
    } else {
        "pikafish"
    };
    let exe = p.join(plain);
    if exe.is_file() {
        return Some(LocatedEngine {
            nnue: p.join("pikafish.nnue"),
            exe,
            engine_dir: p.to_path_buf(),
        });
    }
    None
}
