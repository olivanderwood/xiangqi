//! `BookWalker`: follow one lesson's tree through a full game
//! (Hieu, 2026-11-19 — "Đánh với AI theo thế trận").
//!
//! The Tutor (`tutor.rs`) walks the same tree while teaching, but it
//! grades, snapshots and ends the lesson at a leaf — a game needs
//! none of that. It only needs to know, at the AI's turn, which book
//! replies are still available, and to notice when the game leaves
//! the book (then the engine takes over for the rest of the game).
//! This walker is that minimal piece, so the Play screen can reuse
//! the book without the Tutor's lesson state.
//!
//! The walker owns a clone of the lesson (same as `learn.rs`:
//! `Tutor<'static>` is built from a cloned lesson, because the
//! screens cannot borrow `Shared::book` across updates).

use crate::schema::{BookNode, Lesson};
use crate::tree::{node_at, TreePath};
use xq_core::board::Position;
use xq_core::game::Game;
use xq_core::notation::{parse_cn, parse_wxf};
use xq_core::types::Move;

/// Walk one lesson's tree through a game: the AI's opening replies
/// come from the tree (weighted pick, same RNG as the Tutor) while
/// the game is in it; once a move leaves the tree the game is out
/// of book for good (until an undo replays it back in).
pub struct BookWalker {
    pub lesson: Lesson,
    node_path: TreePath,
    pub out_of_book: bool,
    rng_state: u64,
}

impl BookWalker {
    #[must_use]
    pub fn new(lesson: Lesson, seed: u64) -> Self {
        Self {
            lesson,
            node_path: Vec::new(),
            out_of_book: false,
            rng_state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// xorshift64star: uniform enough for weighted child picks
    /// (same as the Tutor's).
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform double in [0, 1) from the high 53 bits.
    fn next_unit(&mut self) -> f64 {
        ((self.next_rand() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    /// Current tree node (children = legal book replies here).
    #[must_use]
    pub fn current_children(&self) -> &[BookNode] {
        node_at(&self.lesson.tree, &self.node_path)
            .map(|n| n.next.as_slice())
            .unwrap_or(&self.lesson.tree)
    }

    /// Score of the current node (`BookNode::score_cp`): the value
    /// of the position after the move that reached this node, from
    /// that move's mover's perspective. `None` at the root (no move
    /// played yet) or when the node stores no score.
    #[must_use]
    pub fn current_score(&self) -> Option<i32> {
        node_at(&self.lesson.tree, &self.node_path).and_then(|n| n.score_cp)
    }

    /// Parse one book move at a position (Chinese first, WXF
    /// fallback — §5.4 sources use both; same as the Tutor's).
    fn parse_book_mv(pos: &Position, s: &str) -> Option<Move> {
        parse_cn(pos, s).or_else(|_| parse_wxf(pos, s)).ok()
    }

    /// The book replies available at `pos` right now (parsed).
    /// Used by the Play screen's tests to pick an off-book move;
    /// the game itself only needs [`Self::next_move`].
    #[must_use]
    pub fn children_moves(&self, pos: &Position) -> Vec<Move> {
        self.current_children()
            .iter()
            .filter_map(|n| Self::parse_book_mv(pos, &n.mv))
            .collect()
    }

    /// Follow one played move. Returns false (and marks the game
    /// out of book) when the move is not a child of the current
    /// node — the caller then falls back to the engine.
    pub fn advance(&mut self, pos_before: &Position, mv: Move) -> bool {
        if self.out_of_book {
            return false;
        }
        let Some(idx) = self
            .current_children()
            .iter()
            .position(|n| Self::parse_book_mv(pos_before, &n.mv).is_some_and(|m| m == mv))
        else {
            self.out_of_book = true;
            return false;
        };
        self.node_path.push(idx);
        true
    }

    /// Rebuild the tree position from a full move list (after an
    /// undo): replay from the start, following each move while it
    /// is in the tree. Stops at the first off-book (or illegal)
    /// move — an undo past the book-exit point puts the game back
    /// in the book.
    pub fn rebuild(&mut self, start: &Position, moves: &[Move]) {
        self.node_path.clear();
        self.out_of_book = false;
        let mut game = Game::new(start.clone());
        for &mv in moves {
            let pos_before = game.pos.clone();
            if game.play(mv).is_err() {
                break;
            }
            if !self.advance(&pos_before, mv) {
                break;
            }
        }
    }

    /// The AI's next book move at `pos`: a weighted pick among the
    /// current node's children (same rule as
    /// `Tutor::next_opponent_move`). `None` when out of book, when
    /// the node has no children (leaf — the line is done), or when
    /// the picked child does not parse (then the engine takes over).
    /// Read-only for the tree position: the caller advances the
    /// walker by playing the move (see [`Self::advance`]).
    pub fn next_move(&mut self, pos: &Position) -> Option<Move> {
        if self.out_of_book {
            return None;
        }
        let children: Vec<(f64, String)> = self
            .current_children()
            .iter()
            .map(|n| (n.weight.max(0.0), n.mv.clone()))
            .collect();
        if children.is_empty() {
            return None;
        }
        let total: f64 = children.iter().map(|(w, _)| w).sum();
        if total <= 0.0 {
            return None;
        }
        let mut pick = self.next_unit() * total;
        let mut idx = children.len() - 1;
        for (i, (w, _)) in children.iter().enumerate() {
            pick -= w;
            if pick <= 0.0 {
                idx = i;
                break;
            }
        }
        Self::parse_book_mv(pos, &children[idx].1)
    }

    /// Still inside the lesson's tree.
    #[must_use]
    pub fn in_book(&self) -> bool {
        !self.out_of_book
    }

    /// Depth in the tree (plies played while in the book).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.node_path.len()
    }
}

#[cfg(test)]
mod tests {
    use super::BookWalker;
    use crate::schema::{BookNode, Learner, Lesson};
    use xq_core::board::Position;
    use xq_core::notation::parse_cn;

    fn node(mv: &str, main: bool, score_cp: i32, next: Vec<BookNode>) -> BookNode {
        BookNode {
            mv: mv.to_string(),
            note_vi: None,
            next,
            weight: 1.0,
            main,
            trap: false,
            source_ref: None,
            end: None,
            score_cp: Some(score_cp),
        }
    }

    /// 3-ply lesson: Red 炮二平五 -> Black 马8进7 (main) -> Red
    /// 马二进三 / 兵七进一 (same shape as the Tutor's test lesson).
    fn test_lesson() -> Lesson {
        Lesson {
            id: "test".to_string(),
            name_vi: "Test".to_string(),
            name_zh: "Test".to_string(),
            learner: Learner::Red,
            level: 1,
            summary_vi: String::new(),
            start_fen: None,
            max_plies: 40,
            sources: Vec::new(),
            tree: vec![node(
                "炮二平五",
                true,
                30,
                vec![node(
                    "马8进7",
                    true,
                    -10,
                    vec![
                        node("马二进三", true, 25, vec![]),
                        node("兵七进一", false, 15, vec![]),
                    ],
                )],
            )],
        }
    }

    #[test]
    fn walker_follows_the_book_and_picks_a_weighted_reply() {
        let lesson = test_lesson();
        let mut walker = BookWalker::new(lesson, 0x1234_5678);
        assert!(walker.in_book());
        assert_eq!(walker.depth(), 0);
        // Red's first move is the only child at the root.
        let start = Position::startpos();
        let first = walker.next_move(&start).expect("first book move");
        assert_eq!(
            first,
            parse_cn(&start, "炮二平五").expect("parse first move")
        );
        // Play it: the walker follows.
        assert!(walker.advance(&start, first));
        assert_eq!(walker.depth(), 1);
        // Black's reply: the only child here.
        let after_red = {
            let mut game = xq_core::game::Game::new(start.clone());
            game.play(first).expect("play first move");
            game.pos
        };
        let reply = walker.next_move(&after_red).expect("black reply");
        assert_eq!(reply, parse_cn(&after_red, "马8进7").expect("parse reply"));
        assert!(walker.advance(&after_red, reply));
        assert_eq!(walker.depth(), 2);
        // Red's turn: two children — the pick is one of them.
        let after_black = {
            let mut game = xq_core::game::Game::new(start.clone());
            game.play(first).expect("play first move");
            game.play(reply).expect("play reply");
            game.pos
        };
        let third = walker.next_move(&after_black).expect("third book move");
        let candidates = walker.children_moves(&after_black);
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&third));
        assert!(walker.advance(&after_black, third));
        assert_eq!(walker.depth(), 3);
        // The picked third move is a leaf: no children left, so the
        // line is done — the engine takes over from here.
        let after_third = {
            let mut game = xq_core::game::Game::new(start);
            game.play(first).expect("play first move");
            game.play(reply).expect("play reply");
            game.play(third).expect("play third move");
            game.pos
        };
        assert!(walker.next_move(&after_third).is_none());
        assert!(walker.in_book(), "leaf ends the line, not the book");
    }

    #[test]
    fn walker_leaves_the_book_on_an_off_book_move() {
        let lesson = test_lesson();
        let mut walker = BookWalker::new(lesson, 0x1234_5678);
        let start = Position::startpos();
        // A legal move the book does not teach.
        let off = parse_cn(&start, "兵三进一").expect("parse off-book move");
        assert!(!walker.advance(&start, off));
        assert!(!walker.in_book());
        assert_eq!(walker.depth(), 0);
        assert!(walker.next_move(&start).is_none());
        // Further advances stay out of book.
        assert!(!walker.advance(&start, off));
    }

    #[test]
    fn walker_rebuild_restores_the_tree_after_undo() {
        let lesson = test_lesson();
        let mut walker = BookWalker::new(lesson, 0x1234_5678);
        let start = Position::startpos();
        let first = parse_cn(&start, "炮二平五").expect("parse first move");
        let mut game = xq_core::game::Game::new(start.clone());
        game.play(first).expect("play first move");
        let pos_before_reply = game.pos.clone();
        let reply = parse_cn(&pos_before_reply, "马8进7").expect("parse reply");
        game.play(reply).expect("play reply");
        // Walk the two plies in.
        assert!(walker.advance(&start, first));
        assert!(walker.advance(&pos_before_reply, reply));
        assert_eq!(walker.depth(), 2);
        assert!(walker.in_book());
        // Undo the reply: back to one ply, still in the book.
        let moves = &game.moves[..1];
        walker.rebuild(&start, moves);
        assert_eq!(walker.depth(), 1);
        assert!(walker.in_book());
        // Undo everything: back to the root.
        walker.rebuild(&start, &[]);
        assert_eq!(walker.depth(), 0);
        assert!(walker.in_book());
        // Rebuild with an off-book move: out of book again.
        let off = parse_cn(&start, "兵三进一").expect("parse off-book move");
        walker.rebuild(&start, &[off]);
        assert!(!walker.in_book());
        // ...and rebuilding the book line puts it back in.
        walker.rebuild(&start, &[first]);
        assert!(walker.in_book());
        assert_eq!(walker.depth(), 1);
    }
}
