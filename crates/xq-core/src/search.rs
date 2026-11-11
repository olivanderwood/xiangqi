//! Alpha-beta search (§4.2–4.5).
//!
//! Public API (kept exactly as specified in §4.1):
//! `Searcher::search / analyze / score_move / stop_handle / clear`.
//!
//! Implementation notes:
//! * Iterative deepening with a hard time deadline and a soft "don't start
//!   a new depth past 50% of the budget" rule; depth 1 always completes
//!   (a static fallback guarantees a legal move even if the stop flag was
//!   already set).
//! * PVS, transposition table (mate scores adjusted by ply on store/probe),
//!   quiescence (stand-pat + captures only, all evasions when in check,
//!   12-ply cap), check extension, null-move pruning, LMR, killers and
//!   history heuristic, repetition / perpetual-check handling (§4.2 item 10).
//! * Null-move pruning needs a position with the side flipped; `Position`
//!   has no null-move API, so the flipped position is built through a FEN
//!   round-trip (see `null_position`). The cost is one short string
//!   parse per null-move attempt.
//! * Scores are fail-soft and always inside `(-MATE, MATE)`; the search
//!   window is `[-MATE, MATE]`, so no sentinel value can leak into the TT
//!   as a fake mate score.

use crate::analysis;
use crate::board::Position;
use crate::eval::{self, MATE, PERPETUAL};
use crate::tt::{Bound, TranspositionTable};
use crate::types::{Color, Move, PieceKind};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::cmp::Reverse;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Maximum search ply (also the size of the killer table).
const MAX_PLY: usize = 128;
/// Maximum quiescence plies (§4.2 item 6).
const QPLY_MAX: i32 = 12;

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchLimits {
    pub max_depth: Option<u8>,
    pub max_time_ms: Option<u64>,
    pub max_nodes: Option<u64>,
}

impl SearchLimits {
    #[must_use]
    pub fn depth(d: u8) -> Self {
        Self {
            max_depth: Some(d),
            max_time_ms: None,
            max_nodes: None,
        }
    }
    #[must_use]
    pub fn time_ms(ms: u64) -> Self {
        Self {
            max_depth: None,
            max_time_ms: Some(ms),
            max_nodes: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub best: Move,
    pub score: i32,
    pub depth: u8,
    pub pv: Vec<Move>,
    pub nodes: u64,
}

#[derive(Clone, Debug)]
pub struct AnalysisLine {
    pub mv: Move,
    pub score: i32,
    pub depth: u8,
    pub pv: Vec<Move>,
    pub tags: Vec<MoveTag>,
}

/// Difficulty levels (§4.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Master,
}

impl Difficulty {
    #[must_use]
    pub fn max_depth(self) -> Option<u8> {
        match self {
            Difficulty::Easy => Some(2),
            Difficulty::Medium => Some(4),
            Difficulty::Hard => Some(10),
            Difficulty::Master => None,
        }
    }

    #[must_use]
    pub fn time_ms(self) -> u64 {
        match self {
            Difficulty::Easy => 300,
            Difficulty::Medium => 1000,
            Difficulty::Hard => 3000,
            Difficulty::Master => 8000,
        }
    }

    #[must_use]
    pub fn tt_mb(self) -> usize {
        match self {
            Difficulty::Master => 128,
            _ => 32,
        }
    }

    /// Limits for one move at this difficulty.
    #[must_use]
    pub fn limits(self) -> SearchLimits {
        SearchLimits {
            max_depth: self.max_depth(),
            max_time_ms: Some(self.time_ms()),
            max_nodes: None,
        }
    }

    /// Score-noise amplitude (§4.4).
    fn noise_amp(self) -> i32 {
        match self {
            Difficulty::Easy => 60,
            Difficulty::Medium => 20,
            _ => 0,
        }
    }
}

/// One entry of the search path, for repetition detection (§4.2 item 10).
#[derive(Clone, Copy, Debug)]
struct PathEntry {
    hash: u64,
    /// Whether the move leading into this position gave check.
    gave_check: bool,
}

/// Outcome of one completed iterative-deepening run.
struct IdOutcome {
    best: Move,
    score: i32,
    depth: u8,
    /// Raw (unnoised) scores of every root move, best first.
    root_scores: Vec<(Move, i32)>,
}

pub struct Searcher {
    pub tt: TranspositionTable,
    stop: Arc<AtomicBool>,
    pub nodes: u64,
    pub difficulty: Difficulty,
    // --- state for the search currently running ---
    start: Instant,
    hard_deadline: Option<Instant>,
    max_time_ms: Option<u64>,
    max_nodes_limit: Option<u64>,
    aborted: bool,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: [[i32; 90]; 90],
    path: Vec<PathEntry>,
    hist_external: Vec<u64>,
}

impl Searcher {
    #[must_use]
    pub fn new(tt_mb: usize) -> Self {
        Self {
            tt: TranspositionTable::new(tt_mb),
            stop: Arc::new(AtomicBool::new(false)),
            nodes: 0,
            difficulty: Difficulty::Medium,
            start: Instant::now(),
            hard_deadline: None,
            max_time_ms: None,
            max_nodes_limit: None,
            aborted: false,
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 90]; 90],
            path: Vec::new(),
            hist_external: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_difficulty(d: Difficulty) -> Self {
        let mut s = Self::new(d.tt_mb());
        s.difficulty = d;
        s
    }

    #[must_use]
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    pub fn clear(&mut self) {
        self.tt.clear();
        self.nodes = 0;
        self.killers = [[None; 2]; MAX_PLY];
        self.history = [[0; 90]; 90];
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    pub fn search(
        &mut self,
        pos: &Position,
        history: &[u64],
        limits: SearchLimits,
        on_info: &mut dyn FnMut(&SearchResult),
    ) -> SearchResult {
        self.stop.store(false, Ordering::Relaxed);
        let legal = pos.legal_moves();
        assert!(!legal.is_empty(), "search called on a terminal position");
        let max_depth = limits
            .max_depth
            .or_else(|| self.difficulty.max_depth())
            .unwrap_or(64) as i32;
        let max_time = limits.max_time_ms.or(Some(self.difficulty.time_ms()));
        self.begin_run(history, limits.max_nodes, max_time);
        let outcome = self.run_id(pos, max_depth, max_time, &[], Some(on_info));
        let outcome = match outcome {
            Some(o) => o,
            None => {
                // Depth 1 never completed (stop flag / tiny budget): fall
                // back to a static depth-1 pick so a legal move is always
                // returned.
                let (best, score) = self.static_root_pick(pos, &legal);
                IdOutcome {
                    best,
                    score,
                    depth: 1,
                    root_scores: vec![(best, score)],
                }
            }
        };
        let (best, score) = self.choose_root_move(pos, &outcome.root_scores);
        let pv = self.extract_pv(pos, best, outcome.depth);
        SearchResult {
            best,
            score,
            depth: outcome.depth,
            pv,
            nodes: self.nodes,
        }
    }

    /// MultiPV analysis (§4.5): one line per root move, best first, sharing
    /// one TT. Total time is `limits.max_time_ms` (default 1.5 s), split
    /// evenly over the remaining lines.
    pub fn analyze(
        &mut self,
        pos: &Position,
        history: &[u64],
        top_n: usize,
        limits: SearchLimits,
    ) -> Vec<AnalysisLine> {
        self.stop.store(false, Ordering::Relaxed);
        let legal = pos.legal_moves();
        if legal.is_empty() {
            return Vec::new();
        }
        let top_n = if top_n == 0 { 3 } else { top_n }.min(legal.len());
        let max_depth = limits
            .max_depth
            .or_else(|| self.difficulty.max_depth())
            .unwrap_or(64) as i32;
        let total_ms = limits.max_time_ms.unwrap_or(1500);
        self.begin_run(history, limits.max_nodes, Some(total_ms));
        let mut excluded: Vec<Move> = Vec::new();
        let mut lines: Vec<AnalysisLine> = Vec::new();
        for line_idx in 0..top_n {
            let elapsed = self.start.elapsed().as_millis() as u64;
            let remaining = total_ms.saturating_sub(elapsed);
            if line_idx > 0 && remaining < 30 {
                break;
            }
            let lines_left = (top_n - line_idx) as u64;
            let line_budget = (remaining / lines_left).max(30);
            let outcome = self.run_id(pos, max_depth, Some(line_budget), &excluded, None);
            let outcome = match outcome {
                Some(o) => o,
                None => {
                    let rest: Vec<Move> = legal
                        .iter()
                        .copied()
                        .filter(|m| !excluded.contains(m))
                        .collect();
                    if rest.is_empty() {
                        break;
                    }
                    let (best, score) = self.static_root_pick(pos, &rest);
                    IdOutcome {
                        best,
                        score,
                        depth: 1,
                        root_scores: vec![(best, score)],
                    }
                }
            };
            // First line's best move is the noisy best (same rule as
            // `search`, but never the random pick).
            let (best, score) = self.noisy_best(pos, &outcome.root_scores);
            let pv = self.extract_pv(pos, best, outcome.depth);
            let mut tags = analysis::tags_for(pos, best);
            if score >= MATE - 500 {
                // Mate in ceil((MATE - score) / 2) moves.
                let plies = MATE - score;
                let n = ((plies + 1) / 2).max(1).min(u8::MAX as i32) as u8;
                tags.retain(|t| !matches!(t, MoveTag::MateIn(_)));
                tags.insert(0, MoveTag::MateIn(n));
                tags.truncate(2);
            }
            lines.push(AnalysisLine {
                mv: best,
                score,
                depth: outcome.depth,
                pv,
                tags,
            });
            excluded.push(best);
            if excluded.len() >= legal.len() {
                break;
            }
        }
        lines.sort_by_key(|l| Reverse(l.score));
        lines
    }

    /// Score of one specific move at the same depth as the best move (§4.5).
    pub fn score_move(
        &mut self,
        pos: &Position,
        history: &[u64],
        mv: Move,
        limits: SearchLimits,
    ) -> i32 {
        self.stop.store(false, Ordering::Relaxed);
        let depth = limits
            .max_depth
            .or_else(|| self.difficulty.max_depth())
            .unwrap_or(4) as i32;
        let max_time = limits.max_time_ms.or(Some(1000));
        self.begin_run(history, limits.max_nodes, max_time);
        let mut child = pos.clone();
        let u = child.make_move(mv);
        let gave_check = child.in_check(child.side());
        self.path.push(PathEntry {
            hash: child.hash(),
            gave_check,
        });
        let score = -self.negamax(&child, depth - 1, -MATE, MATE, 1, true);
        self.path.pop();
        let _ = u;
        if self.aborted {
            -eval::eval(&child)
        } else {
            score
        }
    }

    // ------------------------------------------------------------------
    // Run setup / iterative deepening
    // ------------------------------------------------------------------

    fn begin_run(&mut self, history: &[u64], max_nodes: Option<u64>, max_time: Option<u64>) {
        self.start = Instant::now();
        self.max_time_ms = max_time;
        self.hard_deadline = max_time.map(|ms| self.start + Duration::from_millis(ms));
        self.max_nodes_limit = max_nodes;
        self.aborted = false;
        self.nodes = 0;
        self.hist_external = history.to_vec();
        self.path.clear();
        self.tt.age = self.tt.age.wrapping_add(1);
    }

    /// Iterative deepening from depth 1. Returns the last completed depth,
    /// or `None` if even depth 1 was aborted.
    fn run_id(
        &mut self,
        pos: &Position,
        max_depth: i32,
        max_time: Option<u64>,
        excluded: &[Move],
        mut on_info: Option<&mut dyn FnMut(&SearchResult)>,
    ) -> Option<IdOutcome> {
        // Per-run deadlines (analyze re-runs this per line with a fresh
        // slice of the total budget).
        self.start = Instant::now();
        self.hard_deadline = max_time.map(|ms| self.start + Duration::from_millis(ms));
        self.max_time_ms = max_time;
        self.aborted = false;
        self.path.clear();
        self.path.push(PathEntry {
            hash: pos.hash(),
            gave_check: false,
        });
        let mut outcome: Option<IdOutcome> = None;
        let mut depth = 1i32;
        while depth <= max_depth {
            if depth > 1 {
                if let Some(ms) = max_time {
                    if self.start.elapsed().as_millis() as u64 > ms / 2 {
                        break;
                    }
                }
            }
            if self.aborted || self.stop.load(Ordering::Relaxed) {
                break;
            }
            match self.search_root(pos, depth, excluded) {
                Some(o) => {
                    let mate_done = o.score >= MATE - o.depth as i32;
                    let result = SearchResult {
                        best: o.best,
                        score: o.score,
                        depth: o.depth,
                        pv: self.extract_pv(pos, o.best, o.depth),
                        nodes: self.nodes,
                    };
                    if let Some(cb) = on_info.as_deref_mut() {
                        cb(&result);
                    }
                    outcome = Some(o);
                    // Mate already forced in fewer plies than the next
                    // depth could improve: no point searching deeper.
                    if mate_done {
                        break;
                    }
                }
                None => break,
            }
            depth += 1;
        }
        self.path.clear();
        outcome
    }

    /// Static depth-1 fallback pick (best move by static eval).
    fn static_root_pick(&self, pos: &Position, legal: &[Move]) -> (Move, i32) {
        let mut best = legal[0];
        let mut best_score = i32::MIN;
        for &m in legal {
            let mut p = pos.clone();
            p.make_move(m);
            let s = -eval::eval(&p);
            if s > best_score {
                best_score = s;
                best = m;
            }
        }
        (best, best_score)
    }

    // ------------------------------------------------------------------
    // Root search
    // ------------------------------------------------------------------

    /// Search one depth from the root. Returns `None` if aborted.
    fn search_root(&mut self, pos: &Position, depth: i32, excluded: &[Move]) -> Option<IdOutcome> {
        let mut moves = pos.legal_moves();
        moves.retain(|m| !excluded.contains(m));
        if moves.is_empty() {
            return None;
        }
        let tt_move = self.tt.probe(pos.hash()).and_then(|e| e.mv);
        self.order_moves(pos, &mut moves, tt_move, 0);
        let mut alpha = -MATE;
        let beta = MATE;
        let mut best = moves[0];
        let mut best_score = -MATE - 1;
        let mut root_scores: Vec<(Move, i32)> = Vec::with_capacity(moves.len());
        for (idx, &m) in moves.iter().enumerate() {
            if self.aborted {
                return None;
            }
            let mut child = pos.clone();
            let u = child.make_move(m);
            let gave_check = child.in_check(child.side());
            self.path.push(PathEntry {
                hash: child.hash(),
                gave_check,
            });
            let score = if idx == 0 {
                -self.negamax(&child, depth - 1, -beta, -alpha, 1, true)
            } else {
                // No LMR at the root: a reduced-depth zero-window probe
                // is not a valid bound for the full-depth score. A late
                // quiet move whose tactic only shows at full depth (e.g.
                // e5e6 in the hanging-rook position, true score 443,
                // reduced probe <= 399) would be recorded with the
                // reduced score and never re-searched, so the root
                // ranking — and `analyze` vs `search` agreement — breaks.
                // PVS with a full-depth zero window is still sound: a
                // move that truly beats alpha fails high and is
                // re-searched with the full window.
                let mut s = -self.negamax(&child, depth - 1, -alpha - 1, -alpha, 1, false);
                if !self.aborted && s > alpha && s < beta {
                    s = -self.negamax(&child, depth - 1, -beta, -alpha, 1, true);
                }
                s
            };
            self.path.pop();
            let _ = u;
            if self.aborted {
                return None;
            }
            root_scores.push((m, score));
            if score > best_score {
                best_score = score;
                best = m;
            }
            if score > alpha {
                alpha = score;
            }
        }
        root_scores.sort_by_key(|&(_, s)| Reverse(s));
        // Store the root position in the TT (helps `extract_pv`).
        self.tt.store(
            pos.hash(),
            Some(best),
            score_to_tt(best_score, 0),
            depth.clamp(0, 255) as u8,
            Bound::Exact,
        );
        Some(IdOutcome {
            best,
            score: best_score,
            depth: depth as u8,
            root_scores,
        })
    }

    // ------------------------------------------------------------------
    // Negamax + quiescence
    // ------------------------------------------------------------------

    fn negamax(
        &mut self,
        pos: &Position,
        mut depth: i32,
        mut alpha: i32,
        mut beta: i32,
        ply: i32,
        is_pv: bool,
    ) -> i32 {
        self.nodes += 1;
        if self.nodes & 2047 == 0 {
            self.check_limits();
        }
        if self.aborted {
            return 0;
        }
        if ply as usize >= MAX_PLY {
            return eval::eval(pos);
        }
        if let Some(s) = self.repetition_score(pos) {
            return s;
        }
        let in_check = pos.in_check(pos.side());
        if in_check {
            depth += 1; // check extension (§4.2 item 7)
        }
        if depth <= 0 {
            return self.qsearch(pos, alpha, beta, ply, 0);
        }
        let hash = pos.hash();
        let alpha_orig = alpha;
        let mut tt_move = None;
        if let Some(e) = self.tt.probe(hash) {
            tt_move = e.mv;
            if !is_pv && e.depth as i32 >= depth {
                let s = score_from_tt(e.score, ply);
                match e.bound {
                    Bound::Exact => return s,
                    Bound::Lower => alpha = alpha.max(s),
                    Bound::Upper => beta = beta.min(s),
                }
                if alpha >= beta {
                    return s;
                }
            }
        }
        let mut moves = Vec::new();
        pos.legal_moves_into(&mut moves);
        if moves.is_empty() {
            return -(MATE - ply);
        }
        // Null-move pruning (§4.2 item 8).
        if !in_check && depth >= 3 && !is_pv && self.has_attackers(pos) {
            let r = if depth >= 6 { 3 } else { 2 };
            let null_pos = null_position(pos);
            self.path.push(PathEntry {
                hash: null_pos.hash(),
                gave_check: false,
            });
            let null_score =
                -self.negamax(&null_pos, depth - 1 - r, -beta, -beta + 1, ply + 1, false);
            self.path.pop();
            if self.aborted {
                return 0;
            }
            if null_score >= beta {
                return null_score;
            }
        }
        self.order_moves(pos, &mut moves, tt_move, ply as usize);
        let mut best = -MATE - 1;
        let mut best_move = None;
        for (idx, &m) in moves.iter().enumerate() {
            let is_capture = pos.piece_at(m.to).is_some();
            let mut child = pos.clone();
            let u = child.make_move(m);
            let gave_check = child.in_check(child.side());
            self.path.push(PathEntry {
                hash: child.hash(),
                gave_check,
            });
            let score = if idx == 0 {
                -self.negamax(&child, depth - 1, -beta, -alpha, ply + 1, true)
            } else {
                let mut red = depth - 1;
                if idx >= 3 && !is_capture && !gave_check && !in_check && depth >= 3 {
                    red = depth - 2; // LMR (§4.2 item 9)
                }
                let mut s = -self.negamax(&child, red, -alpha - 1, -alpha, ply + 1, false);
                if !self.aborted && s > alpha && s < beta {
                    s = -self.negamax(&child, depth - 1, -beta, -alpha, ply + 1, true);
                }
                s
            };
            self.path.pop();
            let _ = u;
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                best_move = Some(m);
            }
            if score > alpha {
                alpha = score;
            }
            if alpha >= beta {
                // Beta cutoff: record killer + history (quiet moves only).
                if !is_capture {
                    self.record_killer(ply as usize, m);
                    let h = &mut self.history[m.from as usize][m.to as usize];
                    *h = (*h + depth * depth).min(1_000_000);
                }
                self.tt.store(
                    hash,
                    best_move,
                    score_to_tt(best, ply),
                    depth.clamp(0, 255) as u8,
                    Bound::Lower,
                );
                return best;
            }
        }
        let bound = if best <= alpha_orig {
            Bound::Upper
        } else {
            Bound::Exact
        };
        self.tt.store(
            hash,
            best_move,
            score_to_tt(best, ply),
            depth.clamp(0, 255) as u8,
            bound,
        );
        best
    }

    fn qsearch(&mut self, pos: &Position, mut alpha: i32, beta: i32, ply: i32, qply: i32) -> i32 {
        self.nodes += 1;
        if self.nodes & 2047 == 0 {
            self.check_limits();
        }
        if self.aborted {
            return 0;
        }
        if let Some(s) = self.repetition_score(pos) {
            return s;
        }
        if qply >= QPLY_MAX {
            return eval::eval(pos);
        }
        let in_check = pos.in_check(pos.side());
        // Fail-soft: `best` always holds a real score (the stand-pat
        // evaluation, or the best capture/evasion found), never a bare
        // window bound. Returning a window bound here used to leak into
        // null-move pruning as a fake score.
        let mut best = -MATE - 1;
        if !in_check {
            let stand = eval::eval(pos);
            if stand >= beta {
                return stand;
            }
            best = stand;
            if stand > alpha {
                alpha = stand;
            }
        }
        let moves: Vec<Move> = if in_check {
            pos.legal_moves()
        } else {
            pos.legal_captures()
        };
        if moves.is_empty() {
            return if in_check { -(MATE - ply) } else { best };
        }
        let mut moves = moves;
        self.order_moves(pos, &mut moves, None, ply as usize);
        for &m in &moves {
            let mut child = pos.clone();
            let u = child.make_move(m);
            let gave_check = child.in_check(child.side());
            self.path.push(PathEntry {
                hash: child.hash(),
                gave_check,
            });
            let score = -self.qsearch(&child, -beta, -alpha, ply + 1, qply + 1);
            self.path.pop();
            let _ = u;
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
            }
            if score > alpha {
                alpha = score;
                if alpha >= beta {
                    return best;
                }
            }
        }
        best
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn check_limits(&mut self) {
        if self.stop.load(Ordering::Relaxed) {
            self.aborted = true;
            return;
        }
        if let Some(d) = self.hard_deadline {
            if Instant::now() >= d {
                self.aborted = true;
                return;
            }
        }
        if let Some(n) = self.max_nodes_limit {
            if self.nodes >= n {
                self.aborted = true;
            }
        }
    }

    fn record_killer(&mut self, ply: usize, m: Move) {
        if ply >= MAX_PLY {
            return;
        }
        let k = &mut self.killers[ply];
        if k[0] != Some(m) {
            k[1] = k[0];
            k[0] = Some(m);
        }
    }

    /// Move ordering: TT move -> captures (MVV-LVA) -> killers -> history.
    fn order_moves(&self, pos: &Position, moves: &mut [Move], tt_move: Option<Move>, ply: usize) {
        let killers = if ply < MAX_PLY {
            self.killers[ply]
        } else {
            [None, None]
        };
        moves.sort_by_cached_key(|&m| {
            if Some(m) == tt_move {
                return Reverse(1_000_000);
            }
            let s = if let Some(cap) = pos.piece_at(m.to) {
                // MVV-LVA: valuable victim first, cheap attacker first.
                let victim = eval::piece_value_mg(cap.kind);
                let attacker = pos
                    .piece_at(m.from)
                    .map_or(0, |p| eval::piece_value_mg(p.kind));
                10_000 + victim * 10 - attacker / 10
            } else if Some(m) == killers[0] {
                9_000
            } else if Some(m) == killers[1] {
                8_000
            } else {
                self.history[m.from as usize][m.to as usize].min(7_000)
            };
            Reverse(s)
        });
    }

    /// Null-move condition: side to move still has >= 1 rook or >= 2
    /// pieces in {Horse, Cannon}.
    fn has_attackers(&self, pos: &Position) -> bool {
        let side = pos.side();
        let mut rooks = 0;
        let mut minors = 0;
        for s in 0..90u8 {
            if let Some(p) = pos.piece_at(s) {
                if p.color == side {
                    match p.kind {
                        PieceKind::Rook => rooks += 1,
                        PieceKind::Horse | PieceKind::Cannon => minors += 1,
                        _ => {}
                    }
                }
            }
        }
        rooks >= 1 || minors >= 2
    }

    /// Repetition / perpetual-check score (§4.2 item 10). Returns `Some`
    /// when the current position has now occurred for the 3rd time in the
    /// game history + search path.
    fn repetition_score(&self, pos: &Position) -> Option<i32> {
        let hash = pos.hash();
        let in_history = self.hist_external.iter().filter(|&&h| h == hash).count();
        // Occurrences in the search path *before* the current node. The
        // current node's entry is the last one in `path` (pushed by the
        // caller before recursing); the root entry is at index 0.
        let path_before = if self.path.is_empty() {
            0
        } else {
            self.path[..self.path.len() - 1]
                .iter()
                .filter(|e| e.hash == hash)
                .count()
        };
        if in_history + path_before < 2 {
            return None;
        }
        // Find the most recent previous occurrence in the path; the cycle
        // is the segment after it. If the previous occurrence is only in
        // the game history, the checks inside the cycle are unknown, so
        // the repetition is scored as a plain draw (0).
        let prev_idx = self.path[..self.path.len() - 1]
            .iter()
            .rposition(|e| e.hash == hash)?;
        let segment = &self.path[prev_idx + 1..];
        // `segment[i]` is the position after move `i` of the cycle. The
        // mover of that move is the side to move at the cycle-start
        // entry (`prev_idx`). The side to move at path entry `k` is the
        // root side flipped `k` times; the root side follows from the
        // current side and the path length (current node is the last
        // entry, index `path.len() - 1`).
        let cur_idx = self.path.len() - 1;
        let root_side = if cur_idx.is_multiple_of(2) {
            pos.side()
        } else {
            pos.side().opposite()
        };
        let mut mover = if prev_idx.is_multiple_of(2) {
            root_side
        } else {
            root_side.opposite()
        };
        let mut red_all = true;
        let mut red_has = false;
        let mut black_all = true;
        let mut black_has = false;
        for e in segment {
            match mover {
                Color::Red => {
                    red_has = true;
                    if !e.gave_check {
                        red_all = false;
                    }
                }
                Color::Black => {
                    black_has = true;
                    if !e.gave_check {
                        black_all = false;
                    }
                }
            }
            mover = mover.opposite();
        }
        let checker = match (red_has && red_all, black_has && black_all) {
            (true, false) => Some(Color::Red),
            (false, true) => Some(Color::Black),
            _ => None,
        };
        match checker {
            None => Some(0),
            Some(c) if c == pos.side() => Some(-PERPETUAL),
            Some(_) => Some(PERPETUAL),
        }
    }

    /// Follow TT moves from `pos` (after playing `first`) to build a PV.
    fn extract_pv(&self, pos: &Position, first: Move, depth: u8) -> Vec<Move> {
        let mut pv = vec![first];
        let mut cur = pos.clone();
        cur.make_move(first);
        for _ in 1..depth {
            let Some(e) = self.tt.probe(cur.hash()) else {
                break;
            };
            let Some(m) = e.mv else { break };
            if !cur.legal_moves().contains(&m) {
                break;
            }
            pv.push(m);
            cur.make_move(m);
        }
        pv
    }

    // ------------------------------------------------------------------
    // Difficulty: deterministic noise + random pick (§4.4)
    // ------------------------------------------------------------------

    /// Deterministic noise for one root move, fixed by (position hash,
    /// move). Amplitude depends on the difficulty.
    fn root_noise(&self, pos: &Position, m: Move) -> i32 {
        let amp = self.difficulty.noise_amp();
        if amp == 0 {
            return 0;
        }
        let key =
            pos.hash() ^ ((m.from as u64) << 8 | m.to as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let mut x = key.wrapping_add(0x9E3779B97F4A7C15);
        // splitmix64, one round is enough for a small noise value.
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
        x ^= x >> 31;
        (x % (2 * amp as u64 + 1)) as i32 - amp
    }

    /// Best root move by noisy score (no random pick). Returns the raw
    /// score of the chosen move.
    fn noisy_best(&self, pos: &Position, scores: &[(Move, i32)]) -> (Move, i32) {
        let mut best = scores[0];
        let mut best_noisy = scores[0].1 + self.root_noise(pos, scores[0].0);
        for &(m, s) in &scores[1..] {
            let n = s + self.root_noise(pos, m);
            if n > best_noisy {
                best_noisy = n;
                best = (m, s);
            }
        }
        best
    }

    /// Full difficulty selection for `search`: noisy best, with the
    /// Easy/Medium random pick — except when the best move mates or any
    /// root move eats a hanging piece (§4.4: weak levels must still mate
    /// in 1 and take hanging pieces).
    ///
    /// The hanging-capture guard must look at *every* root move, not
    /// just the noisy best. Noise (±20 Medium, ±60 Easy) can promote a
    /// quiet move above the capture, and the random pick can then pick a
    /// third move: in the hanging-rook position the true scores are
    /// e5e6 443 / e2e7 413 / e2e4 400, so Medium used to pick e2e4 and
    /// leave the rook. The capture is chosen by noisy score among the
    /// hanging captures (normally there is one).
    fn choose_root_move(&self, pos: &Position, scores: &[(Move, i32)]) -> (Move, i32) {
        if scores.is_empty() {
            unreachable!("choose_root_move called with no root moves");
        }
        let (best_move, best_score) = self.noisy_best(pos, scores);
        if scores.len() < 2 {
            return (best_move, best_score);
        }
        let best_noisy = best_score + self.root_noise(pos, best_move);
        if best_score >= MATE - 500 {
            return (best_move, best_score);
        }
        let hanging = scores
            .iter()
            .copied()
            .filter(|&(m, _)| is_hanging_capture(pos, m))
            .max_by_key(|&(m, s)| s + self.root_noise(pos, m));
        if let Some((m, s)) = hanging {
            return (m, s);
        }
        let mut rng =
            StdRng::seed_from_u64(pos.hash() ^ 0xA5A5_5A5A_3C3C_1111 ^ (self.difficulty as u64));
        match self.difficulty {
            Difficulty::Easy => {
                if rng.gen::<f64>() < 0.25 {
                    let cands: Vec<(Move, i32)> = scores
                        .iter()
                        .copied()
                        .filter(|&(m, s)| s + self.root_noise(pos, m) >= best_noisy - 200)
                        .take(3)
                        .collect();
                    if cands.len() > 1 {
                        let pick = cands[rng.gen_range(0..cands.len())];
                        return pick;
                    }
                }
            }
            Difficulty::Medium if rng.gen::<f64>() < 0.10 => {
                // Second-best move (by noisy score) if within 60.
                let mut noisy: Vec<(Move, i32, i32)> = scores
                    .iter()
                    .map(|&(m, s)| (m, s, s + self.root_noise(pos, m)))
                    .collect();
                noisy.sort_by_key(|&(_, _, n)| Reverse(n));
                if noisy.len() > 1 && noisy[1].2 >= best_noisy - 60 {
                    return (noisy[1].0, noisy[1].1);
                }
            }
            _ => {}
        }
        (best_move, best_score)
    }
}

/// A capture of a hanging piece (§4.4): the victim is worth at least a
/// horse/cannon (250) and the static exchange does not lose material
/// (SEE >= 0), so the piece is undefended or defended by less.
fn is_hanging_capture(pos: &Position, m: Move) -> bool {
    match pos.piece_at(m.to) {
        Some(cap) if eval::piece_value_mg(cap.kind) >= 250 => analysis::see(pos, m) >= 0,
        _ => false,
    }
}

/// Mate-score adjustment for TT storage: make the score independent of
/// the ply it was found at (§4.1).
fn score_to_tt(score: i32, ply: i32) -> i16 {
    let s = if score > MATE - 500 {
        score + ply
    } else if score < -(MATE - 500) {
        score - ply
    } else {
        score
    };
    s.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Reverse of [`score_to_tt`] on probe.
fn score_from_tt(stored: i16, ply: i32) -> i32 {
    let s = stored as i32;
    if s > MATE - 500 {
        s - ply
    } else if s < -(MATE - 500) {
        s + ply
    } else {
        s
    }
}

/// A position identical to `pos` but with the side to move flipped (a
/// null move). Built through a FEN round-trip because `Position` exposes
/// no null-move API.
fn null_position(pos: &Position) -> Position {
    let fen = pos.to_fen();
    let mut parts: Vec<&str> = fen.split_whitespace().collect();
    if parts.len() > 1 {
        parts[1] = if parts[1] == "w" { "b" } else { "w" };
    }
    Position::from_fen(&parts.join(" ")).expect("flipped FEN is valid")
}

/// Rule-based move labels (§4.6). Computed in `analysis.rs`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MoveTag {
    Mate,
    MateIn(u8),
    Capture(PieceKind),
    Check,
    Escape(PieceKind),
    Threat(PieceKind),
    Defend(PieceKind),
    DevelopRook,
    CentralCannon,
    DevelopHorse,
    /// The piece just moved can be captured for less than its value
    /// (SEE < 0 from the mover's point of view).
    Hanging(PieceKind),
}
