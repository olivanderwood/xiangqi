//! Lesson picker screen (§6.3): Red/Black cards, lesson list, intro panel.

use crate::app::{Screen, XiangqiApp};
use crate::board_view::{draw_board, BoardDraw, BoardGeometry};
use crate::i18n::{self, Key};
use xq_book::Lesson;
use xq_core::board::Position;
use xq_core::notation;
use xq_core::types::Color;

pub struct LearnSelectState {
    pub side: Option<Color>,
    pub selected: Option<String>,
}

impl LearnSelectState {
    pub fn new() -> Self {
        Self {
            side: None,
            selected: None,
        }
    }
}

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

impl XiangqiApp {
    pub(crate) fn show_learn_select(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(i18n::t(Key::BackToMenu)).clicked() {
                self.goto(Screen::Menu);
            }
        });
        ui.add_space(8.0);
        // Two big side cards.
        ui.horizontal(|ui| {
            let size = egui::Vec2::new(280.0, 64.0);
            if ui
                .add_sized(size, egui::Button::new(i18n::t(Key::LearnFirst)))
                .clicked()
            {
                self.learn_select.side = Some(Color::Red);
                self.learn_select.selected = None;
            }
            if ui
                .add_sized(size, egui::Button::new(i18n::t(Key::LearnSecond)))
                .clicked()
            {
                self.learn_select.side = Some(Color::Black);
                self.learn_select.selected = None;
            }
        });
        let Some(side) = self.learn_select.side else {
            return;
        };
        ui.add_space(12.0);
        // Lesson list for that side.
        let lessons: Vec<Lesson> = self.book.by_learner(side).into_iter().cloned().collect();
        if lessons.is_empty() {
            ui.label(i18n::t(Key::NoLessonsYet));
            return;
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(420.0);
                for lesson in &lessons {
                    let progress = self.settings.progress.get(&lesson.id);
                    let mut label = format!(
                        "{} {}  {}",
                        lesson.name_vi,
                        lesson.name_zh,
                        "★".repeat(lesson.level as usize),
                    );
                    if let Some(p) = progress {
                        if p.completed {
                            label.push_str(&format!("  {}", i18n::t(Key::CompletedMark)));
                        }
                        label.push_str(&format!(
                            "  {}",
                            i18n::t(Key::BestScore)
                                .replace("{n}", &format!("{:.0}", p.best_book_ratio * 100.0))
                        ));
                    }
                    let selected =
                        self.learn_select.selected.as_deref() == Some(lesson.id.as_str());
                    if ui.selectable_label(selected, label).clicked() {
                        self.learn_select.selected = Some(lesson.id.clone());
                    }
                }
            });
            // Intro panel for the selected lesson.
            if let Some(id) = &self.learn_select.selected {
                if let Some(lesson) = lessons.iter().find(|l| &l.id == id) {
                    ui.vertical(|ui| {
                        ui.set_width(480.0);
                        ui.heading(lesson.name_vi.clone());
                        ui.label(lesson.name_zh.clone());
                        ui.label(
                            i18n::t(Key::LevelStars).replace("{n}", &lesson.level.to_string()),
                        );
                        ui.label(i18n::t(Key::IntroTitle));
                        ui.label(lesson.summary_vi.clone());
                        // Mini board after the first 3 main-branch plies.
                        let pos = main_branch_position(lesson, 3);
                        let size = egui::Vec2::new(300.0, 330.0);
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        let geom = BoardGeometry::new(size, false);
                        draw_board(
                            &ui.painter_at(rect),
                            rect,
                            &geom,
                            &pos,
                            &BoardDraw::default(),
                        );
                        ui.label(i18n::t(Key::SourcesTitle));
                        for src in &lesson.sources {
                            ui.hyperlink_to(src.title.clone(), src.url.clone());
                        }
                        if ui.button(i18n::t(Key::StartLesson)).clicked() {
                            let lesson = lesson.clone();
                            self.learn.start_lesson(lesson, ctx.input(|i| i.time));
                            self.goto(Screen::Learn);
                        }
                    });
                }
            }
        });
    }
}
