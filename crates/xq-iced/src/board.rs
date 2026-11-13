//! The Xiangqi board: geometry, piece drawing, suggestion arrows,
//! drag-and-drop and the 150 ms slide animation.
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
//! A move's slide animation (150 ms) must finish BEFORE the AI,
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
    Action, Canvas, Event, Frame, Geometry, LineDash, Path, Program, Stroke, Text as CanvasText,
};
use iced::widget::text::Alignment;
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme};
use xq_core::board::Position;
use xq_core::types::{file_of, rank_of, Color as SideColor, Piece};

use crate::theme::{
    self, BLACK_PIECE, BLUE, BOARD_LINE, GOLD, PIECE_BG, PIECE_INNER_RADIUS, PIECE_RADIUS, PURPLE,
    RED, RED_PIECE, WOOD,
};

/// A board square: `sq = rank * 9 + file`, rank 0 = Red's back rank
/// (same convention as `xq-core`).
pub type Square = u8;

/// How long a piece takes to slide to its destination (egui app §7.3).
pub const ANIM_DURATION: Duration = Duration::from_millis(150);

/// Default arrow opacity; the hovered arrow is drawn at full opacity
/// (DESIGN-iced.md §1).
pub const ARROW_ALPHA_DEFAULT: f32 = 0.38;
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

/// One sliding piece: from → to over [`ANIM_DURATION`] (150 ms).
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    /// Source square.
    pub from: Square,
    /// Destination square.
    pub to: Square,
    /// When the slide started.
    pub started: Instant,
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

    /// Slide progress 0..1 at `now` (1 once the duration elapsed).
    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        (now.duration_since(self.started).as_secs_f32() / ANIM_DURATION.as_secs_f32()).min(1.0)
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
fn draw_piece(frame: &mut Frame<Renderer>, center: Point, scale: f32, piece: Piece) {
    let color = match piece.color {
        SideColor::Red => RED_PIECE,
        SideColor::Black => BLACK_PIECE,
    };
    frame.fill(&Path::circle(center, PIECE_RADIUS * scale), PIECE_BG);
    frame.stroke(
        &Path::circle(center, PIECE_RADIUS * scale),
        Stroke::default().with_color(color).with_width(2.0 * scale),
    );
    frame.stroke(
        &Path::circle(center, PIECE_INNER_RADIUS * scale),
        Stroke::default().with_color(color).with_width(1.0 * scale),
    );
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

/// Draw one suggestion arrow: line + V arrowhead + optional number
/// badge at the midpoint. `alpha` is 0.38 by default, 1.0 when the
/// arrow is highlighted.
fn draw_arrow(frame: &mut Frame<Renderer>, g: &Geom, arrow: &Arrow) {
    let a = g.square_point(arrow.from);
    let b = g.square_point(arrow.to);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let alpha = if arrow.highlighted {
        ARROW_ALPHA_HIGHLIGHTED
    } else {
        ARROW_ALPHA_DEFAULT
    };
    let (ux, uy) = (dx / len, dy / len);
    let r = PIECE_RADIUS * g.scale;
    let start = Point::new(
        a.x + ux * (r + 4.0 * g.scale),
        a.y + uy * (r + 4.0 * g.scale),
    );
    let end = Point::new(
        b.x - ux * (r + 4.0 * g.scale),
        b.y - uy * (r + 4.0 * g.scale),
    );
    let col = Color {
        a: alpha,
        ..arrow.color.color()
    };
    let width = 3.0 * g.scale;
    let dash_segments = [10.0 * g.scale, 6.0 * g.scale];
    let mut stroke = Stroke::default().with_color(col).with_width(width);
    if arrow.dashed {
        stroke.line_dash = LineDash {
            segments: &dash_segments,
            offset: 0,
        };
    }
    frame.stroke(&Path::line(start, end), stroke);
    // Arrowhead: two short lines forming a V at `end`.
    let head = 12.0 * g.scale;
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
    // Badge at the middle of the arrow.
    if let Some(label) = &arrow.label {
        let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
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
        frame.fill(&Path::rectangle(Point::ORIGIN, bounds.size()), theme::BG);
        let board_pos = Point::new(g.bx, g.by);
        let board_size = Size::new(theme::BOARD_WIDTH * scale, theme::BOARD_HEIGHT * scale);
        frame.fill(&Path::rectangle(board_pos, board_size), WOOD);
        frame.stroke(
            &Path::rectangle(board_pos, board_size),
            Stroke::default()
                .with_color(BOARD_LINE)
                .with_width(2.0 * scale),
        );

        // Grid lines.
        let line = Stroke::default()
            .with_color(BOARD_LINE)
            .with_width(1.5 * scale);
        for rank in 0..10 {
            frame.stroke(&Path::line(g.point(0, rank), g.point(8, rank)), line);
        }
        for file in 0..9 {
            if file == 4 {
                // Middle file is split by the river.
                frame.stroke(&Path::line(g.point(4, 0), g.point(4, 4)), line);
                frame.stroke(&Path::line(g.point(4, 5), g.point(4, 9)), line);
            } else {
                frame.stroke(&Path::line(g.point(file, 0), g.point(file, 9)), line);
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

        // Suggestion / opponent arrows.
        for arrow in &render.arrows {
            draw_arrow(&mut frame, &g, arrow);
        }

        // Pieces (skip the dragged one and the sliding one — they are
        // drawn separately below).
        let anim_to = render.anim.map(|a| a.to);
        for sq in 0..90u8 {
            if Some(sq) == state.drag_sq || Some(sq) == anim_to {
                continue;
            }
            if let Some(piece) = render.position.piece_at(sq) {
                draw_piece(&mut frame, g.square_point(sq), scale, piece);
            }
        }
        // Sliding piece, interpolated.
        if let Some(anim) = render.anim {
            if let Some(piece) = render.position.piece_at(anim.to) {
                let a = g.square_point(anim.from);
                let b = g.square_point(anim.to);
                let prog = anim.progress(Instant::now());
                let pos = Point::new(a.x + (b.x - a.x) * prog, a.y + (b.y - a.y) * prog);
                draw_piece(&mut frame, pos, scale, piece);
            }
        }
        // Dragged piece follows the cursor, on top.
        if let (Some(sq), Some(pos)) = (state.drag_sq, state.drag_pos) {
            if let Some(piece) = render.position.piece_at(sq) {
                draw_piece(&mut frame, pos, scale, piece);
                frame.stroke(
                    &Path::circle(pos, 30.0 * scale),
                    Stroke::default().with_color(BLUE).with_width(3.0 * scale),
                );
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
                if self.render.position.piece_at(sq).is_some() {
                    state.drag_sq = Some(sq);
                    state.drag_pos = Some(pos);
                    state.press_pos = Some(pos);
                    state.dragged = false;
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
}
