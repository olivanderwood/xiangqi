//! `xq-book`: opening book (`openings.json`) + tutor.

pub mod schema;
pub mod tree;
pub mod tutor;

pub use schema::{BookFile, BookNode, Lesson, Source};
pub use tutor::{direction_count, BookHint, LessonSummary, Tutor, TutorEvent};

use xq_core::Color;

#[derive(Debug)]
pub enum BookError {
    Json(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for BookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BookError::Json(e) => write!(f, "lỗi JSON sách khai cuộc: {e}"),
            BookError::Invalid(s) => write!(f, "sách khai cuộc không hợp lệ: {s}"),
        }
    }
}

impl std::error::Error for BookError {}

impl From<serde_json::Error> for BookError {
    fn from(e: serde_json::Error) -> Self {
        BookError::Json(e)
    }
}

pub struct Book {
    pub lessons: Vec<Lesson>,
}

impl Book {
    /// Load the embedded book: the Red lessons (`openings-red.json`)
    /// plus the Black lessons (`openings-black.json`), merged in that
    /// order. The book is split in two because a single 41 MB
    /// `openings.json` exceeds what the GitHub API accepts for a push
    /// (2026-11-12, 12 Black lessons); each half is under the limit.
    /// `gen-book` writes both files from `lessons-src.json`.
    pub fn load_embedded() -> Result<Book, BookError> {
        let red = include_str!("../data/openings-red.json");
        let black = include_str!("../data/openings-black.json");
        let mut lessons = Self::load_str(red)?.lessons;
        lessons.extend(Self::load_str(black)?.lessons);
        Ok(Book { lessons })
    }

    pub fn load_str(text: &str) -> Result<Book, BookError> {
        let file: BookFile = serde_json::from_str(text)?;
        if file.schema_version != 1 {
            return Err(BookError::Invalid(format!(
                "schema_version phải là 1, gặp {}",
                file.schema_version
            )));
        }
        Ok(Book {
            lessons: file.lessons,
        })
    }

    #[must_use]
    pub fn by_learner(&self, c: Color) -> Vec<&Lesson> {
        self.lessons
            .iter()
            .filter(|l| l.learner.color() == c)
            .collect()
    }

    #[must_use]
    pub fn by_id(&self, id: &str) -> Option<&Lesson> {
        self.lessons.iter().find(|l| l.id == id)
    }
}
