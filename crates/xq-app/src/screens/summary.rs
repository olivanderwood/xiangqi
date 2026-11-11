//! Summary screen (§6.5): per-move grade table, position viewer,
//! Learn again / Other lesson / Menu.

use crate::app::{big_button, section_title, Screen, XiangqiApp};
use crate::board_view::{draw_board, BoardDraw, BoardGeometry};
use crate::i18n::{self, Key};
use xq_core::analysis::Grade;
use xq_core::board::Position;

pub struct SummaryRow {
    /// Game ply (1-based) of the learner's move.
    pub ply: usize,
    pub mv: String,
    pub grade: Grade,
    pub best: Option<String>,
}

pub struct SummaryState {
    pub lesson_id: String,
    pub lesson_name: String,
    pub rows: Vec<SummaryRow>,
    pub book_moves: usize,
    pub total: usize,
    pub blunders: usize,
    pub inaccuracies: usize,
    pub end_note: Option<String>,
    pub final_score: i32,
    pub selected_ply: Option<usize>,
    pub positions: Vec<Position>,
    pub start: Position,
}

impl SummaryState {
    pub fn new() -> Self {
        Self {
            lesson_id: String::new(),
            lesson_name: String::new(),
            rows: Vec::new(),
            book_moves: 0,
            total: 0,
            blunders: 0,
            inaccuracies: 0,
            end_note: None,
            final_score: 0,
            selected_ply: None,
            positions: vec![Position::startpos()],
            start: Position::startpos(),
        }
    }
}

fn grade_color(grade: Grade) -> egui::Color32 {
    match grade {
        Grade::Book | Grade::Best => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
        Grade::Good | Grade::Acceptable => egui::Color32::GRAY,
        Grade::Inaccurate => egui::Color32::from_rgb(0xEF, 0x6C, 0x00),
        Grade::Blunder => egui::Color32::from_rgb(0xC0, 0x39, 0x2B),
    }
}

impl XiangqiApp {
    pub(crate) fn show_summary(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading(i18n::t(Key::SummaryTitle));
        ui.label(self.summary.lesson_name.clone());
        ui.label(
            i18n::t(Key::CorrectBookMoves)
                .replace("{n}", &self.summary.book_moves.to_string())
                .replace("{m}", &self.summary.total.to_string()),
        );
        ui.label(
            i18n::t(Key::MistakesAndInaccuracies)
                .replace("{n}", &self.summary.blunders.to_string())
                .replace("{m}", &self.summary.inaccuracies.to_string()),
        );
        if let Some(note) = &self.summary.end_note {
            ui.label(note.clone());
        }
        ui.label(i18n::t(Key::EngineScore).replace("{n}", &self.summary.final_score.to_string()));
        section_title(ui, i18n::t(Key::ViewPosition));
        ui.separator();

        ui.horizontal(|ui| {
            // Grade table.
            ui.vertical(|ui| {
                ui.set_width(420.0);
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        egui::Grid::new("summary_grid")
                            .num_columns(3)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for row in &self.summary.rows {
                                    let selected = self.summary.selected_ply == Some(row.ply);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            format!("{}. {}", row.ply, row.mv),
                                        )
                                        .clicked()
                                    {
                                        self.summary.selected_ply = Some(row.ply);
                                    }
                                    ui.colored_label(grade_color(row.grade), row.grade.label_vi());
                                    ui.label(row.best.clone().unwrap_or_default());
                                    ui.end_row();
                                }
                            });
                    });
            });
            // Position viewer for the selected row.
            if let Some(ply) = self.summary.selected_ply {
                if let Some(pos) = self.summary.positions.get(ply) {
                    let pos = pos.clone();
                    ui.vertical(|ui| {
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
                    });
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if big_button(ui, i18n::t(Key::LearnAgain)).clicked() {
                if let Some(lesson) = self.book.by_id(&self.summary.lesson_id).cloned() {
                    let now = ctx.input(|i| i.time);
                    self.learn.start_lesson(lesson, now);
                    self.goto(Screen::Learn);
                }
            }
            if big_button(ui, i18n::t(Key::OtherLesson)).clicked() {
                self.goto(Screen::LearnSelect);
            }
            if big_button(ui, i18n::t(Key::BackToMenu)).clicked() {
                self.goto(Screen::Menu);
            }
        });
    }
}
