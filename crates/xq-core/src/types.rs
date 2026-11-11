//! Core data types: colours, pieces, moves, errors.

use std::fmt;

/// Side to move / piece colour.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Color {
    Red,
    Black,
}

impl Color {
    #[must_use]
    pub fn opposite(self) -> Color {
        match self {
            Color::Red => Color::Black,
            Color::Black => Color::Red,
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Color::Red => 0,
            Color::Black => 1,
        }
    }
}

/// Kind of a piece (colour-independent).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum PieceKind {
    King,
    Advisor,
    Elephant,
    Horse,
    Rook,
    Cannon,
    Pawn,
}

impl PieceKind {
    /// FEN letter (uppercase = Red in FEN terms).
    #[must_use]
    pub fn fen_char(self) -> char {
        match self {
            PieceKind::King => 'K',
            PieceKind::Advisor => 'A',
            PieceKind::Elephant => 'B',
            PieceKind::Horse => 'N',
            PieceKind::Rook => 'R',
            PieceKind::Cannon => 'C',
            PieceKind::Pawn => 'P',
        }
    }
}

/// A coloured piece.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    #[must_use]
    pub fn new(color: Color, kind: PieceKind) -> Self {
        Self { color, kind }
    }

    /// FEN character for this piece.
    #[must_use]
    pub fn fen_char(self) -> char {
        let c = self.kind.fen_char();
        match self.color {
            Color::Red => c,
            Color::Black => c.to_ascii_lowercase(),
        }
    }

    /// Traditional Chinese character (fixed set, §7.2).
    #[must_use]
    pub fn han_char(self) -> char {
        match (self.color, self.kind) {
            (Color::Red, PieceKind::King) => '帥',
            (Color::Red, PieceKind::Advisor) => '仕',
            (Color::Red, PieceKind::Elephant) => '相',
            (Color::Red, PieceKind::Horse) => '傌',
            (Color::Red, PieceKind::Rook) => '俥',
            (Color::Red, PieceKind::Cannon) => '炮',
            (Color::Red, PieceKind::Pawn) => '兵',
            (Color::Black, PieceKind::King) => '將',
            (Color::Black, PieceKind::Advisor) => '士',
            (Color::Black, PieceKind::Elephant) => '象',
            (Color::Black, PieceKind::Horse) => '馬',
            (Color::Black, PieceKind::Rook) => '車',
            (Color::Black, PieceKind::Cannon) => '砲',
            (Color::Black, PieceKind::Pawn) => '卒',
        }
    }

    /// Simplified Chinese character (used for notation rendering).
    #[must_use]
    pub fn han_char_simplified(self) -> char {
        match (self.color, self.kind) {
            (Color::Red, PieceKind::King) => '帅',
            (Color::Red, PieceKind::Advisor) => '仕',
            (Color::Red, PieceKind::Elephant) => '相',
            (Color::Red, PieceKind::Horse) => '马',
            (Color::Red, PieceKind::Rook) => '车',
            (Color::Red, PieceKind::Cannon) => '炮',
            (Color::Red, PieceKind::Pawn) => '兵',
            (Color::Black, PieceKind::King) => '将',
            (Color::Black, PieceKind::Advisor) => '士',
            (Color::Black, PieceKind::Elephant) => '象',
            (Color::Black, PieceKind::Horse) => '马',
            (Color::Black, PieceKind::Rook) => '车',
            (Color::Black, PieceKind::Cannon) => '炮',
            (Color::Black, PieceKind::Pawn) => '卒',
        }
    }

    /// Vietnamese piece code (§3.5): Tg, S, T, M, X, P, B.
    #[must_use]
    pub fn vn_code(self) -> &'static str {
        match self.kind {
            PieceKind::King => "Tg",
            PieceKind::Advisor => "S",
            PieceKind::Elephant => "T",
            PieceKind::Horse => "M",
            PieceKind::Rook => "X",
            PieceKind::Cannon => "P",
            PieceKind::Pawn => "B",
        }
    }

    /// Vietnamese piece name.
    #[must_use]
    pub fn name_vi(self) -> &'static str {
        match self.kind {
            PieceKind::King => "Tướng",
            PieceKind::Advisor => "Sĩ",
            PieceKind::Elephant => "Tượng",
            PieceKind::Horse => "Mã",
            PieceKind::Rook => "Xe",
            PieceKind::Cannon => "Pháo",
            PieceKind::Pawn => "Tốt",
        }
    }
}

/// A move: source and destination square indices (`sq = rank * 9 + file`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Move {
    pub from: u8,
    pub to: u8,
}

impl Move {
    #[must_use]
    pub fn new(from: u8, to: u8) -> Self {
        Self { from, to }
    }
}

/// Square helpers.
#[must_use]
pub fn sq(file: i32, rank: i32) -> Option<u8> {
    if (0..9).contains(&file) && (0..10).contains(&rank) {
        Some((rank * 9 + file) as u8)
    } else {
        None
    }
}

#[must_use]
pub fn file_of(s: u8) -> i32 {
    (s % 9) as i32
}

#[must_use]
pub fn rank_of(s: u8) -> i32 {
    (s / 9) as i32
}

/// Error parsing a FEN string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FenError {
    InvalidFormat(String),
    InvalidChar(char),
    WrongRowCount,
    WrongColumnCount,
    MissingKing,
    TooManyKings,
    InvalidKingPosition,
    InvalidAdvisorPosition,
    InvalidElephantPosition,
    InvalidPawnPosition,
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FenError::InvalidFormat(s) => write!(f, "FEN không hợp lệ: {s}"),
            FenError::InvalidChar(c) => write!(f, "ký tự FEN lạ: {c}"),
            FenError::WrongRowCount => write!(f, "FEN phải có 10 hàng"),
            FenError::WrongColumnCount => write!(f, "mỗi hàng FEN phải có 9 cột"),
            FenError::MissingKing => write!(f, "thiếu Tướng"),
            FenError::TooManyKings => write!(f, "thừa Tướng"),
            FenError::InvalidKingPosition => write!(f, "Tướng ở ô không hợp lệ"),
            FenError::InvalidAdvisorPosition => write!(f, "Sĩ ở ô không hợp lệ"),
            FenError::InvalidElephantPosition => write!(f, "Tượng ở ô không hợp lệ"),
            FenError::InvalidPawnPosition => write!(f, "Tốt ở ô không hợp lệ"),
        }
    }
}

impl std::error::Error for FenError {}

/// Attempted illegal move in a [`crate::game::Game`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IllegalMove(pub Move);

impl fmt::Display for IllegalMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nước đi không hợp lệ: {}-{}", self.0.from, self.0.to)
    }
}

impl std::error::Error for IllegalMove {}
