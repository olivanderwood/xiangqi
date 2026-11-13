//! Lesson-summary screen (§6.5, `xq-app/src/screens/summary.rs`):
//! per-move grade table, position viewer, Learn again / Other lesson
//! / Menu.
//!
//! The data is built on the Learn screen
//! ([`super::learn::State::build_summary`]) and handed over with
//! [`State::load`]; this screen only displays it. "Học lại bài này"
//! stores a [`LessonChoice`](super::learn::LessonChoice) for the
//! integration step to take with [`State::take_relearn`] and pass to
//! [`super::learn::State::start`] — the same handover as the lesson
//! picker (see `learn.rs` for the contract).

use iced::widget::{column, container, mouse_area, row, scrollable, text};
use iced::{Color, Element, Length, Subscription, Task};
use xq_core::analysis::Grade;
use xq_core::board::Position;

use crate::app::{Screen, Shared};
use crate::board::{BoardProgram, BoardRender};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::screens::learn::LessonChoice;
use crate::theme::{self, FONT_BODY, FONT_SCREEN_TITLE, SPACE_16, SPACE_24, SPACE_4, SPACE_8};
use crate::widgets::{primary_button_with_icon, secondary_button, section_title};

/// One graded learner move in the summary table.
#[derive(Clone, Debug)]
pub struct SummaryRow {
    /// Game ply (1-based) of the learner's move.
    pub ply: usize,
    /// The move, rendered in the user's notation.
    pub mv: String,
    /// The grade the Tutor gave it.
    pub grade: Grade,
    /// The correct ("nước đúng") move, if known.
    pub best: Option<String>,
}

/// Everything the Summary screen shows, built by the Learn screen.
#[derive(Clone, Debug)]
pub struct SummaryData {
    /// [`Lesson::id`](xq_book::Lesson::id) of the finished lesson.
    pub lesson_id: String,
    /// Display name of the lesson.
    pub lesson_name: String,
    /// One row per graded learner move.
    pub rows: Vec<SummaryRow>,
    /// Learner moves graded `Book`.
    pub book_moves: usize,
    /// Total graded learner moves.
    pub total: usize,
    /// Learner moves graded `Blunder`.
    pub blunders: usize,
    /// Learner moves graded `Inaccurate`.
    pub inaccuracies: usize,
    /// End note of the lesson (leaf verdict), if any.
    pub end_note: Option<String>,
    /// Static eval of the final position (learner's perspective).
    pub final_score: i32,
    /// Positions after each game ply (`positions[0]` = start).
    pub positions: Vec<Position>,
    /// Whether the learner played Red (for "Học lại bài này").
    pub learner_is_red: bool,
}

/// Colour of a grade label (same rule as the egui app).
#[must_use]
pub fn grade_color(grade: Grade) -> Color {
    match grade {
        Grade::Book | Grade::Best => theme::GREEN,
        Grade::Good | Grade::Acceptable => theme::TEXT_DIM,
        Grade::Inaccurate => Color::from_rgb8(0xEF, 0x6C, 0x00),
        Grade::Blunder => theme::RED,
    }
}

/// Summary screen state: the loaded data (if any), the selected row,
/// and a pending "learn again" pick for the integration step.
pub struct State {
    data: Option<SummaryData>,
    selected_ply: Option<usize>,
    pending_relearn: Option<LessonChoice>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// Show the position after this game ply in the viewer.
    SelectRow(usize),
    /// Learn this lesson again (stores the pick, then navigates).
    LearnAgain,
    /// A board interaction on the (non-interactive) viewer board.
    Ignored,
}

impl State {
    /// Create the summary screen (no data yet — see [`State::load`]).
    #[must_use]
    pub fn new(_shared: &Shared) -> Self {
        Self {
            data: None,
            selected_ply: None,
            pending_relearn: None,
        }
    }

    /// Load the summary built by the Learn screen.
    pub fn load(&mut self, data: SummaryData) {
        self.data = Some(data);
        self.selected_ply = None;
        self.pending_relearn = None;
    }

    /// Take the stored "learn again" pick (if any) so the
    /// integration step can hand it to the Learn screen. `None`
    /// when "Học lại bài này" was never pressed (or already taken).
    pub fn take_relearn(&mut self) -> Option<LessonChoice> {
        self.pending_relearn.take()
    }

    /// `Navigate` / `ShowSettings` / `ShowAbout` are intercepted by
    /// `App` before they reach here, so this never handles them; the
    /// match stays exhaustive for future variants.
    pub fn update(&mut self, msg: Msg, _shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout => Task::none(),
            Msg::Ignored => Task::none(),
            Msg::SelectRow(ply) => {
                self.selected_ply = Some(ply);
                Task::none()
            }
            Msg::LearnAgain => {
                let Some(data) = &self.data else {
                    return Task::none();
                };
                self.pending_relearn = Some(LessonChoice {
                    lesson_id: data.lesson_id.clone(),
                    learner_is_red: data.learner_is_red,
                });
                Task::done(Msg::Navigate(Screen::Learn))
            }
        }
    }

    /// Stats, grade table (click a row to view the position), viewer
    /// board, Learn again / Other lesson / Menu.
    pub fn view(&self, _shared: &Shared) -> Element<'_, Msg> {
        let Some(data) = &self.data else {
            return container(
                column![
                    text(i18n::t(Key::SummaryTitle)).size(FONT_SCREEN_TITLE),
                    secondary_button(
                        i18n::t(Key::OtherLesson),
                        Msg::Navigate(Screen::LearnSelect)
                    ),
                    secondary_button(i18n::t(Key::BackToMenu), Msg::Navigate(Screen::Home)),
                ]
                .spacing(12),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        };

        let mut col = column![
            text(i18n::t(Key::SummaryTitle))
                .size(FONT_SCREEN_TITLE)
                .font(theme::inter_font_semibold()),
            text(data.lesson_name.clone())
                .size(FONT_BODY)
                .font(theme::inter_font_semibold()),
            text(
                i18n::t(Key::CorrectBookMoves)
                    .replace("{n}", &data.book_moves.to_string())
                    .replace("{m}", &data.total.to_string()),
            )
            .size(FONT_BODY),
            text(
                i18n::t(Key::MistakesAndInaccuracies)
                    .replace("{n}", &data.blunders.to_string())
                    .replace("{m}", &data.inaccuracies.to_string()),
            )
            .size(FONT_BODY),
        ]
        .spacing(SPACE_8);
        if let Some(note) = &data.end_note {
            col = col.push(text(note.clone()).size(FONT_BODY));
        }
        col = col.push(
            text(i18n::t(Key::EngineScore).replace("{n}", &data.final_score.to_string()))
                .size(FONT_BODY),
        );
        col = col.push(section_title(i18n::t(Key::ViewPosition)));

        // Grade table (left) + position viewer (right).
        let mut table = column![].spacing(SPACE_4);
        for row_data in &data.rows {
            let selected = self.selected_ply == Some(row_data.ply);
            let row_el = row![
                text(format!("{}. {}", row_data.ply, row_data.mv))
                    .size(FONT_BODY)
                    .font(theme::inter_font_medium())
                    .width(Length::Fixed(140.0)),
                // Grade labels follow the current language
                // (`i18n::grade_label`).
                text(i18n::grade_label(row_data.grade))
                    .size(FONT_BODY)
                    .font(theme::inter_font_medium())
                    .width(Length::Fixed(140.0))
                    .style({
                        let grade = row_data.grade;
                        move |_: &iced::Theme| iced::widget::text::Style {
                            color: Some(grade_color(grade)),
                        }
                    }),
                text(row_data.best.clone().unwrap_or_default())
                    .size(FONT_BODY)
                    .font(theme::inter_font_medium()),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            let row_box = container(row_el)
                .padding(SPACE_4)
                .width(Length::Fill)
                .style(move |_: &iced::Theme| container::Style {
                    background: if selected {
                        Some(iced::Background::Color(Color {
                            a: 0.11,
                            ..theme::GOLD
                        }))
                    } else {
                        None
                    },
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            table = table.push(mouse_area(row_box).on_press(Msg::SelectRow(row_data.ply)));
        }
        let table_el = scrollable(table)
            .height(Length::Fixed(400.0))
            .width(Length::Fixed(420.0));

        let mut body = row![table_el].spacing(SPACE_16);
        if let Some(ply) = self.selected_ply {
            if let Some(pos) = data.positions.get(ply) {
                let viewer = container(
                    BoardProgram::canvas(BoardRender::new(pos.clone())).map(|_| Msg::Ignored),
                )
                .width(Length::Fixed(300.0))
                .height(Length::Fixed(330.0));
                body = body.push(viewer);
            }
        }
        col = col.push(body);

        col = col.push(
            row![
                primary_button_with_icon(i18n::t(Key::LearnAgain), Icon::BookOpen, Msg::LearnAgain),
                secondary_button(
                    i18n::t(Key::OtherLesson),
                    Msg::Navigate(Screen::LearnSelect)
                ),
                secondary_button(i18n::t(Key::BackToMenu), Msg::Navigate(Screen::Home)),
            ]
            .spacing(SPACE_8),
        );

        container(col)
            .padding(SPACE_24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// No background work (this screen has none, even when real).
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> SummaryData {
        SummaryData {
            lesson_id: "test-lesson".to_string(),
            lesson_name: "Test".to_string(),
            rows: vec![
                SummaryRow {
                    ply: 1,
                    mv: "P2-5".to_string(),
                    grade: Grade::Book,
                    best: Some("P2-5".to_string()),
                },
                SummaryRow {
                    ply: 3,
                    mv: "M2.3".to_string(),
                    grade: Grade::Acceptable,
                    best: Some("P8-5".to_string()),
                },
            ],
            book_moves: 1,
            total: 2,
            blunders: 0,
            inaccuracies: 0,
            end_note: None,
            final_score: 30,
            positions: vec![Position::startpos()],
            learner_is_red: true,
        }
    }

    #[test]
    fn load_stores_data_and_relearn_returns_choice() {
        let shared = Shared::new();
        let mut st = State::new(&shared);
        assert!(st.take_relearn().is_none());
        st.load(sample_data());
        assert!(st.data.is_some());
        // LearnAgain without an update does not store a pick yet.
        assert!(st.take_relearn().is_none());
        let _ = st.update(Msg::LearnAgain, &mut Shared::new());
        let choice = st.take_relearn().expect("relearn choice");
        assert_eq!(choice.lesson_id, "test-lesson");
        assert!(choice.learner_is_red);
        assert!(st.take_relearn().is_none());
    }

    #[test]
    fn select_row_remembers_ply() {
        let shared = Shared::new();
        let mut st = State::new(&shared);
        st.load(sample_data());
        let _ = st.update(Msg::SelectRow(3), &mut Shared::new());
        assert_eq!(st.selected_ply, Some(3));
    }

    #[test]
    fn grade_colors_distinguish_book_from_blunder() {
        assert_eq!(grade_color(Grade::Book), theme::GREEN);
        assert_eq!(grade_color(Grade::Blunder), theme::RED);
        assert_ne!(grade_color(Grade::Book), grade_color(Grade::Inaccurate));
    }
}
