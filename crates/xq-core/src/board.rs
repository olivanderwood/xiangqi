//! Board representation, FEN, move generation, make/unmake.
//!
//! Coordinates: `sq = rank * 9 + file`, file 0..8 = a..i from Red's left,
//! rank 0..9, rank 0 is Red's back rank. Red occupies ranks 0-4, Black 5-9.

use crate::types::{file_of, rank_of, sq, Color, FenError, Move, Piece, PieceKind};
use crate::zobrist;

pub const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

/// A list of moves (plain `Vec` — simple and fast enough at this scale).
pub type MoveList = Vec<Move>;

/// Undo information for one `make_move`.
#[derive(Clone, Copy, Debug)]
pub struct Undo {
    pub captured: Option<Piece>,
    pub hash: u64,
    pub halfmove_clock: u16,
}

/// A Xiangqi position.
#[derive(Clone, Debug)]
pub struct Position {
    board: [Option<Piece>; 90],
    side: Color,
    kings: [u8; 2],
    hash: u64,
    halfmove_clock: u16,
}

impl PartialEq for Position {
    fn eq(&self, other: &Self) -> bool {
        self.board == other.board
            && self.side == other.side
            && self.kings == other.kings
            && self.hash == other.hash
            && self.halfmove_clock == other.halfmove_clock
    }
}

impl Eq for Position {}

fn in_palace(color: Color, s: u8) -> bool {
    let f = file_of(s);
    let r = rank_of(s);
    (3..=5).contains(&f)
        && match color {
            Color::Red => (0..=2).contains(&r),
            Color::Black => (7..=9).contains(&r),
        }
}

fn elephant_ok(color: Color, s: u8) -> bool {
    let f = file_of(s);
    let r = rank_of(s);
    match color {
        Color::Red => r <= 4 && [0, 2, 4, 6, 8].contains(&f) && [0, 2, 4].contains(&r),
        Color::Black => r >= 5 && [0, 2, 4, 6, 8].contains(&f) && [5, 7, 9].contains(&r),
    }
}

fn advisor_ok(color: Color, s: u8) -> bool {
    let f = file_of(s);
    let r = rank_of(s);
    match color {
        Color::Red => matches!((f, r), (3, 0) | (5, 0) | (4, 1) | (3, 2) | (5, 2)),
        Color::Black => matches!((f, r), (3, 7) | (5, 7) | (4, 8) | (3, 9) | (5, 9)),
    }
}

/// Maximum pieces of one kind a side may have on the board (a legal
/// Xiangqi position can never hold more).
#[must_use]
pub fn max_count(kind: PieceKind) -> u8 {
    match kind {
        PieceKind::King => 1,
        PieceKind::Advisor
        | PieceKind::Elephant
        | PieceKind::Horse
        | PieceKind::Rook
        | PieceKind::Cannon => 2,
        PieceKind::Pawn => 5,
    }
}

/// Whether a piece of this kind and colour may stand on square `s`,
/// ignoring the other pieces: a General in the palace, an Advisor
/// on one of the palace's 5 points (4 corners + centre), an
/// Elephant on its own half on an even file, a Pawn never behind
/// its starting rank; Horse, Rook and Cannon may stand anywhere.
#[must_use]
pub fn square_ok_for(kind: PieceKind, color: Color, s: u8) -> bool {
    match kind {
        PieceKind::King => in_palace(color, s),
        PieceKind::Advisor => advisor_ok(color, s),
        PieceKind::Elephant => elephant_ok(color, s),
        PieceKind::Pawn => match color {
            Color::Red => rank_of(s) >= 3,
            Color::Black => rank_of(s) <= 6,
        },
        PieceKind::Horse | PieceKind::Rook | PieceKind::Cannon => true,
    }
}

/// Why a setup board is not a legal position (editor / recognition).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupError {
    /// One side holds more than [`max_count`] pieces of this kind.
    TooMany { kind: PieceKind, color: Color },
    /// This piece stands on a square its kind may never occupy.
    BadSquare {
        kind: PieceKind,
        color: Color,
        sq: u8,
    },
}

/// Validate a setup board (editor / photo recognition): every piece
/// on a legal square ([`square_ok_for`]) and no side over the max
/// count of any kind ([`max_count`]). Unlike [`Position::from_fen`],
/// a missing General is allowed — the board may be mid-setup — and
/// the two Generals facing each other is NOT checked here (the
/// editor checks that separately, since it needs both Generals
/// present). Returns the first problem found.
pub fn validate_setup(board: &[Option<Piece>; 90]) -> Result<(), SetupError> {
    let mut counts = [[0u8; 7]; 2];
    for (i, slot) in board.iter().enumerate() {
        let Some(piece) = slot else { continue };
        if !square_ok_for(piece.kind, piece.color, i as u8) {
            return Err(SetupError::BadSquare {
                kind: piece.kind,
                color: piece.color,
                sq: i as u8,
            });
        }
        let c = piece.color.index();
        let k = piece.kind as usize;
        counts[c][k] += 1;
        if counts[c][k] > max_count(piece.kind) {
            return Err(SetupError::TooMany {
                kind: piece.kind,
                color: piece.color,
            });
        }
    }
    Ok(())
}

fn piece_from_fen_char(c: char) -> Option<Piece> {
    let (color, kind) = match c {
        'K' => (Color::Red, PieceKind::King),
        'A' => (Color::Red, PieceKind::Advisor),
        'B' => (Color::Red, PieceKind::Elephant),
        'N' => (Color::Red, PieceKind::Horse),
        'R' => (Color::Red, PieceKind::Rook),
        'C' => (Color::Red, PieceKind::Cannon),
        'P' => (Color::Red, PieceKind::Pawn),
        'k' => (Color::Black, PieceKind::King),
        'a' => (Color::Black, PieceKind::Advisor),
        'b' => (Color::Black, PieceKind::Elephant),
        'n' => (Color::Black, PieceKind::Horse),
        'r' => (Color::Black, PieceKind::Rook),
        'c' => (Color::Black, PieceKind::Cannon),
        'p' => (Color::Black, PieceKind::Pawn),
        _ => return None,
    };
    Some(Piece::new(color, kind))
}

impl Position {
    #[must_use]
    pub fn startpos() -> Self {
        Self::from_fen(START_FEN).expect("START_FEN is valid")
    }

    pub fn from_fen(s: &str) -> Result<Self, FenError> {
        let fields: Vec<&str> = s.split_whitespace().collect();
        if fields.is_empty() {
            return Err(FenError::InvalidFormat("chuỗi FEN rỗng".to_string()));
        }
        let rows: Vec<&str> = fields[0].split('/').collect();
        if rows.len() != 10 {
            return Err(FenError::WrongRowCount);
        }
        let mut board: [Option<Piece>; 90] = [None; 90];
        let mut kings = [0u8; 2];
        let mut king_count = [0usize; 2];
        for (i, row) in rows.iter().enumerate() {
            let rank = 9 - i as i32;
            let mut file = 0i32;
            for c in row.chars() {
                if c.is_ascii_digit() {
                    file += c.to_digit(10).unwrap() as i32;
                } else if let Some(p) = piece_from_fen_char(c) {
                    if file >= 9 {
                        return Err(FenError::WrongColumnCount);
                    }
                    let s = sq(file, rank).unwrap();
                    // Position legality checks (§3.4).
                    match p.kind {
                        PieceKind::King => {
                            if !in_palace(p.color, s) {
                                return Err(FenError::InvalidKingPosition);
                            }
                            kings[p.color.index()] = s;
                            king_count[p.color.index()] += 1;
                        }
                        PieceKind::Advisor => {
                            if !advisor_ok(p.color, s) {
                                return Err(FenError::InvalidAdvisorPosition);
                            }
                        }
                        PieceKind::Elephant => {
                            if !elephant_ok(p.color, s) {
                                return Err(FenError::InvalidElephantPosition);
                            }
                        }
                        PieceKind::Pawn => {
                            let ok = match p.color {
                                Color::Red => rank >= 3,
                                Color::Black => rank <= 6,
                            };
                            if !ok {
                                return Err(FenError::InvalidPawnPosition);
                            }
                        }
                        _ => {}
                    }
                    board[s as usize] = Some(p);
                    file += 1;
                } else {
                    return Err(FenError::InvalidChar(c));
                }
            }
            if file != 9 {
                return Err(FenError::WrongColumnCount);
            }
        }
        if king_count[0] != 1 || king_count[1] != 1 {
            if king_count[0] == 0 || king_count[1] == 0 {
                return Err(FenError::MissingKing);
            }
            return Err(FenError::TooManyKings);
        }
        let side = if fields.len() > 1 {
            match fields[1] {
                "w" | "r" => Color::Red,
                "b" => Color::Black,
                other => {
                    return Err(FenError::InvalidFormat(format!(
                        "bên đi không hợp lệ: {other}"
                    )))
                }
            }
        } else {
            Color::Red
        };
        let halfmove_clock: u16 = if fields.len() > 4 {
            fields[4].parse().unwrap_or(0)
        } else {
            0
        };
        let mut pos = Self {
            board,
            side,
            kings,
            hash: 0,
            halfmove_clock,
        };
        pos.hash = zobrist::compute(&pos);
        Ok(pos)
    }

    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut out = String::new();
        for rank in (0..10).rev() {
            let mut empty = 0;
            for file in 0..9 {
                let s = sq(file, rank).unwrap();
                match self.board[s as usize] {
                    None => empty += 1,
                    Some(p) => {
                        if empty > 0 {
                            out.push_str(&empty.to_string());
                            empty = 0;
                        }
                        out.push(p.fen_char());
                    }
                }
            }
            if empty > 0 {
                out.push_str(&empty.to_string());
            }
            if rank > 0 {
                out.push('/');
            }
        }
        let side = match self.side {
            Color::Red => "w",
            Color::Black => "b",
        };
        out.push_str(&format!(" {side} - - {} 1", self.halfmove_clock));
        out
    }

    #[must_use]
    pub fn piece_at(&self, s: u8) -> Option<Piece> {
        self.board[s as usize]
    }

    /// Validate this position as a setup (editor / FEN input): every
    /// piece on a legal square and no side over the max count of any
    /// kind. [`Position::from_fen`] already guarantees the squares
    /// and exactly one General per side, so this adds the count
    /// check that `from_fen` deliberately skips (puzzles can be
    /// non-standard there — the editor is stricter).
    pub fn validate_setup(&self) -> Result<(), SetupError> {
        validate_setup(&self.board)
    }

    #[must_use]
    pub fn side(&self) -> Color {
        self.side
    }

    #[must_use]
    pub fn king_sq(&self, c: Color) -> u8 {
        self.kings[c.index()]
    }

    #[must_use]
    pub fn hash(&self) -> u64 {
        self.hash
    }

    #[must_use]
    pub fn halfmove_clock(&self) -> u16 {
        self.halfmove_clock
    }

    /// Count pieces of a kind (either colour when `color` is `None`).
    #[must_use]
    pub fn count_kind(&self, kind: PieceKind, color: Option<Color>) -> usize {
        self.board
            .iter()
            .flatten()
            .filter(|p| p.kind == kind && color.is_none_or(|c| p.color == c))
            .count()
    }

    /// Is `sq` attacked by any piece of colour `by`?
    #[must_use]
    pub fn is_attacked(&self, target: u8, by: Color) -> bool {
        let tf = file_of(target);
        let tr = rank_of(target);
        // 1. Sliding pieces (rook / cannon / flying general).
        const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for (df, dr) in DIRS {
            let mut f = tf + df;
            let mut r = tr + dr;
            let mut screen_seen = false;
            while (0..9).contains(&f) && (0..10).contains(&r) {
                let s = sq(f, r).unwrap();
                if let Some(p) = self.board[s as usize] {
                    if !screen_seen {
                        if p.color == by {
                            if p.kind == PieceKind::Rook {
                                return true;
                            }
                            // Flying general: enemy king counts as a rook
                            // vertically; orthogonal adjacency also attacks.
                            if p.kind == PieceKind::King
                                && (df == 0 || (f - tf).abs() + (r - tr).abs() == 1)
                            {
                                return true;
                            }
                        }
                        screen_seen = true;
                    } else if p.color == by && p.kind == PieceKind::Cannon {
                        return true;
                    } else {
                        // Second piece that is not an enemy cannon ends the ray.
                        break;
                    }
                    // (First piece is only a screen: keep scanning for a
                    // cannon behind it.)
                }
                f += df;
                r += dr;
            }
        }
        // 2. Horse: attacker offsets, leg = diagonal square next to target.
        const HORSE: [(i32, i32); 8] = [
            (1, 2),
            (-1, 2),
            (1, -2),
            (-1, -2),
            (2, 1),
            (-2, 1),
            (2, -1),
            (-2, -1),
        ];
        for (df, dr) in HORSE {
            let af = tf + df;
            let ar = tr + dr;
            if !(0..9).contains(&af) || !(0..10).contains(&ar) {
                continue;
            }
            let leg = sq(tf + df.signum(), tr + dr.signum());
            if let Some(leg) = leg {
                if self.board[leg as usize].is_some() {
                    continue;
                }
            }
            let s = sq(af, ar).unwrap();
            if let Some(p) = self.board[s as usize] {
                if p.color == by && p.kind == PieceKind::Horse {
                    return true;
                }
            }
        }
        // 3. Pawn: a pawn attacks the squares it can move to — straight
        // forward, plus sideways once it has crossed the river.
        // Xiangqi pawns never attack diagonally (unlike western chess):
        // a Red pawn on f6 DOES attack f7, and a crossed pawn on e6
        // DOES attack f6 — the old code only checked the two diagonal
        // squares and missed both.
        let pawn_rank = match by {
            Color::Red => tr - 1,
            Color::Black => tr + 1,
        };
        if (0..10).contains(&pawn_rank) {
            let s = sq(tf, pawn_rank).unwrap();
            if let Some(p) = self.board[s as usize] {
                if p.color == by && p.kind == PieceKind::Pawn {
                    return true;
                }
            }
        }
        // Sideways attack: only a pawn that has crossed the river can
        // move sideways, and it stands on the target's rank.
        let crossed = match by {
            Color::Red => (5..=9).contains(&tr),
            Color::Black => (0..=4).contains(&tr),
        };
        if crossed {
            for df in [-1, 1] {
                let pf = tf + df;
                if (0..9).contains(&pf) {
                    let s = sq(pf, tr).unwrap();
                    if let Some(p) = self.board[s as usize] {
                        if p.color == by && p.kind == PieceKind::Pawn {
                            return true;
                        }
                    }
                }
            }
        }
        // 4. Advisor (diagonal neighbour) and Elephant (2-diagonal, eye clear).
        for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let af = tf + df;
            let ar = tr + dr;
            if (0..9).contains(&af) && (0..10).contains(&ar) {
                let s = sq(af, ar).unwrap();
                if let Some(p) = self.board[s as usize] {
                    if p.color == by && p.kind == PieceKind::Advisor {
                        return true;
                    }
                }
            }
            let ef = tf + 2 * df;
            let er = tr + 2 * dr;
            if (0..9).contains(&ef) && (0..10).contains(&er) {
                let eye = sq(tf + df, tr + dr).unwrap();
                if self.board[eye as usize].is_none() {
                    let s = sq(ef, er).unwrap();
                    if let Some(p) = self.board[s as usize] {
                        if p.color == by && p.kind == PieceKind::Elephant {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    #[must_use]
    pub fn in_check(&self, c: Color) -> bool {
        self.is_attacked(self.kings[c.index()], c.opposite())
    }

    /// Generate pseudo-legal moves for the side to move into `out`.
    pub fn pseudo_legal_moves(&self, out: &mut MoveList) {
        out.clear();
        let side = self.side;
        for s in 0..90u8 {
            let Some(p) = self.board[s as usize] else {
                continue;
            };
            if p.color != side {
                continue;
            }
            self.gen_piece_moves(s, p, out, false);
        }
    }

    /// Generate pseudo-legal captures only.
    pub fn captures(&self, out: &mut MoveList) {
        out.clear();
        let side = self.side;
        for s in 0..90u8 {
            let Some(p) = self.board[s as usize] else {
                continue;
            };
            if p.color != side {
                continue;
            }
            self.gen_piece_moves(s, p, out, true);
        }
    }

    fn gen_piece_moves(&self, from: u8, p: Piece, out: &mut MoveList, captures_only: bool) {
        let f = file_of(from);
        let r = rank_of(from);
        let side = p.color;
        let push = |to: u8, out: &mut MoveList| match self.board[to as usize] {
            None => {
                if !captures_only {
                    out.push(Move::new(from, to));
                }
            }
            Some(t) => {
                // Capturing the enemy king is never a legal move.
                if t.color != side && t.kind != PieceKind::King {
                    out.push(Move::new(from, to));
                }
            }
        };
        match p.kind {
            PieceKind::King => {
                for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some(to) = sq(f + df, r + dr) {
                        if in_palace(side, to) {
                            push(to, out);
                        }
                    }
                }
            }
            PieceKind::Advisor => {
                for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    if let Some(to) = sq(f + df, r + dr) {
                        if advisor_ok(side, to) {
                            push(to, out);
                        }
                    }
                }
            }
            PieceKind::Elephant => {
                for (df, dr) in [(2, 2), (2, -2), (-2, 2), (-2, -2)] {
                    if let Some(to) = sq(f + df, r + dr) {
                        if elephant_ok(side, to) {
                            let eye = sq(f + df / 2, r + dr / 2).unwrap();
                            if self.board[eye as usize].is_none() {
                                push(to, out);
                            }
                        }
                    }
                }
            }
            PieceKind::Horse => {
                // (straight step, diagonal step)
                const MOVES: [((i32, i32), (i32, i32)); 8] = [
                    ((0, 1), (1, 1)),
                    ((0, 1), (-1, 1)),
                    ((0, -1), (1, -1)),
                    ((0, -1), (-1, -1)),
                    ((1, 0), (1, 1)),
                    ((1, 0), (1, -1)),
                    ((-1, 0), (-1, 1)),
                    ((-1, 0), (-1, -1)),
                ];
                for ((lf, lr), (df, dr)) in MOVES {
                    let leg = sq(f + lf, r + lr);
                    if let Some(leg) = leg {
                        if self.board[leg as usize].is_some() {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if let Some(to) = sq(f + lf + df, r + lr + dr) {
                        push(to, out);
                    }
                }
            }
            PieceKind::Rook | PieceKind::Cannon => {
                for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let mut nf = f + df;
                    let mut nr = r + dr;
                    let mut screen = false;
                    while let Some(to) = sq(nf, nr) {
                        match self.board[to as usize] {
                            None => {
                                if p.kind == PieceKind::Rook {
                                    if !captures_only {
                                        out.push(Move::new(from, to));
                                    }
                                } else if !screen && !captures_only {
                                    out.push(Move::new(from, to));
                                }
                            }
                            Some(t) => {
                                if p.kind == PieceKind::Rook {
                                    if t.color != side && t.kind != PieceKind::King {
                                        out.push(Move::new(from, to));
                                    }
                                    break;
                                }
                                // Cannon.
                                if !screen {
                                    screen = true;
                                } else {
                                    if t.color != side && t.kind != PieceKind::King {
                                        out.push(Move::new(from, to));
                                    }
                                    break;
                                }
                            }
                        }
                        nf += df;
                        nr += dr;
                    }
                }
            }
            PieceKind::Pawn => {
                let fwd = match side {
                    Color::Red => 1,
                    Color::Black => -1,
                };
                let crossed = match side {
                    Color::Red => r >= 5,
                    Color::Black => r <= 4,
                };
                if let Some(to) = sq(f, r + fwd) {
                    push(to, out);
                }
                if crossed {
                    for df in [-1, 1] {
                        if let Some(to) = sq(f + df, r) {
                            push(to, out);
                        }
                    }
                }
            }
        }
    }

    /// All legal moves (pseudo-legal filtered by king safety).
    #[must_use]
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut out = Vec::new();
        self.legal_moves_into(&mut out);
        out
    }

    /// Legal moves appended to `out` (make/unmake in place, no per-move clone).
    pub fn legal_moves_into(&self, out: &mut Vec<Move>) {
        let mut pseudo = MoveList::new();
        self.pseudo_legal_moves(&mut pseudo);
        let side = self.side;
        let mut work = self.clone();
        for m in pseudo {
            let u = work.make_move(m);
            // The mover's king must not be attacked after the move.
            if !work.is_attacked(work.kings[side.index()], side.opposite()) {
                out.push(m);
            }
            work.unmake_move(m, u);
        }
    }

    /// Legal captures only (for quiescence).
    #[must_use]
    pub fn legal_captures(&self) -> Vec<Move> {
        let mut pseudo = MoveList::new();
        self.captures(&mut pseudo);
        let side = self.side;
        let mut work = self.clone();
        let mut out = Vec::new();
        for m in pseudo {
            let u = work.make_move(m);
            if !work.is_attacked(work.kings[side.index()], side.opposite()) {
                out.push(m);
            }
            work.unmake_move(m, u);
        }
        out
    }

    pub fn make_move(&mut self, m: Move) -> Undo {
        let undo = Undo {
            captured: self.board[m.to as usize],
            hash: self.hash,
            halfmove_clock: self.halfmove_clock,
        };
        let piece = self.board[m.from as usize].expect("moving piece exists");
        // Hash: remove piece from `from`, remove captured, add piece at `to`.
        self.hash ^= zobrist::piece_key(piece.color, piece.kind, m.from);
        if let Some(cap) = undo.captured {
            self.hash ^= zobrist::piece_key(cap.color, cap.kind, m.to);
        }
        self.hash ^= zobrist::piece_key(piece.color, piece.kind, m.to);
        self.hash ^= zobrist::side_key();
        self.board[m.from as usize] = None;
        self.board[m.to as usize] = Some(piece);
        if piece.kind == PieceKind::King {
            self.kings[piece.color.index()] = m.to;
        }
        self.halfmove_clock = if undo.captured.is_some() {
            0
        } else {
            self.halfmove_clock + 1
        };
        self.side = self.side.opposite();
        undo
    }

    pub fn unmake_move(&mut self, m: Move, u: Undo) {
        self.side = self.side.opposite();
        let piece = self.board[m.to as usize].expect("moved piece exists");
        self.board[m.to as usize] = u.captured;
        self.board[m.from as usize] = Some(piece);
        if piece.kind == PieceKind::King {
            self.kings[piece.color.index()] = m.from;
        }
        self.hash = u.hash;
        self.halfmove_clock = u.halfmove_clock;
    }

    /// Does `m` give check? (Assumes `m` is legal.)
    #[must_use]
    pub fn gives_check(&self, m: Move) -> bool {
        let mut copy = self.clone();
        copy.make_move(m);
        copy.in_check(copy.side)
    }
}
