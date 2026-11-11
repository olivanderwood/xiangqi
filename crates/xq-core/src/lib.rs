//! `xq-core`: Xiangqi rules + AI engine. No UI dependencies.

pub mod analysis;
pub mod board;
pub mod eval;
pub mod eval_params;
pub mod game;
pub mod movegen;
pub mod notation;
pub mod search;
pub mod tt;
pub mod types;
pub mod zobrist;

pub use board::{Position, Undo, START_FEN};
pub use game::{Game, GameResult, Reason};
pub use types::{Color, FenError, IllegalMove, Move, Piece, PieceKind};
