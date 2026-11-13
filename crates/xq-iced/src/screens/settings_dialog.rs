//! Settings + About dialogs (P4), shown as overlays on top of the
//! active screen (`App::view` stacks this over the screen with
//! `iced::widget::Stack` — Iced 0.14 has no modal dialog).
//!
//! Both dialogs share one module and one [`State`], distinguished
//! by [`Kind`]: `App` builds the state with [`State::settings`] or
//! [`State::about`] when it intercepts `ShowSettings` / `ShowAbout`,
//! and closes it on [`Msg::Close`]. The module follows the screen
//! contract (`State` / `Msg` / `update` / `view` / `subscription`)
//! except that there is no `State::new` — the two constructors above
//! are the entry points, because the dialog has a kind.
//!
//! # Settings — parity with the egui app
//!
//! Every control of `xq-app/src/app.rs::settings_sections`, in the
//! same order: language (VI/EN), sound + animation toggles, move
//! notation, difficulty 0–4 (custom-Elo slider when 4), engine
//! section (engine in use, threads 1–8, hash 0–1024 MB step 64 with
//! "auto" at 0, diversity / tournament / trace-log toggles,
//! Pikafish path). Every change applies at once and is saved
//! automatically (`Settings::save`); there is no Save button and no
//! "restore defaults" button, matching egui.
//!
//! Deliberate deviations from egui (see the report for P4):
//! * No `AboutNoSound` note: that egui label ("sound is only a
//!   stored toggle") is stale — the Iced app really plays sounds
//!   (rodio, `sound.rs`), so repeating it would be false.
//! * The "Thử lại Pikafish" button sends `Msg::RetryEngine`, which
//!   `App` intercepts (the install receiver lives on `App`, not in
//!   this module); the button renders disabled while an install is
//!   running (`App` passes `install_running` into `view`).
//! * The trace-log toggle syncs the live `EngineLog` in `Shared`
//!   at once (egui syncs it every frame; here the toggle is the
//!   only writer, so syncing on toggle is equivalent).
//! * "Mở engine.log" opens the folder holding the real log file
//!   (`shared.engine_log.path()`); when the log could not be
//!   created the button is hidden, like egui (which only shows it
//!   when `engine_log` is known).
//!
//! # About — parity with the egui app
//!
//! App name + version, Pikafish 2026-09-06, engine in use, NNUE
//! note, opening-book lesson count (from the embedded book),
//! font credit, sound + ChessDB notes, "Mở thư mục engine" (only
//! when Pikafish is locatable — resolved once, when the dialog
//! opens) and "Mở engine.log" (opens the log folder, see above).

use std::path::PathBuf;

use iced::widget::{
    column, container, mouse_area, radio, row, scrollable, slider, text, text_input, toggler,
    Button, Stack,
};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::app::Shared;
use crate::engine_manager::open_folder;
use crate::i18n::{self, difficulty_name, Key, Language};
use crate::settings::NotationChoice;
use crate::theme::{self, FONT_BODY, FONT_CAPTION, FONT_SCREEN_TITLE, SPACE_12, SPACE_16, SPACE_8};
use crate::widgets::{primary_button, secondary_button, section_title};

/// Which dialog this state shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Settings,
    About,
}

/// Dialog state: which dialog, the Pikafish-path draft (text input
/// needs owned state), and the resolved engine dir for About (found
/// once, when the dialog opens).
pub struct State {
    kind: Kind,
    pikafish_path: String,
    engine_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Close the dialog (intercepted by `App`, like `Navigate`).
    Close,
    /// UI language changed.
    LanguageChanged(Language),
    /// Sound on/off.
    SoundToggled(bool),
    /// Piece-slide animation on/off.
    AnimateToggled(bool),
    /// Move notation changed.
    NotationChanged(NotationChoice),
    /// Difficulty 0–4 changed.
    DifficultyChanged(u8),
    /// Custom Elo (difficulty 4) changed.
    CustomEloChanged(u16),
    /// Pikafish threads changed.
    ThreadsChanged(u32),
    /// Pikafish hash (MB) changed (0 = auto).
    HashChanged(u32),
    /// Varied Master moves on/off.
    DiversityToggled(bool),
    /// Tournament mode on/off.
    TournamentToggled(bool),
    /// Trace UCI lines into engine.log on/off.
    TraceLogToggled(bool),
    /// Pikafish path text edited.
    PikafishPathChanged(String),
    /// Re-run the Pikafish install with the current settings.
    /// Intercepted by `App` (which owns the install receiver);
    /// `update` below never acts on it.
    RetryEngine,
    /// Open the engine folder in the file manager (About).
    OpenEngineFolder,
    /// Open the engine-log folder in the file manager (About).
    OpenEngineLog,
}

impl State {
    /// Create the Settings dialog, drafting the Pikafish path from
    /// the current settings.
    #[must_use]
    pub fn settings(shared: &Shared) -> Self {
        Self {
            kind: Kind::Settings,
            pikafish_path: shared.settings.pikafish_path.clone(),
            engine_dir: None,
        }
    }

    /// Create the About dialog, resolving the engine dir once via
    /// `xq_pikafish::locate` (same lookup the installer uses, so
    /// "Mở thư mục engine" opens the dir holding the binary + NNUE).
    /// `None` when Pikafish cannot be located — the button is then
    /// hidden, like egui (which only shows it when `engine_dir` is
    /// known).
    #[must_use]
    pub fn about(shared: &Shared) -> Self {
        let engine_dir = xq_pikafish::locate(shared.settings.pikafish_path().as_deref())
            .ok()
            .map(|located| located.engine_dir);
        Self {
            kind: Kind::About,
            pikafish_path: shared.settings.pikafish_path.clone(),
            engine_dir,
        }
    }

    /// Which dialog this state shows (used by tests only).
    #[cfg(test)]
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Apply one dialog message. Every settings change writes
    /// through to `shared.settings` and saves immediately (no Save
    /// button). Language changes also call `i18n::set_language`,
    /// and because every `view` calls `i18n::t` at render time the
    /// whole app re-renders in the new language on the next frame —
    /// no per-screen refresh is needed. Sound changes also sync the
    /// shared player (egui parity: turning sound off silences the
    /// next move).
    pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg> {
        match msg {
            Msg::Close => Task::none(),
            Msg::LanguageChanged(lang) => {
                shared.settings.language = match lang {
                    Language::Vietnamese => "vi".to_string(),
                    Language::English => "en".to_string(),
                };
                i18n::set_language(lang);
                shared.settings.save();
                Task::none()
            }
            Msg::SoundToggled(on) => {
                shared.settings.sound = on;
                if let Ok(mut sfx) = shared.sound.try_lock() {
                    sfx.set_enabled(on);
                }
                shared.settings.save();
                Task::none()
            }
            Msg::AnimateToggled(on) => {
                shared.settings.animate = on;
                shared.settings.save();
                Task::none()
            }
            Msg::NotationChanged(notation) => {
                shared.settings.notation = notation;
                shared.settings.save();
                Task::none()
            }
            Msg::DifficultyChanged(d) => {
                shared.settings.difficulty = d;
                shared.settings.save();
                Task::none()
            }
            Msg::CustomEloChanged(elo) => {
                shared.settings.custom_elo = elo;
                shared.settings.save();
                Task::none()
            }
            Msg::ThreadsChanged(threads) => {
                shared.settings.engine_threads = threads;
                shared.settings.save();
                Task::none()
            }
            Msg::HashChanged(mb) => {
                shared.settings.engine_hash_mb = mb;
                shared.settings.save();
                Task::none()
            }
            Msg::DiversityToggled(on) => {
                shared.settings.diversity = on;
                shared.settings.save();
                Task::none()
            }
            Msg::TournamentToggled(on) => {
                shared.settings.tournament = on;
                shared.settings.save();
                Task::none()
            }
            Msg::TraceLogToggled(on) => {
                // Persist the setting AND sync the live log at
                // once (egui syncs `log.set_trace_enabled` every
                // frame; here the toggle is the only writer, so
                // syncing on toggle is equivalent).
                shared.settings.engine_trace_log = on;
                if let Some(log) = &shared.engine_log {
                    log.set_trace_enabled(on);
                }
                shared.settings.save();
                Task::none()
            }
            Msg::PikafishPathChanged(path) => {
                self.pikafish_path = path.clone();
                shared.settings.pikafish_path = path;
                shared.settings.save();
                Task::none()
            }
            Msg::RetryEngine => {
                // Intercepted by `App` (which owns the install
                // receiver and its `None` guard); reaching here
                // is a no-op by design.
                Task::none()
            }
            Msg::OpenEngineFolder => {
                if let Some(dir) = &self.engine_dir {
                    open_folder(dir);
                }
                Task::none()
            }
            Msg::OpenEngineLog => {
                // Open the folder holding the real log file
                // (`engine.log`'s parent), like egui — never a
                // guessed folder. No log (`None`) means no button
                // in the view, so this is unreachable then.
                if let Some(log) = &shared.engine_log {
                    if let Some(parent) = log.path().parent() {
                        open_folder(parent);
                    }
                }
                Task::none()
            }
        }
    }

    /// Full-screen overlay: a dim backdrop (click closes the dialog)
    /// with the centred card on top. `Stack` dispatches mouse events
    /// topmost-first and stops once a widget captures the event, so
    /// clicks on the card's buttons / sliders / text input never
    /// reach the backdrop — only clicks outside the card (and on the
    /// card's empty padding, which no widget captures) close it.
    /// `install_running` (from `App`, which owns the install
    /// receiver) disables the "Thử lại Pikafish" button while an
    /// install is in flight.
    pub fn view(&self, shared: &Shared, install_running: bool) -> Element<'_, Msg> {
        let (title, body, body_height) = match self.kind {
            Kind::Settings => (
                i18n::t(Key::Settings),
                self.view_settings(shared, install_running),
                Length::Fixed(520.0),
            ),
            Kind::About => (i18n::t(Key::About), self.view_about(shared), Length::Shrink),
        };
        let card = container(
            column![
                text(title)
                    .size(FONT_SCREEN_TITLE)
                    .font(theme::inter_font_semibold()),
                scrollable(body).height(body_height).width(Length::Fill),
                secondary_button(i18n::t(Key::Close), Msg::Close),
            ]
            .spacing(SPACE_12)
            .width(Length::Fill),
        )
        .padding(SPACE_16)
        .width(Length::Fixed(560.0))
        .style(theme::card_container_style);
        let backdrop = mouse_area(
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_: &Theme| container::Style {
                    background: Some(iced::Background::Color(iced::Color {
                        a: 0.6,
                        ..theme::BG
                    })),
                    ..Default::default()
                }),
        )
        .on_press(Msg::Close);
        Stack::with_children([
            backdrop.into(),
            container(card)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Settings body, in egui's section order.
    fn view_settings(&self, shared: &Shared, install_running: bool) -> Element<'_, Msg> {
        let s = &shared.settings;
        let lang = Language::from_name(&s.language);

        let mut col = column![
            section_title(i18n::t(Key::LanguageLabel)),
            row![
                radio(
                    i18n::t(Key::LanguageVi),
                    Language::Vietnamese,
                    Some(lang),
                    Msg::LanguageChanged,
                ),
                radio(
                    i18n::t(Key::LanguageEn),
                    Language::English,
                    Some(lang),
                    Msg::LanguageChanged,
                ),
            ]
            .spacing(16),
            section_title(i18n::t(Key::SoundOn)),
            toggler(s.sound)
                .label(i18n::t(Key::SoundOn))
                .on_toggle(Msg::SoundToggled),
            toggler(s.animate)
                .label(i18n::t(Key::AnimateLabel))
                .on_toggle(Msg::AnimateToggled),
            section_title(i18n::t(Key::NotationLabel)),
            row![
                radio(
                    i18n::t(Key::NotationVn),
                    NotationChoice::Vietnamese,
                    Some(s.notation),
                    Msg::NotationChanged,
                ),
                radio(
                    i18n::t(Key::NotationEn),
                    NotationChoice::English,
                    Some(s.notation),
                    Msg::NotationChanged,
                ),
            ]
            .spacing(16),
            section_title(i18n::t(Key::Difficulty)),
        ]
        .spacing(SPACE_8);
        for d in 0..=4u8 {
            col = col.push(radio(
                difficulty_name(d),
                d,
                Some(s.difficulty),
                Msg::DifficultyChanged,
            ));
        }
        if s.difficulty == 4 {
            col = col.push(
                row![
                    text(i18n::t(Key::EngineCustomElo)).size(FONT_BODY),
                    slider(1200..=3000u16, s.custom_elo, Msg::CustomEloChanged)
                        .step(50u16)
                        .width(Length::Fixed(220.0)),
                    text(format!("Elo {}", s.custom_elo)).size(FONT_BODY),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            );
        }

        col = col.push(section_title(i18n::t(Key::EngineSection)));
        col = col.push(
            text(i18n::t(Key::EngineInUse).replace("{n}", &shared.engine_name)).size(FONT_BODY),
        );
        col = col.push(
            row![
                text(i18n::t(Key::EngineThreads)).size(FONT_BODY),
                slider(1..=8u32, s.engine_threads, Msg::ThreadsChanged).width(Length::Fixed(220.0)),
                text(format!("{}", s.engine_threads)).size(FONT_BODY),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        );
        let hash_label = if s.engine_hash_mb == 0 {
            i18n::t(Key::EngineHashAuto).to_string()
        } else {
            format!("{} MB", s.engine_hash_mb)
        };
        col = col.push(
            row![
                text(i18n::t(Key::EngineHash)).size(FONT_BODY),
                slider(0..=1024u32, s.engine_hash_mb, Msg::HashChanged)
                    .step(64u32)
                    .width(Length::Fixed(220.0)),
                text(hash_label).size(FONT_BODY),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        );
        col = col.push(
            toggler(s.diversity)
                .label(i18n::t(Key::EngineDiversity))
                .on_toggle(Msg::DiversityToggled),
        );
        col = col.push(
            toggler(s.tournament)
                .label(i18n::t(Key::EngineTournament))
                .on_toggle(Msg::TournamentToggled),
        );
        col = col.push(
            toggler(s.engine_trace_log)
                .label(i18n::t(Key::EngineTraceLog))
                .on_toggle(Msg::TraceLogToggled),
        );
        col = col.push(text(i18n::t(Key::EnginePath)).size(FONT_BODY));
        col = col.push(
            text_input(i18n::t(Key::EnginePath), &self.pikafish_path)
                .on_input(Msg::PikafishPathChanged),
        );
        // "Thử lại Pikafish" (egui parity): re-runs the install
        // with the current threads / hash / path. Disabled while
        // an install is running — `App` also guards the spawn
        // itself, so a double click can never start two installs.
        // `App` intercepts `Msg::RetryEngine`; this module never
        // acts on it.
        if install_running {
            col = col.push(disabled_button(i18n::t(Key::EngineInstalling)));
        } else {
            col = col.push(primary_button(i18n::t(Key::EngineRetry), Msg::RetryEngine));
        }
        col = col.push(text(i18n::t(Key::SettingsEngineApplyNote)).size(FONT_CAPTION));
        col.into()
    }

    /// About body, in egui's order, plus the book count and the
    /// sound / ChessDB notes.
    fn view_about(&self, shared: &Shared) -> Element<'_, Msg> {
        let mut col = column![
            text(i18n::t(Key::AboutBody).replace("{n}", env!("CARGO_PKG_VERSION"))).size(FONT_BODY),
            text(i18n::t(Key::AboutPikafishVersion)).size(FONT_BODY),
            text(i18n::t(Key::AboutEngineBody)).size(FONT_BODY),
            text(i18n::t(Key::AboutEngine).replace("{n}", &shared.engine_name)).size(FONT_BODY),
            text(i18n::t(Key::AboutNnue)).size(FONT_BODY),
            text(i18n::t(Key::AboutBook).replace("{n}", &shared.book.lessons.len().to_string()))
                .size(FONT_BODY),
            text(i18n::t(Key::AboutFonts)).size(FONT_BODY),
            text(i18n::t(Key::AboutSoundNote)).size(FONT_BODY),
            text(i18n::t(Key::AboutChessDbNote)).size(FONT_BODY),
        ]
        .spacing(SPACE_8);
        let mut buttons = row![].spacing(SPACE_8);
        if self.engine_dir.is_some() {
            buttons = buttons.push(secondary_button(
                i18n::t(Key::OpenEngineFolder),
                Msg::OpenEngineFolder,
            ));
        }
        // "Mở engine.log" only when the log exists (egui shows it
        // only when `engine_log` is known); with no log there is
        // nothing to open, so the button is hidden rather than
        // opening a guessed folder.
        if shared.engine_log.is_some() {
            buttons = buttons.push(secondary_button(
                i18n::t(Key::OpenEngineLog),
                Msg::OpenEngineLog,
            ));
        }
        col = col.push(buttons);
        col.into()
    }

    /// The dialogs have no background work.
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::none()
    }
}

/// A button with no `on_press` (rendered disabled by Iced) — the
/// "Thử lại Pikafish" button while an install is running.
fn disabled_button(label: &str) -> Button<'_, Msg> {
    Button::new(text(label).size(FONT_BODY))
        .style(theme::secondary_button_style)
        .padding(12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn settings_state_drafts_path_from_shared() {
        let mut shared = Shared::new();
        shared.settings.pikafish_path = "/tmp/pikafish".to_string();
        let st = State::settings(&shared);
        assert_eq!(st.kind(), Kind::Settings);
        assert_eq!(st.pikafish_path, "/tmp/pikafish");
        assert!(st.engine_dir.is_none());
    }

    #[test]
    fn about_state_resolves_engine_dir_or_none() {
        let shared = Shared::new();
        let st = State::about(&shared);
        assert_eq!(st.kind(), Kind::About);
        // Either Pikafish is locatable (dir exists) or it is not.
        if let Some(dir) = &st.engine_dir {
            assert!(dir.is_dir());
        }
    }

    #[test]
    fn new_is_the_settings_dialog() {
        let shared = Shared::new();
        assert_eq!(State::settings(&shared).kind(), Kind::Settings);
    }

    #[test]
    fn trace_toggle_syncs_engine_log() {
        // The toggle must sync the live `EngineLog`, not just the
        // persisted setting: a trace line is dropped while the
        // toggle is off and lands in the file while it is on.
        // Unique markers (per process) so no other test's writes
        // can match; the real config-dir log is used, like every
        // other `Shared::new` in the test suite.
        let pid = std::process::id();
        let mut shared = Shared::new();
        let log = shared.engine_log.as_ref().expect("log created").clone();
        let path = log.path();
        let mut st = State::settings(&shared);
        let off_marker = format!("trace-marker-{pid}-off");
        let on_marker = format!("trace-marker-{pid}-on");
        let off_again_marker = format!("trace-marker-{pid}-off-again");
        let _ = st.update(Msg::TraceLogToggled(false), &mut shared);
        log.write(xq_pikafish::LogLevel::Trace, &off_marker);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!content.contains(&off_marker), "trace off must drop");
        let _ = st.update(Msg::TraceLogToggled(true), &mut shared);
        log.write(xq_pikafish::LogLevel::Trace, &on_marker);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(content.contains(&on_marker), "trace on must log");
        let _ = st.update(Msg::TraceLogToggled(false), &mut shared);
        log.write(xq_pikafish::LogLevel::Trace, &off_again_marker);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!content.contains(&off_again_marker), "trace off again");
    }

    #[test]
    fn open_engine_folder_without_dir_is_a_noop() {
        let shared = Shared::new();
        let mut st = State::settings(&shared);
        assert!(st.engine_dir.is_none());
        let mut shared = Shared::new();
        let _ = st.update(Msg::OpenEngineFolder, &mut shared);
    }

    #[test]
    fn language_change_persists_and_applies_at_once() {
        // Isolate the settings file: point the config dir at a temp
        // dir so the save never touches the real user config.
        let dir = std::env::temp_dir().join(format!("xq-iced-dialog-test-{}", std::process::id()));
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let mut shared = Shared::new();
        assert_eq!(shared.settings.language, "vi");
        let mut st = State::settings(&shared);
        let _ = st.update(Msg::LanguageChanged(Language::English), &mut shared);
        assert_eq!(shared.settings.language, "en");
        assert_eq!(i18n::language(), Language::English);
        let saved =
            crate::settings::Settings::load_from(&dir.join("xiangqi").join("settings.json"));
        assert_eq!(saved.language, "en");
        // Restore Vietnamese + the previous config dir.
        let _ = st.update(Msg::LanguageChanged(Language::Vietnamese), &mut shared);
        assert_eq!(i18n::language(), Language::Vietnamese);
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
