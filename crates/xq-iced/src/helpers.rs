//! Pure-logic helpers shared by the screens, ported from the egui
//! app (`crates/xq-app/src/app.rs`).
//!
//! These functions touch no widgets: they only format scores, render
//! move names and walk a [`Game`]. Screens call them from `update`
//! and `view` alike.

use crate::settings::NotationChoice;
use xq_core::board::Position;
use xq_core::eval;
use xq_core::game::Game;
use xq_core::notation;
use xq_core::types::Move;

/// Advantage of a hint score (centipawns, side-to-move's perspective)
/// as a short pawn string: `+0.8`, `-0.3`, `M3` for mate in 3.
/// Ported verbatim from `xq-app/src/app.rs::format_advantage`.
#[must_use]
pub fn format_advantage(cp: i32) -> String {
    if cp >= eval::MATE - 500 {
        return format!("M{}", (eval::MATE - cp + 1) / 2);
    }
    if cp <= -eval::MATE + 500 {
        return format!("-M{}", (eval::MATE + cp + 1) / 2);
    }
    format!("{:+.1}", cp as f32 / 100.0)
}

/// Render one move with the chosen notation. Ported from
/// `XiangqiApp::render_move` (`xq-app/src/app.rs`); the egui version
/// read the notation from `self.settings`, here the caller passes it
/// explicitly because this is a free function with no `Shared` in
/// scope.
#[must_use]
pub fn render_move(pos: &Position, mv: Move, notation: NotationChoice) -> String {
    match notation {
        NotationChoice::Vietnamese => notation::render_vn(pos, mv),
        NotationChoice::English => notation::render_en(pos, mv),
    }
}

/// Position after `ply` moves of `game` (0 = start position).
/// Ported from `XiangqiApp::position_at` (`xq-app/src/app.rs`).
#[must_use]
pub fn position_at(game: &Game, ply: usize) -> Position {
    let mut p = game.start.clone();
    for m in game.moves.iter().take(ply) {
        p.make_move(*m);
    }
    p
}

/// All moves of `game` rendered in the chosen notation, in order.
/// Ported from `XiangqiApp::move_strings` (`xq-app/src/app.rs`).
#[must_use]
pub fn move_strings(game: &Game, notation: NotationChoice) -> Vec<String> {
    let mut out = Vec::with_capacity(game.moves.len());
    let mut p = game.start.clone();
    for m in &game.moves {
        out.push(render_move(&p, *m, notation));
        p.make_move(*m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use xq_core::board::Position;

    fn start() -> Position {
        Position::from_fen(xq_core::board::START_FEN).expect("start FEN")
    }

    #[test]
    fn advantage_formats_as_pawns_with_sign() {
        assert_eq!(format_advantage(80), "+0.8");
        assert_eq!(format_advantage(-30), "-0.3");
        assert_eq!(format_advantage(0), "+0.0");
        assert_eq!(format_advantage(150), "+1.5");
    }

    #[test]
    fn advantage_formats_mate() {
        assert_eq!(format_advantage(29_999), "M1");
        assert_eq!(format_advantage(-29_999), "-M1");
        assert_eq!(format_advantage(29_901), "M50");
    }

    #[test]
    fn render_move_vietnamese_and_english() {
        let pos = start();
        // Red cannon on file 1 (sq 19) to file 4 (sq 22): short
        // Vietnamese code `P8-5`, WXF `C8=5` (`render_vn` is the
        // short code form, not the long "Pháo 2 bình 5" form —
        // that is `render_cn`).
        let mv = Move::new(19, 22);
        assert_eq!(render_move(&pos, mv, NotationChoice::Vietnamese), "P8-5");
        assert_eq!(render_move(&pos, mv, NotationChoice::English), "C8=5");
    }

    #[test]
    fn position_at_zero_is_start_and_one_applies_first_move() {
        let mut game = Game::new(start());
        let mv = Move::new(19, 22);
        game.moves.push(mv);
        assert_eq!(position_at(&game, 0), start());
        let after = position_at(&game, 1);
        assert!(after.piece_at(22).is_some());
        assert!(after.piece_at(19).is_none());
    }

    #[test]
    fn move_strings_renders_in_order() {
        let mut game = Game::new(start());
        game.moves.push(Move::new(19, 22));
        // Black horse on file 1 (sq 82) to file 2, rank 7 (sq 65).
        game.moves.push(Move::new(82, 65));
        let strings = move_strings(&game, NotationChoice::Vietnamese);
        assert_eq!(strings.len(), 2);
        assert_eq!(strings[0], "P8-5");
        assert_eq!(strings[1], "M2.3");
    }
}
