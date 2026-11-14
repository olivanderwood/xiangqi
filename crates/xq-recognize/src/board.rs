//! Map piece detections to board squares, and build a `Position`.
//!
//! Follows the Xiangqi Arena web code (`OnlineGameRecognitionModel
//! .detectionsToBoard` / `ChessRecognitionModel.detectionsToBoard`):
//! the board's 4 corners (in the original image) define the board
//! rectangle; the centre pawn corrects the column offset. Without
//! calibration the rectangle is estimated from the detections'
//! bounding box (best-effort; the editor is the safety net).
//!
//! Orientation: the image row 0 is the top. The generals fix the
//! orientation — if the Red general is above the Black general the
//! photo is upside-down (Red at the top) and the grid is flipped
//! so Red ends up at the bottom, as FEN expects.

use crate::yolo::Detection;
use crate::RecognizeError;
use xq_core::board::Position;
use xq_core::types::{sq, Color, Piece, PieceKind};

/// Board calibration, in the *original* image's pixel coordinates:
/// the 4 board corners + the centre pawn. The app asks the user to
/// mark them on the photo (the upstream flow).
#[derive(Clone, Copy, Debug)]
pub struct BoardCalibration {
    /// Top-left, top-right, bottom-right, bottom-left.
    pub corners: [(f32, f32); 4],
    /// The centre pawn (centre file).
    pub pawn: (f32, f32),
}

/// A 10x9 board grid: row 0 is the image top, col 0 the image
/// left. Each cell holds the detected piece (or `None`).
pub type Grid = [[Option<Piece>; 9]; 10];

/// The physical model's 14 classes: 0-6 red, 7-13 black, in the
/// order king, advisor, elephant, horse, cannon, rook, pawn.
pub const PHYSICAL_CLASSES: usize = 14;

/// Map a physical-model class index to a piece. `None` for an
/// out-of-range index.
pub fn physical_class_to_piece(class: usize) -> Option<Piece> {
    let (color, kind) = match class {
        0 => (Color::Red, PieceKind::King),
        1 => (Color::Red, PieceKind::Advisor),
        2 => (Color::Red, PieceKind::Elephant),
        3 => (Color::Red, PieceKind::Horse),
        4 => (Color::Red, PieceKind::Cannon),
        5 => (Color::Red, PieceKind::Rook),
        6 => (Color::Red, PieceKind::Pawn),
        7 => (Color::Black, PieceKind::King),
        8 => (Color::Black, PieceKind::Advisor),
        9 => (Color::Black, PieceKind::Elephant),
        10 => (Color::Black, PieceKind::Horse),
        11 => (Color::Black, PieceKind::Rook),
        12 => (Color::Black, PieceKind::Cannon),
        13 => (Color::Black, PieceKind::Pawn),
        _ => return None,
    };
    Some(Piece::new(color, kind))
}

/// Map detections to a 10x9 grid (row 0 = image top, col 0 = image
/// left), each cell holding the detected piece (or `None`). When
/// two detections land on one square the higher-confidence one
/// wins. `calibration` is `None` for the auto-estimate.
pub fn detections_to_grid(
    detections: &[Detection],
    calibration: Option<&BoardCalibration>,
) -> Grid {
    let mut grid: Grid = [[None; 9]; 10];
    if detections.is_empty() {
        return grid;
    }
    let (min_x, max_x, min_y, max_y, col_offset) = match calibration {
        Some(cal) => {
            let xs = cal.corners.map(|c| c.0);
            let ys = cal.corners.map(|c| c.1);
            let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            // The centre pawn sits on file 4: its column corrects
            // any horizontal shift of the corner box.
            let cell_w = (max_x - min_x) / 8.0;
            let pawn_col = ((cal.pawn.0 - min_x) / cell_w).round() as i32;
            (min_x, max_x, min_y, max_y, 4 - pawn_col)
        }
        None => {
            // Auto-estimate: the detections' bounding box is the
            // board (the outermost pieces sit near the edges).
            let min_x = detections
                .iter()
                .map(|d| d.cx)
                .fold(f32::INFINITY, f32::min);
            let max_x = detections
                .iter()
                .map(|d| d.cx)
                .fold(f32::NEG_INFINITY, f32::max);
            let min_y = detections
                .iter()
                .map(|d| d.cy)
                .fold(f32::INFINITY, f32::min);
            let max_y = detections
                .iter()
                .map(|d| d.cy)
                .fold(f32::NEG_INFINITY, f32::max);
            (min_x, max_x, min_y, max_y, 0)
        }
    };
    let cell_w = (max_x - min_x) / 8.0;
    let cell_h = (max_y - min_y) / 9.0;
    if cell_w <= 0.0 || cell_h <= 0.0 {
        return grid;
    }
    // Highest confidence first, so a later (weaker) detection never
    // overwrites a stronger one on the same square.
    let mut sorted: Vec<&Detection> = detections.iter().collect();
    sorted.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for det in sorted {
        let col = ((det.cx - min_x) / cell_w).round() as i32 + col_offset;
        let row = ((det.cy - min_y) / cell_h).round() as i32;
        if !(0..9).contains(&col) || !(0..10).contains(&row) {
            continue;
        }
        let (row, col) = (row as usize, col as usize);
        if grid[row][col].is_none() {
            grid[row][col] = physical_class_to_piece(det.class);
        }
    }
    repair_confused_pieces(&mut grid);
    grid
}

/// Whether a piece kind may sit on `file`/`rank` (0-based file,
/// 0-based rank from Red's side). Horses, rooks, cannons and pawns
/// may sit anywhere; kings and advisors must be in the palace, and
/// elephants must be on their own side on an even file and a rank
/// of the right parity. Mirrors `Position::from_fen`'s placement
/// checks (xq-core).
fn kind_valid_on_square(kind: PieceKind, color: Color, file: i32, rank: i32) -> bool {
    match kind {
        PieceKind::King | PieceKind::Advisor => {
            // Palace: files 3-5, ranks 0-2 (Red) / 7-9 (Black).
            if !(3..=5).contains(&file) {
                return false;
            }
            let in_palace = match color {
                Color::Red => (0..=2).contains(&rank),
                Color::Black => (7..=9).contains(&rank),
            };
            if !in_palace {
                return false;
            }
            if kind == PieceKind::King {
                return true;
            }
            // Advisors sit on the palace diagonals only: the
            // centre + the 4 corners of the 3x3 palace. (A diagonal
            // move changes file and rank by 1 each, so the square
            // is a corner of the palace or its centre.)
            let centre_rank = match color {
                Color::Red => 1,
                Color::Black => 8,
            };
            (file == 4 && rank == centre_rank)
                || ((file == 3 || file == 5)
                    && (rank == centre_rank - 1 || rank == centre_rank + 1))
        }
        PieceKind::Elephant => {
            // Own side, even file, rank of the right parity.
            if file % 2 != 0 {
                return false;
            }
            match color {
                Color::Red => (0..=4).contains(&rank) && rank % 2 == 0,
                Color::Black => (5..=9).contains(&rank) && rank % 2 == 1,
            }
        }
        PieceKind::Horse | PieceKind::Rook | PieceKind::Cannon | PieceKind::Pawn => true,
    }
}

/// Repair pieces the classifier confused: the screenshot classifier
/// (64x64) mixes up Horse and Elephant (both diagonal movers with
/// similar glyphs) and, less often, Advisor with Horse. When a piece
/// sits on a square its kind can never occupy, it is almost
/// certainly that confusion — a Horse may sit anywhere, so the
/// piece becomes a Horse. This is what turns a grid with one
/// impossible Elephant into a valid position instead of voiding
/// the whole recognition.
fn repair_confused_pieces(grid: &mut Grid) {
    for (r, row) in grid.iter_mut().enumerate() {
        for (c, slot) in row.iter_mut().enumerate() {
            let Some(piece) = *slot else { continue };
            // Grid row 0 is the image top = rank 9.
            let rank = 9 - r as i32;
            let file = c as i32;
            if kind_valid_on_square(piece.kind, piece.color, file, rank) {
                continue;
            }
            // Elephant or Advisor on an impossible square: it is a
            // Horse the classifier misnamed. Kings are never
            // repaired (a wrong general is a bigger problem, and a
            // Horse cannot be a general).
            if matches!(piece.kind, PieceKind::Elephant | PieceKind::Advisor) {
                *slot = Some(Piece::new(piece.color, PieceKind::Horse));
            }
        }
    }
}

/// Build a `Position` from a grid (row 0 = image top). The side to
/// move is Red (the photo does not say whose turn it is — the app
/// asks the user after recognition). The grid is flipped when the
/// Red general is above the Black general, so Red ends up at the
/// bottom as FEN expects.
pub fn grid_to_position(grid: &Grid) -> Result<Position, RecognizeError> {
    let mut grid = *grid;
    // Orientation from the generals: Red must be below Black.
    let mut red_row: Option<usize> = None;
    let mut black_row: Option<usize> = None;
    for (r, row) in grid.iter().enumerate() {
        for piece in row.iter().flatten() {
            if piece.kind == PieceKind::King {
                match piece.color {
                    Color::Red => red_row = Some(r),
                    Color::Black => black_row = Some(r),
                }
            }
        }
    }
    match (red_row, black_row) {
        (Some(r), Some(b)) if r < b => {
            // Red above Black: the photo is upside-down.
            grid.reverse();
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(RecognizeError::NoPosition(
                "only one general recognised".to_string(),
            ));
        }
        (None, None) => {
            return Err(RecognizeError::NoPosition(
                "no general recognised".to_string(),
            ));
        }
        _ => {}
    }
    // FEN: row 0 of the FEN is the top (rank 9), row 9 the bottom
    // (rank 0). The grid row 0 is already the top.
    let mut fen = String::new();
    let mut pieces = 0;
    for (r, row) in grid.iter().enumerate() {
        let mut empty = 0;
        for piece in row.iter() {
            match piece {
                None => empty += 1,
                Some(p) => {
                    if empty > 0 {
                        fen.push_str(&empty.to_string());
                        empty = 0;
                    }
                    fen.push(p.fen_char());
                    pieces += 1;
                }
            }
        }
        if empty > 0 {
            fen.push_str(&empty.to_string());
        }
        if r < 9 {
            fen.push('/');
        }
    }
    if pieces < 2 {
        return Err(RecognizeError::NoPosition(
            "too few pieces recognised".to_string(),
        ));
    }
    // Side to move: Red. The app asks the user to pick the side
    // after recognition; Red is the default.
    fen.push_str(" w - - 0 1");
    Position::from_fen(&fen).map_err(|e| RecognizeError::NoPosition(e.to_string()))
}

/// Count the pieces on a grid, for picking the better of two
/// recognition results (a photo vs. a screenshot pipeline).
pub fn grid_piece_count(grid: &Grid) -> usize {
    grid.iter().flatten().filter(|p| p.is_some()).count()
}

/// The square index (`rank * 9 + file`) for a grid cell. Kept for
/// callers that need the index instead of a FEN.
pub fn grid_square(row: usize, col: usize) -> Option<u8> {
    // Grid row 0 is the top (rank 9); rank = 9 - row.
    sq(col as i32, 9 - row as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(cx: f32, cy: f32, class: usize) -> Detection {
        Detection {
            cx,
            cy,
            w: 60.0,
            h: 60.0,
            class,
            confidence: 0.9,
        }
    }

    #[test]
    fn physical_classes_map_to_pieces() {
        assert_eq!(
            physical_class_to_piece(0),
            Some(Piece::new(Color::Red, PieceKind::King))
        );
        assert_eq!(
            physical_class_to_piece(6),
            Some(Piece::new(Color::Red, PieceKind::Pawn))
        );
        assert_eq!(
            physical_class_to_piece(7),
            Some(Piece::new(Color::Black, PieceKind::King))
        );
        // Upstream order for Black is rook (11) before cannon (12)
        // — unlike Red, where cannon (4) comes before rook (5).
        assert_eq!(
            physical_class_to_piece(11),
            Some(Piece::new(Color::Black, PieceKind::Rook))
        );
        assert_eq!(
            physical_class_to_piece(12),
            Some(Piece::new(Color::Black, PieceKind::Cannon))
        );
        assert_eq!(
            physical_class_to_piece(13),
            Some(Piece::new(Color::Black, PieceKind::Pawn))
        );
        assert_eq!(physical_class_to_piece(14), None);
    }

    #[test]
    fn grid_maps_detections_with_calibration() {
        // A 720x900 board image: corners at (0,0)..(720,900),
        // centre pawn at (360, 500) (file 4). Cell 90x100.
        let cal = BoardCalibration {
            corners: [(0.0, 0.0), (720.0, 0.0), (720.0, 900.0), (0.0, 900.0)],
            pawn: (360.0, 500.0),
        };
        // Red king at the bottom centre (file 4, row 9 -> y 900),
        // Black king at the top centre (file 4, row 0 -> y 0).
        let dets = vec![det(360.0, 900.0, 0), det(360.0, 0.0, 7)];
        let grid = detections_to_grid(&dets, Some(&cal));
        assert_eq!(grid[9][4], Some(Piece::new(Color::Red, PieceKind::King)));
        assert_eq!(grid[0][4], Some(Piece::new(Color::Black, PieceKind::King)));
    }

    #[test]
    fn grid_flips_when_red_is_on_top() {
        // Red king at the top, Black king at the bottom: the photo
        // is upside-down, so the position must flip Red to the
        // bottom.
        let mut grid = [[None; 9]; 10];
        grid[0][4] = Some(Piece::new(Color::Red, PieceKind::King));
        grid[9][4] = Some(Piece::new(Color::Black, PieceKind::King));
        let pos = grid_to_position(&grid).expect("position");
        // Red king must be on rank 0 (bottom), Black on rank 9.
        assert_eq!(
            pos.piece_at(sq(4, 0).unwrap()),
            Some(Piece::new(Color::Red, PieceKind::King))
        );
        assert_eq!(
            pos.piece_at(sq(4, 9).unwrap()),
            Some(Piece::new(Color::Black, PieceKind::King))
        );
    }

    #[test]
    fn grid_without_generals_is_rejected() {
        let grid = [[None; 9]; 10];
        assert!(grid_to_position(&grid).is_err());
        let mut grid = [[None; 9]; 10];
        grid[0][4] = Some(Piece::new(Color::Red, PieceKind::King));
        assert!(grid_to_position(&grid).is_err());
    }

    #[test]
    fn elephant_on_odd_file_is_repaired_to_horse() {
        // The screenshot classifier mixes up Horse and Elephant.
        // An Elephant on file 3 (odd) can never be an Elephant,
        // so it becomes a Horse (which may sit anywhere).
        let mut grid = [[None; 9]; 10];
        // Row 4 = rank 5, col 3 = file 3: the misclassified piece
        // from Hieu's screenshot (image 2).
        grid[4][3] = Some(Piece::new(Color::Black, PieceKind::Elephant));
        repair_confused_pieces(&mut grid);
        assert_eq!(grid[4][3], Some(Piece::new(Color::Black, PieceKind::Horse)));
    }

    #[test]
    fn elephant_on_valid_square_is_kept() {
        let mut grid = [[None; 9]; 10];
        // Row 2 = rank 7, col 4 = file 4: a valid Black Elephant.
        grid[2][4] = Some(Piece::new(Color::Black, PieceKind::Elephant));
        repair_confused_pieces(&mut grid);
        assert_eq!(
            grid[2][4],
            Some(Piece::new(Color::Black, PieceKind::Elephant))
        );
    }

    #[test]
    fn advisor_outside_palace_is_repaired_to_horse() {
        let mut grid = [[None; 9]; 10];
        // Row 3 = rank 6, col 5 = file 5: outside the Red palace.
        grid[3][5] = Some(Piece::new(Color::Red, PieceKind::Advisor));
        repair_confused_pieces(&mut grid);
        assert_eq!(grid[3][5], Some(Piece::new(Color::Red, PieceKind::Horse)));
    }

    #[test]
    fn king_is_never_repaired() {
        let mut grid = [[None; 9]; 10];
        // A King outside the palace is wrong, but a Horse cannot
        // be a general — leave it for the position check to reject.
        grid[5][0] = Some(Piece::new(Color::Red, PieceKind::King));
        repair_confused_pieces(&mut grid);
        assert_eq!(grid[5][0], Some(Piece::new(Color::Red, PieceKind::King)));
    }
}
