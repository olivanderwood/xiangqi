//! A full game: history, undo, and end-of-game detection (§3.3).

use crate::board::{Position, Undo};
use crate::types::{Color, IllegalMove, Move, PieceKind};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Reason {
    Checkmate,
    Stalemate,
    PerpetualCheck,
    Repetition,
    MoveLimit,
    InsufficientMaterial,
    Resign,
    Agreement,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameResult {
    Ongoing,
    RedWins(Reason),
    BlackWins(Reason),
    Draw(Reason),
}

impl GameResult {
    #[must_use]
    pub fn is_over(self) -> bool {
        !matches!(self, GameResult::Ongoing)
    }

    /// Short Vietnamese description.
    #[must_use]
    pub fn describe_vi(self) -> &'static str {
        match self {
            GameResult::Ongoing => "Đang đánh",
            GameResult::RedWins(Reason::Checkmate) => "Đỏ thắng — chiếu hết",
            GameResult::RedWins(Reason::Stalemate) => "Đỏ thắng — Đen hết nước",
            GameResult::RedWins(Reason::PerpetualCheck) => "Đỏ thắng — Đen chiếu dai",
            GameResult::RedWins(_) => "Đỏ thắng",
            GameResult::BlackWins(Reason::Checkmate) => "Đen thắng — chiếu hết",
            GameResult::BlackWins(Reason::Stalemate) => "Đen thắng — Đỏ hết nước",
            GameResult::BlackWins(Reason::PerpetualCheck) => "Đen thắng — Đỏ chiếu dai",
            GameResult::BlackWins(_) => "Đen thắng",
            GameResult::Draw(Reason::Repetition) => "Hòa — lặp nước",
            GameResult::Draw(Reason::PerpetualCheck) => "Hòa — lặp nước",
            GameResult::Draw(Reason::MoveLimit) => "Hòa — 120 nửa nước không ăn quân",
            GameResult::Draw(Reason::InsufficientMaterial) => "Hòa — không đủ lực chiếu",
            GameResult::Draw(Reason::Agreement) => "Hòa — hai bên đồng ý",
            GameResult::Draw(_) => "Hòa",
        }
    }
}

pub struct Game {
    pub start: Position,
    pub pos: Position,
    pub moves: Vec<Move>,
    undos: Vec<Undo>,
    hashes: Vec<u64>,
    /// `checks[i]` = move `moves[i]` gave check.
    checks: Vec<bool>,
    resigned: Option<Color>,
    agreed_draw: bool,
}

impl Game {
    #[must_use]
    pub fn new(start: Position) -> Self {
        let hash = start.hash();
        Self {
            start: start.clone(),
            pos: start,
            moves: Vec::new(),
            undos: Vec::new(),
            hashes: vec![hash],
            checks: Vec::new(),
            resigned: None,
            agreed_draw: false,
        }
    }

    #[must_use]
    pub fn hashes(&self) -> &[u64] {
        &self.hashes
    }

    #[must_use]
    pub fn checks(&self) -> &[bool] {
        &self.checks
    }

    pub fn play(&mut self, m: Move) -> Result<(), IllegalMove> {
        if !self.pos.legal_moves().contains(&m) {
            return Err(IllegalMove(m));
        }
        let gives_check = self.pos.gives_check(m);
        let u = self.pos.make_move(m);
        self.moves.push(m);
        self.undos.push(u);
        self.hashes.push(self.pos.hash());
        self.checks.push(gives_check);
        Ok(())
    }

    pub fn undo(&mut self) -> Option<Move> {
        let m = self.moves.pop()?;
        let u = self.undos.pop().unwrap();
        self.pos.unmake_move(m, u);
        self.hashes.pop();
        self.checks.pop();
        Some(m)
    }

    pub fn resign(&mut self, c: Color) {
        self.resigned = Some(c);
    }

    pub fn agree_draw(&mut self) {
        self.agreed_draw = true;
    }

    /// Who (if anyone) checked on every one of their own moves inside the
    /// repetition cycle ending at the current position? Returns the
    /// perpetual-checking colour, if exactly one side qualifies.
    fn perpetual_checker(&self) -> Option<Color> {
        let cur = *self.hashes.last().unwrap();
        // Previous occurrence of the current hash.
        let prev = self.hashes[..self.hashes.len() - 1]
            .iter()
            .rposition(|&h| h == cur)?;
        // Moves in the cycle are `moves[prev..]` (hash index i corresponds
        // to the position after move i-1; move index prev..len-1).
        let cycle_moves = &self.moves[prev..];
        if cycle_moves.is_empty() {
            return None;
        }
        let mut red_checks = true;
        let mut red_has = false;
        let mut black_checks = true;
        let mut black_has = false;
        // Move i (0-based in `moves`) was made by the side to move at that
        // point: even ply from start = start side. Reconstruct from hashes
        // length: move index k was made by the side that was to move before
        // it, i.e. start side flipped k times.
        let start_side = self.start.side();
        for (i, &gave_check) in self.checks[prev..].iter().enumerate() {
            let mover = if (prev + i) % 2 == 0 {
                start_side
            } else {
                start_side.opposite()
            };
            match mover {
                Color::Red => {
                    red_has = true;
                    if !gave_check {
                        red_checks = false;
                    }
                }
                Color::Black => {
                    black_has = true;
                    if !gave_check {
                        black_checks = false;
                    }
                }
            }
        }
        match (red_has && red_checks, black_has && black_checks) {
            (true, false) => Some(Color::Red),
            (false, true) => Some(Color::Black),
            _ => None,
        }
    }

    #[must_use]
    pub fn result(&self) -> GameResult {
        if let Some(c) = self.resigned {
            return match c {
                Color::Red => GameResult::BlackWins(Reason::Resign),
                Color::Black => GameResult::RedWins(Reason::Resign),
            };
        }
        if self.agreed_draw {
            return GameResult::Draw(Reason::Agreement);
        }
        // No legal moves: checkmate or stalemate (both lose in Xiangqi).
        if self.pos.legal_moves().is_empty() {
            let winner = self.pos.side().opposite();
            let reason = if self.pos.in_check(self.pos.side()) {
                Reason::Checkmate
            } else {
                Reason::Stalemate
            };
            return match winner {
                Color::Red => GameResult::RedWins(reason),
                Color::Black => GameResult::BlackWins(reason),
            };
        }
        // Threefold repetition.
        let cur = *self.hashes.last().unwrap();
        let count = self.hashes.iter().filter(|&&h| h == cur).count();
        if count >= 3 {
            return match self.perpetual_checker() {
                Some(Color::Red) => GameResult::BlackWins(Reason::PerpetualCheck),
                Some(Color::Black) => GameResult::RedWins(Reason::PerpetualCheck),
                None => GameResult::Draw(Reason::Repetition),
            };
        }
        // 120 half-moves without a capture.
        if self.pos.halfmove_clock() >= 120 {
            return GameResult::Draw(Reason::MoveLimit);
        }
        // Insufficient material: neither side has Rook/Horse/Cannon/Pawn.
        let has_attacker = |c: Color| {
            [
                PieceKind::Rook,
                PieceKind::Horse,
                PieceKind::Cannon,
                PieceKind::Pawn,
            ]
            .iter()
            .any(|&k| self.pos.count_kind(k, Some(c)) > 0)
        };
        if !has_attacker(Color::Red) && !has_attacker(Color::Black) {
            return GameResult::Draw(Reason::InsufficientMaterial);
        }
        GameResult::Ongoing
    }
}
