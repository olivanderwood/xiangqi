//! `PikafishEngine`: the `Engine` trait over a UCI subprocess
//! (integration spec §1.2, §7.3).
//!
//! Responsibilities: spawn + handshake, restart the process once per
//! session after a crash/timeout, validate every `bestmove` against
//! `xq-core` legality, and re-apply the level after a restart. The
//! app-level fallback to `BuiltinEngine` (banner, "try Pikafish again")
//! lives in the app, not here: this engine only reports errors.

use crate::locate::LocatedEngine;
use crate::options::EngineOptions;
use crate::uci::{SessionTimeouts, UciSession};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use xq_core::notation::iccs;
use xq_core::types::Move;
use xq_engine::{
    Engine, EngineError, EngineRequest, GradeThresholds, Level, Line, Score, SearchInfo,
    SearchLimits, StopHandle,
};

pub struct PikafishEngine {
    session: Option<UciSession>,
    located: LocatedEngine,
    options: EngineOptions,
    /// Args passed to the executable on every spawn, including restarts
    /// (empty for real Pikafish; the fake engine takes its script here).
    spawn_args: Vec<String>,
    stop: Arc<AtomicBool>,
    level: Level,
    name: String,
    /// Restarts performed in this session (max 1, §7.3).
    restarts: u32,
    timeouts: SessionTimeouts,
}

impl PikafishEngine {
    /// Spawn Pikafish and run the handshake. The NNUE file must exist.
    pub fn spawn(located: LocatedEngine, options: EngineOptions) -> Result<Self, EngineError> {
        Self::spawn_with_timeouts(located, options, SessionTimeouts::default())
    }

    /// Same as [`PikafishEngine::spawn`] with explicit timeouts (tests).
    pub fn spawn_with_timeouts(
        located: LocatedEngine,
        options: EngineOptions,
        timeouts: SessionTimeouts,
    ) -> Result<Self, EngineError> {
        Self::spawn_with_timeouts_and_args(located, options, timeouts, Vec::new())
    }

    /// Same as [`PikafishEngine::spawn_with_timeouts`] with extra args
    /// for the executable (tests: the fake engine's `--script` etc.).
    /// The args are reused on every restart.
    pub fn spawn_with_timeouts_and_args(
        located: LocatedEngine,
        options: EngineOptions,
        timeouts: SessionTimeouts,
        spawn_args: Vec<String>,
    ) -> Result<Self, EngineError> {
        if !located.nnue.is_file() {
            return Err(EngineError::NotFound);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let session = UciSession::start_with_timeouts(
            &located.exe,
            &spawn_args,
            &located.engine_dir,
            &options,
            Arc::clone(&stop),
            timeouts,
        )?;
        let name = if session.name.is_empty() {
            "Pikafish".to_string()
        } else {
            session.name.clone()
        };
        Ok(Self {
            session: Some(session),
            located,
            options,
            spawn_args,
            stop,
            level: Level::Medium,
            name,
            restarts: 0,
            timeouts,
        })
    }

    /// Is the child process still running?
    #[must_use]
    pub fn is_alive(&mut self) -> bool {
        self.session.as_mut().is_some_and(UciSession::is_alive)
    }

    /// Options the engine did not advertise (skipped at startup).
    #[must_use]
    pub fn missing_options(&self) -> Vec<String> {
        self.session
            .as_ref()
            .map_or_else(Vec::new, |s| s.missing_options.clone())
    }

    fn start_session(&mut self) -> Result<UciSession, EngineError> {
        UciSession::start_with_timeouts(
            &self.located.exe,
            &self.spawn_args,
            &self.located.engine_dir,
            &self.options,
            Arc::clone(&self.stop),
            self.timeouts,
        )
    }

    /// A live session, restarting the process once if it died (§7.3).
    /// A second crash in the same session is an error: the app then falls
    /// back to the builtin engine for the rest of the session.
    fn ensure_session(&mut self) -> Result<&mut UciSession, EngineError> {
        let alive = self.session.as_mut().is_some_and(UciSession::is_alive);
        if alive {
            return Ok(self.session.as_mut().unwrap());
        }
        if self.restarts >= 1 {
            crate::log::log_warn("Pikafish chết lần 2 trong phiên, chuyển sang AI tích hợp");
            return Err(EngineError::Crashed(
                self.session
                    .as_mut()
                    .and_then(|s| s.try_wait())
                    .map(|st| st.code().unwrap_or(-1)),
            ));
        }
        self.restarts += 1;
        crate::log::log_warn("Pikafish chết giữa chừng, khởi chạy lại (1 lần/phiên)");
        let mut session = self.start_session()?;
        // Re-apply the stored level on the fresh process (P6 fills in
        // the actual option writes; until then the level is stored only).
        let _ = session.new_game();
        self.session = Some(session);
        Ok(self.session.as_mut().unwrap())
    }

    /// Run `op` on a live session, restarting once and retrying on a
    /// crash/timeout (§7.3).
    fn with_restart<T>(
        &mut self,
        mut op: impl FnMut(&mut UciSession) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        match op(self.ensure_session()?) {
            Ok(v) => Ok(v),
            Err(e) if restartable(&e) => {
                // The process is dead (or hung and killed); drop it and
                // try once on a fresh one. `ensure_session` enforces the
                // one-restart budget.
                self.session = None;
                op(self.ensure_session()?)
            }
            Err(e) => Err(e),
        }
    }
}

fn restartable(e: &EngineError) -> bool {
    matches!(e, EngineError::Crashed(_) | EngineError::Timeout(_))
}

impl Engine for PikafishEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn new_game(&mut self) -> Result<(), EngineError> {
        self.with_restart(|s| s.new_game())
    }

    fn set_level(&mut self, level: Level) -> Result<(), EngineError> {
        // No UCI option to send: the real binary (2026-09-06) has no
        // UCI_LimitStrength / UCI_Elo / Skill Level, so the level is
        // enforced by the `go` limits of each request instead
        // (`limits_for`, `levels.rs`). Storing it is enough; the next
        // search uses the new level's limits.
        self.level = level;
        Ok(())
    }

    fn limits_for(&self, level: Level) -> SearchLimits {
        crate::levels::pikafish_limits(level)
    }

    fn weaken(&self, level: Level) -> Option<xq_engine::Weaken> {
        crate::levels::pikafish_level(level).weaken
    }

    fn supports_diversity(&self) -> bool {
        true
    }

    fn grade_limits(&self) -> SearchLimits {
        // Learn mode §6: hint analysis and grading share one movetime.
        SearchLimits::time_ms(1500)
    }

    fn hint_limits(&self) -> SearchLimits {
        SearchLimits::time_ms(1500)
    }

    fn best_move(
        &mut self,
        req: &EngineRequest,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Line, EngineError> {
        let line = self.with_restart(|s| s.best_move(req, &req.limits, on_info))?;
        // Every bestmove must be legal (§2.5) — never play garbage.
        let pos = req.position()?;
        if !pos.legal_moves().contains(&line.mv) {
            return Err(EngineError::IllegalMove(format!(
                "{} ở thế {}",
                iccs(line.mv),
                pos.to_fen()
            )));
        }
        Ok(line)
    }

    fn analyze(
        &mut self,
        req: &EngineRequest,
        top_n: usize,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Vec<Line>, EngineError> {
        let lines = self.with_restart(|s| s.analyze(req, top_n, &req.limits, on_info))?;
        let pos = req.position()?;
        let legal = pos.legal_moves();
        for line in &lines {
            if !legal.contains(&line.mv) {
                return Err(EngineError::IllegalMove(format!(
                    "{} ở thế {}",
                    iccs(line.mv),
                    pos.to_fen()
                )));
            }
        }
        Ok(lines)
    }

    fn score_move(&mut self, req: &EngineRequest, mv: Move) -> Result<Score, EngineError> {
        self.with_restart(|s| s.score_move(req, mv, &req.limits))
    }

    fn stop_handle(&self) -> StopHandle {
        StopHandle::from_flag(Arc::clone(&self.stop))
    }

    fn shutdown(&mut self) {
        if let Some(mut session) = self.session.take() {
            session.quit();
        }
    }

    fn set_stop_flag(&mut self, flag: &Arc<AtomicBool>) {
        self.stop = Arc::clone(flag);
        if let Some(session) = self.session.as_mut() {
            session.set_stop_flag(flag);
        }
    }

    fn level(&self) -> Level {
        self.level
    }

    fn grade_thresholds(&self) -> GradeThresholds {
        GradeThresholds::PIKAFISH
    }
}
