//! Lesson picker screen: choose side + opening (DESIGN-iced.md
//! §2.3, `xq-app/src/screens/learn_select.rs`, mockup page 3).
//!
//! A centred 880 px card: title + subtitle, a "Bạn chơi bên"
//! label over two large Tiên/Hậu buttons, the lesson list for the
//! chosen side (name + level stars and a green Check icon when
//! completed, from the shared embedded book), an intro panel (name,
//! level, summary — no board preview), and "Bắt đầu học"
//! bottom-right (only enabled once a lesson is picked).
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
use crate::i18n::{self, Key};
use crate::icons::{icon_tinted, Icon, ICON_SIZE_SMALL};
use crate::screens::learn::LessonChoice;
use crate::theme::{
    self, BG, BORDER, FONT_BODY, FONT_CAPTION, FONT_CARD_TITLE, FONT_SCREEN_TITLE, GOLD, GREEN,
    RADIUS_10, RED, SPACE_12, SPACE_16, SPACE_24, SPACE_32, SPACE_8, SURFACE2, TEXT, TEXT_DIM,
};
use crate::widgets::{
    card_style, ghost_button_with_icon, header_divider, primary_button_with_icon, HEADER_HEIGHT,
};
use xq_book::Lesson;
use xq_core::types::Color;

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

    /// The Tiên/Hậu picker (mockup page 3, UI-FIX-2 part 3): a
    /// "Bạn chơi bên" label over two large side-by-side buttons,
    /// each 60 px tall with a 24 px piece glyph (帥 red / 將
    /// black) and an 18 px label. The old 320 px segmented pill
    /// (40 px buttons, 14 px text) was too easy to miss, so the
    /// pill is gone — the two buttons now fill the card width.
    /// Selected = gold fill + dark text, as in the sample;
    /// unselected Red = red text + red border on surface-2,
    /// unselected Black = darker fill + light text + grey border,
    /// so the sides read clearly even before either is picked.
    /// Behaviour is unchanged: each button sends
    /// [`Msg::SelectSide`].
    fn side_control(&self) -> Element<'_, Msg> {
        let side_button = |label: &'static str,
                           glyph: &'static str,
                           side: Color|
         -> Element<'_, Msg> {
            let selected = self.side == Some(side);
            let is_red = side == Color::Red;
            // One text colour for the whole button (glyph +
            // label): dark on gold when selected, bright red
            // for unselected Red, light for unselected Black.
            let ink = if selected {
                BG
            } else if is_red {
                RED
            } else {
                TEXT
            };
            let content = row![
                text(glyph)
                    .size(24.0)
                    .font(theme::cjk_font())
                    .style(move |_: &iced::Theme| iced::widget::text::Style { color: Some(ink) }),
                text(label)
                    .size(18.0)
                    .font(theme::inter_font_semibold())
                    .style(move |_: &iced::Theme| iced::widget::text::Style { color: Some(ink) }),
            ]
            .spacing(SPACE_8)
            .align_y(iced::Alignment::Center);
            Button::new(content)
                .on_press(Msg::SelectSide(side))
                .style(
                    move |_: &iced::Theme, status: iced::widget::button::Status| {
                        let fill = if selected {
                            match status {
                                iced::widget::button::Status::Hovered => {
                                    iced::Color::from_rgb8(0xE0, 0xB2, 0x5C)
                                }
                                iced::widget::button::Status::Pressed => {
                                    iced::Color::from_rgb8(0xB8, 0x8A, 0x3A)
                                }
                                _ => GOLD,
                            }
                        } else {
                            match status {
                                iced::widget::button::Status::Hovered => SURFACE2,
                                iced::widget::button::Status::Pressed => BORDER,
                                _ => {
                                    if is_red {
                                        SURFACE2
                                    } else {
                                        BG
                                    }
                                }
                            }
                        };
                        iced::widget::button::Style {
                            background: Some(iced::Background::Color(fill)),
                            text_color: ink,
                            border: iced::Border {
                                color: if selected {
                                    GOLD
                                } else if is_red {
                                    RED
                                } else {
                                    BORDER
                                },
                                width: 1.0,
                                radius: RADIUS_10.into(),
                            },
                            ..Default::default()
                        }
                    },
                )
                .height(Length::Fixed(60.0))
                .width(Length::Fill)
                .into()
        };
        column![
            text(i18n::t(Key::LearnSideLabel))
                .size(FONT_CARD_TITLE)
                .font(theme::inter_font_semibold()),
            row![
                side_button(i18n::t(Key::LearnFirst), "帥", Color::Red),
                side_button(i18n::t(Key::LearnSecond), "將", Color::Black),
            ]
            .spacing(SPACE_12),
        ]
        .spacing(SPACE_8)
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
                        // Intro panel: name, level, summary — no board
                        // preview (Hieu 2026-11-13: the board image
                        // in the opening intro is unnecessary).
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
        // The card is ~440 px tall with a lesson picked (intro panel
        // without the board preview), so it fits a 720 px window
        // under the 64 px header; it still scrolls vertically if the
        // window is shorter or the lesson list is long.
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
