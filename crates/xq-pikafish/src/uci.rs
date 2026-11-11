//! UCI session: handshake, options, position, search (integration spec §2).
//!
//! One [`UciSession`] owns a [`Process`] and speaks UCI over its stdin /
//! stdout. It knows nothing about the app's UI or about locating the
//! engine binary — that is `PikafishEngine`'s job (P4).

use crate::options::EngineOptions;
use crate::parse::{parse_line, EngineOption, InfoLine, ParsedLine};
use crate::process::{Process, UciEvent};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use xq_core::notation::iccs;
use xq_core::types::Move;
use xq_engine::{EngineError, EngineRequest, Line, Score, SearchInfo, SearchLimits};

/// How long to wait for `uciok` after sending `uci`.
const UCIOK_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait for `readyok` after `isready` (NNUE load is slow).
const READYOK_TIMEOUT: Duration = Duration::from_secs(15);
/// Extra time after `movetime` before declaring the engine hung.
const BESTMOVE_SLACK: Duration = Duration::from_millis(3000);
/// Time to wait for `bestmove` after sending `stop`.
const STOP_WAIT: Duration = Duration::from_secs(1);
/// Hard cap for depth-only searches.
const DEPTH_SEARCH_CAP: Duration = Duration::from_secs(120);

/// Session timeouts. Tests use shorter values so a hung-engine test
/// finishes in well under the 16 s the spec allows.
#[derive(Clone, Copy, Debug)]
pub struct SessionTimeouts {
    pub uciok: Duration,
    pub readyok: Duration,
    pub stop_wait: Duration,
    /// Extra time after `movetime` before declaring the engine hung.
    pub bestmove_slack: Duration,
}

impl Default for SessionTimeouts {
    fn default() -> Self {
        Self {
            uciok: UCIOK_TIMEOUT,
            readyok: READYOK_TIMEOUT,
            stop_wait: STOP_WAIT,
            bestmove_slack: BESTMOVE_SLACK,
        }
    }
}

/// A live UCI session with one engine process.
pub struct UciSession {
    process: Process,
    /// Engine name from `id name` (e.g. `Pikafish 2026-09-06`).
    pub name: String,
    /// Options the engine advertised during the handshake.
    pub options: Vec<EngineOption>,
    /// Options we wanted but the engine did not advertise (skipped).
    pub missing_options: Vec<String>,
    /// Current MultiPV setting (starts at 1).
    multipv: usize,
    /// Shared stop flag (from the engine's `StopHandle`).
    stop: Arc<AtomicBool>,
    /// `None` until probed by [`UciSession::probe_searchmoves`].
    pub searchmoves_supported: Option<bool>,
    timeouts: SessionTimeouts,
}

impl UciSession {
    /// Spawn the engine and run the handshake (`uci` / options /
    /// `isready`). `stop` is the shared stop flag. `args` are passed to
    /// the executable (empty for real Pikafish; the fake engine takes
    /// `--script` / `--log` / `--state`).
    pub fn start(
        exe: &Path,
        args: &[String],
        cwd: &Path,
        options: &EngineOptions,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, EngineError> {
        Self::start_with_timeouts(exe, args, cwd, options, stop, SessionTimeouts::default())
    }

    /// Same as [`UciSession::start`] with explicit timeouts (tests).
    pub fn start_with_timeouts(
        exe: &Path,
        args: &[String],
        cwd: &Path,
        options: &EngineOptions,
        stop: Arc<AtomicBool>,
        timeouts: SessionTimeouts,
    ) -> Result<Self, EngineError> {
        let process = Process::spawn(exe, args, cwd)?;
        let mut session = Self {
            process,
            name: String::new(),
            options: Vec::new(),
            missing_options: Vec::new(),
            multipv: 1,
            stop,
            searchmoves_supported: None,
            timeouts,
        };
        session.process.send("uci")?;
        let deadline = Instant::now() + timeouts.uciok;
        loop {
            match session.next_event(deadline)? {
                Some(ParsedLine::UciOk) => break,
                Some(ParsedLine::Id(key, value)) => {
                    if key == "name" {
                        session.name = value;
                    }
                }
                Some(ParsedLine::Option(o)) => session.options.push(o),
                Some(_) => {}
                None => {
                    session.process.kill();
                    return Err(EngineError::Timeout("uciok"));
                }
            }
        }
        // Options, only those the engine actually advertised (§2.1).
        session.set_option_if_present("EvalFile", &options.eval_file.to_string_lossy())?;
        session.set_option_if_present("Threads", &options.threads.to_string())?;
        session.set_option_if_present("Hash", &options.hash_mb.to_string())?;
        session.set_option_if_present("Repetition Rule", "AsianRule")?;
        session.set_option_if_present("Sixty Move Rule", "true")?;
        session.set_option_if_present("Rule60MaxPly", "120")?;
        session.set_option_if_present("Ponder", "false")?;
        session.isready(timeouts.readyok)?;
        Ok(session)
    }

    /// Send one option if the engine advertised it; otherwise record it
    /// as missing and skip it (a wrong option name must not break the
    /// session).
    fn set_option_if_present(&mut self, name: &str, value: &str) -> Result<(), EngineError> {
        if EngineOption::exists(&self.options, name).is_some() {
            self.process
                .send(&format!("setoption name {name} value {value}"))?;
        } else {
            self.missing_options.push(name.to_string());
        }
        Ok(())
    }

    /// Send `isready` and wait for `readyok`.
    pub fn isready(&mut self, timeout: Duration) -> Result<(), EngineError> {
        self.check_alive()?;
        self.process.send("isready")?;
        let deadline = Instant::now() + timeout;
        loop {
            match self.next_event(deadline)? {
                Some(ParsedLine::ReadyOk) => return Ok(()),
                Some(_) => {}
                None => return Err(EngineError::Timeout("readyok")),
            }
        }
    }

    /// Start a new game: `ucinewgame` + `isready`.
    pub fn new_game(&mut self) -> Result<(), EngineError> {
        self.check_alive()?;
        self.process.send("ucinewgame")?;
        self.isready(Duration::from_secs(5))
    }

    /// Send the position: always `start_fen` + the whole move list, never
    /// just the current FEN, so the engine can detect repetition (§2.2).
    pub fn send_position(&mut self, req: &EngineRequest) -> Result<(), EngineError> {
        self.check_alive()?;
        let mut cmd = format!("position fen {}", req.start_fen);
        if !req.moves.is_empty() {
            cmd.push_str(" moves");
            for mv in &req.moves {
                cmd.push(' ');
                cmd.push_str(&iccs(*mv));
            }
        }
        self.process.send(&cmd)
    }

    /// Best move for the request, streaming progress through `on_info`.
    pub fn best_move(
        &mut self,
        req: &EngineRequest,
        limits: &SearchLimits,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Line, EngineError> {
        self.stop.store(false, Ordering::SeqCst);
        self.send_position(req)?;
        let (cmd, allowance) = go_command(limits, None, self.timeouts.bestmove_slack);
        self.process.send(&cmd)?;
        let (best, lines) = self.wait_bestmove(allowance, on_info)?;
        let Some(mv) = best else {
            return Err(EngineError::Protocol(
                "engine trả bestmove (none) ở thế còn nước".to_string(),
            ));
        };
        // The best line is multipv 1's final info; fall back to the raw
        // bestmove with an unknown score if no info arrived.
        let line = lines.get(&1).cloned().unwrap_or(Line {
            mv,
            score: Score::Cp(0),
            depth: 0,
            pv: vec![mv],
        });
        Ok(Line { mv, ..line })
    }

    /// Top-`top_n` lines, best first. Restores `MultiPV` to 1 afterwards.
    pub fn analyze(
        &mut self,
        req: &EngineRequest,
        top_n: usize,
        limits: &SearchLimits,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Vec<Line>, EngineError> {
        self.stop.store(false, Ordering::SeqCst);
        let top_n = top_n.max(1);
        if self.multipv != top_n {
            self.set_multipv(top_n)?;
        }
        let result = (|| {
            self.send_position(req)?;
            let (cmd, allowance) = go_command(limits, None, self.timeouts.bestmove_slack);
            self.process.send(&cmd)?;
            let (best, lines) = self.wait_bestmove(allowance, on_info)?;
            let _ = best;
            let mut out: Vec<Line> = lines.into_values().collect();
            out.sort_by_key(|l| std::cmp::Reverse(l.score.to_cp_clamped()));
            Ok(out)
        })();
        // Always restore MultiPV 1 (§2.3), even on error.
        if self.multipv != 1 {
            let _ = self.set_multipv(1);
        }
        result
    }

    /// Score of one specific move, from the side-to-move's perspective.
    /// Prefers `go ... searchmoves <mv>`; falls back to searching the
    /// child position and negating the score (§2.5).
    pub fn score_move(
        &mut self,
        req: &EngineRequest,
        mv: Move,
        limits: &SearchLimits,
    ) -> Result<Score, EngineError> {
        if self.searchmoves_supported.is_none() {
            self.searchmoves_supported = Some(self.probe_searchmoves()?);
        }
        if self.searchmoves_supported == Some(true) {
            self.stop.store(false, Ordering::SeqCst);
            self.send_position(req)?;
            let (cmd, allowance) = go_command(limits, Some(mv), self.timeouts.bestmove_slack);
            self.process.send(&cmd)?;
            let (best, lines) = self.wait_bestmove(allowance, &mut |_| {})?;
            let _ = best;
            return lines
                .get(&1)
                .map(|l| l.score)
                .ok_or_else(|| EngineError::Protocol("searchmoves: không có điểm".into()));
        }
        // Fallback: play the move, search the child, negate.
        let mut child_moves = req.moves.clone();
        child_moves.push(mv);
        let child_req = EngineRequest::new(req.start_fen.clone(), child_moves, req.limits);
        let line = self.best_move(&child_req, limits, &mut |_| {})?;
        Ok(line.score.negated())
    }

    /// Probe whether `go ... searchmoves` is honoured: a depth-1 search
    /// restricted to one legal move must return exactly that move.
    pub fn probe_searchmoves(&mut self) -> Result<bool, EngineError> {
        let start = xq_core::board::Position::startpos();
        let mv = xq_core::notation::parse_iccs("h2e2").expect("h2e2 parses");
        let req = EngineRequest::new(start.to_fen(), Vec::new(), SearchLimits::depth(1));
        self.stop.store(false, Ordering::SeqCst);
        self.send_position(&req)?;
        self.process.send("go depth 1 searchmoves h2e2")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let (best, _) = self.wait_bestmove_until(deadline, &mut |_| {})?;
        Ok(best == Some(mv))
    }

    /// Is the child still running?
    #[must_use]
    pub fn is_alive(&mut self) -> bool {
        self.process.is_alive()
    }

    /// Send `stop` (best effort).
    pub fn stop(&mut self) {
        let _ = self.process.send("stop");
    }

    /// Quit the engine, waiting up to 2 s before killing it.
    pub fn quit(&mut self) {
        self.process.quit(Duration::from_secs(2));
    }

    /// Use an externally owned stop flag (see `SharedEngine` in
    /// `xq-engine`): the wait loop polls this flag, so a swap must not
    /// orphan it. Only called while no search runs (the engine is
    /// behind the shared mutex during searches).
    pub fn set_stop_flag(&mut self, flag: &Arc<AtomicBool>) {
        self.stop = Arc::clone(flag);
    }

    /// Exit status if the child has finished.
    pub fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        self.process.try_wait()
    }

    // -- internals -------------------------------------------------------

    fn check_alive(&mut self) -> Result<(), EngineError> {
        if self.process.is_alive() {
            Ok(())
        } else {
            Err(EngineError::Crashed(
                self.process.try_wait().map(|s| s.code().unwrap_or(-1)),
            ))
        }
    }

    fn set_multipv(&mut self, n: usize) -> Result<(), EngineError> {
        if EngineOption::exists(&self.options, "MultiPV").is_some() {
            self.process
                .send(&format!("setoption name MultiPV value {n}"))?;
            self.isready(Duration::from_secs(5))?;
        } else {
            self.missing_options.push("MultiPV".to_string());
        }
        self.multipv = n;
        Ok(())
    }

    /// Next parsed event before `deadline`; `None` on timeout.
    /// `Err` only when the child died or the reader failed.
    fn next_event(&mut self, deadline: Instant) -> Result<Option<ParsedLine>, EngineError> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let slice = (deadline - now).min(Duration::from_millis(20));
            match self.process.recv_event_timeout(slice) {
                Some(UciEvent::Line(line)) => {
                    crate::log::log_trace(&format!("← {line}"));
                    return Ok(Some(parse_line(&line)));
                }
                Some(UciEvent::Exited) => {
                    return Err(EngineError::Crashed(
                        self.process.try_wait().map(|s| s.code().unwrap_or(-1)),
                    ));
                }
                Some(UciEvent::ReadError(e)) => {
                    return Err(EngineError::Protocol(format!("đọc stdout: {e}")));
                }
                None => {}
            }
        }
    }

    /// Wait for `bestmove` until `deadline` (absolute), streaming info.
    /// A user stop sends `stop` and gives the engine 1 s; a missed
    /// deadline sends `stop`, waits 1 s, then kills the engine.
    fn wait_bestmove_until(
        &mut self,
        deadline: Instant,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<(Option<Move>, BTreeMap<usize, Line>), EngineError> {
        let mut finals: BTreeMap<usize, Line> = BTreeMap::new();
        let mut user_stop_sent = false;
        let mut user_stop_deadline: Option<Instant> = None;
        let mut hung_stop_deadline: Option<Instant> = None;
        loop {
            // Stop requested: send `stop` once, then give the engine 1 s.
            if self.stop.load(Ordering::SeqCst) && !user_stop_sent {
                self.stop();
                user_stop_sent = true;
                user_stop_deadline = Some(Instant::now() + self.timeouts.stop_wait);
            }
            let now = Instant::now();
            // Once a stop is in flight, its grace deadline replaces the
            // search deadline (which may already have passed).
            let stop_limit = user_stop_deadline
                .into_iter()
                .chain(hung_stop_deadline)
                .min();
            let limit = stop_limit.unwrap_or(deadline);
            if now >= limit {
                if user_stop_deadline.is_some_and(|d| now >= d) {
                    return Err(EngineError::Stopped);
                }
                if hung_stop_deadline.is_some_and(|d| now >= d) {
                    self.process.kill();
                    return Err(EngineError::Timeout("bestmove"));
                }
                // Search deadline missed and no stop in flight yet:
                // `stop`, one more second, then kill.
                self.stop();
                hung_stop_deadline = Some(now + self.timeouts.stop_wait);
                continue;
            }
            match self.next_event(limit)? {
                Some(ParsedLine::Info(info)) => {
                    self.note_info(info, &mut finals, on_info);
                }
                Some(ParsedLine::BestMove(best, _)) => {
                    // A bestmove that arrives after the user asked to stop
                    // is a cancellation, not a result (§7.3): the caller
                    // must not mistake it for a completed search.
                    if user_stop_sent {
                        return Err(EngineError::Stopped);
                    }
                    return Ok((best, finals));
                }
                Some(_) => {}
                None => {}
            }
        }
    }

    /// Wait for `bestmove`, allowing `allowance` past the search's own
    /// time budget.
    fn wait_bestmove(
        &mut self,
        allowance: Duration,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<(Option<Move>, BTreeMap<usize, Line>), EngineError> {
        self.wait_bestmove_until(Instant::now() + allowance, on_info)
    }

    /// Record one final (non-bound) info line and stream progress.
    fn note_info(
        &self,
        info: InfoLine,
        finals: &mut BTreeMap<usize, Line>,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) {
        if let (Some(score), Some(&first)) = (info.score, info.pv.first()) {
            if info.bound.is_none() {
                finals.insert(
                    info.multipv,
                    Line {
                        mv: first,
                        score,
                        depth: info.depth,
                        pv: info.pv.clone(),
                    },
                );
            }
        }
        if !finals.is_empty() {
            on_info(&SearchInfo {
                depth: info.depth,
                seldepth: info.seldepth,
                nodes: info.nodes,
                nps: info.nps,
                lines: finals.values().cloned().collect(),
            });
        }
    }
}

impl Drop for UciSession {
    fn drop(&mut self) {
        self.quit();
    }
}

/// Build the `go` command for the limits (§2.3): both a depth cap and
/// a movetime cap are sent when both are set (`go depth 8 movetime
/// 1000` — the engine stops at whichever comes first); `searchmoves`
/// restricts the root moves when given. Returns the command and how
/// long `bestmove` may take past the search start.
fn go_command(
    limits: &SearchLimits,
    searchmoves: Option<Move>,
    slack: Duration,
) -> (String, Duration) {
    let mut cmd = String::from("go");
    if let Some(d) = limits.max_depth {
        cmd.push_str(&format!(" depth {d}"));
    }
    let allowance = if let Some(ms) = limits.max_time_ms {
        cmd.push_str(&format!(" movetime {ms}"));
        Duration::from_millis(ms) + slack
    } else if limits.max_depth.is_some() {
        DEPTH_SEARCH_CAP
    } else {
        cmd.push_str(" movetime 1000");
        Duration::from_millis(1000) + slack
    };
    if let Some(mv) = searchmoves {
        cmd.push_str(&format!(" searchmoves {}", iccs(mv)));
    }
    (cmd, allowance)
}
