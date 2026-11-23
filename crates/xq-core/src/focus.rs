//! Focus mode (Hieu, 2026-11-23): a slow-play mode for games
//! against the AI. On the human's turn the clock runs per move:
//!
//! * the board takes no move before [`FOCUS_MIN_THINK`] has passed
//!   — the player must think at least one minute;
//! * reaching [`FOCUS_LIMIT`] without a move loses the game on
//!   time ([`Reason::Timeout`](crate::game::Reason)).
//!
//! These are pure functions of the elapsed time on the current turn,
//! so the UI owns the clock (`Instant`) and the rules stay testable
//! without one. The bounds are inclusive: at exactly 60 s a move is
//! allowed, at exactly 90 s the game is lost.

use std::time::Duration;

/// Minimum thinking time per human move.
pub const FOCUS_MIN_THINK: Duration = Duration::from_secs(60);

/// Per-move time limit: reaching this without a move loses the game.
pub const FOCUS_LIMIT: Duration = Duration::from_secs(90);

/// Whether a move may be played after thinking for `elapsed`.
#[must_use]
pub fn can_move(elapsed: Duration) -> bool {
    elapsed >= FOCUS_MIN_THINK
}

/// Whether the turn has run out of time after `elapsed`.
#[must_use]
pub fn timed_out(elapsed: Duration) -> bool {
    elapsed >= FOCUS_LIMIT
}

/// Time left until a move is allowed (zero once it is).
#[must_use]
pub fn wait_remaining(elapsed: Duration) -> Duration {
    FOCUS_MIN_THINK.saturating_sub(elapsed)
}

/// Time left until the turn times out (zero once it has).
#[must_use]
pub fn move_remaining(elapsed: Duration) -> Duration {
    FOCUS_LIMIT.saturating_sub(elapsed)
}

/// A duration as `m:ss` for the countdown (`90 s` → `"1:30"`).
#[must_use]
pub fn fmt_mmss(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_think_is_inclusive() {
        assert!(!can_move(Duration::from_secs(59)));
        assert!(!can_move(Duration::from_millis(59_999)));
        assert!(can_move(Duration::from_secs(60)));
        assert!(can_move(Duration::from_secs(61)));
    }

    #[test]
    fn limit_is_inclusive() {
        assert!(!timed_out(Duration::from_secs(89)));
        assert!(!timed_out(Duration::from_millis(89_999)));
        assert!(timed_out(Duration::from_secs(90)));
        assert!(timed_out(Duration::from_secs(120)));
    }

    #[test]
    fn remaining_times_saturate_at_zero() {
        assert_eq!(wait_remaining(Duration::from_secs(0)), FOCUS_MIN_THINK);
        assert_eq!(
            wait_remaining(Duration::from_secs(45)),
            Duration::from_secs(15)
        );
        assert_eq!(wait_remaining(Duration::from_secs(60)), Duration::ZERO);
        assert_eq!(wait_remaining(Duration::from_secs(200)), Duration::ZERO);
        assert_eq!(
            move_remaining(Duration::from_secs(80)),
            Duration::from_secs(10)
        );
        assert_eq!(move_remaining(Duration::from_secs(90)), Duration::ZERO);
        assert_eq!(move_remaining(Duration::from_secs(200)), Duration::ZERO);
    }

    #[test]
    fn mmss_formats_like_a_clock() {
        assert_eq!(fmt_mmss(Duration::from_secs(0)), "0:00");
        assert_eq!(fmt_mmss(Duration::from_secs(5)), "0:05");
        assert_eq!(fmt_mmss(Duration::from_secs(60)), "1:00");
        assert_eq!(fmt_mmss(Duration::from_secs(90)), "1:30");
        assert_eq!(fmt_mmss(Duration::from_secs(65)), "1:05");
    }
}
