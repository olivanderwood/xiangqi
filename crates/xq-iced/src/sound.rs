//! Sound effects for moves, captures, checks and checkmate
//! (Hieu, 2026-11-12).
//!
//! Tones are synthesized in code — no audio files are needed.
//! `rodio` plays them on its own thread, so `play_*` never blocks
//! the UI thread.
//!
//! # Wiring (for the integrator)
//!
//! The module is declared from `board_view.rs` (see the `#[path]`
//! declaration there) because `main.rs` is owned by the integrator;
//! the player lives at `crate::board_view::sound::SoundPlayer`. If
//! the integrator prefers a top-level module, add `mod sound;` to
//! `main.rs` and remove the `#[path]` declaration from `board_view.rs`
//! (the file itself stays at `crates/xq-app/src/sound.rs`).
//!
//! Create one player (e.g. in `XiangqiApp`) and keep it alive —
//! dropping it stops playback:
//! ```ignore
//! let mut sfx = SoundPlayer::new();
//! sfx.set_enabled(settings.sound);
//! ```
//! After a move is played, call exactly one of these, decided from
//! the position BEFORE the move and the position AFTER it:
//! ```ignore
//! // `pos_before` = position before the move, `pos_after` = after.
//! let captured = pos_before.piece_at(mv.to).is_some();
//! let gives_check = pos_after.in_check(pos_after.side());
//! let is_mate = gives_check && pos_after.legal_moves().is_empty();
//! if is_mate {
//!     sfx.play_mate();
//! } else if gives_check {
//!     sfx.play_check();
//! } else if captured {
//!     sfx.play_capture();
//! } else {
//!     sfx.play_move();
//! }
//! ```
//! (Checkmate wins over check, check wins over capture — a checking
//! capture plays the check sound, a mating move the mate sound.)
//!
//! If there is no audio device, `SoundPlayer` silently disables
//! itself: every `play_*` is a no-op and nothing ever panics.

// The player is constructed in `XiangqiApp` (shared via
// `SharedSound`) and called after a move is played.

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamBuilder};
use std::sync::{Arc, Mutex};
use xq_core::board::Position;
use xq_core::types::Move;

/// A sound player shared between the app and its screens
/// (`XiangqiApp` owns one; each screen holds a clone). The mutex is
/// only held for the short `play_*` call, never across a search.
pub type SharedSound = Arc<Mutex<SoundPlayer>>;

/// Play the sound for a move just played, decided from the position
/// before the move and the position after it. Checkmate wins over
/// check, check wins over capture. Never blocks, never panics: a
/// missing audio device or a busy player is silently a no-op.
pub fn sound_for_move(shared: &SharedSound, pos_before: &Position, mv: Move, pos_after: &Position) {
    let Ok(mut sfx) = shared.try_lock() else {
        return;
    };
    if !sfx.is_enabled() {
        return;
    }
    let captured = pos_before.piece_at(mv.to).is_some();
    let gives_check = pos_after.in_check(pos_after.side());
    let is_mate = gives_check && pos_after.legal_moves().is_empty();
    if is_mate {
        sfx.play_mate();
    } else if gives_check {
        sfx.play_check();
    } else if captured {
        sfx.play_capture();
    } else {
        sfx.play_move();
    }
}

/// Sample rate for the synthesized tones.
const SAMPLE_RATE: u32 = 44_100;

/// One sound player. Cheap to create; the audio stream is opened
/// lazily on the first `play_*` call, so constructing it never blocks
/// startup and never fails when there is no audio device.
pub struct SoundPlayer {
    enabled: bool,
    /// The output stream must be kept alive while sounds play.
    stream: Option<OutputStream>,
    /// Whether opening the stream has been attempted already.
    tried: bool,
}

impl SoundPlayer {
    /// Create a player (enabled). No audio device is touched yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: true,
            stream: None,
            tried: false,
        }
    }

    /// Turn sound on/off (follows `settings.sound`).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether sound is currently on.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Open the output stream on first use. A failure (no device)
    /// just leaves the player disabled — never a panic.
    fn ensure_stream(&mut self) {
        if self.tried {
            return;
        }
        self.tried = true;
        if let Ok(stream) = OutputStreamBuilder::open_default_stream() {
            self.stream = Some(stream);
        }
    }

    /// Queue synthesized samples for playback (non-blocking: the
    /// mixer plays them on rodio's audio thread).
    fn play_samples(&mut self, samples: Vec<f32>) {
        if !self.enabled || samples.is_empty() {
            return;
        }
        self.ensure_stream();
        if let Some(stream) = &self.stream {
            stream
                .mixer()
                .add(SamplesBuffer::new(1, SAMPLE_RATE, samples));
        }
    }

    /// A normal move: one short wooden "knock".
    pub fn play_move(&mut self) {
        self.play_samples(note(520.0, 0.08, 38.0, 0.55));
    }

    /// A capture: a lower, sharper and slightly longer knock.
    pub fn play_capture(&mut self) {
        let low = note(330.0, 0.14, 26.0, 0.7);
        let snap = note(660.0, 0.06, 55.0, 0.35);
        self.play_samples(mix(&low, &snap));
    }

    /// A check: two rising warning notes.
    pub fn play_check(&mut self) {
        let mut samples = note(880.0, 0.09, 10.0, 0.5);
        samples.extend(note(1174.0, 0.13, 8.0, 0.5));
        self.play_samples(samples);
    }

    /// Checkmate: a short rising arpeggio ending on a low, decisive note.
    pub fn play_mate(&mut self) {
        let mut samples = note(523.0, 0.12, 8.0, 0.5);
        samples.extend(note(659.0, 0.12, 8.0, 0.5));
        samples.extend(note(784.0, 0.12, 8.0, 0.5));
        samples.extend(note(1046.0, 0.16, 7.0, 0.5));
        samples.extend(note(392.0, 0.28, 6.0, 0.55));
        self.play_samples(samples);
    }
}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// One decaying sine note: `freq` Hz, `dur_secs` long, exponential
/// decay `decay` (/s) and peak amplitude `vol` (0..1). A fast decay
/// sounds like a wooden knock; a slow one like a bell.
fn note(freq: f32, dur_secs: f32, decay: f32, vol: f32) -> Vec<f32> {
    let n = (dur_secs * SAMPLE_RATE as f32).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SAMPLE_RATE as f32;
        let env = (-decay * t).exp();
        out.push(vol * (2.0 * std::f32::consts::PI * freq * t).sin() * env);
    }
    out
}

/// Mix two same-length sample buffers (the shorter one is padded with
/// silence). Used to give the capture knock a sharp overtone.
fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0);
        out.push(x.clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_has_expected_length_and_bounds() {
        let samples = note(440.0, 0.1, 10.0, 0.5);
        assert_eq!(samples.len(), (0.1 * SAMPLE_RATE as f32) as usize);
        assert!(samples.iter().all(|s| s.abs() <= 0.5 + 1e-6));
        assert!(samples[0].abs() < 0.01);
        assert!(!samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn mix_clamps_and_pads() {
        let a = vec![0.8, 0.8];
        let b = vec![0.8];
        let m = mix(&a, &b);
        assert_eq!(m.len(), 2);
        assert!((m[0] - 1.0).abs() < 1e-6);
        assert!((m[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn player_methods_do_not_panic_without_audio() {
        let mut p = SoundPlayer::new();
        p.set_enabled(true);
        p.play_move();
        p.play_capture();
        p.play_check();
        p.play_mate();
        p.set_enabled(false);
        p.play_move();
    }
}
