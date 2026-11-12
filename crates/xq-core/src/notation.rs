//! Move notation: ICCS, Chinese (TQ), Vietnamese (VN) and WXF (§3.5).
//!
//! Rendering is the single source of truth: parsing is done by rendering
//! every legal move in the requested form and matching the (normalised)
//! input string, so `parse(render(m)) == m` holds by construction and
//! ambiguous input is rejected instead of guessed.

use crate::board::Position;
use crate::types::{file_of, rank_of, Color, Move, PieceKind};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotationError {
    InvalidFormat(String),
    NoSuchMove(String),
    Ambiguous(String),
}

impl fmt::Display for NotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotationError::InvalidFormat(s) => write!(f, "ký hiệu nước đi sai định dạng: {s}"),
            NotationError::NoSuchMove(s) => write!(f, "không có nước hợp lệ khớp: {s}"),
            NotationError::Ambiguous(s) => write!(f, "ký hiệu mơ hồ, khớp nhiều nước: {s}"),
        }
    }
}

impl std::error::Error for NotationError {}

// ---------------------------------------------------------------- ICCS

/// `h2e2` — file letter a..i + rank digit (rank index 0..9).
#[must_use]
pub fn iccs(m: Move) -> String {
    let f = |s: u8| (b'a' + file_of(s) as u8) as char;
    format!(
        "{}{}{}{}",
        f(m.from),
        rank_of(m.from),
        f(m.to),
        rank_of(m.to)
    )
}

pub fn parse_iccs(s: &str) -> Result<Move, NotationError> {
    let b = s.as_bytes();
    if b.len() != 4 || !b[0].is_ascii_lowercase() || !b[2].is_ascii_lowercase() {
        return Err(NotationError::InvalidFormat(s.to_string()));
    }
    let file = |c: u8| -> Result<i32, NotationError> {
        if (b'a'..=b'i').contains(&c) {
            Ok((c - b'a') as i32)
        } else {
            Err(NotationError::InvalidFormat(s.to_string()))
        }
    };
    let rank = |c: u8| -> Result<i32, NotationError> {
        if c.is_ascii_digit() {
            Ok((c - b'0') as i32)
        } else {
            Err(NotationError::InvalidFormat(s.to_string()))
        }
    };
    let from = file(b[0])? + rank(b[1])? * 9;
    let to = file(b[2])? + rank(b[3])? * 9;
    Ok(Move::new(from as u8, to as u8))
}

// ------------------------------------------------------------ shared bits

const CN_NUM: [char; 9] = ['一', '二', '三', '四', '五', '六', '七', '八', '九'];

/// Column number as seen by the side to move: Red 9-file, Black file+1.
fn col_num(file: i32, side: Color) -> i32 {
    match side {
        Color::Red => 9 - file,
        Color::Black => file + 1,
    }
}

fn is_straight(kind: PieceKind) -> bool {
    matches!(
        kind,
        PieceKind::King | PieceKind::Rook | PieceKind::Cannon | PieceKind::Pawn
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Forward,
    Back,
    Sideways,
}

fn action_of(pos: &Position, m: Move) -> Action {
    let side = pos.piece_at(m.from).map(|p| p.color).unwrap_or(Color::Red);
    let dr = rank_of(m.to) - rank_of(m.from);
    match side {
        Color::Red => {
            if dr > 0 {
                Action::Forward
            } else if dr < 0 {
                Action::Back
            } else {
                Action::Sideways
            }
        }
        Color::Black => {
            if dr < 0 {
                Action::Forward
            } else if dr > 0 {
                Action::Back
            } else {
                Action::Sideways
            }
        }
    }
}

/// Pieces of the same kind+colour on the same file as `from`, ordered
/// front-first (front = closest to the enemy).
fn same_file_group(pos: &Position, m: Move) -> Vec<u8> {
    let piece = pos.piece_at(m.from).expect("moving piece");
    let f = file_of(m.from);
    let mut group: Vec<u8> = (0..90u8)
        .filter(|&s| file_of(s) == f && pos.piece_at(s) == Some(piece))
        .collect();
    group.sort_by_key(|&s| match piece.color {
        Color::Red => -rank_of(s),
        Color::Black => rank_of(s),
    });
    group
}

/// Is there another file that also holds >= 2 pieces of the same kind+colour?
fn has_other_stack(pos: &Position, m: Move) -> bool {
    let piece = pos.piece_at(m.from).expect("moving piece");
    let f = file_of(m.from);
    for other_file in 0..9 {
        if other_file == f {
            continue;
        }
        let n = (0..90u8)
            .filter(|&s| file_of(s) == other_file && pos.piece_at(s) == Some(piece))
            .count();
        if n >= 2 {
            return true;
        }
    }
    false
}

fn num_cn(n: i32, side: Color) -> String {
    if side == Color::Red {
        CN_NUM[(n - 1) as usize].to_string()
    } else {
        n.to_string()
    }
}

// ------------------------------------------------------------ Chinese (TQ)

/// Render in simplified Chinese notation, e.g. `炮二平五`, `马八进七`,
/// `前车进二`, `前三进一`.
#[must_use]
pub fn render_cn(pos: &Position, m: Move) -> String {
    let piece = pos.piece_at(m.from).expect("moving piece");
    let side = piece.color;
    let group = same_file_group(pos, m);
    let from_col = col_num(file_of(m.from), side);
    let dest_col = col_num(file_of(m.to), side);
    let prefix = if group.len() == 1 {
        num_cn(from_col, side)
    } else {
        let idx = group.iter().position(|&s| s == m.from).unwrap();
        let front_back: String = match group.len() {
            2 => match idx {
                0 => "前".to_string(),
                _ => "后".to_string(),
            },
            3 => match idx {
                0 => "前".to_string(),
                1 => "中".to_string(),
                _ => "后".to_string(),
            },
            _ => CN_NUM[idx].to_string(),
        };
        if has_other_stack(pos, m) {
            // Two stacked files: 前/后 + column, piece char omitted
            // (spec §3.5 example: 前三进一).
            return format!("{}{}{}", front_back, num_cn(from_col, side), {
                let action = match action_of(pos, m) {
                    Action::Forward => "进",
                    Action::Back => "退",
                    Action::Sideways => "平",
                };
                let suffix = if is_straight(piece.kind) && action_of(pos, m) != Action::Sideways {
                    let steps = (rank_of(m.to) - rank_of(m.from)).abs();
                    num_cn(steps, side)
                } else {
                    num_cn(dest_col, side)
                };
                format!("{action}{suffix}")
            });
        }
        // Single stack: 前/后 comes *before* the piece char: 前车进二.
        return format!(
            "{}{}{}{}",
            front_back,
            piece.han_char_simplified(),
            match action_of(pos, m) {
                Action::Forward => "进",
                Action::Back => "退",
                Action::Sideways => "平",
            },
            if is_straight(piece.kind) && action_of(pos, m) != Action::Sideways {
                num_cn((rank_of(m.to) - rank_of(m.from)).abs(), side)
            } else {
                num_cn(dest_col, side)
            }
        );
    };
    let action = match action_of(pos, m) {
        Action::Forward => "进",
        Action::Back => "退",
        Action::Sideways => "平",
    };
    let suffix = if is_straight(piece.kind) && action_of(pos, m) != Action::Sideways {
        let steps = (rank_of(m.to) - rank_of(m.from)).abs();
        num_cn(steps, side)
    } else {
        num_cn(dest_col, side)
    };
    format!(
        "{}{}{}{}",
        piece.han_char_simplified(),
        prefix,
        action,
        suffix
    )
}

fn normalize_cn(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '車' | '俥' => '车',
            '馬' | '傌' => '马',
            '砲' => '炮',
            '將' => '将',
            '帥' => '帅',
            '進' => '进',
            '１' => '1',
            '２' => '2',
            '３' => '3',
            '４' => '4',
            '５' => '5',
            '６' => '6',
            '７' => '7',
            '８' => '8',
            '９' => '9',
            _ => c,
        })
        .collect()
}

pub fn parse_cn(pos: &Position, s: &str) -> Result<Move, NotationError> {
    let want = normalize_cn(s.trim());
    let matches: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|&m| render_cn(pos, m) == want)
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(NotationError::NoSuchMove(s.to_string())),
        _ => Err(NotationError::Ambiguous(s.to_string())),
    }
}

// ------------------------------------------------------------ Vietnamese

/// Render in Vietnamese notation, e.g. `P2-5`, `M8.7`, `Xt.2`.
#[must_use]
pub fn render_vn(pos: &Position, m: Move) -> String {
    let piece = pos.piece_at(m.from).expect("moving piece");
    let side = piece.color;
    let group = same_file_group(pos, m);
    let from_col = col_num(file_of(m.from), side);
    let dest_col = col_num(file_of(m.to), side);
    let prefix = if group.len() == 1 {
        from_col.to_string()
    } else {
        let idx = group.iter().position(|&s| s == m.from).unwrap();
        let fb: String = match group.len() {
            2 => match idx {
                0 => "t".to_string(),
                _ => "s".to_string(),
            },
            3 => match idx {
                0 => "t".to_string(),
                1 => "g".to_string(),
                _ => "s".to_string(),
            },
            _ => (idx + 1).to_string(),
        };
        if has_other_stack(pos, m) {
            format!("{fb}{from_col}")
        } else {
            fb.to_string()
        }
    };
    let act = action_of(pos, m);
    let action = match act {
        Action::Forward => ".",
        Action::Back => "/",
        Action::Sideways => "-",
    };
    let suffix = if is_straight(piece.kind) && act != Action::Sideways {
        (rank_of(m.to) - rank_of(m.from)).abs().to_string()
    } else {
        dest_col.to_string()
    };
    format!("{}{}{}{}", piece.vn_code(), prefix, action, suffix)
}

pub fn parse_vn(pos: &Position, s: &str) -> Result<Move, NotationError> {
    let want = s.trim().to_string();
    let matches: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|&m| render_vn(pos, m) == want)
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(NotationError::NoSuchMove(s.to_string())),
        _ => Err(NotationError::Ambiguous(s.to_string())),
    }
}

// ------------------------------------------------------------ WXF

fn wxf_piece(kind: PieceKind) -> char {
    match kind {
        PieceKind::King => 'K',
        PieceKind::Advisor => 'A',
        PieceKind::Elephant => 'E',
        PieceKind::Horse => 'H',
        PieceKind::Rook => 'R',
        PieceKind::Cannon => 'C',
        PieceKind::Pawn => 'P',
    }
}

/// Render in WXF notation, e.g. `C2=5`, `H2+3`, `R1+1`.
#[must_use]
pub fn render_wxf(pos: &Position, m: Move) -> String {
    let piece = pos.piece_at(m.from).expect("moving piece");
    let side = piece.color;
    let group = same_file_group(pos, m);
    let from_col = col_num(file_of(m.from), side);
    let dest_col = col_num(file_of(m.to), side);
    let prefix = if group.len() == 1 {
        from_col.to_string()
    } else {
        let idx = group.iter().position(|&s| s == m.from).unwrap();
        let fb: String = if idx == 0 {
            "F".to_string()
        } else if idx == group.len() - 1 {
            "B".to_string()
        } else {
            (idx + 1).to_string()
        };
        if has_other_stack(pos, m) {
            format!("{fb}{from_col}")
        } else {
            fb
        }
    };
    let act = action_of(pos, m);
    let action = match act {
        Action::Forward => "+",
        Action::Back => "-",
        Action::Sideways => "=",
    };
    let suffix = if is_straight(piece.kind) && act != Action::Sideways {
        (rank_of(m.to) - rank_of(m.from)).abs().to_string()
    } else {
        dest_col.to_string()
    };
    format!("{}{}{}{}", wxf_piece(piece.kind), prefix, action, suffix)
}

/// Render in English (WXF) notation, e.g. `C2=5`, `H2+3`, `R1+1`
/// (Hieu, 2026-11-12: move names are Vietnamese or English; Chinese
/// characters appear only on the board pieces). Same as `render_wxf`.
#[must_use]
pub fn render_en(pos: &Position, m: Move) -> String {
    render_wxf(pos, m)
}

/// Parse an English (WXF) move string. Same as `parse_wxf`.
pub fn parse_en(pos: &Position, s: &str) -> Result<Move, NotationError> {
    parse_wxf(pos, s)
}

pub fn parse_wxf(pos: &Position, s: &str) -> Result<Move, NotationError> {
    let want = s.trim().to_string();
    let matches: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|&m| render_wxf(pos, m) == want)
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(NotationError::NoSuchMove(s.to_string())),
        _ => Err(NotationError::Ambiguous(s.to_string())),
    }
}
