//! Lesson picker screen: choose side + opening (DESIGN-iced.md
//! §2.3, `xq-app/src/screens/learn_select.rs`).
//!
//! Two big side cards (Tiên/Red, Hậu/Black), the lesson list for the
//! chosen side (name + level stars + completed mark, from the shared
//! embedded book), an intro panel with a mini board after the first
//! 3 main-branch plies, and "Bắt đầu học" (only enabled once a
//! lesson is picked).
//!
//! # Lesson handover (integration)
//!
//! Picking a lesson does not start it: [`Msg::StartLesson`] stores
//! the pick as a [`LessonChoice`](super::learn::LessonChoice) and
//! sends `Navigate(Screen::Learn)`; the coordinator then calls
//! [`State::take_choice`] and [`super::learn::State::start`] with it
//! (see `learn.rs` for the contract).

use iced::widget::{column, container, row, scrollable, text, Button};
use iced::{Element, Length, Subscription, Task};

use crate::app::{Screen, Shared};
use crate::board::{BoardProgram, BoardRender};
use crate::i18n::{self, Key};
use crate::screens::learn::LessonChoice;
use crate::theme::{self, FONT_BODY, FONT_SCREEN_TITLE};
use crate::widgets::{primary_button, secondary_button};
use xq_book::Lesson;
use xq_core::board::Position;
use xq_core::notation;
use xq_core::types::Color;

/// Position after the first `n` plies of the lesson's main branch
/// (following `main` children, else the first child).
#[must_use]
pub fn main_branch_position(lesson: &Lesson, plies: usize) -> Position {
    let mut pos = lesson
        .start_fen
        .as_deref()
        .and_then(|f| Position::from_fen(f).ok())
        .unwrap_or_else(Position::startpos);
    let mut level = lesson.tree.as_slice();
    for _ in 0..plies {
        let Some(node) = level.iter().find(|n| n.main).or_else(|| level.first()) else {
            break;
        };
        let Ok(mv) = notation::parse_cn(&pos, &node.mv) else {
            break;
        };
        pos.make_move(mv);
        level = node.next.as_slice();
    }
    pos
}

/// Learn-select screen state.
#[derive(Default)]
pub struct State {
    /// Chosen side (`None` until the learner picks Tiên/Hậu).
    pub side: Option<Color>,
    /// Chosen lesson id.
    pub selected: Option<String>,
    /// The pick, stored by [`Msg::StartLesson`] for the integration
    /// step to take with [`State::take_choice`].
    pending_choice: Option<LessonChoice>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Go to another screen (intercepted by `App::update`).
    Navigate(Screen),
    /// Open the Settings dialog (intercepted by `App::update`).
    ShowSettings,
    /// Open the About dialog (intercepted by `App::update`).
    ShowAbout,
    /// Pick a side (clears the lesson pick).
    SelectSide(Color),
    /// Pick a lesson.
    SelectLesson(String),
    /// Store the pick and go to the Learn screen.
    StartLesson,
    /// A board interaction on the (non-interactive) preview board.
    Ignored,
}

impl State {
    /// Create the picker (no side picked yet).
    #[must_use]
    pub fn new(_shared: &Shared) -> Self {
        Self::default()
    }

    /// Take the stored pick (if any) so the integration step can
    /// hand it to the Learn screen. `None` when "Bắt đầu học" was
    /// never pressed (or the pick was already taken).
    pub fn take_choice(&mut self) -> Option<LessonChoice> {
        self.pending_choice.take()
    }

    /// `Navigate` / `ShowSettings` / `ShowAbout` are intercepted by
    /// `App` before they reach here, so this never handles them; the
    /// match stays exhaustive for future variants.
    pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Navigate(_) | Msg::ShowSettings | Msg::ShowAbout => Task::none(),
            Msg::Ignored => Task::none(),
            Msg::SelectSide(side) => {
                self.side = Some(side);
                self.selected = None;
                Task::none()
            }
            Msg::SelectLesson(id) => {
                self.selected = Some(id);
                Task::none()
            }
            Msg::StartLesson => {
                let Some(id) = self.selected.clone() else {
                    return Task::none();
                };
                let Some(lesson) = shared.book.by_id(&id) else {
                    return Task::none();
                };
                self.pending_choice = Some(LessonChoice {
                    lesson_id: id,
                    learner_is_red: lesson.learner.color() == Color::Red,
                });
                Task::done(Msg::Navigate(Screen::Learn))
            }
        }
    }

    /// Side cards, lesson list for the chosen side, intro panel with
    /// a mini board, "Bắt đầu học".
    pub fn view(&self, shared: &Shared) -> Element<'_, Msg> {
        let header = row![
            secondary_button(i18n::t(Key::BackToMenu), Msg::Navigate(Screen::Home)),
            column![
                text(i18n::t(Key::LearnOpenings)).size(FONT_SCREEN_TITLE),
                text(i18n::t(Key::LearnSelectSubtitle)).size(FONT_BODY),
            ]
            .spacing(4),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        let side_button = |label: &'static str, side: Color| -> Element<'_, Msg> {
            if self.side == Some(side) {
                primary_button(label, Msg::SelectSide(side))
                    .width(Length::Fixed(300.0))
                    .height(Length::Fixed(72.0))
                    .into()
            } else {
                secondary_button(label, Msg::SelectSide(side))
                    .width(Length::Fixed(300.0))
                    .height(Length::Fixed(72.0))
                    .into()
            }
        };
        let sides = row![
            side_button(i18n::t(Key::LearnFirst), Color::Red),
            side_button(i18n::t(Key::LearnSecond), Color::Black),
        ]
        .spacing(12);

        let mut col = column![header, sides].spacing(12);
        let Some(side) = self.side else {
            return container(col)
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        let lessons: Vec<&Lesson> = shared.book.by_learner(side);
        if lessons.is_empty() {
            col = col.push(text(i18n::t(Key::NoLessonsYet)).size(FONT_BODY));
            return container(col)
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        // Lesson list (left) + intro panel (right).
        let mut list = column![].spacing(8);
        for lesson in &lessons {
            let progress = shared.settings.progress.get(&lesson.id);
            let mut label = format!("{}  {}", lesson.name_vi, "★".repeat(lesson.level as usize),);
            if let Some(p) = progress {
                if p.completed {
                    label.push_str(&format!("  {}", i18n::t(Key::CompletedMark)));
                }
            }
            let selected = self.selected.as_deref() == Some(lesson.id.as_str());
            // `label` is built per lesson (owned `String`): the button
            // takes it by value, so the row owns its text instead of
            // borrowing a local.
            let btn: Element<'_, Msg> = if selected {
                Button::new(text(label).size(FONT_BODY))
                    .on_press(Msg::SelectLesson(lesson.id.clone()))
                    .style(theme::primary_button_style)
                    .padding(12)
                    .width(Length::Fill)
                    .into()
            } else {
                Button::new(text(label).size(FONT_BODY))
                    .on_press(Msg::SelectLesson(lesson.id.clone()))
                    .style(theme::secondary_button_style)
                    .padding(12)
                    .width(Length::Fill)
                    .into()
            };
            list = list.push(btn);
        }
        let list_el = scrollable(list)
            .height(Length::Fill)
            .width(Length::Fixed(420.0));

        let mut body = row![list_el].spacing(12);
        if let Some(id) = &self.selected {
            if let Some(lesson) = lessons.iter().find(|l| &l.id == id) {
                let pos = main_branch_position(lesson, 3);
                let preview =
                    container(BoardProgram::canvas(BoardRender::new(pos)).map(|_| Msg::Ignored))
                        .width(Length::Fixed(300.0))
                        .height(Length::Fixed(330.0));
                // The intro only renders once a lesson is picked, so
                // "Bắt đầu học" is always enabled here.
                let start_btn: Element<'_, Msg> =
                    primary_button(i18n::t(Key::StartLesson), Msg::StartLesson).into();
                let intro = column![
                    text(lesson.name_vi.clone()).size(FONT_SCREEN_TITLE),
                    text(i18n::t(Key::LevelStars).replace("{n}", &lesson.level.to_string()))
                        .size(FONT_BODY),
                    text(i18n::t(Key::IntroTitle)).size(FONT_BODY),
                    text(lesson.summary_vi.clone()).size(FONT_BODY),
                    preview,
                    start_btn,
                ]
                .spacing(8)
                .width(Length::Fixed(480.0));
                body = body.push(intro);
            }
        }

        col = col.push(body);
        container(col)
            .padding(12)
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
    use crate::app::Shared;

    #[test]
    fn main_branch_position_advances_three_plies() {
        let shared = Shared::new();
        let lesson = shared.book.lessons.first().expect("a lesson");
        let pos = main_branch_position(lesson, 3);
        assert_ne!(pos, Position::startpos());
        let pos0 = main_branch_position(lesson, 0);
        assert_eq!(pos0, Position::startpos());
    }

    #[test]
    fn start_lesson_stores_choice_for_learn() {
        let mut shared = Shared::new();
        let mut st = State::new(&shared);
        assert!(st.take_choice().is_none());
        let _ = st.update(Msg::SelectSide(Color::Red), &mut shared);
        let lessons = shared.book.by_learner(Color::Red);
        assert!(!lessons.is_empty(), "a Red lesson exists");
        let id = lessons[0].id.clone();
        let _ = st.update(Msg::SelectLesson(id.clone()), &mut shared);
        let _ = st.update(Msg::StartLesson, &mut shared);
        let choice = st.take_choice().expect("choice stored");
        assert_eq!(choice.lesson_id, id);
        assert!(choice.learner_is_red);
        // Taking twice yields nothing the second time.
        assert!(st.take_choice().is_none());
    }

    #[test]
    fn start_lesson_without_pick_stores_nothing() {
        let mut shared = Shared::new();
        let mut st = State::new(&shared);
        let _ = st.update(Msg::StartLesson, &mut shared);
        assert!(st.take_choice().is_none());
    }

    #[test]
    fn picking_a_side_clears_the_lesson() {
        let mut shared = Shared::new();
        let mut st = State::new(&shared);
        let _ = st.update(Msg::SelectSide(Color::Red), &mut shared);
        let _ = st.update(Msg::SelectLesson("some-id".to_string()), &mut shared);
        assert_eq!(st.selected.as_deref(), Some("some-id"));
        let _ = st.update(Msg::SelectSide(Color::Black), &mut shared);
        assert!(st.selected.is_none());
    }
}
