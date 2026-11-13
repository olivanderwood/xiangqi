//! Lesson picker screen: choose side + opening (DESIGN-iced.md
//! §2.3, `xq-app/src/screens/learn_select.rs`, mockup page 3).
//!
//! A centred 880 px card: title + subtitle, a segmented Tiên/Hậu
//! control, the lesson list for the chosen side (name + level stars
//! and a green Check icon when completed, from the shared embedded
//! book), an intro panel with a mini board after the first 3
//! main-branch plies, and "Bắt đầu học" bottom-right (only enabled
//! once a lesson is picked).
//!
//! # Lesson handover (integration)
//!
//! Picking a lesson does not start it: [`Msg::StartLesson`] stores
//! the pick as a [`LessonChoice`](super::learn::LessonChoice) and
//! sends `Navigate(Screen::Learn)`; the coordinator then calls
//! [`State::take_choice`] and [`super::learn::State::start`] with it
//! (see `learn.rs` for the contract).

use iced::widget::{column, container, row, scrollable, text, Button, Space};
use iced::{Element, Length, Subscription, Task};

use crate::app::{Screen, Shared};
use crate::board::{BoardProgram, BoardRender};
use crate::i18n::{self, Key};
use crate::icons::{icon_tinted, Icon, ICON_SIZE_SMALL};
use crate::screens::learn::LessonChoice;
use crate::theme::{
    self, FONT_BODY, FONT_CAPTION, FONT_SCREEN_TITLE, GREEN, SPACE_12, SPACE_16, SPACE_24,
    SPACE_32, SPACE_4, SPACE_8, TEXT_DIM,
};
use crate::widgets::{
    card_style, ghost_button_with_icon, header_divider, primary_button_with_icon,
    segment_button_style, HEADER_HEIGHT,
};
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

    /// Header bar (mockup page 3): 64 px surface bar, ghost "Quay
    /// lại" with arrow icon, 20 px Semibold title.
    fn header() -> Element<'static, Msg> {
        container(
            row![
                ghost_button_with_icon(
                    i18n::t(Key::BackToMenu),
                    Icon::ArrowLeft,
                    Msg::Navigate(Screen::Home)
                ),
                text(i18n::t(Key::LearnOpenings))
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold())
                    .width(Length::Fill),
            ]
            .spacing(SPACE_16)
            .align_y(iced::Alignment::Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: SPACE_24,
            bottom: 0.0,
            left: SPACE_24,
        })
        .width(Length::Fill)
        .height(Length::Fixed(HEADER_HEIGHT))
        .style(|_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(theme::SURFACE)),
            ..Default::default()
        })
        .into()
    }

    /// The Tiên/Hậu segmented control (mockup page 3: 320 px pill,
    /// dark background, 40 px buttons, gold = selected). The mockup
    /// labels it "Bạn chơi bên", but there is no i18n key for that
    /// label (and `i18n.rs` is out of scope here), so the control
    /// stands alone — the two side buttons name the sides.
    fn side_control(&self) -> Element<'_, Msg> {
        let side_button = |label: &'static str, side: Color| -> Element<'_, Msg> {
            let selected = self.side == Some(side);
            Button::new(text(label).size(14.0).font(theme::inter_font_semibold()))
                .on_press(Msg::SelectSide(side))
                .style(segment_button_style(selected))
                .height(Length::Fixed(40.0))
                .width(Length::Fill)
                .into()
        };
        container(
            row![
                side_button(i18n::t(Key::LearnFirst), Color::Red),
                side_button(i18n::t(Key::LearnSecond), Color::Black),
            ]
            .spacing(SPACE_4),
        )
        .padding(SPACE_4)
        .width(Length::Fixed(320.0))
        .style(|_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(theme::BG)),
            border: iced::Border {
                color: theme::BORDER,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
    }

    /// One lesson row: name + level stars, and a green Check icon +
    /// "Đã hoàn thành" when the lesson is done. `CompletedMark`
    /// carries its own "✓" glyph, so the glyph is stripped here —
    /// the icon replaces it, it must not appear twice.
    fn lesson_row(&self, lesson: &Lesson, progress_completed: bool) -> Element<'_, Msg> {
        let mut label = row![
            text(format!(
                "{}  {}",
                lesson.name_vi,
                "★".repeat(lesson.level as usize),
            ))
            .size(FONT_BODY)
            .font(theme::inter_font_medium()),
            Space::new().width(Length::Fill),
        ]
        .spacing(SPACE_8)
        .align_y(iced::Alignment::Center);
        if progress_completed {
            let done_text = i18n::t(Key::CompletedMark)
                .trim_start_matches('✓')
                .trim_start();
            label = label.push(
                icon_tinted(Icon::Check, GREEN)
                    .width(ICON_SIZE_SMALL)
                    .height(ICON_SIZE_SMALL),
            );
            label = label.push(
                text(done_text)
                    .size(FONT_CAPTION)
                    .font(theme::inter_font_medium())
                    .style(|_: &iced::Theme| iced::widget::text::Style { color: Some(GREEN) }),
            );
        }
        let selected = self.selected.as_deref() == Some(lesson.id.as_str());
        let style = if selected {
            theme::primary_button_style
        } else {
            theme::secondary_button_style
        };
        Button::new(label)
            .on_press(Msg::SelectLesson(lesson.id.clone()))
            .style(style)
            .padding(SPACE_12)
            .width(Length::Fill)
            .into()
    }

    /// Centred 880 px card: title, subtitle, side control, lesson
    /// list + intro panel, "Bắt đầu học" bottom-right.
    pub fn view(&self, shared: &Shared) -> Element<'_, Msg> {
        let mut card_col = column![
            text(i18n::t(Key::LearnOpenings))
                .size(FONT_SCREEN_TITLE)
                .font(theme::inter_font_semibold()),
            text(i18n::t(Key::LearnSelectSubtitle))
                .size(FONT_BODY)
                .style(|_: &iced::Theme| iced::widget::text::Style {
                    color: Some(TEXT_DIM),
                }),
            self.side_control(),
        ]
        .spacing(SPACE_8);

        if let Some(side) = self.side {
            let lessons: Vec<&Lesson> = shared.book.by_learner(side);
            if lessons.is_empty() {
                card_col = card_col.push(text(i18n::t(Key::NoLessonsYet)).size(FONT_BODY));
            } else {
                // Lesson list (left, fixed height so the card's
                // height stays deterministic) + intro panel (right).
                let mut list = column![].spacing(SPACE_8);
                for lesson in &lessons {
                    let completed = shared
                        .settings
                        .progress
                        .get(&lesson.id)
                        .is_some_and(|p| p.completed);
                    list = list.push(self.lesson_row(lesson, completed));
                }
                let list_el = scrollable(list)
                    .height(Length::Fixed(320.0))
                    .width(Length::Fixed(380.0));

                let mut body = row![list_el].spacing(SPACE_16);
                if let Some(id) = &self.selected {
                    if let Some(lesson) = lessons.iter().find(|l| &l.id == id) {
                        let pos = main_branch_position(lesson, 3);
                        let preview = container(
                            BoardProgram::canvas(BoardRender::new(pos)).map(|_| Msg::Ignored),
                        )
                        .width(Length::Fixed(300.0))
                        .height(Length::Fixed(330.0));
                        let intro = column![
                            text(lesson.name_vi.clone())
                                .size(FONT_SCREEN_TITLE)
                                .font(theme::inter_font_semibold()),
                            text(
                                i18n::t(Key::LevelStars).replace("{n}", &lesson.level.to_string())
                            )
                            .size(FONT_BODY),
                            text(i18n::t(Key::IntroTitle)).size(FONT_BODY),
                            text(lesson.summary_vi.clone()).size(FONT_BODY),
                            preview,
                        ]
                        .spacing(SPACE_8)
                        .width(Length::Fill);
                        body = body.push(intro);
                    }
                }
                card_col = card_col.push(body);

                // "Bắt đầu học" (mockup: bottom-right, 200 × 48,
                // gold, BookOpen icon). Disabled-looking until a
                // lesson is picked — the button simply has no
                // `on_press` then, so Iced renders it disabled.
                let start_row = if self.selected.is_some() {
                    row![
                        Space::new().width(Length::Fill),
                        primary_button_with_icon(
                            i18n::t(Key::StartLesson),
                            Icon::BookOpen,
                            Msg::StartLesson
                        )
                        .width(Length::Fixed(200.0))
                        .height(Length::Fixed(48.0)),
                    ]
                } else {
                    row![
                        Space::new().width(Length::Fill),
                        Button::new(
                            row![
                                icon_tinted(Icon::BookOpen, TEXT_DIM),
                                text(i18n::t(Key::StartLesson))
                                    .size(FONT_BODY)
                                    .font(theme::inter_font_semibold()),
                            ]
                            .spacing(SPACE_8)
                            .align_y(iced::Alignment::Center),
                        )
                        .style(theme::secondary_button_style)
                        .padding(SPACE_12)
                        .width(Length::Fixed(200.0))
                        .height(Length::Fixed(48.0)),
                    ]
                };
                card_col = card_col.push(start_row);
            }
        }

        let card = container(card_col.spacing(SPACE_24))
            .padding(SPACE_32)
            .width(Length::Fixed(880.0))
            .style(card_style);
        // The card is ~770 px tall with a lesson picked (intro panel
        // + 330 px preview), taller than a 720 px window even before
        // the 64 px header — so it scrolls vertically instead of
        // clipping; at 800 px it still needs the scroll when the
        // intro is shown.
        column![
            Self::header(),
            header_divider(),
            container(
                scrollable(container(card).width(Length::Fill).center_x(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(SPACE_24),
        ]
        .spacing(0)
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
