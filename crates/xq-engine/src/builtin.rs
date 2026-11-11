//! [`BuiltinEngine`]: the hand-written alpha-beta searcher wrapped in the
//! [`Engine`](crate::Engine) trait, kept as the fallback when Pikafish is
//! unavailable (integration spec §1.2).

use crate::{
    Engine, EngineError, EngineRequest, GradeThresholds, Level, Line, Score, SearchInfo, StopHandle,
};
use std::sync::{atomic::AtomicBool, Arc};
use xq_core::search::{Difficulty, Searcher};
use xq_core::types::Move;

/// Map a shared [`Level`] to the classic builtin difficulty.
fn to_difficulty(level: Level) -> Difficulty {
    match level {
        Level::Easy => Difficulty::Easy,
        Level::Medium => Difficulty::Medium,
        Level::Hard => Difficulty::Hard,
        Level::Master => Difficulty::Master,
        Level::Elo(x) => {
            if x < 1600 {
                Difficulty::Easy
            } else if x < 2100 {
                Difficulty::Medium
            } else if x < 2600 {
                Difficulty::Hard
            } else {
                Difficulty::Master
            }
        }
    }
}

pub struct BuiltinEngine {
    searcher: Searcher,
    level: Level,
    /// The shared stop flag (see `SharedEngine`); the searcher polls
    /// this, not a flag of its own, so a swap keeps stopping working.
    stop: Arc<AtomicBool>,
}

impl BuiltinEngine {
    #[must_use]
    pub fn new() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let mut searcher = Searcher::with_difficulty(Difficulty::Medium);
        searcher.set_stop_flag(Arc::clone(&stop));
        Self {
            searcher,
            level: Level::Medium,
            stop,
        }
    }

    #[must_use]
    pub fn with_level(level: Level) -> Self {
        let mut e = Self::new();
        let _ = e.set_level(level);
        e
    }

    fn searcher(&mut self) -> &mut Searcher {
        &mut self.searcher
    }
}

impl Default for BuiltinEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for BuiltinEngine {
    fn name(&self) -> &str {
        "Builtin"
    }

    fn new_game(&mut self) -> Result<(), EngineError> {
        self.searcher.clear();
        Ok(())
    }

    fn set_level(&mut self, level: Level) -> Result<(), EngineError> {
        self.level = level;
        // Rebuild so the TT size matches the difficulty (Master: 128 MB).
        self.searcher = Searcher::with_difficulty(to_difficulty(level));
        // The rebuild created a fresh flag; re-apply the shared one.
        self.searcher.set_stop_flag(Arc::clone(&self.stop));
        Ok(())
    }

    fn best_move(
        &mut self,
        req: &EngineRequest,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Line, EngineError> {
        let (pos, history) = req.position_with_history()?;
        if pos.legal_moves().is_empty() {
            return Err(EngineError::Protocol(
                "best_move gọi ở thế hết nước".to_string(),
            ));
        }
        let result = self
            .searcher()
            .search(&pos, &history, req.limits, &mut |sr| {
                on_info(&SearchInfo {
                    depth: sr.depth,
                    seldepth: sr.depth,
                    nodes: sr.nodes,
                    nps: 0,
                    lines: vec![Line {
                        mv: sr.best,
                        score: Score::from_cp(sr.score),
                        depth: sr.depth,
                        pv: sr.pv.clone(),
                    }],
                });
            });
        Ok(Line {
            mv: result.best,
            score: Score::from_cp(result.score),
            depth: result.depth,
            pv: result.pv,
        })
    }

    fn analyze(
        &mut self,
        req: &EngineRequest,
        top_n: usize,
        on_info: &mut dyn FnMut(&SearchInfo),
    ) -> Result<Vec<Line>, EngineError> {
        let (pos, history) = req.position_with_history()?;
        let lines = self
            .searcher()
            .analyze(&pos, &history, top_n, req.limits)
            .into_iter()
            .map(|l| Line {
                mv: l.mv,
                score: Score::from_cp(l.score),
                depth: l.depth,
                pv: l.pv,
            })
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            let depth = lines.iter().map(|l| l.depth).max().unwrap_or(0);
            on_info(&SearchInfo {
                depth,
                seldepth: depth,
                nodes: self.searcher.nodes,
                nps: 0,
                lines: lines.clone(),
            });
        }
        Ok(lines)
    }

    fn score_move(&mut self, req: &EngineRequest, mv: Move) -> Result<Score, EngineError> {
        let (pos, history) = req.position_with_history()?;
        if !pos.legal_moves().contains(&mv) {
            return Err(EngineError::IllegalMove(format!(
                "nước {} không hợp lệ",
                xq_core::notation::iccs(mv)
            )));
        }
        let score = self.searcher().score_move(&pos, &history, mv, req.limits);
        Ok(Score::from_cp(score))
    }

    fn stop_handle(&self) -> StopHandle {
        StopHandle::from_flag(Arc::clone(&self.stop))
    }

    fn set_stop_flag(&mut self, flag: &Arc<AtomicBool>) {
        self.stop = Arc::clone(flag);
        self.searcher.set_stop_flag(Arc::clone(flag));
    }

    fn level(&self) -> Level {
        self.level
    }

    fn grade_thresholds(&self) -> GradeThresholds {
        GradeThresholds::BUILTIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SearchLimits;
    use xq_core::board::Position;

    fn startpos_req(limits: SearchLimits) -> EngineRequest {
        EngineRequest::new(Position::startpos().to_fen(), Vec::new(), limits)
    }

    #[test]
    fn best_move_matches_searcher() {
        let mut engine = BuiltinEngine::new();
        let mut searcher = Searcher::with_difficulty(Difficulty::Medium);
        let limits = SearchLimits::depth(3);
        let req = startpos_req(limits);
        let (pos, history) = req.position_with_history().unwrap();
        let mut infos = 0;
        let line = engine
            .best_move(&req, &mut |_| infos += 1)
            .expect("best_move");
        let expected = searcher.search(&pos, &history, limits, &mut |_| {});
        assert_eq!(line.mv, expected.best);
        assert_eq!(line.score, Score::from_cp(expected.score));
        assert_eq!(line.depth, expected.depth);
        assert!(infos > 0, "on_info should be called during search");
    }

    #[test]
    fn analyze_matches_searcher() {
        let mut engine = BuiltinEngine::new();
        let mut searcher = Searcher::with_difficulty(Difficulty::Medium);
        let limits = SearchLimits::depth(3);
        let req = startpos_req(limits);
        let (pos, history) = req.position_with_history().unwrap();
        let lines = engine.analyze(&req, 3, &mut |_| {}).expect("analyze");
        let expected = searcher.analyze(&pos, &history, 3, limits);
        assert_eq!(lines.len(), expected.len());
        for (got, want) in lines.iter().zip(expected.iter()) {
            assert_eq!(got.mv, want.mv);
            assert_eq!(got.score, Score::from_cp(want.score));
        }
        // Best first.
        for w in lines.windows(2) {
            assert!(w[0].score.to_cp_clamped() >= w[1].score.to_cp_clamped());
        }
    }

    #[test]
    fn score_move_matches_searcher() {
        let mut engine = BuiltinEngine::new();
        let mut searcher = Searcher::with_difficulty(Difficulty::Medium);
        let limits = SearchLimits::depth(3);
        let req = startpos_req(limits);
        let (pos, history) = req.position_with_history().unwrap();
        let mv = xq_core::notation::parse_iccs("h2e2").unwrap();
        let got = engine.score_move(&req, mv).expect("score_move");
        let want = searcher.score_move(&pos, &history, mv, limits);
        assert_eq!(got, Score::from_cp(want));
    }

    #[test]
    fn score_move_illegal_is_error() {
        let mut engine = BuiltinEngine::new();
        let req = startpos_req(SearchLimits::depth(2));
        // a1a2 starts from an empty square: not a legal move from startpos.
        let mv = xq_core::notation::parse_iccs("a1a2").unwrap();
        assert!(matches!(
            engine.score_move(&req, mv),
            Err(EngineError::IllegalMove(_))
        ));
    }

    #[test]
    fn request_replays_moves_and_history() {
        let mut engine = BuiltinEngine::new();
        let h2e2 = xq_core::notation::parse_iccs("h2e2").unwrap();
        let h9g7 = xq_core::notation::parse_iccs("h9g7").unwrap();
        let req = EngineRequest::new(
            Position::startpos().to_fen(),
            vec![h2e2, h9g7],
            SearchLimits::depth(2),
        );
        let (pos, history) = req.position_with_history().unwrap();
        assert_eq!(history.len(), 3, "start + 2 moves");
        assert_eq!(pos.side(), xq_core::types::Color::Red);
        let line = engine.best_move(&req, &mut |_| {}).expect("best_move");
        let mut check = Position::startpos();
        check.make_move(h2e2);
        check.make_move(h9g7);
        assert!(check.legal_moves().contains(&line.mv));
    }

    #[test]
    fn score_mate_mapping() {
        assert_eq!(Score::from_cp(30000 - 4), Score::Mate(2));
        assert_eq!(Score::from_cp(-(30000 - 4)), Score::Mate(-2));
        assert_eq!(Score::from_cp(35), Score::Cp(35));
        assert_eq!(Score::Mate(3).to_cp_clamped(), 30000 - 3);
        assert_eq!(Score::Mate(-3).to_cp_clamped(), -(30000 - 3));
        assert_eq!(Score::Cp(40).negated(), Score::Cp(-40));
        assert_eq!(Score::Mate(2).negated(), Score::Mate(-2));
    }

    #[test]
    fn level_mapping() {
        assert_eq!(Level::Elo(1400).builtin_limits().max_depth, Some(2));
        assert_eq!(Level::Elo(1900).builtin_limits().max_depth, Some(4));
        assert_eq!(Level::Elo(2400).builtin_limits().max_depth, Some(10));
        assert_eq!(Level::Master.builtin_limits().max_depth, None);
    }

    #[test]
    fn stop_flag_survives_engine_swap() {
        // The stop flag lives in the SharedEngine, not in the engine:
        // after swapping the engine behind the handle, the ORIGINAL
        // handle must still stop the NEW engine's searches. (The flag
        // is reset at the start of each search, so stop it mid-search.)
        let shared = crate::shared_engine(Box::new(BuiltinEngine::new()));
        let first_handle = shared.stop_handle();
        assert!(!first_handle.is_stopped());
        {
            let mut guard = shared.lock().expect("engine lock");
            *guard = Box::new(BuiltinEngine::with_level(Level::Hard));
            guard.set_stop_flag(&shared.stop_flag());
        }
        // The new engine's own handle is the same flag.
        assert!(!shared
            .lock()
            .expect("engine lock")
            .stop_handle()
            .is_stopped());
        let shared2 = shared.clone();
        let worker = std::thread::spawn(move || {
            let mut engine = shared2.lock().expect("engine lock");
            let req = startpos_req(SearchLimits::time_ms(30_000));
            engine.best_move(&req, &mut |_| {})
        });
        std::thread::sleep(std::time::Duration::from_millis(200));
        let t0 = std::time::Instant::now();
        first_handle.stop();
        let _ = worker.join().expect("search thread");
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(5),
            "tìm kiếm phải dừng nhanh khi cờ stop đã đặt giữa chừng, mất {:?}",
            t0.elapsed()
        );
    }
}
