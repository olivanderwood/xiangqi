//! Board drawing + screen<->square mapping (§7.1–7.3).
//!
//! `BoardGeometry` maps squares to points on the board (rect-relative
//! coordinates: add the board rect's `min` to get screen coordinates).
//! `draw_board` paints the board, pieces, highlights and hint arrows;
//! `BoardInteraction` turns pointer input into moves.

use egui::{
    Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Shape, Stroke, Vec2,
};
use xq_core::board::Position;
use xq_core::types::{Color, Move, Piece};

pub const WOOD: Color32 = Color32::from_rgb(0xE8, 0xC8, 0x87);
pub const LINE_COLOR: Color32 = Color32::from_rgb(0x5A, 0x3A, 0x1A);
pub const PIECE_BG: Color32 = Color32::from_rgb(0xF3, 0xE2, 0xB8);
pub const RED_PIECE: Color32 = Color32::from_rgb(0xB2, 0x22, 0x22);
pub const BLACK_PIECE: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);
pub const SELECT_COLOR: Color32 = Color32::from_rgb(0x2E, 0x7D, 0x32);
pub const LAST_MOVE_COLOR: Color32 = Color32::from_rgb(0x1B, 0x7A, 0x3D);

fn cjk_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("cjk-bold".into()))
}
fn cjk(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("cjk".into()))
}
fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Board geometry: cell size and origin, board flipped or not.
#[derive(Clone, Copy, Debug)]
pub struct BoardGeometry {
    pub cell: f32,
    pub origin: egui::Pos2,
    pub flipped: bool,
}

impl BoardGeometry {
    #[must_use]
    pub fn new(available: egui::Vec2, flipped: bool) -> Self {
        let cell = (available.x / 10.0).min(available.y / 11.0);
        // Centre the 10x11-cell board in the available rect.
        let ox = (available.x - 10.0 * cell) / 2.0 + 0.5 * cell;
        let oy = (available.y - 11.0 * cell) / 2.0 + 0.5 * cell;
        Self {
            cell,
            origin: egui::Pos2::new(ox, oy),
            flipped,
        }
    }

    /// Whole board rect (including the 0.5-cell margin) for an available rect.
    #[must_use]
    pub fn board_size(&self) -> Vec2 {
        Vec2::new(10.0 * self.cell, 11.0 * self.cell)
    }

    /// Square -> screen position (centre of the point), relative to the
    /// board rect's `min`. Name mandated by the spec (§7.1).
    #[allow(clippy::wrong_self_convention)]
    #[must_use]
    pub fn to_screen(&self, sq: u8) -> egui::Pos2 {
        let file = (sq % 9) as f32;
        let rank = (sq / 9) as f32;
        let (f, r) = if self.flipped {
            (8.0 - file, 9.0 - rank)
        } else {
            (file, rank)
        };
        egui::Pos2::new(
            self.origin.x + f * self.cell,
            self.origin.y + (9.0 - r) * self.cell,
        )
    }

    /// Screen position (relative to the board rect's `min`) -> square, if
    /// inside the board. Name mandated by the spec (§7.1).
    #[allow(clippy::wrong_self_convention)]
    #[must_use]
    pub fn from_screen(&self, p: egui::Pos2) -> Option<u8> {
        let fx = (p.x - self.origin.x) / self.cell;
        let fy = (p.y - self.origin.y) / self.cell;
        if !(0.0..9.0).contains(&fx) || !(0.0..10.0).contains(&fy) {
            return None;
        }
        // Nearest intersection point.
        let fi = fx.round() as i32;
        let ri = 9 - fy.round() as i32;
        if !(0..9).contains(&fi) || !(0..10).contains(&ri) {
            return None;
        }
        let (f, r) = if self.flipped {
            (8 - fi, 9 - ri)
        } else {
            (fi, ri)
        };
        Some((r * 9 + f) as u8)
    }
}

/// Legal moves starting from `from` in the current position.
#[must_use]
pub fn legal_targets(pos: &Position, from: u8) -> Vec<Move> {
    pos.legal_moves()
        .into_iter()
        .filter(|m| m.from == from)
        .collect()
}

/// The "horse leg" square a horse move bends through, if `from -> to` is
/// a horse move (§7.3: hint arrows for horses bend through the leg).
#[must_use]
pub fn horse_leg(from: u8, to: u8) -> Option<u8> {
    let (f0, r0) = ((from % 9) as i32, (from / 9) as i32);
    let (f1, r1) = ((to % 9) as i32, (to / 9) as i32);
    let (df, dr) = (f1 - f0, r1 - r0);
    let (lf, lr) = match (df.abs(), dr.abs()) {
        (1, 2) => (f0, r0 + dr.signum()),
        (2, 1) => (f0 + df.signum(), r0),
        _ => return None,
    };
    if (0..9).contains(&lf) && (0..10).contains(&lr) {
        Some((lr * 9 + lf) as u8)
    } else {
        None
    }
}

/// Column labels, top then bottom, for the current flip state.
/// Red side uses Chinese numerals 九…一 (left to right from Red's view);
/// Black side uses 1…9.
#[must_use]
pub fn column_labels(flipped: bool) -> (Vec<&'static str>, Vec<&'static str>) {
    let red = vec!["九", "八", "七", "六", "五", "四", "三", "二", "一"];
    let black = vec!["1", "2", "3", "4", "5", "6", "7", "8", "9"];
    if flipped {
        (red, black)
    } else {
        (black, red)
    }
}

/// Red's share of the vertical eval bar (0..1), from a score in Red's
/// perspective (§6.4): `tanh(score / 400)` mapped to 0..1.
#[must_use]
pub fn eval_bar_fraction(score_red: i32) -> f32 {
    0.5 * (1.0 + (f64::from(score_red) / 400.0).tanh() as f32)
}

/// A sliding-piece animation for a move just played (§7.3, 150 ms).
#[derive(Clone, Copy, Debug)]
pub struct MoveAnim {
    pub mv: Move,
    pub piece: Piece,
    pub start: f64,
    pub dur: f32,
}

impl MoveAnim {
    #[must_use]
    pub fn new(mv: Move, piece: Piece, start: f64, dur: f32) -> Self {
        Self {
            mv,
            piece,
            start,
            dur,
        }
    }

    /// 0..1 progress at time `now`; `None` when finished.
    #[must_use]
    pub fn progress(&self, now: f64) -> Option<f32> {
        let p = ((now - self.start) / f64::from(self.dur)) as f32;
        if p >= 1.0 {
            None
        } else {
            Some(p.clamp(0.0, 1.0))
        }
    }

    /// Interpolated centre of the moving piece at time `now`.
    #[must_use]
    pub fn pos_at(&self, geom: &BoardGeometry, rect_min: Pos2, now: f64) -> Pos2 {
        let p = self.progress(now).unwrap_or(1.0);
        let a = geom.to_screen(self.mv.from) + rect_min.to_vec2();
        let b = geom.to_screen(self.mv.to) + rect_min.to_vec2();
        a + (b - a) * p
    }
}

/// One hint arrow (§7.3).
///
/// All three ranks share one colour: the rank is shown by the number
/// badge drawn on the arrow (1 = best), not by colour.
#[derive(Clone, Copy, Debug)]
pub struct HintArrow {
    pub from: u8,
    pub to: u8,
    pub color: Color32,
    /// Whether the moving piece is a horse (arrow bends through the leg).
    pub horse: bool,
    /// 0-based rank among the hints (0 = best). Drawn as `rank + 1`.
    pub rank: usize,
}

impl HintArrow {
    #[must_use]
    pub fn new(from: u8, to: u8, color: Color32, horse: bool, rank: usize) -> Self {
        Self {
            from,
            to,
            color,
            horse,
            rank,
        }
    }

    /// Hint arrow for a ranked hint: one shared colour for every rank;
    /// the rank is carried in `rank` and drawn as a number badge.
    #[must_use]
    pub fn for_rank(from: u8, to: u8, rank: usize, horse: bool) -> Self {
        let color = Color32::from_rgba_unmultiplied(0x2F, 0x6F, 0xC8, 170);
        Self::new(from, to, color, horse, rank)
    }
}

/// Everything `draw_board` needs besides the position and geometry.
pub struct BoardDraw<'a> {
    pub selected: Option<u8>,
    pub targets: &'a [Move],
    pub last_move: Option<Move>,
    /// Square of the king currently in check, if any.
    pub check_sq: Option<u8>,
    /// Time the current check started (for the 1 s "Chiếu!" flash).
    pub check_since: Option<f64>,
    pub now: f64,
    pub arrows: &'a [HintArrow],
    pub drag: Option<(u8, Pos2)>,
    pub anim: Option<MoveAnim>,
    pub hover_sq: Option<u8>,
}

impl Default for BoardDraw<'_> {
    fn default() -> Self {
        Self {
            selected: None,
            targets: &[],
            last_move: None,
            check_sq: None,
            check_since: None,
            now: 0.0,
            arrows: &[],
            drag: None,
            anim: None,
            hover_sq: None,
        }
    }
}

fn draw_piece(painter: &Painter, center: Pos2, radius: f32, piece: Piece, scale: f32) {
    let r = radius * scale;
    let color = match piece.color {
        Color::Red => RED_PIECE,
        Color::Black => BLACK_PIECE,
    };
    // Soft shadow.
    painter.circle_filled(
        center + Vec2::new(2.0, 2.0),
        r,
        Color32::from_rgba_unmultiplied(0, 0, 0, 64),
    );
    painter.circle_filled(center, r, PIECE_BG);
    painter.circle_stroke(center, r, Stroke::new(2.0_f32, color));
    painter.circle_stroke(center, r - 0.09 * radius * 2.0, Stroke::new(1.0_f32, color));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        piece.han_char().to_string(),
        cjk_bold(1.1 * r),
        color,
    );
}

fn corner_brackets(painter: &Painter, center: Pos2, cell: f32, color: Color32) {
    let o = 0.30 * cell;
    let a = 0.16 * cell;
    let w = 2.5_f32;
    for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let cx = center.x + sx * o;
        let cy = center.y + sy * o;
        painter.line_segment(
            [Pos2::new(cx - sx * a, cy), Pos2::new(cx, cy)],
            Stroke::new(w, color),
        );
        painter.line_segment(
            [Pos2::new(cx, cy - sy * a), Pos2::new(cx, cy)],
            Stroke::new(w, color),
        );
    }
}

fn star_point(painter: &Painter, center: Pos2, cell: f32, file: i32, rank: i32) {
    let off = 0.09 * cell;
    let len = 0.13 * cell;
    let w = 1.5_f32;
    for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        // Skip directions that run off the board edge.
        if file == 0 && sx < 0.0 || file == 8 && sx > 0.0 {
            continue;
        }
        if rank == 0 && sy > 0.0 || rank == 9 && sy < 0.0 {
            // Note: screen y grows downwards, so rank 0 is at the bottom.
            continue;
        }
        let x = center.x + sx * off;
        let y = center.y + sy * off;
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new(x + sx * len, y)],
            Stroke::new(w, LINE_COLOR),
        );
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new(x, y + sy * len)],
            Stroke::new(w, LINE_COLOR),
        );
    }
}

fn draw_arrow(painter: &Painter, geom: &BoardGeometry, rect_min: Pos2, arrow: &HintArrow) {
    let cell = geom.cell;
    let width = 0.12 * cell;
    let head_len = 0.30 * cell;
    let head_half = 0.15 * cell;
    let at = |sq: u8| geom.to_screen(sq) + rect_min.to_vec2();
    let a = at(arrow.from);
    let b = at(arrow.to);
    let mut points: Vec<Pos2> = vec![a];
    if arrow.horse {
        if let Some(leg) = horse_leg(arrow.from, arrow.to) {
            points.push(at(leg));
        }
    }
    points.push(b);
    // Draw the polyline, shortening the last segment for the arrow head.
    let n = points.len();
    for (i, w) in points.windows(2).enumerate() {
        let (p, q) = (w[0], w[1]);
        let end = if i == n - 2 {
            let dir = (q - p).normalized();
            q - dir * head_len
        } else {
            q
        };
        painter.line_segment([p, end], Stroke::new(width, arrow.color));
    }
    // Arrow head triangle at the destination.
    let before = points[points.len() - 2];
    let dir = (b - before).normalized();
    let base = b - dir * head_len;
    let perp = Vec2::new(-dir.y, dir.x);
    painter.add(Shape::convex_polygon(
        vec![b, base + perp * head_half, base - perp * head_half],
        arrow.color,
        Stroke::NONE,
    ));
    // Number badge (1-based rank): spread along the arrow by rank and
    // fanned slightly across it, so the 1-4 badges never sit on top of
    // each other — they used to collide on shared horse legs and on the
    // midpoints of parallel arrows. The badge stays on its arrow line,
    // so the number still reads as "this arrow is hint N".
    let frac = [0.30, 0.45, 0.60, 0.75][arrow.rank.min(3)];
    let total: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut target = frac * total;
    let mut badge_at = *points.last().unwrap_or(&b);
    let mut seg_dir = (b - before).normalized();
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let len = (q - p).length();
        if target <= len {
            seg_dir = (q - p).normalized();
            badge_at = p + seg_dir * target;
            break;
        }
        target -= len;
    }
    let perp = Vec2::new(-seg_dir.y, seg_dir.x);
    badge_at += perp * (arrow.rank as f32 - 1.5_f32) * 0.28 * cell;
    painter.circle_filled(
        badge_at,
        0.19 * cell,
        Color32::from_rgba_unmultiplied(0x1A, 0x1A, 0x1A, 220),
    );
    painter.circle_stroke(
        badge_at,
        0.19 * cell,
        Stroke::new(
            1.5_f32,
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 220),
        ),
    );
    painter.text(
        badge_at,
        Align2::CENTER_CENTER,
        (arrow.rank + 1).to_string(),
        cjk_bold(0.30 * cell),
        Color32::WHITE,
    );
}

/// Draw the whole board into `rect` (whose size must fit `geom`).
pub fn draw_board(
    painter: &Painter,
    rect: Rect,
    geom: &BoardGeometry,
    pos: &Position,
    draw: &BoardDraw,
) {
    let cell = geom.cell;
    let at = |sq: u8| geom.to_screen(sq) + rect.min.to_vec2();
    let sq_at = |file: i32, rank: i32| -> Pos2 {
        let f = file as f32;
        let r = rank as f32;
        let (f, r) = if geom.flipped {
            (8.0 - f, 9.0 - r)
        } else {
            (f, r)
        };
        Pos2::new(
            rect.min.x + geom.origin.x + f * cell,
            rect.min.y + geom.origin.y + (9.0 - r) * cell,
        )
    };

    // Wooden background + double frame.
    let board_rect = Rect::from_min_size(rect.min, geom.board_size());
    painter.rect_filled(board_rect, egui::CornerRadius::ZERO, WOOD);
    painter.rect_stroke(
        board_rect,
        egui::CornerRadius::ZERO,
        Stroke::new(3.0_f32, LINE_COLOR),
        egui::StrokeKind::Inside,
    );
    painter.rect_stroke(
        board_rect.shrink(4.0),
        egui::CornerRadius::ZERO,
        Stroke::new(1.0_f32, LINE_COLOR),
        egui::StrokeKind::Inside,
    );

    // Grid: verticals (border files cross the river, others stop at it).
    for f in 0..9 {
        let x0 = sq_at(f, 0).x;
        let y_top = sq_at(f, 9).y;
        let y_bot = sq_at(f, 0).y;
        if f == 0 || f == 8 {
            painter.line_segment(
                [Pos2::new(x0, y_top), Pos2::new(x0, y_bot)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
        } else {
            let y_river_top = sq_at(f, 5).y;
            let y_river_bot = sq_at(f, 4).y;
            painter.line_segment(
                [Pos2::new(x0, y_top), Pos2::new(x0, y_river_top)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
            painter.line_segment(
                [Pos2::new(x0, y_river_bot), Pos2::new(x0, y_bot)],
                Stroke::new(1.5_f32, LINE_COLOR),
            );
        }
    }
    // Horizontals.
    for r in 0..10 {
        let y = sq_at(0, r).y;
        painter.line_segment(
            [Pos2::new(sq_at(0, r).x, y), Pos2::new(sq_at(8, r).x, y)],
            Stroke::new(1.5_f32, LINE_COLOR),
        );
    }
    // Palace diagonals.
    for (f0, r0, f1, r1) in [(3, 0, 5, 2), (5, 0, 3, 2), (3, 7, 5, 9), (5, 7, 3, 9)] {
        painter.line_segment(
            [sq_at(f0, r0), sq_at(f1, r1)],
            Stroke::new(1.5_f32, LINE_COLOR),
        );
    }
    // Star points at the cannon / pawn starting squares.
    for (f, r) in [
        (1, 2),
        (7, 2),
        (1, 7),
        (7, 7),
        (0, 3),
        (2, 3),
        (4, 3),
        (6, 3),
        (8, 3),
        (0, 6),
        (2, 6),
        (4, 6),
        (6, 6),
        (8, 6),
    ] {
        star_point(painter, sq_at(f, r), cell, f, r);
    }
    // River text.
    let river_y = (sq_at(0, 4).y + sq_at(0, 5).y) / 2.0;
    painter.text(
        Pos2::new(sq_at(1, 4).x + 0.5 * cell, river_y),
        Align2::CENTER_CENTER,
        "楚河",
        cjk(0.5 * cell),
        LINE_COLOR,
    );
    painter.text(
        Pos2::new(sq_at(6, 4).x + 0.5 * cell, river_y),
        Align2::CENTER_CENTER,
        "漢界",
        cjk(0.5 * cell),
        LINE_COLOR,
    );
    // Column labels in the top / bottom margins.
    let (top, bottom) = column_labels(geom.flipped);
    for (i, label) in top.iter().enumerate() {
        let is_cn = label
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        painter.text(
            Pos2::new(sq_at(i as i32, 9).x, rect.min.y + 0.27 * cell),
            Align2::CENTER_CENTER,
            *label,
            if is_cn {
                cjk(0.32 * cell)
            } else {
                sans(0.32 * cell)
            },
            LINE_COLOR,
        );
    }
    for (i, label) in bottom.iter().enumerate() {
        let is_cn = label
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        painter.text(
            Pos2::new(sq_at(i as i32, 0).x, rect.min.y + 10.73 * cell),
            Align2::CENTER_CENTER,
            *label,
            if is_cn {
                cjk(0.32 * cell)
            } else {
                sans(0.32 * cell)
            },
            LINE_COLOR,
        );
    }

    // Last-move highlight: a translucent amber wash on the from/to
    // squares (easy to spot at a glance) plus the corner brackets.
    // The wash is drawn under the pieces, so the board layout and
    // the pieces themselves are untouched.
    if let Some(mv) = draw.last_move {
        for sq in [mv.from, mv.to] {
            let c = at(sq);
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(0.92 * cell)),
                egui::CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0xFF, 0xC1, 0x07, 70),
            );
        }
        corner_brackets(painter, at(mv.from), cell, LAST_MOVE_COLOR);
        corner_brackets(painter, at(mv.to), cell, LAST_MOVE_COLOR);
    }
    // Capture flash: the last move was a capture exactly when the
    // halfmove clock is 0 after it (this codebase resets the clock
    // only on captures — see `Position::make_move`). An expanding
    // ring at the captured square fades out over ~400 ms from the
    // move's slide start; without a slide animation there is no
    // start time, so a steady ring marks the square until the next
    // move instead. The ring is gone as soon as the next move is a
    // quiet one (clock > 0), so it never gets stuck.
    if pos.halfmove_clock() == 0 {
        if let Some(mv) = draw.last_move {
            let c = at(mv.to);
            let (radius, alpha) = match draw.anim.filter(|a| a.mv == mv) {
                Some(anim) => {
                    let elapsed = (draw.now - anim.start) as f32;
                    let p = (elapsed / 0.4).clamp(0.0, 1.0);
                    (0.48 + 0.30 * p, ((1.0 - p) * 220.0) as u8)
                }
                None => (0.58, 150),
            };
            if alpha > 0 {
                painter.circle_stroke(
                    c,
                    radius * cell,
                    Stroke::new(
                        4.0_f32,
                        Color32::from_rgba_unmultiplied(0xE6, 0x51, 0x00, alpha),
                    ),
                );
            }
        }
    }
    // Hint arrows (under the pieces).
    for arrow in draw.arrows {
        draw_arrow(painter, geom, rect.min, arrow);
    }
    // Check halo around the king in check — pulsing, so it is easy to
    // spot. Checkmate (king in check with no legal move) gets a
    // stronger effect: a thicker, faster-pulsing halo plus a
    // "Chiếu hết!" / "Checkmate!" banner across the board.
    if let Some(ksq) = draw.check_sq {
        // Checkmate = in check and no legal move for the side to move.
        // `legal_moves` is only generated while in check (rare), so
        // this costs nothing on a normal frame.
        let is_mate = pos.legal_moves().is_empty();
        let speed = if is_mate { 12.0 } else { 8.0 };
        let pulse = 0.5 + 0.5 * ((draw.now * speed) as f32).sin();
        let c = at(ksq);
        if is_mate {
            // Strong halo: wider radius swing, higher alpha, thick ring.
            let radius = (0.58 + 0.10 * pulse) * cell;
            let alpha = (140.0 + 115.0 * pulse) as u8;
            painter.circle_filled(
                c,
                radius,
                Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, alpha / 4),
            );
            painter.circle_stroke(
                c,
                radius,
                Stroke::new(
                    5.0_f32,
                    Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, alpha),
                ),
            );
            // Banner across the middle of the board.
            let mate_text = match crate::i18n::language() {
                crate::i18n::Language::English => "Checkmate!",
                crate::i18n::Language::Vietnamese => "Chiếu hết!",
            };
            let mid = Pos2::new(
                rect.min.x + geom.board_size().x / 2.0,
                rect.min.y + geom.board_size().y / 2.0,
            );
            let banner_h = 0.62 * cell;
            painter.rect_filled(
                Rect::from_center_size(mid, Vec2::new(geom.board_size().x * 0.72, banner_h)),
                egui::CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0x7B, 0x1F, 0x1F, 225),
            );
            painter.text(
                mid,
                Align2::CENTER_CENTER,
                mate_text,
                sans(0.38 * cell),
                Color32::WHITE,
            );
        } else {
            // Check: pulsing radius + alpha, plus a thin outer ring
            // that expands and fades in a loop (a "shockwave").
            let radius = (0.56 + 0.06 * pulse) * cell;
            let alpha = (120.0 + 100.0 * pulse) as u8;
            painter.circle_filled(
                c,
                radius,
                Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, alpha / 3),
            );
            painter.circle_stroke(
                c,
                radius,
                Stroke::new(
                    3.0_f32,
                    Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, alpha),
                ),
            );
            let wave_p = ((draw.now * 1.25) as f32).fract();
            painter.circle_stroke(
                c,
                (0.55 + 0.30 * wave_p) * cell,
                Stroke::new(
                    2.0_f32,
                    Color32::from_rgba_unmultiplied(
                        0xFF,
                        0x00,
                        0x00,
                        ((1.0 - wave_p) * 140.0) as u8,
                    ),
                ),
            );
        }
        if let Some(since) = draw.check_since {
            if draw.now - since < 1.0 && !is_mate {
                painter.text(
                    c + Vec2::new(0.0, -0.85 * cell),
                    Align2::CENTER_CENTER,
                    crate::i18n::t(crate::i18n::Key::Check),
                    sans(0.34 * cell),
                    Color32::from_rgb(0xFF, 0x00, 0x00),
                );
            }
        }
    }
    // Legal-move dots / capture rings for the selected piece.
    if let Some(from) = draw.selected {
        for m in draw.targets {
            if m.from != from {
                continue;
            }
            let c = at(m.to);
            if pos.piece_at(m.to).is_some() {
                painter.circle_stroke(
                    c,
                    0.50 * cell,
                    Stroke::new(3.0_f32, Color32::from_rgb(0xC0, 0x39, 0x2B)),
                );
            } else {
                painter.circle_filled(
                    c,
                    0.11 * cell,
                    Color32::from_rgba_unmultiplied(0x2E, 0x7D, 0x32, 200),
                );
            }
        }
    }
    // Hover highlight.
    if let Some(sq) = draw.hover_sq {
        if draw.selected.is_none() {
            painter.circle_stroke(
                at(sq),
                0.46 * cell,
                Stroke::new(
                    1.5_f32,
                    Color32::from_rgba_unmultiplied(0x2E, 0x7D, 0x32, 120),
                ),
            );
        }
    }

    // Pieces.
    let radius = 0.44 * cell;
    // During a slide animation the moving piece is drawn once, at its
    // interpolated position below — not at its destination square. The
    // old code skipped `anim_from`, but `pos` is the position AFTER the
    // move, so the from-square is already empty and the skip was a
    // no-op: the piece showed at the destination (from `pos`) AND as
    // the sliding piece (at the old square when progress = 0), i.e. a
    // duplicated piece at the old square (Hieu, 2026-11-12, after
    // continuing from a lesson). Skip the destination only while the
    // animation is still running; once it finishes (`progress` is
    // `None`) the piece is drawn at its destination normally.
    let anim_from = draw.anim.map(|a| a.mv.from);
    let anim_to = draw
        .anim
        .filter(|a| a.progress(draw.now).is_some())
        .map(|a| a.mv.to);
    let drag_sq = draw.drag.map(|(sq, _)| sq);
    for sq in 0..90u8 {
        let Some(piece) = pos.piece_at(sq) else {
            continue;
        };
        if Some(sq) == anim_from || Some(sq) == anim_to || Some(sq) == drag_sq {
            continue; // drawn at its animated / dragged position below
        }
        let scale = if draw.selected == Some(sq) { 1.08 } else { 1.0 };
        if draw.selected == Some(sq) {
            painter.circle_stroke(
                at(sq),
                radius * 1.08 + 3.0,
                Stroke::new(3.0_f32, SELECT_COLOR),
            );
        }
        draw_piece(painter, at(sq), radius, piece, scale);
    }
    // Animated piece.
    if let Some(anim) = draw.anim {
        if let Some(p) = anim.progress(draw.now) {
            let _ = p;
            draw_piece(
                painter,
                anim.pos_at(geom, rect.min, draw.now),
                radius,
                anim.piece,
                1.0,
            );
        }
    }
    // Dragged piece follows the pointer.
    if let Some((sq, p)) = draw.drag {
        if let Some(piece) = pos.piece_at(sq) {
            draw_piece(painter, p, radius, piece, 1.08);
        }
    }
}

/// Pointer interaction state for one board (§7.3).
#[derive(Default, Clone, Debug)]
pub struct BoardInteraction {
    pub selected: Option<u8>,
    pub drag_sq: Option<u8>,
    pub drag_pos: Pos2,
    pub drag_moved: bool,
    pub hover_sq: Option<u8>,
}

impl BoardInteraction {
    pub fn clear(&mut self) {
        self.selected = None;
        self.drag_sq = None;
        self.drag_moved = false;
    }

    /// Handle one frame's pointer response. Returns a move when the user
    /// completed one (click-click on a legal destination, or drag-release
    /// on one). Only pieces of `movable` (the side allowed to move now)
    /// can be selected.
    pub fn handle(
        &mut self,
        resp: &Response,
        rect: Rect,
        geom: &BoardGeometry,
        pos: &Position,
        movable: Color,
    ) -> Option<Move> {
        let to_sq = |p: Pos2| geom.from_screen(p - rect.min.to_vec2());
        self.hover_sq = resp.hover_pos().and_then(to_sq);

        if resp.drag_started() {
            if let Some(p) = resp.interact_pointer_pos().and_then(to_sq) {
                if let Some(piece) = pos.piece_at(p) {
                    if piece.color == movable {
                        self.selected = Some(p);
                        self.drag_sq = Some(p);
                        self.drag_pos = resp.interact_pointer_pos().unwrap_or_default();
                        self.drag_moved = false;
                    }
                }
            }
        }
        if resp.dragged() && self.drag_sq.is_some() {
            if let Some(p) = resp.interact_pointer_pos() {
                if (p - self.drag_pos).length() > 2.0 {
                    self.drag_moved = true;
                }
                self.drag_pos = p;
            }
        }
        if resp.drag_stopped() {
            if let Some(from) = self.drag_sq.take() {
                if self.drag_moved {
                    if let Some(to) = to_sq(self.drag_pos) {
                        let mv = Move::new(from, to);
                        if pos.legal_moves().contains(&mv) {
                            self.selected = None;
                            return Some(mv);
                        }
                    }
                    // Dropped on an illegal square: piece returns home,
                    // selection stays so the user can click a destination.
                    return None;
                }
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos().and_then(to_sq) {
                if self.selected == Some(p) {
                    self.selected = None;
                    return None;
                }
                if let Some(piece) = pos.piece_at(p) {
                    if piece.color == movable {
                        self.selected = Some(p);
                        return None;
                    }
                }
                if let Some(from) = self.selected {
                    let mv = Move::new(from, p);
                    if pos.legal_moves().contains(&mv) {
                        self.selected = None;
                        return Some(mv);
                    }
                }
                self.selected = None;
            } else {
                self.selected = None;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::board::Position;

    fn geom(flipped: bool) -> BoardGeometry {
        BoardGeometry::new(Vec2::new(500.0, 550.0), flipped)
    }

    #[test]
    fn to_from_screen_roundtrip_not_flipped() {
        let g = geom(false);
        for sq in 0..90u8 {
            let p = g.to_screen(sq);
            assert_eq!(g.from_screen(p), Some(sq), "sq {sq}");
        }
    }

    #[test]
    fn to_from_screen_roundtrip_flipped() {
        let g = geom(true);
        for sq in 0..90u8 {
            let p = g.to_screen(sq);
            assert_eq!(g.from_screen(p), Some(sq), "sq {sq} flipped");
        }
    }

    #[test]
    fn flipped_is_a_180_rotation() {
        let a = geom(false);
        let b = geom(true);
        // Square a0 (red rook corner) sits bottom-left unflipped and
        // top-right flipped.
        let p0 = a.to_screen(0);
        let p1 = b.to_screen(0);
        assert!(p0.x < 250.0 && p0.y > 275.0);
        assert!(p1.x > 250.0 && p1.y < 275.0);
        // Centre squares map to their 180-degree counterpart: sq 40
        // (file 4, rank 4) <-> sq 49 (file 4, rank 5).
        assert_eq!(a.to_screen(40), b.to_screen(49));
        assert_eq!(a.to_screen(49), b.to_screen(40));
    }

    #[test]
    fn from_screen_outside_is_none() {
        let g = geom(false);
        assert_eq!(g.from_screen(Pos2::new(-10.0, 100.0)), None);
        assert_eq!(g.from_screen(Pos2::new(600.0, 100.0)), None);
        assert_eq!(g.from_screen(Pos2::new(100.0, -5.0)), None);
        assert_eq!(g.from_screen(Pos2::new(100.0, 600.0)), None);
    }

    #[test]
    fn column_labels_swap_when_flipped() {
        let (top, bottom) = column_labels(false);
        assert_eq!(top[0], "1");
        assert_eq!(top[8], "9");
        assert_eq!(bottom[0], "九");
        assert_eq!(bottom[8], "一");
        let (top, bottom) = column_labels(true);
        assert_eq!(top[0], "九");
        assert_eq!(bottom[0], "1");
    }

    #[test]
    fn horse_leg_bends_correctly() {
        // b0 -> c2 (file 1->2, rank 0->2): leg is b1 (sq 10).
        assert_eq!(horse_leg(1, 20), Some(10));
        // b0 -> a2: leg b1 as well.
        assert_eq!(horse_leg(1, 18), Some(10));
        // b0 -> d1 (file 1->3, rank 0->1): leg c0 (sq 2).
        assert_eq!(horse_leg(1, 12), Some(2));
        // Not a horse move.
        assert_eq!(horse_leg(16, 13), None);
    }

    #[test]
    fn legal_targets_startpos_cannon() {
        let pos = Position::startpos();
        // Red cannon h2 = file 7, rank 2 -> sq 25; 11 quiet moves plus a
        // capture of the black horse on h9 (sq 88), jumping the black
        // cannon on h7 (sq 70) as a screen.
        let targets = legal_targets(&pos, 25);
        let tos: Vec<u8> = targets.iter().map(|m| m.to).collect();
        assert!(tos.contains(&22), "cannon h2 can go to e2, got {tos:?}");
        assert!(
            tos.contains(&88),
            "cannon h2 can capture on h9, got {tos:?}"
        );
        assert_eq!(targets.len(), 12, "got {tos:?}");
    }

    #[test]
    fn eval_bar_fraction_is_symmetric() {
        assert!((eval_bar_fraction(0) - 0.5).abs() < 1e-6);
        assert!(eval_bar_fraction(400) > 0.5);
        assert!(eval_bar_fraction(-400) < 0.5);
        assert!((eval_bar_fraction(400) - (1.0 - eval_bar_fraction(-400))).abs() < 1e-6);
    }

    #[test]
    fn move_anim_progress() {
        let pos = Position::startpos();
        let piece = pos.piece_at(25).unwrap();
        let anim = MoveAnim::new(Move::new(25, 22), piece, 10.0, 0.15);
        assert_eq!(anim.progress(10.0), Some(0.0));
        assert!(anim.progress(10.2).is_none());
        let g = geom(false);
        let p0 = anim.pos_at(&g, Pos2::ZERO, 10.0);
        assert_eq!(p0, g.to_screen(25));
    }

    #[test]
    fn piece_kind_for_arrow() {
        // Sanity: the piece on h2 really is a cannon (used by hint arrows).
        let pos = Position::startpos();
        assert_eq!(
            pos.piece_at(25).unwrap().kind,
            xq_core::types::PieceKind::Cannon
        );
    }
}
