//! The Xiangqi board: geometry, piece drawing, suggestion arrows,
//! drag-and-drop and the 400 ms slide animation.
//!
//! Ported from the spike (`~/workspace/iced-spike/src/main.rs`:
//! `Geom` ~648, `draw_piece` ~695, `draw_arrow` ~725,
//! `BoardProgram`/`BoardState` ~794–1060) and adapted to the real
//! core types (`xq_core::board::Position`, `xq_core::types::Piece`).
//! All three board screens (Play / Learn / Practice, agents P1–P3)
//! share this module — they build a [`BoardRender`] and a
//! [`BoardProgram`] and handle [`BoardMsg`].
//!
//! # INVARIANT — the piece must land before any computation starts
//!
//! A move's slide animation (400 ms) must finish BEFORE the AI,
//! grading, hint or ChessDB work for the new position starts (Hieu,
//! restated 2026-11-12, three times in one day). In Elm terms:
//!
//! * `update()` only triggers AI / grading / hint / ChessDB work when
//!   `board.is_animating() == false`;
//! * the tick subscription only runs while an animation or the
//!   flashing check halo is active.
//!
//! The animation state lives in the screen-owned [`BoardState`]
//! (not inside the canvas program) precisely so `update()` can read
//! it. [`BoardProgram`] only renders a snapshot of it
//! ([`BoardRender::anim`]) and owns the drag state internally
//! ([`DragState`], private), which no computation ever needs to read.
//!
//! # Geometry (DESIGN-iced.md §1)
//!
//! Fixed board 592 × 656 px, cell 64, margin 40,
//! `scale = min(w / 592, h / 656)`; piece centre
//! `(40 + x·64, 40 + y·64)` in board-local coordinates, `y = 0` is
//! the Red row (bottom). Squares are `u8` indices,
//! `sq = rank * 9 + file`, rank 0 = Red's back rank, exactly as in
//! `xq-core`.

use std::time::{Duration, Instant};

use iced::alignment::Vertical;
use iced::mouse::{self, Button, Cursor};
use iced::widget::canvas::{
    gradient::{Gradient, Linear}, Action, Canvas, Event, Frame, Geometry, LineDash, Path, Program,
    Stroke, Text as CanvasText,
};
use iced::widget::text::Alignment;
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme};
use xq_core::board::Position;
use xq_core::types::{file_of, rank_of, Color as SideColor, Piece, PieceKind};

use crate::theme::{
    self, BLACK_PIECE, BLUE, BOARD_LINE, GOLD, PIECE_FACE_BOTTOM, PIECE_FACE_TOP,
    PIECE_INNER_RADIUS, PIECE_RADIUS, PIECE_RIM, PURPLE, RED, RED_PIECE, WOOD_BOTTOM,
    WOOD_EDGE_LIGHT, WOOD_TOP,
};

/// A board square: `sq = rank * 9 + file`, rank 0 = Red's back rank
/// (same convention as `xq-core`).
pub type Square = u8;

/// How long a piece takes to slide to its destination (egui app
/// §7.3). 400 ms (Hieu, 2026-11-14): the lift-up / travel / set-down
/// arc needs the extra time to read (300 ms was too quick for the
/// lift to register), still short enough that the AI / grading work
/// waiting for the landing never feels delayed.
pub const ANIM_DURATION: Duration = Duration::from_millis(400);

/// How much bigger a piece looks at the top of its lift arc
/// (12%: the disc visibly rises off the board without dwarfing its
/// neighbours).
pub const LIFT_SCALE: f32 = 0.12;

/// How far (in board px at scale 1) a piece rises off the board at
/// the top of its lift arc.
pub const LIFT_RISE: f32 = 5.0;

/// Default arrow opacity; the hovered arrow is drawn at full opacity
/// (DESIGN-iced.md §1). Hieu (2026-11-13) found the design's 38%
/// too faint — arrows must be thick and clear, so the default is
/// 65% now.
pub const ARROW_ALPHA_DEFAULT: f32 = 0.65;
/// Opacity of the hovered (highlighted) arrow.
pub const ARROW_ALPHA_HIGHLIGHTED: f32 = 1.0;

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Board geometry for one canvas bounds: scale + origin, derived
/// from the design's fixed 592 × 656 board.
///
/// Coordinates are WIDGET-LOCAL: the canvas `Frame` is created from
/// `bounds.size()` and `Cursor::position_in(bounds)` already
/// subtracts the widget position, so `bx`/`by` are centred within
/// the widget and never include `bounds.x`/`bounds.y` (adding them
/// double-counts the widget offset and misplaces both drawing and
/// hit-testing whenever the canvas is not at the window origin —
/// integration fix, 2026-11-13).
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    /// `min(w / 592, h / 656)`.
    pub scale: f32,
    /// Board top-left x (widget-local).
    pub bx: f32,
    /// Board top-left y (widget-local).
    pub by: f32,
    /// Cell size in screen px (64 × scale).
    pub cell: f32,
    /// Margin in screen px (40 × scale).
    pub margin: f32,
    /// Whether the board is flipped (Black at the bottom).
    pub flipped: bool,
}

impl Geom {
    /// Geometry for the given canvas bounds, unflipped (Red at the
    /// bottom).
    #[must_use]
    pub fn from_bounds(bounds: Rectangle) -> Self {
        Self::from_bounds_flipped(bounds, false)
    }

    /// Geometry for the given canvas bounds, with an explicit flip
    /// state.
    #[must_use]
    pub fn from_bounds_flipped(bounds: Rectangle, flipped: bool) -> Self {
        let scale = (bounds.width / crate::theme::BOARD_WIDTH)
            .min(bounds.height / crate::theme::BOARD_HEIGHT);
        let cell = crate::theme::BOARD_CELL * scale;
        let margin = crate::theme::BOARD_MARGIN * scale;
        // Widget-local centring: NO bounds.x / bounds.y here (see
        // the struct docs).
        let bx = (bounds.width - crate::theme::BOARD_WIDTH * scale) / 2.0;
        let by = (bounds.height - crate::theme::BOARD_HEIGHT * scale) / 2.0;
        Self {
            scale,
            bx,
            by,
            cell,
            margin,
            flipped,
        }
    }

    /// Screen centre of the point (`file` 0..8, `rank` 0..9, rank 0 =
    /// Red's back rank at the bottom). When flipped, the board is
    /// mirrored: file 0 is drawn at the right, rank 0 at the top.
    #[must_use]
    pub fn point(&self, file: i32, rank: i32) -> Point {
        let (file, rank) = if self.flipped {
            (8 - file, 9 - rank)
        } else {
            (file, rank)
        };
        Point::new(
            self.bx + self.margin + file as f32 * self.cell,
            self.by + self.margin + (9 - rank) as f32 * self.cell,
        )
    }

    /// Centre of a square.
    #[must_use]
    pub fn square_point(&self, sq: Square) -> Point {
        self.point(file_of(sq), rank_of(sq))
    }

    /// Screen point → nearest board square, if within half a cell of
    /// a grid point (spike behaviour: round to the nearest point).
    /// The input point is widget-local (see [`Geom::from_bounds`]).
    #[must_use]
    pub fn square_at(&self, p: Point) -> Option<Square> {
        let fx = (p.x - self.bx - self.margin) / self.cell;
        let fy = (p.y - self.by - self.margin) / self.cell;
        if !(0.0..9.0).contains(&fx) || !(0.0..10.0).contains(&fy) {
            return None;
        }
        let mut file = fx.round() as i32;
        let mut rank = 9 - fy.round() as i32;
        if self.flipped {
            file = 8 - file;
            rank = 9 - rank;
        }
        if !(0..9).contains(&file) || !(0..10).contains(&rank) {
            return None;
        }
        Some((rank * 9 + file) as u8)
    }
}

// ---------------------------------------------------------------------------
// Animation
// ---------------------------------------------------------------------------

/// One sliding piece: from → to over [`ANIM_DURATION`] (400 ms).
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    /// Source square.
    pub from: Square,
    /// Destination square.
    pub to: Square,
    /// When the slide started.
    pub started: Instant,
}

/// How lifted a piece is at a raw (uneased) progress `p` in 0..1:
/// 0 at both ends, 1 mid-flight — a smooth arc, so the piece rises
/// off the board, travels, and settles back down (Hieu, 2026-11-14).
/// Pure, so tests can check the arc without an [`Anim`].
#[must_use]
pub fn lift_at(p: f32) -> f32 {
    (std::f32::consts::PI * p.clamp(0.0, 1.0)).sin()
}

impl Anim {
    /// Start a slide now.
    #[must_use]
    pub fn new(from: Square, to: Square) -> Self {
        Self {
            from,
            to,
            started: Instant::now(),
        }
    }

    /// Raw slide progress 0..1 at `now` (1 once the duration elapsed).
    #[must_use]
    pub fn linear_progress(&self, now: Instant) -> f32 {
        (now.duration_since(self.started).as_secs_f32() / ANIM_DURATION.as_secs_f32()).min(1.0)
    }

    /// Slide progress 0..1 at `now` (1 once the duration elapsed),
    /// eased out (quadratic) so the piece decelerates into its
    /// square instead of stopping abruptly at constant speed.
    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        let p = self.linear_progress(now);
        1.0 - (1.0 - p) * (1.0 - p)
    }

    /// Lift 0..1 at `now`: how far along the pick-up arc the piece
    /// is (0 resting on its square, 1 at the top of the arc).
    #[must_use]
    pub fn lift(&self, now: Instant) -> f32 {
        lift_at(self.linear_progress(now))
    }

    /// Whether the slide has finished at `now`.
    #[must_use]
    pub fn is_finished(&self, now: Instant) -> bool {
        now.duration_since(self.started) >= ANIM_DURATION
    }
}

/// Screen-owned board state: the slide animation (plus, later, the
/// check-halo clock). The canvas program only renders a snapshot of
/// this — see the module docs for why the animation lives here and
/// not inside [`BoardProgram`].
///
/// The gate every screen must enforce (see the invariant above):
/// start AI / grading / hint / ChessDB work only when
/// [`BoardState::is_animating`] is false.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardState {
    /// The slide in progress, if any.
    pub anim: Option<Anim>,
}

impl BoardState {
    /// Whether a piece is still sliding.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.anim.is_some()
    }

    /// Start sliding a piece from `from` to `to`.
    pub fn start_anim(&mut self, from: Square, to: Square) {
        self.anim = Some(Anim::new(from, to));
    }

    /// Clear a finished animation. Returns true when an animation
    /// was cleared (the screen should then trigger the work that was
    /// waiting for the piece to land).
    pub fn finish_anim(&mut self, now: Instant) -> bool {
        if self.anim.is_some_and(|a| a.is_finished(now)) {
            self.anim = None;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Render description
// ---------------------------------------------------------------------------

/// Arrow colour (DESIGN-iced.md §1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArrowColor {
    /// Your suggestion (opening book).
    Blue,
    /// Opponent suggestion.
    Purple,
}

impl ArrowColor {
    /// The theme colour for this arrow.
    #[must_use]
    pub fn color(self) -> Color {
        match self {
            ArrowColor::Blue => BLUE,
            ArrowColor::Purple => PURPLE,
        }
    }
}

/// One suggestion arrow on the board.
#[derive(Clone, Debug)]
pub struct Arrow {
    /// Source square.
    pub from: Square,
    /// Destination square.
    pub to: Square,
    /// Arrow colour.
    pub color: ArrowColor,
    /// Dashed line (opponent moves, DESIGN-iced.md §1).
    pub dashed: bool,
    /// Badge text drawn at the arrow's midpoint (e.g. `"1"`–`"4"`).
    /// `None` means no badge.
    pub label: Option<String>,
    /// Highlighted (hovered) arrows draw at 100% opacity; the rest
    /// at 38% (DESIGN-iced.md §1).
    pub highlighted: bool,
}

impl Arrow {
    /// A plain suggestion arrow (blue, solid, with a number badge).
    #[must_use]
    pub fn suggestion(from: Square, to: Square, number: usize) -> Self {
        Self {
            from,
            to,
            color: ArrowColor::Blue,
            dashed: false,
            label: Some(number.to_string()),
            highlighted: false,
        }
    }

    /// An opponent-move arrow (purple, dashed, numbered badge —
    /// same order as the choice panel, so the numbers match).
    #[must_use]
    pub fn opponent(from: Square, to: Square, number: usize) -> Self {
        Self {
            from,
            to,
            color: ArrowColor::Purple,
            dashed: true,
            label: Some(number.to_string()),
            highlighted: false,
        }
    }
}

/// Everything [`BoardProgram`] needs to draw one frame. Screens
/// build this fresh in `view()` from their own state.
#[derive(Clone, Debug)]
pub struct BoardRender {
    /// The position to draw.
    pub position: Position,
    /// Selected square (blue ring), if any.
    pub selected: Option<Square>,
    /// Legal destinations for the selected piece (blue dots; a
    /// destination occupied by an enemy piece draws a red ring).
    pub legal_targets: Vec<Square>,
    /// Last move (both squares get a gold tint).
    pub last_move: Option<(Square, Square)>,
    /// Suggestion / opponent arrows.
    pub arrows: Vec<Arrow>,
    /// Square of the general in check (flashing red halo), if any.
    pub check_square: Option<Square>,
    /// The slide in progress (a snapshot of the screen's
    /// [`BoardState::anim`]); the sliding piece is drawn interpolated
    /// between its squares.
    pub anim: Option<Anim>,
    /// Check-halo pulse 0..1 (~2 Hz, from the screen's tick); 0.5
    /// when the screen does not tick.
    pub pulse: f32,
    /// Whether the board is flipped (Black at the bottom). Screens
    /// set this from their own `flipped` state (auto-flip when the
    /// human/learner plays Black, plus a manual Flip button).
    pub flipped: bool,
    /// Which colour's pieces can be dragged (`None` = dragging
    /// disabled — the board is read-only or no side may move yet).
    /// Only that colour's pieces lift off their square and follow
    /// the cursor; pressing any other piece still reports
    /// `DragStarted`, so a click on it can capture with the
    /// selected piece — the piece just never lifts.
    pub draggable: Option<SideColor>,
}

impl BoardRender {
    /// A render of `position` with nothing selected and no overlays.
    #[must_use]
    pub fn new(position: Position) -> Self {
        Self {
            position,
            selected: None,
            legal_targets: Vec::new(),
            last_move: None,
            arrows: Vec::new(),
            check_square: None,
            anim: None,
            pulse: 0.5,
            flipped: false,
            draggable: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Messages the board canvas sends to its owning screen.
#[derive(Debug, Clone)]
pub enum BoardMsg {
    /// A square was clicked (empty square, or a click that is not a
    /// drag — the screen decides select vs. move).
    SquareClicked(Square),
    /// A drag started on a piece (the screen should select it).
    DragStarted(Square),
    /// The dragged piece moved (cursor position, in canvas space).
    DragMoved(Point),
    /// A drag ended (drop position, in canvas space — use
    /// [`square_at_point`] with the canvas bounds to get the square;
    /// screens that keep the canvas size can also use
    /// [`Geom::square_at`]).
    DragEnded(Point),
    /// The hovered square changed (`None` when the cursor left the
    /// board). Screens use this for hover highlights.
    HoverSquare(Option<Square>),
}

// ---------------------------------------------------------------------------
// Drawing helpers (spike-proven)
// ---------------------------------------------------------------------------

/// Draw one piece: outer disc r=27, inner ring r=22, Han character
/// 30 px in the piece colour (DESIGN-iced.md §1).
/// One piece as a solid 3D block (Hieu, 2026-11-14 — like the
/// reference board: bevelled rim, face lit from above, soft shadow
/// on the board). Iced 0.14 has no radial gradient and no blur, so
/// the depth is layered vector shapes, bottom to top:
///
/// * two offset shadow circles (a faint wide one + a darker core),
///   which separate from the disc and fade as `lift` grows;
/// * the rim: a full disc in a darker wood tone — the visible ring
///   around the top face is the bevel / side wall, so the piece
///   reads as a block, not a flat disc;
/// * the top face: a diagonal linear gradient, light top-left to
///   darker bottom-right;
/// * the inner ring (as before) + a soft white highlight near the
///   top, inside the face;
/// * the glyph, with a 1 px dark copy underneath it for an
///   engraved look.
///
/// `lift` is 0 for a resting piece, 1 at the top of the pick-up arc
/// (the sliding / dragged piece); it grows the disc
/// (`LIFT_SCALE`), raises it, and thins its shadow.
fn draw_piece(frame: &mut Frame<Renderer>, center: Point, scale: f32, piece: Piece, lift: f32) {
    let color = match piece.color {
        SideColor::Red => RED_PIECE,
        SideColor::Black => BLACK_PIECE,
    };
    let r = PIECE_RADIUS * scale;
    let shadow_drop = (3.0 + 5.0 * lift) * scale;
    // Soft outer shadow, then the darker core shadow.
    frame.fill(
        &Path::circle(
            Point::new(center.x + 1.0 * scale, center.y + 4.0 * scale + shadow_drop),
            r * (1.0 + 0.05 * lift),
        ),
        Color {
            a: 0.14 * (1.0 - 0.3 * lift),
            ..Color::BLACK
        },
    );
    frame.fill(
        &Path::circle(
            Point::new(center.x + 1.0 * scale, center.y + 2.0 * scale + shadow_drop),
            r * 0.96,
        ),
        Color {
            a: 0.26 * (1.0 - 0.3 * lift),
            ..Color::BLACK
        },
    );
    // Bevelled rim (side wall) + gradient top face.
    frame.fill(&Path::circle(center, r), PIECE_RIM);
    let face_r = r - 2.5 * scale;
    frame.fill(
        &Path::circle(center, face_r),
        Gradient::Linear(
            Linear::new(
                Point::new(center.x - r * 0.5, center.y - r * 0.6),
                Point::new(center.x + r * 0.5, center.y + r * 0.6),
            )
            .add_stop(0.0, PIECE_FACE_TOP)
            .add_stop(1.0, PIECE_FACE_BOTTOM),
        ),
    );
    frame.stroke(
        &Path::circle(center, PIECE_INNER_RADIUS * scale),
        Stroke::default().with_color(color).with_width(1.0 * scale),
    );
    // Soft light catching the top of the face.
    frame.fill(
        &Path::circle(
            Point::new(center.x - r * 0.22, center.y - r * 0.42),
            r * 0.32,
        ),
        Color {
            a: 0.28,
            ..Color::WHITE
        },
    );
    // Engraved glyph: a dark copy 1 px below, then the glyph itself.
    let glyph_pos = Point::new(center.x, center.y + 1.2 * scale);
    frame.fill_text(CanvasText {
        content: piece.han_char().to_string(),
        position: glyph_pos,
        size: Pixels(theme::FONT_PIECE * scale),
        color: Color {
            a: 0.22,
            ..Color::BLACK
        },
        font: theme::cjk_font(),
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        ..Default::default()
    });
    frame.fill_text(CanvasText {
        content: piece.han_char().to_string(),
        position: center,
        size: Pixels(theme::FONT_PIECE * scale),
        color,
        font: theme::cjk_font(),
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        ..Default::default()
    });
}

/// The "horse leg" square a horse move bends through, if `from -> to`
/// is a horse move (ported from the egui app, `board_view::horse_leg`).
/// Squares are 0..89, `file = sq % 9`, `rank = sq / 9`: a (1, 2) move
/// bends through `(f0, r0 + dr.signum())`, a (2, 1) move through
/// `(f0 + df.signum(), r0)`; anything else is not a horse move.
#[must_use]
pub fn horse_leg(from: Square, to: Square) -> Option<Square> {
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

/// Screen points of an arrow's polyline, ready to stroke.
/// The arrowhead points at the TARGET square's centre — the
/// intersection of the two grid lines — never at the edge of the
/// piece there (Hieu, 2026-11-14). When a piece sits on the target
/// square the arrow is drawn on top of it (the board draws arrows
/// after the pieces), so the tip lands on the piece's face.
///
/// * Non-horse arrow: two points, `[start, end]` — `start` is the
///   source square's centre moved out by `PIECE_RADIUS + 4` (×
///   scale) so the line emerges from the source piece's edge
///   instead of crossing its face; `end` is the target square's
///   centre, unshortened.
/// * Horse arrow (`is_horse` and `horse_leg(from, to)` is `Some`):
///   three points, `[start, leg, end]` — the polyline bends through
///   the horse leg, like the egui app. `start` is the source
///   centre moved toward the leg by `PIECE_RADIUS + 4`; `end` is
///   the target square's centre, unshortened.
///
/// Returns an empty vec when `from == to` (nothing to draw).
#[must_use]
pub fn arrow_points(g: &Geom, from: Square, to: Square, is_horse: bool) -> Vec<Point> {
    let a = g.square_point(from);
    let b = g.square_point(to);
    let inset = PIECE_RADIUS * g.scale + 4.0 * g.scale;
    if is_horse {
        if let Some(leg) = horse_leg(from, to) {
            let leg_pt = g.square_point(leg);
            let dx = leg_pt.x - a.x;
            let dy = leg_pt.y - a.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1.0 {
                return Vec::new();
            }
            let start = Point::new(a.x + dx / len * inset, a.y + dy / len * inset);
            return vec![start, leg_pt, b];
        }
    }
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return Vec::new();
    }
    let (ux, uy) = (dx / len, dy / len);
    vec![Point::new(a.x + ux * inset, a.y + uy * inset), b]
}

/// The point `fraction` (0..1) of the way along a polyline, measured
/// by segment length. For a straight (2-point) polyline this is the
/// midpoint at 0.5; for a bent horse arrow it lands on whichever
/// segment the halfway mark falls on, so the badge stays on the bend.
#[must_use]
pub fn point_at_fraction(points: &[Point], fraction: f32) -> Point {
    if points.is_empty() {
        return Point::ORIGIN;
    }
    if points.len() == 1 {
        return points[0];
    }
    let total: f32 = points
        .windows(2)
        .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
        .sum();
    if total < 1e-6 {
        return points[0];
    }
    let mut target = fraction.clamp(0.0, 1.0) * total;
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let len = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
        if target <= len {
            let t = if len < 1e-6 { 0.0 } else { target / len };
            return Point::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
        }
        target -= len;
    }
    *points.last().unwrap()
}

/// Draw one suggestion arrow: line + V arrowhead + optional number
/// badge. `alpha` is 0.38 by default, 1.0 when the arrow is
/// highlighted. `is_horse` says the piece on `arrow.from` is a horse
/// (the caller reads it from the position); the arrow only bends when
/// the move is actually horse-shaped — see [`arrow_points`].
/// The badge sits at 50% of the polyline length (the midpoint for a
/// straight arrow, on the bend for a horse arrow).
fn draw_arrow(frame: &mut Frame<Renderer>, g: &Geom, arrow: &Arrow, is_horse: bool) {
    let points = arrow_points(g, arrow.from, arrow.to, is_horse);
    if points.len() < 2 {
        return;
    }
    let alpha = if arrow.highlighted {
        ARROW_ALPHA_HIGHLIGHTED
    } else {
        ARROW_ALPHA_DEFAULT
    };
    let col = Color {
        a: alpha,
        ..arrow.color.color()
    };
    // Thick and clear (Hieu 2026-11-13): 5 px at 1x, up from 3 px.
    let width = 5.0 * g.scale;
    let dash_segments = [10.0 * g.scale, 6.0 * g.scale];
    let mut stroke = Stroke::default().with_color(col).with_width(width);
    if arrow.dashed {
        stroke.line_dash = LineDash {
            segments: &dash_segments,
            offset: 0,
        };
    }
    // The polyline (one segment for a straight arrow, two for a bent
    // horse arrow); the dash pattern, if any, applies to both.
    for w in points.windows(2) {
        frame.stroke(&Path::line(w[0], w[1]), stroke);
    }
    // Arrowhead: two short lines forming a V at the end, pointing
    // along the LAST segment (the leg -> destination segment for a
    // bent horse arrow).
    let end = *points.last().unwrap();
    let before = points[points.len() - 2];
    let dx = end.x - before.x;
    let dy = end.y - before.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    // Bigger head to match the thicker shaft (16 px at 1x, was 12).
    let head = 16.0 * g.scale;
    let (px, py) = (-uy, ux);
    let b1 = Point::new(
        end.x - ux * head + px * head * 0.55,
        end.y - uy * head + py * head * 0.55,
    );
    let b2 = Point::new(
        end.x - ux * head - px * head * 0.55,
        end.y - uy * head - py * head * 0.55,
    );
    frame.stroke(&Path::line(end, b1), stroke);
    frame.stroke(&Path::line(end, b2), stroke);
    // Badge at 50% of the polyline length.
    if let Some(label) = &arrow.label {
        let mid = point_at_fraction(&points, 0.5);
        frame.fill(&Path::circle(mid, 11.0 * g.scale), col);
        frame.fill_text(CanvasText {
            content: label.clone(),
            position: mid,
            size: Pixels(13.0 * g.scale),
            color: Color::WHITE,
            font: Font::default(),
            align_x: Alignment::Center,
            align_y: Vertical::Center,
            ..Default::default()
        });
    }
}

// ---------------------------------------------------------------------------
// Canvas program
// ---------------------------------------------------------------------------

/// The board canvas program. Screens create one per `view()` from a
/// [`BoardRender`] snapshot; the drag state is internal
/// ([`DragState`]) and the animation snapshot comes from the screen's
/// [`BoardState`].
pub struct BoardProgram {
    /// What to draw this frame.
    pub render: BoardRender,
}

/// Drag state, owned by the canvas (not by the screen): press
/// position, whether the press turned into a drag, and the last
/// hovered square (so [`BoardMsg::HoverSquare`] only fires on change).
#[derive(Default)]
pub struct DragState {
    drag_sq: Option<Square>,
    drag_pos: Option<Point>,
    press_pos: Option<Point>,
    dragged: bool,
    last_hover: Option<Square>,
}

impl DragState {
    /// Whether the dragged piece is lifted off its square and drawn
    /// at the cursor. True only once the press has moved past the
    /// drag threshold (`dragged`); a plain press without movement
    /// (a click) leaves the piece on its square — otherwise the
    /// piece looks lifted off its position before the mouse is
    /// released, even though the user never dragged.
    fn lifted(&self) -> bool {
        self.dragged && self.drag_sq.is_some() && self.drag_pos.is_some()
    }
}

impl BoardProgram {
    /// A program rendering `render`.
    #[must_use]
    pub fn new(render: BoardRender) -> Self {
        Self { render }
    }

    /// A canvas widget for `render`, filling its parent.
    #[must_use]
    pub fn canvas(render: BoardRender) -> Element<'static, BoardMsg> {
        Canvas::new(Self::new(render))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl Program<BoardMsg> for BoardProgram {
    type State = DragState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let g = Geom::from_bounds_flipped(bounds, self.render.flipped);
        let scale = g.scale;
        let render = &self.render;

        // Background + wooden board (592×656, DESIGN-iced.md §1).
        // The board is a raised slab, like the reference: layered
        // soft shadows under it, a vertical wood gradient on its
        // face, a lighter inner edge (top light) and faint grain
        // streaks — Iced has no blur, so the shadow is three
        // expanding rects with falling alpha.
        frame.fill(&Path::rectangle(Point::ORIGIN, bounds.size()), theme::BG);
        let board_pos = Point::new(g.bx, g.by);
        let board_size = Size::new(theme::BOARD_WIDTH * scale, theme::BOARD_HEIGHT * scale);
        for (expand, alpha) in [(12.0, 0.08), (7.0, 0.12), (3.0, 0.18)] {
            frame.fill(
                &Path::rectangle(
                    Point::new(board_pos.x - expand * scale, board_pos.y - expand * scale),
                    Size::new(
                        board_size.width + 2.0 * expand * scale,
                        board_size.height + 2.0 * expand * scale,
                    ),
                ),
                Color {
                    a: alpha,
                    ..Color::BLACK
                },
            );
        }
        frame.fill(
            &Path::rectangle(board_pos, board_size),
            Gradient::Linear(
                Linear::new(
                    board_pos,
                    Point::new(board_pos.x, board_pos.y + board_size.height),
                )
                .add_stop(0.0, WOOD_TOP)
                .add_stop(1.0, WOOD_BOTTOM),
            ),
        );
        frame.stroke(
            &Path::rectangle(board_pos, board_size),
            Stroke::default()
                .with_color(BOARD_LINE)
                .with_width(2.0 * scale),
        );
        // Bevel: a lighter line just inside the border (top light).
        frame.stroke(
            &Path::rectangle(
                Point::new(board_pos.x + 2.0 * scale, board_pos.y + 2.0 * scale),
                Size::new(
                    board_size.width - 4.0 * scale,
                    board_size.height - 4.0 * scale,
                ),
            ),
            Stroke::default()
                .with_color(Color {
                    a: 0.45,
                    ..WOOD_EDGE_LIGHT
                })
                .with_width(1.5 * scale),
        );
        // Faint wood grain: three horizontal streaks across the face.
        for fy in [0.22, 0.52, 0.78] {
            let y = board_pos.y + board_size.height * fy;
            frame.stroke(
                &Path::line(
                    Point::new(board_pos.x + 4.0 * scale, y),
                    Point::new(board_pos.x + board_size.width - 4.0 * scale, y),
                ),
                Stroke::default()
                    .with_color(Color {
                        a: 0.05,
                        ..BOARD_LINE
                    })
                    .with_width(1.0 * scale),
            );
        }

        // Grid lines.
        let line = Stroke::default()
            .with_color(BOARD_LINE)
            .with_width(1.5 * scale);
        for rank in 0..10 {
            frame.stroke(&Path::line(g.point(0, rank), g.point(8, rank)), line);
        }
        for file in 0..9 {
            if file == 0 || file == 8 {
                // Edge files are the board border: continuous across
                // the river (the border rectangle covers them too).
                frame.stroke(&Path::line(g.point(file, 0), g.point(file, 9)), line);
            } else {
                // No vertical line across the river (sở hà hán giới):
                // every inner file stops at the river banks (rank 4
                // and rank 5) instead of drawing squares over it.
                frame.stroke(&Path::line(g.point(file, 0), g.point(file, 4)), line);
                frame.stroke(&Path::line(g.point(file, 5), g.point(file, 9)), line);
            }
        }
        // Palace diagonals.
        for (f0, r0, f1, r1) in [(3, 0, 5, 2), (5, 0, 3, 2), (3, 7, 5, 9), (5, 7, 3, 9)] {
            frame.stroke(&Path::line(g.point(f0, r0), g.point(f1, r1)), line);
        }
        // Position dots (cannon / soldier points).
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
            frame.fill(&Path::circle(g.point(f, r), 2.5 * scale), BOARD_LINE);
        }
        // River text. Mirrored with the board when flipped.
        for (label, fx) in [("楚河", 1.5), ("漢界", 6.5)] {
            let fx = if g.flipped { 8.0 - fx } else { fx };
            frame.fill_text(CanvasText {
                content: label.to_string(),
                position: Point::new(
                    g.bx + g.margin + fx * g.cell,
                    g.by + g.margin + 4.5 * g.cell,
                ),
                size: Pixels(28.0 * scale),
                color: BOARD_LINE,
                font: theme::cjk_font(),
                align_x: Alignment::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });
        }

        // Last move: gold tint.
        if let Some((from, to)) = render.last_move {
            for sq in [from, to] {
                frame.fill(
                    &Path::circle(g.square_point(sq), PIECE_RADIUS * scale),
                    Color { a: 0.30, ..GOLD },
                );
            }
        }
        // Selection: blue ring.
        if let Some(sq) = render.selected {
            frame.stroke(
                &Path::circle(g.square_point(sq), 30.0 * scale),
                Stroke::default().with_color(BLUE).with_width(3.0 * scale),
            );
        }
        // Legal targets: blue dots; captures: red rings.
        for sq in &render.legal_targets {
            if render.position.piece_at(*sq).is_some() {
                frame.stroke(
                    &Path::circle(g.square_point(*sq), PIECE_RADIUS * scale),
                    Stroke::default().with_color(RED).with_width(3.0 * scale),
                );
            } else {
                frame.fill(&Path::circle(g.square_point(*sq), 6.0 * scale), BLUE);
            }
        }
        // Flashing check halo.
        if let Some(sq) = render.check_square {
            let radius = 30.0 * scale + 4.0 * scale * render.pulse;
            let alpha = 0.35 + 0.45 * render.pulse;
            frame.stroke(
                &Path::circle(g.square_point(sq), radius),
                Stroke::default()
                    .with_color(Color { a: alpha, ..RED })
                    .with_width(4.0 * scale),
            );
        }

        // Pieces (skip the dragged one and the sliding one — they are
        // drawn separately below). The dragged piece is only skipped
        // once it is actually lifted (the press moved past the drag
        // threshold); a plain press without movement leaves it on
        // its square.
        let anim_to = render.anim.map(|a| a.to);
        for sq in 0..90u8 {
            if (state.lifted() && Some(sq) == state.drag_sq) || Some(sq) == anim_to {
                continue;
            }
            if let Some(piece) = render.position.piece_at(sq) {
                draw_piece(&mut frame, g.square_point(sq), scale, piece, 0.0);
            }
        }
        // Suggestion / opponent arrows, drawn ON TOP of the pieces:
        // the tip points at the target square's centre, so when a
        // piece sits there the arrow overlays its face (Hieu,
        // 2026-11-14). A horse arrow bends through its leg; the
        // moving piece is read from the position, so `Arrow` needs
        // no `horse` flag and no screen changes.
        for arrow in &render.arrows {
            let is_horse = render
                .position
                .piece_at(arrow.from)
                .is_some_and(|p| p.kind == PieceKind::Horse);
            draw_arrow(&mut frame, &g, arrow, is_horse);
        }
        // Sliding piece, interpolated — and lifted: it rises off the
        // board mid-flight (bigger, higher, thinner shadow) and
        // settles back down onto its square (Hieu, 2026-11-14).
        if let Some(anim) = render.anim {
            if let Some(piece) = render.position.piece_at(anim.to) {
                let a = g.square_point(anim.from);
                let b = g.square_point(anim.to);
                let now = Instant::now();
                let prog = anim.progress(now);
                let lift = anim.lift(now);
                let pos = Point::new(
                    a.x + (b.x - a.x) * prog,
                    a.y + (b.y - a.y) * prog - LIFT_RISE * scale * lift,
                );
                draw_piece(
                    &mut frame,
                    pos,
                    scale * (1.0 + LIFT_SCALE * lift),
                    piece,
                    lift,
                );
            }
        }
        // Dragged piece follows the cursor, on top — only once the
        // press has turned into a real drag. Before that (plain
        // click, no movement) the piece stays on its square. While
        // dragged it is fully lifted, like mid-slide.
        if state.lifted() {
            if let (Some(sq), Some(pos)) = (state.drag_sq, state.drag_pos) {
                if let Some(piece) = render.position.piece_at(sq) {
                    draw_piece(&mut frame, pos, scale * (1.0 + LIFT_SCALE), piece, 1.0);
                    frame.stroke(
                        &Path::circle(pos, 30.0 * scale),
                        Stroke::default().with_color(BLUE).with_width(3.0 * scale),
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<BoardMsg>> {
        let g = Geom::from_bounds_flipped(bounds, self.render.flipped);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let sq = g.square_at(pos)?;
                if let Some(piece) = self.render.position.piece_at(sq) {
                    // Only the draggable colour's pieces lift and
                    // follow the cursor. Pressing any other piece
                    // still reports `DragStarted`, so the screen can
                    // capture with the selected piece on a click —
                    // the piece just never lifts.
                    if self.render.draggable.is_some_and(|c| piece.color == c) {
                        state.drag_sq = Some(sq);
                        state.drag_pos = Some(pos);
                        state.press_pos = Some(pos);
                        state.dragged = false;
                    }
                    return Some(Action::publish(BoardMsg::DragStarted(sq)));
                }
                Some(Action::publish(BoardMsg::SquareClicked(sq)))
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                if state.drag_sq.is_some() {
                    if let Some(press) = state.press_pos {
                        let dx = pos.x - press.x;
                        let dy = pos.y - press.y;
                        if dx * dx + dy * dy > 16.0 {
                            state.dragged = true;
                        }
                    }
                    state.drag_pos = Some(pos);
                    return Some(Action::publish(BoardMsg::DragMoved(pos)));
                }
                // Hover tracking (only fires when the square changes).
                let hover = g.square_at(pos);
                if hover != state.last_hover {
                    state.last_hover = hover;
                    return Some(Action::publish(BoardMsg::HoverSquare(hover)));
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                let from = state.drag_sq.take()?;
                state.drag_pos = None;
                state.press_pos = None;
                let was_dragged = std::mem::take(&mut state.dragged);
                if was_dragged {
                    if let Some(pos) = cursor.position_in(bounds) {
                        return Some(Action::publish(BoardMsg::DragEnded(pos)));
                    }
                    return Some(Action::request_redraw());
                }
                // A plain click on a piece was already reported as
                // DragStarted (the screen selects on that); a click
                // that started on a piece but ended elsewhere without
                // moving is a no-op here. A click on an empty square
                // was reported as SquareClicked on press.
                let _ = from;
                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if state.drag_sq.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if let Some(pos) = cursor.position_in(bounds) {
            let g = Geom::from_bounds_flipped(bounds, self.render.flipped);
            if let Some(sq) = g.square_at(pos) {
                if self.render.position.piece_at(sq).is_some() {
                    return mouse::Interaction::Pointer;
                }
            }
        }
        mouse::Interaction::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds_1x() -> Rectangle {
        Rectangle {
            x: 0.0,
            y: 0.0,
            width: 592.0,
            height: 656.0,
        }
    }

    #[test]
    fn lift_arc_is_zero_at_both_ends_and_one_mid_flight() {
        // The pick-up arc (Hieu, 2026-11-14): the piece rests on its
        // square at both ends and is highest halfway through.
        assert!((lift_at(0.0) - 0.0).abs() < 1e-6);
        assert!((lift_at(1.0) - 0.0).abs() < 1e-6);
        assert!((lift_at(0.5) - 1.0).abs() < 1e-6);
        assert!((lift_at(0.25) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((lift_at(0.75) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn anim_lift_follows_the_arc_and_duration_is_400_ms() {
        assert_eq!(ANIM_DURATION, Duration::from_millis(400));
        let anim = Anim {
            from: 0,
            to: 1,
            started: Instant::now() - Duration::from_millis(200),
        };
        let now = Instant::now();
        assert!(
            (anim.lift(now) - 1.0).abs() < 0.05,
            "mid-flight, fully lifted"
        );
        let fresh = Anim::new(0, 1);
        assert!(
            fresh.lift(Instant::now()) < 0.05,
            "just started, on the board"
        );
        let done = Anim {
            from: 0,
            to: 1,
            started: Instant::now() - Duration::from_millis(500),
        };
        assert!(done.lift(Instant::now()) < 1e-6, "finished, settled down");
        assert!(done.is_finished(Instant::now()));
    }

    #[test]
    fn geom_scale_is_min_of_width_and_height_ratios() {
        let g = Geom::from_bounds(bounds_1x());
        assert!((g.scale - 1.0).abs() < 1e-6);
        // Twice as wide as tall: height limits the scale.
        let g = Geom::from_bounds(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1184.0,
            height: 656.0,
        });
        assert!((g.scale - 1.0).abs() < 1e-6);
        // Both doubled: scale 2.
        let g = Geom::from_bounds(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1184.0,
            height: 1312.0,
        });
        assert!((g.scale - 2.0).abs() < 1e-6);
        assert!((g.cell - 128.0).abs() < 1e-6);
        assert!((g.margin - 80.0).abs() < 1e-6);
    }

    #[test]
    fn geom_piece_centres_follow_the_design_formula() {
        let g = Geom::from_bounds(bounds_1x());
        // Rank 0 (Red back rank) is the bottom row: y = 40 + 9·64.
        assert_eq!(g.point(0, 0), Point::new(40.0, 616.0));
        assert_eq!(g.point(8, 0), Point::new(552.0, 616.0));
        // Rank 9 (Black back rank) is the top row: y = 40.
        assert_eq!(g.point(0, 9), Point::new(40.0, 40.0));
        assert_eq!(g.point(4, 4), Point::new(296.0, 360.0));
        // Square helpers agree with file/rank helpers.
        assert_eq!(g.square_point(0), Point::new(40.0, 616.0));
        assert_eq!(g.square_point(89), Point::new(552.0, 40.0));
    }

    #[test]
    fn geom_hit_test_maps_points_to_squares() {
        let g = Geom::from_bounds(bounds_1x());
        assert_eq!(g.square_at(Point::new(40.0, 616.0)), Some(0));
        assert_eq!(g.square_at(Point::new(552.0, 40.0)), Some(89));
        assert_eq!(g.square_at(Point::new(296.0, 360.0)), Some(40));
        // Near a point (within half a cell) still hits it.
        assert_eq!(g.square_at(Point::new(50.0, 620.0)), Some(0));
        // Outside the board: no square.
        assert_eq!(g.square_at(Point::new(10.0, 10.0)), None);
        assert_eq!(g.square_at(Point::new(600.0, 700.0)), None);
    }

    #[test]
    fn geom_is_widget_local_not_window_absolute() {
        // The canvas Frame is widget-local, so a widget offset must
        // NOT shift the geometry (integration fix, 2026-11-13).
        let shifted = Rectangle {
            x: 200.0,
            y: 100.0,
            width: 592.0,
            height: 656.0,
        };
        let g = Geom::from_bounds(shifted);
        assert_eq!(g.point(0, 0), Point::new(40.0, 616.0));
        assert_eq!(g.square_at(Point::new(40.0, 616.0)), Some(0));
    }

    #[test]
    fn geom_flipped_mirrors_points_and_hit_test() {
        let g = Geom::from_bounds_flipped(bounds_1x(), true);
        // Rank 0 (Red) is drawn at the top row when flipped.
        assert_eq!(g.point(0, 0), Point::new(552.0, 40.0));
        assert_eq!(g.point(8, 9), Point::new(40.0, 616.0));
        // Hit-testing is the inverse: the same screen point maps to
        // the mirrored square.
        assert_eq!(g.square_at(Point::new(552.0, 40.0)), Some(0));
        assert_eq!(g.square_at(Point::new(40.0, 616.0)), Some(89));
        // Round-trip: every square's centre maps back to itself.
        for sq in 0..90u8 {
            assert_eq!(g.square_at(g.square_point(sq)), Some(sq));
        }
    }

    #[test]
    fn anim_start_and_finish() {
        let mut state = BoardState::default();
        assert!(!state.is_animating());
        state.start_anim(0, 9);
        assert!(state.is_animating());
        assert!(!state.finish_anim(Instant::now()));
        let later = Instant::now() + ANIM_DURATION + Duration::from_millis(10);
        assert!(state.finish_anim(later));
        assert!(!state.is_animating());
    }

    /// Distance from `p` to segment `a -> b` (0 when on the segment).
    fn dist_to_segment(p: Point, a: Point, b: Point) -> f32 {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len2 = dx * dx + dy * dy;
        let t = if len2 < 1e-12 {
            0.0
        } else {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        };
        let q = Point::new(a.x + dx * t, a.y + dy * t);
        ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
    }

    fn dist_to_polyline(p: Point, points: &[Point]) -> f32 {
        points
            .windows(2)
            .map(|w| dist_to_segment(p, w[0], w[1]))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn horse_leg_bends_correctly() {
        // Mirror of the egui test (board_view.rs): b0 -> c2 (file
        // 1->2, rank 0->2): leg is b1 (sq 10).
        assert_eq!(horse_leg(1, 20), Some(10));
        // b0 -> a2: leg b1 as well.
        assert_eq!(horse_leg(1, 18), Some(10));
        // b0 -> d1 (file 1->3, rank 0->1): leg c0 (sq 2).
        assert_eq!(horse_leg(1, 12), Some(2));
        // Not a horse move.
        assert_eq!(horse_leg(16, 13), None);
        // A (2, 1) move in the other direction: d1 -> b2, leg c1.
        assert_eq!(horse_leg(12, 19), Some(11));
        // Same square / straight move: no leg.
        assert_eq!(horse_leg(1, 1), None);
        assert_eq!(horse_leg(0, 9), None);
    }

    #[test]
    fn arrow_points_horse_bends_through_leg() {
        let g = Geom::from_bounds(bounds_1x());
        // Red horse on b0 (sq 1) -> c2 (sq 20), leg b1 (sq 10).
        let pts = arrow_points(&g, 1, 20, true);
        assert_eq!(pts.len(), 3, "bent arrow has 3 points, got {pts:?}");
        // The start is the source centre moved toward the leg by
        // PIECE_RADIUS + 4, so the line emerges from the source
        // piece's edge instead of crossing its face.
        let src = g.square_point(1);
        let leg = g.square_point(10);
        let start_d = ((pts[0].x - src.x).powi(2) + (pts[0].y - src.y).powi(2)).sqrt();
        assert!(
            (start_d - (PIECE_RADIUS + 4.0)).abs() < 1e-4,
            "start inset {start_d}"
        );
        assert!(dist_to_segment(pts[0], src, leg) < 1e-4);
        assert_eq!(pts[1], leg, "middle point is the leg");
        // The tip points at the destination square's centre
        // (unshortened) — it overlays a piece sitting there.
        assert_eq!(pts[2], g.square_point(20), "tip is the target centre");
    }

    #[test]
    fn arrow_points_non_horse_stays_straight() {
        let g = Geom::from_bounds(bounds_1x());
        // Same squares, but the moving piece is not a horse.
        let pts = arrow_points(&g, 1, 20, false);
        assert_eq!(pts.len(), 2, "straight arrow has 2 points");
        // The start is shortened by PIECE_RADIUS + 4, as before;
        // the tip is the target square's centre, unshortened.
        let a = g.square_point(1);
        let b = g.square_point(20);
        let start_d = ((pts[0].x - a.x).powi(2) + (pts[0].y - a.y).powi(2)).sqrt();
        assert!((start_d - (PIECE_RADIUS + 4.0)).abs() < 1e-4);
        assert_eq!(pts[1], b, "tip is the target centre");
        // A horse piece making a non-horse-shaped move stays straight.
        let pts = arrow_points(&g, 16, 13, true);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[1], g.square_point(13), "tip is the target centre");
        // A zero-length arrow has no points.
        assert!(arrow_points(&g, 5, 5, false).is_empty());
        assert!(arrow_points(&g, 5, 5, true).is_empty());
    }

    #[test]
    fn bent_arrow_badge_lies_on_the_polyline() {
        let g = Geom::from_bounds(bounds_1x());
        let pts = arrow_points(&g, 1, 20, true);
        let badge = point_at_fraction(&pts, 0.5);
        assert!(
            dist_to_polyline(badge, &pts) < 1e-4,
            "badge {badge:?} off the bent polyline {pts:?}"
        );
        // Halfway along the drawn polyline (first segment 33 px of
        // ~123.5 px total — the 31 px inset moved from the tip to
        // the tail) falls on the leg -> destination segment.
        assert!(dist_to_segment(badge, pts[1], pts[2]) < 1e-4);
        // A straight arrow's badge is still its midpoint.
        let straight = arrow_points(&g, 1, 20, false);
        let mid = point_at_fraction(&straight, 0.5);
        let expected = Point::new(
            (straight[0].x + straight[1].x) / 2.0,
            (straight[0].y + straight[1].y) / 2.0,
        );
        assert!((mid.x - expected.x).abs() < 1e-4 && (mid.y - expected.y).abs() < 1e-4);
    }

    #[test]
    fn board_render_defaults_are_empty() {
        let pos = Position::from_fen(xq_core::board::START_FEN).expect("start FEN");
        let render = BoardRender::new(pos);
        assert!(render.selected.is_none());
        assert!(render.legal_targets.is_empty());
        assert!(render.arrows.is_empty());
        assert!(render.check_square.is_none());
        assert!(render.anim.is_none());
    }

    #[test]
    fn press_without_movement_does_not_lift_piece() {
        // Press on the Red chariot (square 0, centre (40, 616) at 1x)
        // and release without moving: the piece must stay on its
        // square — it is only lifted once the pointer actually
        // moves past the drag threshold.
        let mut render = BoardRender::new(Position::startpos());
        render.draggable = Some(SideColor::Red);
        let program = BoardProgram::new(render);
        let mut state = DragState::default();
        let bounds = bounds_1x();
        let press = Point::new(40.0, 616.0);

        let action = program.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(Button::Left)),
            bounds,
            Cursor::Available(press),
        );
        assert!(action.is_some());
        assert_eq!(state.drag_sq, Some(0));
        assert!(!state.dragged);
        assert!(!state.lifted());

        // A 1 px wiggle stays below the 4 px threshold: still not lifted.
        program.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(41.0, 616.0),
            }),
            bounds,
            Cursor::Available(Point::new(41.0, 616.0)),
        );
        assert!(!state.dragged);
        assert!(!state.lifted());

        // Moving past the threshold turns the press into a drag: lifted.
        program.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(50.0, 616.0),
            }),
            bounds,
            Cursor::Available(Point::new(50.0, 616.0)),
        );
        assert!(state.dragged);
        assert!(state.lifted());

        // Release clears the drag.
        program.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(Button::Left)),
            bounds,
            Cursor::Available(Point::new(50.0, 616.0)),
        );
        assert_eq!(state.drag_sq, None);
        assert!(!state.lifted());
    }

    #[test]
    fn press_on_non_draggable_piece_does_not_lift() {
        // Only Red may be dragged: pressing a Black piece still
        // reports the click (so the screen can capture with the
        // selected piece) but the piece never lifts, however far
        // the pointer moves, and releasing reports nothing.
        let mut render = BoardRender::new(Position::startpos());
        render.draggable = Some(SideColor::Red);
        let program = BoardProgram::new(render);
        let mut state = DragState::default();
        let bounds = bounds_1x();
        // Black chariot, square 81, centre (40, 40) at 1x.
        let press = Point::new(40.0, 40.0);

        let action = program.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(Button::Left)),
            bounds,
            Cursor::Available(press),
        );
        assert!(action.is_some());
        assert_eq!(state.drag_sq, None);
        assert!(!state.dragged);
        assert!(!state.lifted());

        // Moving far past the threshold still does not lift it.
        program.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(200.0, 200.0),
            }),
            bounds,
            Cursor::Available(Point::new(200.0, 200.0)),
        );
        assert_eq!(state.drag_sq, None);
        assert!(!state.lifted());

        // Release reports nothing (no drag ever started).
        let action = program.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(Button::Left)),
            bounds,
            Cursor::Available(Point::new(200.0, 200.0)),
        );
        assert!(action.is_none());
        assert_eq!(state.drag_sq, None);
        assert!(!state.lifted());
    }
}
