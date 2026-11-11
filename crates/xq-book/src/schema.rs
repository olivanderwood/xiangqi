//! `openings.json` schema (§5.2).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct End {
    pub verdict: Verdict,
    pub note_vi: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    RedBetter,
    Equal,
    BlackBetter,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookNode {
    pub mv: String,
    #[serde(default)]
    pub note_vi: Option<String>,
    #[serde(default)]
    pub next: Vec<BookNode>,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub main: bool,
    #[serde(default)]
    pub trap: bool,
    #[serde(default)]
    pub source_ref: Option<usize>,
    #[serde(default)]
    pub end: Option<End>,
    /// Pikafish score of this move (centipawns, from the mover's
    /// perspective at the parent position — the same convention as the
    /// UCI MultiPV line scores). Written by `tools/gen-book` for EVERY
    /// node: the dataset is the book (Hieu, 2026-11-11), so the Tutor
    /// never calls a live engine for hints or grading — the same
    /// position always shows the same directions, scores and order.
    /// `None` only in hand-written books; `validate_book` requires it.
    #[serde(default)]
    pub score_cp: Option<i32>,
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub name_vi: String,
    pub name_zh: String,
    pub learner: Learner,
    pub level: u8,
    pub summary_vi: String,
    #[serde(default)]
    pub start_fen: Option<String>,
    #[serde(default = "default_max_plies")]
    pub max_plies: usize,
    pub sources: Vec<Source>,
    pub tree: Vec<BookNode>,
}

fn default_max_plies() -> usize {
    30
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Learner {
    Red,
    Black,
}

impl Learner {
    #[must_use]
    pub fn color(self) -> xq_core::Color {
        match self {
            Learner::Red => xq_core::Color::Red,
            Learner::Black => xq_core::Color::Black,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookFile {
    pub schema_version: u32,
    pub lessons: Vec<Lesson>,
}
