//! Move grading, SEE and `MoveTag` (§4.5–4.6).

use crate::board::Position;
use crate::eval::piece_value_mg;
use crate::search::MoveTag;
use crate::types::{file_of, rank_of, Color, Move, Piece, PieceKind};

/// Grade of a learner's move (§4.5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Grade {
    Book,
    Best,
    Good,
    Acceptable,
    Inaccurate,
    Blunder,
}

impl Grade {
    /// Vietnamese label.
    #[must_use]
    pub fn label_vi(self) -> &'static str {
        match self {
            Grade::Book => "Đúng sách",
            Grade::Best => "Nước tốt nhất",
            Grade::Good => "Tốt",
            Grade::Acceptable => "Chấp nhận được",
            Grade::Inaccurate => "Thiếu chính xác",
            Grade::Blunder => "Sai lầm",
        }
    }
}

/// Grade a move from `delta = score(best) - score(move)` (§4.5).
#[must_use]
pub fn grade_from_delta(delta: i32, in_book: bool) -> Grade {
    if in_book {
        return Grade::Book;
    }
    match delta {
        i32::MIN..=20 => Grade::Best,
        21..=60 => Grade::Good,
        61..=150 => Grade::Acceptable,
        151..=300 => Grade::Inaccurate,
        _ => Grade::Blunder,
    }
}

// ---------------------------------------------------------------------------
// SEE
// ---------------------------------------------------------------------------

/// Does `piece` on `from` attack `target` given the occupancy `board`?
/// Cannon needs exactly one screen; a horse needs a clear leg.
fn attacks_square(board: &[Option<Piece>; 90], from: u8, piece: Piece, target: u8) -> bool {
    let ff = file_of(from);
    let fr = rank_of(from);
    let tf = file_of(target);
    let tr = rank_of(target);
    let df = tf - ff;
    let dr = tr - fr;
    match piece.kind {
        PieceKind::King => df.abs() + dr.abs() == 1,
        PieceKind::Advisor => df.abs() == 1 && dr.abs() == 1,
        PieceKind::Elephant => {
            if df.abs() == 2 && dr.abs() == 2 {
                let eye = ((fr + dr / 2) * 9 + (ff + df / 2)) as u8;
                board[eye as usize].is_none()
            } else {
                false
            }
        }
        PieceKind::Horse => {
            const OFFSETS: [(i32, i32); 8] = [
                (1, 2),
                (-1, 2),
                (1, -2),
                (-1, -2),
                (2, 1),
                (-2, 1),
                (2, -1),
                (-2, -1),
            ];
            OFFSETS.iter().any(|&(odf, odr)| {
                if df == odf && dr == odr {
                    // The leg is one step in the direction of the
                    // two-square component.
                    let (lf, lr) = if odf.abs() == 2 {
                        (ff + odf.signum(), fr)
                    } else {
                        (ff, fr + odr.signum())
                    };
                    board[(lr * 9 + lf) as usize].is_none()
                } else {
                    false
                }
            })
        }
        PieceKind::Rook | PieceKind::Cannon => {
            if df != 0 && dr != 0 {
                return false;
            }
            let step_f = df.signum();
            let step_r = dr.signum();
            let mut f = ff + step_f;
            let mut r = fr + step_r;
            let mut between = 0;
            while f != tf || r != tr {
                if board[(r * 9 + f) as usize].is_some() {
                    between += 1;
                }
                f += step_f;
                r += step_r;
            }
            match piece.kind {
                PieceKind::Rook => between == 0,
                PieceKind::Cannon => between == 1,
                _ => unreachable!(),
            }
        }
        PieceKind::Pawn => {
            let fwd = match piece.color {
                Color::Red => 1,
                Color::Black => -1,
            };
            let crossed = match piece.color {
                Color::Red => fr >= 5,
                Color::Black => fr <= 4,
            };
            if dr == fwd && df == 0 {
                true
            } else {
                crossed && dr == 0 && df.abs() == 1
            }
        }
    }
}

/// Least valuable attacker of `by` on `target`, if any. The king is never
/// an attacker in SEE (kings cannot be captured), and the piece standing
/// on `target` itself is not an attacker of its own square.
fn least_attacker(board: &[Option<Piece>; 90], target: u8, by: Color) -> Option<(u8, Piece)> {
    let mut best: Option<(u8, Piece)> = None;
    for s in 0..90u8 {
        if s == target {
            continue;
        }
        let Some(p) = board[s as usize] else {
            continue;
        };
        if p.color != by || p.kind == PieceKind::King {
            continue;
        }
        if !attacks_square(board, s, p, target) {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, bp)) => piece_value_mg(p.kind) < piece_value_mg(bp.kind),
        };
        if better {
            best = Some((s, p));
        }
    }
    best
}

/// Core SEE: net material gain for `side` capturing the piece on `target`
/// (whose value is `target_value`). If `first` is `Some`, that piece is
/// forced to be the first attacker (used for `see`, where the move being
/// evaluated is the first capture); with `None` the least valuable
/// attacker is used (used for the Hanging tag, where the enemy will use
/// its cheapest attacker).
///
/// The capture sequence is built as the list of attacking pieces in
/// order (`captors[0]` captures the target, `captors[1]` recaptures
/// `captors[0]`, ...). Backward induction then gives the value of the
/// exchange: at step `k` the side to move wins `c_k` (`c_0` = target
/// value, `c_k` = `captors[k-1]` for `k >= 1`) minus whatever the
/// opponent still gains by continuing, and a side only continues while
/// it still gains from it.
fn see_exchange(
    board: &mut [Option<Piece>; 90],
    side: Color,
    target: u8,
    target_value: i32,
    first: Option<(u8, Piece)>,
) -> i32 {
    let mut captors: Vec<i32> = Vec::new();
    let mut cur_side = side;
    if let Some((sq, piece)) = first {
        captors.push(piece_value_mg(piece.kind));
        board[sq as usize] = None;
        cur_side = side.opposite();
    }
    while let Some((sq, piece)) = least_attacker(board, target, cur_side) {
        captors.push(piece_value_mg(piece.kind));
        board[sq as usize] = None;
        cur_side = cur_side.opposite();
    }
    if captors.is_empty() {
        // No attacker at all (only possible when `first` is `None`).
        return 0;
    }
    let mut score = 0i32; // value of the exchange from step `k+1` onward
    for k in (0..captors.len()).rev() {
        let c = if k == 0 { target_value } else { captors[k - 1] };
        score = c - score.max(0);
    }
    score
}

/// Net gain for the side to move if it captures the piece on `sq`
/// (0 when the piece cannot be captured).
fn see_on_square(pos: &Position, sq: u8) -> i32 {
    let Some(target) = pos.piece_at(sq) else {
        return 0;
    };
    let mut board = [None; 90];
    for s in 0..90u8 {
        board[s as usize] = pos.piece_at(s);
    }
    see_exchange(
        &mut board,
        pos.side(),
        sq,
        piece_value_mg(target.kind),
        None,
    )
}

/// Static Exchange Evaluation of the capture `mv` (§4.6): net material
/// gain for the mover, assuming optimal recaptures. Cannon screens and
/// horse legs are honoured.
#[must_use]
pub fn see(pos: &Position, mv: Move) -> i32 {
    let Some(target) = pos.piece_at(mv.to) else {
        return 0;
    };
    let Some(moving) = pos.piece_at(mv.from) else {
        return 0;
    };
    let mut board = [None; 90];
    for s in 0..90u8 {
        board[s as usize] = pos.piece_at(s);
    }
    // The moving piece vacates `from` (which may open a discovered line)
    // and lands on `to`. The target value is the original piece on `to`,
    // not the mover now standing there.
    board[mv.from as usize] = None;
    board[mv.to as usize] = Some(moving);
    see_exchange(
        &mut board,
        pos.side(),
        mv.to,
        piece_value_mg(target.kind),
        Some((mv.from, moving)),
    )
}

// ---------------------------------------------------------------------------
// Move tags
// ---------------------------------------------------------------------------

/// Is `sq` attacked by any piece of colour `by`?
///
/// This is `Position::is_attacked` with one correction that matters for
/// the tags (but not for the engine, which only ever queries king
/// squares): `is_attacked` counts an enemy king anywhere on the same
/// file as attacking the square, which is the flying-general rule and
/// is only true when the square holds a king. Here a king attacks only
/// its adjacent squares; the flying-general case is added back
/// explicitly when `sq` holds a king.
fn is_attacked_by(pos: &Position, sq: u8, by: Color) -> bool {
    let mut board = [None; 90];
    for s in 0..90u8 {
        board[s as usize] = pos.piece_at(s);
    }
    for s in 0..90u8 {
        if s == sq {
            continue;
        }
        if let Some(p) = board[s as usize] {
            if p.color == by && attacks_square(&board, s, p, sq) {
                return true;
            }
        }
    }
    // Flying general: an enemy king on the same file with nothing
    // between attacks a king standing on `sq`.
    if let Some(king) = board[sq as usize] {
        if king.kind == PieceKind::King {
            let kf = file_of(sq);
            let kr = rank_of(sq);
            for s in 0..90u8 {
                if let Some(p) = board[s as usize] {
                    if p.color == by && p.kind == PieceKind::King && file_of(s) == kf {
                        let er = rank_of(s);
                        let (lo, hi) = if er < kr { (er, kr) } else { (kr, er) };
                        if (lo + 1..hi).all(|r| board[(r * 9 + kf) as usize].is_none()) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Rule-based tags for a move (§4.6), in table priority order, at most 2.
/// `MateIn` is not produced here (it needs the search score); `analyze`
/// adds it.
#[must_use]
pub fn tags_for(pos: &Position, m: Move) -> Vec<MoveTag> {
    let mover = pos.side();
    let enemy = mover.opposite();
    let Some(moving) = pos.piece_at(m.from) else {
        return Vec::new();
    };
    let captured = pos.piece_at(m.to);
    let mut child = pos.clone();
    child.make_move(m);

    // (priority, tag); lower priority number comes first in the table.
    let mut tagged: Vec<(u8, MoveTag)> = Vec::new();
    let add = |prio: u8, tag: MoveTag, tagged: &mut Vec<(u8, MoveTag)>| {
        if !tagged.iter().any(|(_, t)| *t == tag) {
            tagged.push((prio, tag));
        }
    };

    if child.legal_moves().is_empty() {
        add(0, MoveTag::Mate, &mut tagged);
    }
    if let Some(cap) = captured {
        add(2, MoveTag::Capture(cap.kind), &mut tagged);
    }
    if pos.gives_check(m) {
        add(3, MoveTag::Check, &mut tagged);
    }
    // Escape: the moving piece was attacked and undefended, and its
    // destination is safe.
    let attacked_before = is_attacked_by(pos, m.from, enemy);
    let defended_before = is_attacked_by(pos, m.from, mover);
    if attacked_before && !defended_before && !is_attacked_by(&child, m.to, enemy) {
        add(4, MoveTag::Escape(moving.kind), &mut tagged);
    }
    // Threat: after the move, the move attacks an enemy piece (not the
    // king) that is undefended, or worth more than the attacking piece
    // (the cheapest attacker; the king counts as worth 0 when it is the
    // only attacker). The threat must be attributable to this move:
    // either the piece just moved is the attacker, or the target was
    // not attacked before the move. Without that, every quiet move in
    // the startpos would be tagged Threat(Horse) because the cannons
    // on b2/h2 already attack the horses on b9/h9 through the black
    // cannons on b7/h7.
    let mut child_board = [None; 90];
    for s in 0..90u8 {
        child_board[s as usize] = child.piece_at(s);
    }
    let mut threat: Option<PieceKind> = None;
    let mut threat_value = -1;
    for s in 0..90u8 {
        let Some(p) = child.piece_at(s) else {
            continue;
        };
        if p.color != enemy || p.kind == PieceKind::King {
            continue;
        }
        if !is_attacked_by(&child, s, mover) {
            continue;
        }
        let attacker_value = match least_attacker(&child_board, s, mover) {
            Some((_, a)) => piece_value_mg(a.kind),
            // Only the king attacks: the king is worth 0 for comparison.
            None => piece_value_mg(PieceKind::King),
        };
        let defended = is_attacked_by(&child, s, enemy);
        let v = piece_value_mg(p.kind);
        if defended && v <= attacker_value {
            continue;
        }
        let moved_is_attacker = attacks_square(&child_board, m.to, moving, s);
        if !moved_is_attacker && is_attacked_by(pos, s, mover) {
            continue;
        }
        if v > threat_value {
            threat_value = v;
            threat = Some(p.kind);
        }
    }
    if let Some(k) = threat {
        add(5, MoveTag::Threat(k), &mut tagged);
    }
    // Defend: an own piece (other than the mover) that was attacked and
    // undefended before the move is defended afterwards.
    let mut defend: Option<PieceKind> = None;
    let mut defend_value = -1;
    for s in 0..90u8 {
        if s == m.from {
            continue;
        }
        let Some(p) = pos.piece_at(s) else {
            continue;
        };
        if p.color != mover || p.kind == PieceKind::King {
            continue;
        }
        if !is_attacked_by(pos, s, enemy) || is_attacked_by(pos, s, mover) {
            continue;
        }
        if !is_attacked_by(&child, s, mover) {
            continue;
        }
        let v = piece_value_mg(p.kind);
        if v > defend_value {
            defend_value = v;
            defend = Some(p.kind);
        }
    }
    if let Some(k) = defend {
        add(6, MoveTag::Defend(k), &mut tagged);
    }
    let back_rank = match mover {
        Color::Red => 0,
        Color::Black => 9,
    };
    if moving.kind == PieceKind::Rook {
        let leaves_back = rank_of(m.from) == back_rank;
        let dest_file = file_of(m.to);
        let mut open_file = true;
        for s in 0..90u8 {
            if let Some(p) = child.piece_at(s) {
                if p.color == mover && p.kind == PieceKind::Pawn && file_of(s) == dest_file {
                    open_file = false;
                    break;
                }
            }
        }
        if leaves_back || open_file {
            add(7, MoveTag::DevelopRook, &mut tagged);
        }
    }
    if moving.kind == PieceKind::Cannon && file_of(m.to) == 4 {
        add(8, MoveTag::CentralCannon, &mut tagged);
    }
    if moving.kind == PieceKind::Horse && rank_of(m.from) == back_rank {
        add(9, MoveTag::DevelopHorse, &mut tagged);
    }
    // Hanging: the enemy can capture the piece just moved for a net gain
    // (SEE < 0 from the mover's point of view).
    if see_on_square(&child, m.to) > 0 {
        add(10, MoveTag::Hanging(moving.kind), &mut tagged);
    }

    tagged.sort_by_key(|(p, _)| *p);
    tagged.truncate(2);
    tagged.into_iter().map(|(_, t)| t).collect()
}

/// Vietnamese explanation from at most 2 tags (§4.6).
#[must_use]
pub fn explain_tags(tags: &[MoveTag]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for t in tags.iter().take(2) {
        parts.push(match t {
            MoveTag::Mate => "Chiếu hết!".to_string(),
            MoveTag::MateIn(n) => format!("Chiếu hết sau {n} nước"),
            MoveTag::Capture(k) => format!("Ăn {} của đối phương", piece_name(*k)),
            MoveTag::Check => "Chiếu tướng, buộc đối phương phải đỡ".to_string(),
            MoveTag::Escape(k) => format!("Đưa {} thoát khỏi bị bắt", piece_name(*k)),
            MoveTag::Threat(k) => format!("Đe dọa bắt {}", piece_name(*k)),
            MoveTag::Defend(k) => format!("Bảo vệ {}", piece_name(*k)),
            MoveTag::DevelopRook => "Ra Xe, chiếm cột mở".to_string(),
            MoveTag::CentralCannon => "Pháo đầu, nhắm vào Tốt giữa".to_string(),
            MoveTag::DevelopHorse => "Lên Mã, phát triển quân".to_string(),
            MoveTag::Hanging(k) => format!("Cẩn thận: {} có thể bị ăn", piece_name(*k)),
        });
    }
    parts.join(". ")
}

fn piece_name(k: PieceKind) -> &'static str {
    match k {
        PieceKind::King => "Tướng",
        PieceKind::Advisor => "Sĩ",
        PieceKind::Elephant => "Tượng",
        PieceKind::Horse => "Mã",
        PieceKind::Rook => "Xe",
        PieceKind::Cannon => "Pháo",
        PieceKind::Pawn => "Tốt",
    }
}
