//! App shell: `Screen` enum + the top-level Iced application
//! (`App` + `Message` + `Shared`).
//!
//! Every screen lives in `screens/` and follows the module contract:
//!
//! ```ignore
//! pub struct State { /* ... */ }
//! pub enum Msg {
//!     Navigate(Screen), // App intercepts, screen never handles
//!     ShowSettings,     // App intercepts, opens the dialog
//!     ShowAbout,        // App intercepts, opens the dialog
//!     /* ... screen-specific variants ... */
//! }
//! impl State {
//!     pub fn new(shared: &Shared) -> Self;
//!     pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg>;
//!     pub fn view(&self, shared: &Shared) -> Element<'_, Msg>;
//!     pub fn subscription(&self) -> Subscription<Msg>;
//! }
//! ```
//!
//! `App::update` intercepts the three shared variants before
//! delegating: `Navigate(s)` calls [`App::goto`], `ShowSettings` /
//! `ShowAbout` open the dialogs, everything else is delegated to the
//! owning screen and mapped back to [`Message`].
//!
//! # INVARIANT — the piece must land before any computation starts
//!
//! A move's slide animation (300 ms) must finish BEFORE the AI,
//! grading, hint or ChessDB work for the new position starts (Hieu,
//! restated 2026-11-12, three times in one day). In Elm terms:
//! `update()` only triggers AI / grading / hint / ChessDB work when
//! the screen's board is NOT animating (`board.is_animating() ==
//! false` — see `board::BoardState`); the tick subscription only
//! runs while an animation or the flashing check halo is active.
//! Screens P1/P2/P3 enforce this in their own `update()`; this file
//! states the rule so no screen can claim not to know it. The
//! Settings / About dialogs are overlays on top of the active screen
//! (built with `iced::widget::Stack` — Iced 0.14 has no modal dialog,
//! see `docs/SPIKE-iced.md` §2). The real dialogs live in
//! `screens::settings_dialog` (P4); `App` only owns the open dialog's
//! state (`App::dialog`) and the backdrop-click close.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use iced::widget::Stack;
use iced::{Element, Length, Size, Subscription, Task};
use xq_book::Book;
use xq_engine::{shared_engine, BuiltinEngine, SharedEngine};

use crate::bridge;
use crate::engine_log::EngineLog;
#[cfg(target_arch = "wasm32")]
use crate::engine_manager::EngineEvent;
#[cfg(not(target_arch = "wasm32"))]
use crate::engine_manager::{self, EngineEvent};
use crate::i18n;
use crate::screens::{
    history, home, learn, learn_select, play, practice, settings_dialog, summary,
};
use crate::settings::{settings_path, Settings};
use crate::sound::{SharedSound, SoundPlayer};
use crate::theme;

/// The app's screens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Main menu.
    Home,
    /// Play vs AI.
    Play,
    /// Learn openings: choose side + lesson.
    LearnSelect,
    /// Learn openings: play a lesson.
    Learn,
    /// Self practice.
    Practice,
    /// Lesson summary.
    Summary,
    /// Saved games: "Lịch sử đấu" (Play) or "Ván cờ đã lưu"
    /// (Practice). The mode lives in the History state.
    History,
}

/// State shared by every screen.
pub struct Shared {
    /// Persisted settings (loaded once at startup, saved on change).
    pub settings: Settings,
    /// Sound effects (shared; `try_lock`ed, never blocking).
    pub sound: SharedSound,
    /// The shared engine (Builtin until Pikafish is pre-warmed in —
    /// P1 wires the pre-warm via `engine_manager::spawn_install`).
    /// Held here so Play / Learn / Practice all use the same engine
    /// the worker threads use.
    pub engine: SharedEngine,
    /// The embedded opening book (Pikafish is the book: one
    /// precomputed dataset — see `docs/DECISIONS-learn.md`).
    pub book: Book,
    /// Display name of the active engine ("AI tích hợp" until
    /// Pikafish is installed).
    pub engine_name: String,
    /// Rotating engine log (`None` when the log directory cannot
    /// be created — logging is then silently disabled, egui
    /// parity). Lives in `<config_dir>/xiangqi/logs/engine.log`
    /// (the parent dir of `settings.json`); the Pikafish log sink
    /// writes into it, and the About dialog opens its folder.
    pub engine_log: Option<Arc<EngineLog>>,
    /// Saved games and lesson progress (Hieu, 2026-11-14):
    /// `<config_dir>/xiangqi/saved_games.json`. Loaded once at
    /// startup; screens mutate it and call `save()` after a change.
    pub saved_games: crate::saved_games::SavedGames,
    /// Sync mirroring Phase 1 (2026-11-27): worker + trạng thái
    /// đăng nhập + đề nghị tiếp tục ván từ máy khác. Desktop: worker
    /// thread thật; PWA: vỏ rỗng (trạng thái luôn `Off`).
    pub sync: crate::sync::SyncState,
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

impl Shared {
    /// Load settings, apply the saved language, build the sound
    /// player, the builtin engine and the embedded book.
    #[must_use]
    pub fn new() -> Self {
        let settings = Settings::load();
        i18n::set_language(i18n::Language::from_name(&settings.language));
        let sound: SharedSound = Arc::new(Mutex::new(SoundPlayer::new()));
        if let Ok(mut sfx) = sound.lock() {
            sfx.set_enabled(settings.sound);
        }
        let engine = shared_engine(Box::new(BuiltinEngine::new()));
        let book = Book::load_embedded().unwrap_or(Book {
            lessons: Vec::new(),
        });
        // Rotating engine log next to the settings file: the log
        // lives in `<parent of settings.json>/logs` (`EngineLog`
        // appends `/logs` itself). A missing log dir disables
        // logging instead of breaking startup (egui parity, except
        // that egui puts the log next to eframe's storage dir while
        // Iced keeps it next to `settings.json` — see the field
        // docs). The trace switch follows the saved setting, and
        // the sink is installed once here so Pikafish warnings /
        // UCI lines are recorded from the first install attempt.
        let engine_log = settings_path().parent().and_then(EngineLog::new);
        if let Some(log) = &engine_log {
            log.set_trace_enabled(settings.engine_trace_log);
            let sink = Arc::clone(log);
            xq_pikafish::set_log_sink(Some(Arc::new(move |level, msg| {
                sink.write(level, msg);
            })));
        }
        let device_id = settings.device_id.clone();
        Self {
            settings,
            sound,
            engine,
            book,
            engine_name: "AI tích hợp".to_string(),
            engine_log,
            saved_games: crate::saved_games::SavedGames::load(),
            sync: crate::sync::SyncState::new(device_id),
        }
    }
}

/// The top-level application state.
pub struct App {
    screen: Screen,
    /// Current window size, owned here — not by the screens. A
    /// screen's own resize subscription only starts when the screen
    /// becomes visible, and on web it can deliver a stale size at
    /// that moment (Hieu, 2026-11-26: Play rendered the wide branch
    /// at 390 px — small centred board, panel clipped away — because
    /// its `window` was never the true size). This subscription runs
    /// from boot, so it always carries the true size, and every
    /// screen gets it forwarded (see `Message::WindowResized`),
    /// visible or not. Seeded 1280 × 800 like the screens.
    window: Size,
    shared: Shared,
    home: home::State,
    play: play::State,
    learn_select: learn_select::State,
    learn: learn::State,
    practice: practice::State,
    summary: summary::State,
    history: history::State,
    /// The open Settings / About dialog, if any (`None` = no dialog).
    /// Built fresh from `Shared` when opened, dropped on close or on
    /// navigation.
    dialog: Option<settings_dialog::State>,
    /// Pending Pikafish install (startup pre-warm / the Settings
    /// dialog's "Thử lại Pikafish" button). The single receiver —
    /// `Some` while an install is running, `None` otherwise — is
    /// the guard against two installs running at once (egui
    /// `engine_install_rx`): `App` only spawns a new install when
    /// this is `None`, and clears it when the outcome arrives.
    /// Owned here, not by Play, so the pre-warm survives screen
    /// changes and the retry button (a dialog message) can reach it.
    install_rx: Option<Arc<Mutex<Receiver<EngineEvent>>>>,
}

/// Top-level messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// Home screen message.
    Home(home::Msg),
    /// Play screen message.
    Play(play::Msg),
    /// Learn-select screen message.
    LearnSelect(learn_select::Msg),
    /// Learn screen message.
    Learn(learn::Msg),
    /// Practice screen message.
    Practice(practice::Msg),
    /// Summary screen message.
    Summary(summary::Msg),
    /// Saved-games list message.
    History(history::Msg),
    /// Settings / About dialog message.
    Dialog(settings_dialog::Msg),
    /// Go to a screen.
    Goto(Screen),
    /// Open the Settings dialog.
    ShowSettings,
    /// Open the About dialog.
    ShowAbout,
    /// Pikafish installed and is now the active engine (from the
    /// App-level install bridge; forwarded to Play — see
    /// `App::update`).
    EngineInstalled {
        name: String,
        dir: std::path::PathBuf,
    },
    /// Pikafish is unavailable; the builtin engine stays active
    /// (from the App-level install bridge; forwarded to Play).
    EngineUnavailable(String),
    /// The embedded CJK subset font finished loading (boot task).
    FontLoaded(Result<(), iced::font::Error>),
    /// The window was resized: stored here and forwarded to every
    /// screen that tracks it (home / play / learn / practice), so no
    /// screen ever lays out from a stale size — see `App::window`.
    WindowResized(Size),
    /// Sync poll (every 500 ms): drain the sync worker's responses
    /// and route them — session responses update `Shared::sync`,
    /// game responses go to the Play screen, `PulledAll` may raise
    /// the "tiếp tục ván từ máy khác" offer on Home.
    SyncPoll,
}

impl App {
    /// Create the app: all screens start from the same `Shared`.
    /// Returns the app plus the boot task (loads the embedded CJK
    /// subset font). The Pikafish pre-warm starts here, owned by
    /// `App` (see `install_rx`): Play no longer spawns its own
    /// install, so there is exactly one install receiver.
    pub fn new() -> (Self, Task<Message>) {
        let shared = Shared::new();
        // wasm32: no Pikafish pre-warm — there is no subprocess on
        // wasm, and the install bridge's forwarder thread would
        // panic there. The engine on wasm is Pikafish-WASM, started
        // by each screen's `WasmAiWorker` (see `ai_worker.rs`); Play
        // marks itself ready from the driver's handshake, so no
        // install receiver exists and no boot banner is sent.
        #[cfg(not(target_arch = "wasm32"))]
        let install_rx = Some(Arc::new(Mutex::new(engine_manager::spawn_install(
            shared.engine.clone(),
            shared.settings.pikafish_path(),
            shared.settings.engine_threads,
            shared.settings.engine_hash_mb,
            shared.settings.level(),
        ))));
        #[cfg(target_arch = "wasm32")]
        let install_rx = None;
        let app = Self {
            screen: Screen::Home,
            window: Size::new(1280.0, 800.0),
            home: home::State::new(&shared),
            play: play::State::new(&shared),
            learn_select: learn_select::State::new(&shared),
            learn: learn::State::new(&shared),
            practice: practice::State::new(&shared),
            summary: summary::State::new(&shared),
            history: history::State::new(&shared),
            shared,
            dialog: None,
            install_rx,
        };
        // Sync mirroring: khôi phục phiên ngay lúc mở app — desktop
        // từ keychain, PWA từ Firebase JS SDK (IndexedDB). Không có
        // phiên thì worker trả SessionRestoreFailed, trạng thái cứ ở
        // Off — bình thường.
        let mut app = app;
        {
            let req_id = app.shared.sync.worker.alloc_req_id();
            app.shared.sync.status = crate::sync::SyncStatus::SigningIn;
            app.shared
                .sync
                .worker
                .send(crate::sync::SyncRequest::RestoreSession { req_id });
        }
        let boot = iced::Task::batch([theme::load_cjk_font(), theme::load_inter_fonts()])
            .map(Message::FontLoaded);
        (app, boot)
    }

    /// Go to a screen, closing any open dialog (dialogs are transient
    /// per screen).
    pub fn goto(&mut self, screen: Screen) {
        self.screen = screen;
        self.dialog = None;
    }

    /// Open the Settings dialog (fresh state from the current
    /// settings, replacing any open dialog).
    fn open_settings(&mut self) {
        self.dialog = Some(settings_dialog::State::settings(&self.shared));
    }

    /// Open the About dialog (fresh state, engine dir resolved on
    /// open), replacing any open dialog.
    fn open_about(&mut self) {
        self.dialog = Some(settings_dialog::State::about(&self.shared));
    }

    /// Drain the sync worker's responses (called from
    /// `Message::SyncPoll`, twice a second): session responses
    /// update `Shared::sync`, game responses go to the Play screen,
    /// `PulledAll` may raise the "tiếp tục ván từ máy khác" offer.
    fn drain_sync_responses(&mut self) {
        while let Some(resp) = self.shared.sync.try_recv() {
            self.handle_sync_response(resp);
        }
    }

    /// Route one sync response (see `drain_sync_responses`).
    fn handle_sync_response(&mut self, resp: crate::sync::SyncResponse) {
        use crate::sync::{SyncResponse, SyncStatus};
        match resp {
            SyncResponse::SessionRestored { tokens, .. } => {
                self.shared.sync.status = SyncStatus::SignedIn {
                    email: tokens.email.clone(),
                };
                self.shared.settings.sync_email = Some(tokens.email);
                self.shared.settings.save();
            }
            SyncResponse::SessionRestoreFailed { reason, .. } => {
                // "chưa đăng nhập" là bình thường (mở app lần đầu) —
                // trạng thái cứ ở Off, không hiện lỗi.
                if reason == "chua dang nhap" {
                    self.shared.sync.status = SyncStatus::Off;
                } else {
                    self.shared.sync.status = SyncStatus::Error(reason);
                }
            }
            SyncResponse::SignedIn { tokens, .. } => {
                self.shared.sync.status = SyncStatus::SignedIn {
                    email: tokens.email.clone(),
                };
                self.shared.settings.sync_email = Some(tokens.email);
                self.shared.settings.save();
            }
            SyncResponse::SignInFailed { reason, .. } => {
                self.shared.sync.status = SyncStatus::Error(reason);
            }
            SyncResponse::SignedOut { .. } => {
                self.shared.sync.status = SyncStatus::Off;
                self.shared.settings.sync_email = None;
                self.shared.settings.save();
                self.shared.sync.resume_offer = None;
            }
            SyncResponse::PulledAll { games, .. } => {
                self.maybe_offer_remote_game(games);
            }
            SyncResponse::Pushed { .. }
            | SyncResponse::PushRejected { .. }
            | SyncResponse::PushFailed { .. }
            | SyncResponse::RemoteUpdate { .. } => {
                // Game-level: the Play screen owns the game — forward
                // it there (it ignores anything that is not its
                // current synced game).
                let _ = self.play.update(play::Msg::Sync(resp), &mut self.shared);
            }
            SyncResponse::PulledOne { .. } | SyncResponse::SyncFailed { .. } => {
                // Phase 1 không dùng PulledOne; SyncFailed cho
                // request lẻ thì Play đã nhận qua PushFailed — bỏ qua.
            }
        }
    }

    /// Sau `PulledAll`: nếu cloud có ván Play đang dở MỚI HƠN ván
    /// dở trên máy này (hoặc máy này không có ván dở) thì đề nghị
    /// tiếp tục ván đó trên Home. Ván giống hệt (cùng uuid + cùng
    /// nước) thì không đề nghị gì.
    fn maybe_offer_remote_game(&mut self, games: Vec<crate::saved_games::SavedGame>) {
        use crate::saved_games::SavedMode;
        let Some(remote) = games
            .iter()
            .filter(|g| g.mode == SavedMode::Play && !g.finished && !g.deleted)
            .max_by_key(|g| (g.updated_unix, g.version))
        else {
            return;
        };
        let local = self.shared.saved_games.unfinished_play();
        let newer_than_local = match local {
            None => true,
            Some(l) => {
                (remote.updated_unix, remote.version) > (l.updated_unix, l.version)
                    && (remote.uuid != l.uuid || remote.moves != l.moves)
            }
        };
        if !newer_than_local {
            return;
        }
        // Nhãn nguồn: device id rút gọn (tên máy kia không có trong
        // entry — chỉ có id; xem `SyncEntry::updated_by_device`).
        let short = remote
            .updated_by_device
            .rsplit('-')
            .next()
            .unwrap_or("")
            .chars()
            .take(8)
            .collect::<String>();
        self.shared.sync.resume_offer = Some(crate::sync::ResumeOffer {
            game: remote.clone(),
            from_label: if short.is_empty() {
                "máy khác".to_string()
            } else {
                format!("máy khác (…{short})")
            },
        });
    }

    /// Chấp nhận đề nghị tiếp tục ván từ máy khác: ván dở cũ trên
    /// máy này (nếu có và cũ hơn) vào Lịch sử dạng bản lưu trữ, rồi
    /// mở ván remote trên màn Play.
    fn accept_sync_resume(&mut self) {
        let Some(offer) = self.shared.sync.resume_offer.take() else {
            return;
        };
        if let Some(local) = self.shared.saved_games.unfinished_play() {
            if local.uuid != offer.game.uuid {
                // Ván cũ hơn vào Lịch sử: đánh dấu đã xong (để không
                // còn được đề nghị tiếp tục) + đổi nhãn.
                let mut archived = local.clone();
                archived.finished = true;
                archived.label = format!("{} — bản lưu trữ từ máy này", archived.label);
                self.shared.saved_games.add(archived);
                self.shared.saved_games.save();
            }
        }
        self.play.resume_saved(&offer.game, &mut self.shared);
        self.goto(Screen::Play);
    }

    /// Dừng theo dõi ván sync (khi rời màn Play — state Play bị reset
    /// nhưng worker vẫn poll uuid cũ nếu không bảo dừng).
    fn stop_sync_watch(&mut self) {
        self.shared
            .sync
            .worker
            .send(crate::sync::SyncRequest::Unwatch);
    }

    /// Iced `update`: intercept the shared variants, delegate the
    /// rest to the owning screen.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Goto(screen) => {
                self.goto(screen);
                Task::none()
            }
            Message::WindowResized(size) => {
                // Owned here (see `App::window`): store it and forward
                // it to every screen that lays out from it, visible or
                // not, so a screen never renders from its seeded size
                // after navigation. The forwards are plain size stores
                // (each returns `Task::none()`).
                self.window = size;
                Task::batch([
                    self.home
                        .update(home::Msg::WindowResized(size), &mut self.shared)
                        .map(Message::Home),
                    self.play
                        .update(play::Msg::WindowResized(size), &mut self.shared)
                        .map(Message::Play),
                    self.learn
                        .update(learn::Msg::WindowResized(size), &mut self.shared)
                        .map(Message::Learn),
                    self.practice
                        .update(practice::Msg::WindowResized(size), &mut self.shared)
                        .map(Message::Practice),
                ])
            }
            Message::ShowSettings => {
                self.open_settings();
                Task::none()
            }
            Message::SyncPoll => {
                self.drain_sync_responses();
                Task::none()
            }
            Message::ShowAbout => {
                self.open_about();
                Task::none()
            }
            Message::EngineInstalled { name, dir } => {
                // One install outcome: clear the receiver (the
                // guard is free again, so a later retry may spawn),
                // record the engine name, and forward the event to
                // Play's state — unconditionally, not only when Play
                // is the visible screen: the App-level bridge
                // consumes the event as soon as it arrives (the
                // pre-warm usually finishes while the user is still
                // on Home), so forwarding only on Play would lose
                // it. Play keeps its own `engine_ready` flag and
                // banner from the forwarded message.
                self.install_rx = None;
                self.shared.engine_name = name.clone();
                self.play
                    .update(play::Msg::EngineInstalled { name, dir }, &mut self.shared)
                    .map(Message::Play)
            }
            Message::EngineUnavailable(msg) => {
                self.install_rx = None;
                self.shared.engine_name = "AI tích hợp".to_string();
                self.play
                    .update(play::Msg::EngineUnavailable(msg), &mut self.shared)
                    .map(Message::Play)
            }
            Message::Dialog(settings_dialog::Msg::RetryEngine) => {
                // "Thử lại Pikafish" (intercepted here, not by the
                // dialog — the dialog only has `Shared` and cannot
                // touch `install_rx`). Re-runs the install with
                // the current settings; the outcome arrives via
                // `install_rx` like the startup pre-warm. Only
                // when no install is running — the `None` guard is
                // what keeps two installs from ever running at once.
                //
                // wasm32: there is no install to retry — the web
                // build runs Pikafish-WASM, started by each
                // screen's `WasmAiWorker` (recreated when the
                // screen is re-entered). A no-op.
                #[cfg(target_arch = "wasm32")]
                return Task::none();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if self.install_rx.is_none() {
                        let level = self.shared.settings.level();
                        self.install_rx =
                            Some(Arc::new(Mutex::new(engine_manager::spawn_install(
                                self.shared.engine.clone(),
                                self.shared.settings.pikafish_path(),
                                self.shared.settings.engine_threads,
                                self.shared.settings.engine_hash_mb,
                                level,
                            ))));
                    }
                    Task::none()
                }
            }
            Message::Dialog(settings_dialog::Msg::Close) => {
                self.dialog = None;
                Task::none()
            }
            Message::Dialog(settings_dialog::Msg::SignIn) => {
                // PWA only: đăng nhập có thể redirect (popup bị chặn
                // trên iOS standalone) rồi tải lại trang — lưu ván
                // đang dở trước để không mất ván khi tải lại.
                // Desktop mở trình duyệt hệ thống, không tải lại
                // app nên không cần.
                #[cfg(target_arch = "wasm32")]
                if self.play.needs_save_prompt() {
                    self.play.save_current_game(&mut self.shared, false);
                }
                match &mut self.dialog {
                    Some(dialog) => dialog
                        .update(settings_dialog::Msg::SignIn, &mut self.shared)
                        .map(Message::Dialog),
                    None => Task::none(),
                }
            }
            Message::Dialog(msg) => match &mut self.dialog {
                Some(dialog) => dialog.update(msg, &mut self.shared).map(Message::Dialog),
                None => Task::none(),
            },
            Message::FontLoaded(_) => Task::none(),
            Message::Home(msg) => match msg {
                home::Msg::Navigate(screen) => {
                    self.goto(screen);
                    Task::none()
                }
                home::Msg::SyncResumeAccept => {
                    self.accept_sync_resume();
                    Task::none()
                }
                home::Msg::SyncResumeDismiss => {
                    self.shared.sync.resume_offer = None;
                    Task::none()
                }
                home::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                home::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                home::Msg::WindowResized(size) => self
                    .home
                    .update(home::Msg::WindowResized(size), &mut self.shared)
                    .map(Message::Home),
            },
            Message::Play(msg) => match msg {
                play::Msg::Navigate(screen) => {
                    // Back to the menu is an exit (Hieu, 2026-11-14):
                    // a game in progress asks to save first instead
                    // of navigating away; otherwise the mode resets
                    // completely, so re-entering it starts fresh.
                    if screen == Screen::Home && self.play.needs_save_prompt() {
                        self.play.show_save_prompt();
                        Task::none()
                    } else {
                        if screen == Screen::Home {
                            self.play = play::State::new(&self.shared);
                            self.stop_sync_watch();
                        }
                        self.goto(screen);
                        Task::none()
                    }
                }
                // Exit save prompt: save the unfinished game (or
                // not), then reset Play and go to the menu.
                play::Msg::SavePromptSave => {
                    self.play.save_current_game(&mut self.shared, false);
                    self.play = play::State::new(&self.shared);
                    self.stop_sync_watch();
                    self.goto(Screen::Home);
                    Task::none()
                }
                play::Msg::SavePromptDiscard => {
                    self.play = play::State::new(&self.shared);
                    self.stop_sync_watch();
                    self.goto(Screen::Home);
                    Task::none()
                }
                // Open the game history ("Lịch sử đấu").
                play::Msg::OpenHistory => {
                    self.history.open(crate::saved_games::SavedMode::Play);
                    self.goto(Screen::History);
                    Task::none()
                }
                play::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                play::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                other => self.play.update(other, &mut self.shared).map(Message::Play),
            },
            Message::LearnSelect(msg) => match msg {
                learn_select::Msg::Navigate(screen) => {
                    // Hand the picked lesson to the Learn screen
                    // before switching (integration wiring, 2026-11-13).
                    if screen == Screen::Learn {
                        if let Some(choice) = self.learn_select.take_choice() {
                            // Saved progress (Hieu, 2026-11-14):
                            // resume the lesson where the learner left
                            // off instead of starting it fresh.
                            if let Some(saved) = self
                                .shared
                                .saved_games
                                .lesson_progress(&choice.lesson_id)
                                .cloned()
                            {
                                self.learn.resume_saved(&saved, &mut self.shared);
                            } else {
                                self.learn.start(
                                    &choice.lesson_id,
                                    choice.learner_is_red,
                                    &mut self.shared,
                                );
                            }
                        }
                    }
                    // Hand the picked lesson to the Play screen for
                    // a book game (Hieu, 2026-11-19): "Đánh với AI
                    // theo thế trận".
                    if screen == Screen::Play {
                        if let Some(choice) = self.learn_select.take_book_choice() {
                            self.play.start_book_game(
                                &choice.lesson_id,
                                choice.learner_is_red,
                                &mut self.shared,
                            );
                        }
                    }
                    self.goto(screen);
                    Task::none()
                }
                learn_select::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                learn_select::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                other => self
                    .learn_select
                    .update(other, &mut self.shared)
                    .map(Message::LearnSelect),
            },
            Message::Learn(msg) => match msg {
                learn::Msg::Navigate(screen) => {
                    // Back to the menu is an exit: an unfinished
                    // lesson asks to save its progress first instead
                    // of navigating away; otherwise the mode resets
                    // completely, so re-entering it starts fresh
                    // (Hieu, 2026-11-14). A finished lesson is 100%
                    // and needs no saved entry.
                    if screen == Screen::Home && self.learn.needs_save_prompt() {
                        self.learn.show_save_prompt();
                        Task::none()
                    } else {
                        // Hand the built summary to the Summary screen
                        // before switching (integration wiring, 2026-11-13).
                        if screen == Screen::Summary {
                            if let Some(data) = self.learn.take_pending_summary() {
                                self.summary.load(data);
                            }
                        }
                        if screen == Screen::Home {
                            self.learn = learn::State::new(&self.shared);
                        }
                        self.goto(screen);
                        Task::none()
                    }
                }
                // Exit save prompt: save the lesson progress (or
                // not), then reset Learn and go to the menu.
                learn::Msg::SavePromptSave => {
                    self.learn.save_progress(&mut self.shared);
                    self.learn = learn::State::new(&self.shared);
                    self.goto(Screen::Home);
                    Task::none()
                }
                learn::Msg::SavePromptDiscard => {
                    self.learn = learn::State::new(&self.shared);
                    self.goto(Screen::Home);
                    Task::none()
                }
                learn::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                learn::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                learn::Msg::ContinueWithAi => {
                    // "Chơi tiếp với máy": the finished lesson's game
                    // continues as a Vs-AI game (egui `ContinueWithAi`
                    // → `start_from_learn`, §6.5). No handover when
                    // the lesson ended in checkmate: stay on the
                    // Learn screen with the complete dialog open.
                    if let Some((game, human)) = self.learn.take_continue_game() {
                        self.play.start_from_learn(game, human);
                        self.goto(Screen::Play);
                    }
                    Task::none()
                }
                other => self
                    .learn
                    .update(other, &mut self.shared)
                    .map(Message::Learn),
            },
            Message::Practice(msg) => match msg {
                practice::Msg::Navigate(screen) => {
                    // Back to the menu is an exit: a game in progress
                    // asks to save first instead of navigating away;
                    // otherwise the mode resets completely, so
                    // re-entering it starts from scratch (Hieu,
                    // 2026-11-14). A fresh State is the reset — it
                    // rebuilds the setup phase, the editor and the
                    // game, and spawns a fresh AI worker (the old
                    // worker's thread exits when its channels drop).
                    if screen == Screen::Home && self.practice.needs_save_prompt() {
                        self.practice.show_save_prompt();
                        Task::none()
                    } else {
                        if screen == Screen::Home {
                            self.practice = practice::State::new(&self.shared);
                        }
                        self.goto(screen);
                        Task::none()
                    }
                }
                // Exit save prompt: save the unfinished game (or
                // not), then reset Practice and go to the menu.
                practice::Msg::SavePromptSave => {
                    self.practice.save_current_game(&mut self.shared, false);
                    self.practice = practice::State::new(&self.shared);
                    self.goto(Screen::Home);
                    Task::none()
                }
                practice::Msg::SavePromptDiscard => {
                    self.practice = practice::State::new(&self.shared);
                    self.goto(Screen::Home);
                    Task::none()
                }
                // Open the saved-games list ("Ván cờ đã lưu").
                practice::Msg::OpenSavedGames => {
                    self.history.open(crate::saved_games::SavedMode::Practice);
                    self.goto(Screen::History);
                    Task::none()
                }
                practice::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                practice::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                other => self
                    .practice
                    .update(other, &mut self.shared)
                    .map(Message::Practice),
            },
            Message::Summary(msg) => match msg {
                summary::Msg::Navigate(screen) => {
                    // "Học lại bài này": hand the pick back to Learn
                    // before switching (integration wiring, 2026-11-13).
                    if screen == Screen::Learn {
                        if let Some(choice) = self.summary.take_relearn() {
                            self.learn.start(
                                &choice.lesson_id,
                                choice.learner_is_red,
                                &mut self.shared,
                            );
                        }
                    }
                    self.goto(screen);
                    Task::none()
                }
                summary::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                summary::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                other => self
                    .summary
                    .update(other, &mut self.shared)
                    .map(Message::Summary),
            },
            Message::History(msg) => match msg {
                history::Msg::Navigate(screen) => {
                    self.goto(screen);
                    Task::none()
                }
                history::Msg::ShowSettings => {
                    self.open_settings();
                    Task::none()
                }
                history::Msg::ShowAbout => {
                    self.open_about();
                    Task::none()
                }
                history::Msg::LoadGame(id) => {
                    let Some(entry) = self
                        .shared
                        .saved_games
                        .entries
                        .iter()
                        .find(|e| e.id == id)
                        .cloned()
                    else {
                        return Task::none();
                    };
                    match entry.mode {
                        crate::saved_games::SavedMode::Play => {
                            self.play = play::State::new(&self.shared);
                            self.play.resume_saved(&entry, &mut self.shared);
                            self.goto(Screen::Play);
                        }
                        crate::saved_games::SavedMode::Practice => {
                            self.practice = practice::State::new(&self.shared);
                            self.practice.load_saved(&entry, &mut self.shared);
                            self.goto(Screen::Practice);
                        }
                        crate::saved_games::SavedMode::Learn => {
                            // Lesson progress is resumed from the
                            // lesson list, not from this screen.
                        }
                    }
                    Task::none()
                }
                history::Msg::DeleteGame(id) => {
                    self.shared.saved_games.remove(&id);
                    self.shared.saved_games.save();
                    Task::none()
                }
                history::Msg::Noop => Task::none(),
            },
        }
    }

    /// Iced `view`: the active screen, plus the Settings / About
    /// dialog overlay when one is open (`settings_dialog::State::view`
    /// already builds the dim backdrop + centred card; `App` only
    /// stacks it over the screen).
    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = match self.screen {
            Screen::Home => self.home.view(&self.shared).map(Message::Home),
            Screen::Play => self.play.view(&self.shared).map(Message::Play),
            Screen::LearnSelect => self
                .learn_select
                .view(&self.shared)
                .map(Message::LearnSelect),
            Screen::Learn => self.learn.view(&self.shared).map(Message::Learn),
            Screen::Practice => self.practice.view(&self.shared).map(Message::Practice),
            Screen::Summary => self.summary.view(&self.shared).map(Message::Summary),
            Screen::History => self.history.view(&self.shared).map(Message::History),
        };
        let Some(dialog) = &self.dialog else {
            return base;
        };
        Stack::with_children([
            base,
            dialog
                .view(&self.shared, self.install_rx.is_some())
                .map(Message::Dialog),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Iced `subscription`: only the ACTIVE screen's subscription
    /// runs (so at most one ChessDB bridge is ever live — see
    /// `chessdb_panel.rs` for why that matters), plus the open
    /// dialog's (currently none — the dialogs have no background
    /// work — but the wiring stays so a future dialog subscription
    /// cannot be silently dropped), plus the Pikafish-install
    /// bridge while an install is running. The install bridge is
    /// App-level (not Play's) so the startup pre-warm is observed
    /// on every screen, including Home.
    pub fn subscription(&self) -> Subscription<Message> {
        let screen = match self.screen {
            Screen::Home => self.home.subscription().map(Message::Home),
            Screen::Play => self.play.subscription().map(Message::Play),
            Screen::LearnSelect => self.learn_select.subscription().map(Message::LearnSelect),
            Screen::Learn => self.learn.subscription().map(Message::Learn),
            Screen::Practice => self.practice.subscription().map(Message::Practice),
            Screen::Summary => self.summary.subscription().map(Message::Summary),
            Screen::History => Subscription::none(),
        };
        let mut subs = vec![screen];
        // The window size is owned here (see `App::window`): this
        // subscription runs from boot, so it always carries the true
        // size. The screens do NOT subscribe to resizes themselves —
        // a screen's own subscription only starts when the screen
        // becomes visible, and on web it can deliver a stale size at
        // that moment.
        subs.push(iced::window::resize_events().map(|(_id, size)| Message::WindowResized(size)));
        // Sync poll (Phase 1): drain the worker's responses twice a
        // second — session state, remote moves, resume offers. The
        // timer runs on wasm too, but the wasm worker never answers,
        // so the drain is a no-op there.
        subs.push(
            iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::SyncPoll),
        );
        if let Some(rx) = &self.install_rx {
            subs.push(
                bridge::from_mpsc("engine-install", Arc::clone(rx)).map(engine_install_bridge_msg),
            );
        }
        if let Some(dialog) = &self.dialog {
            subs.push(dialog.subscription().map(Message::Dialog));
        }
        Subscription::batch(subs)
    }
}

/// Bridge output mapping: engine-install event → top-level message
/// (same mapping Play used for its own bridge before the install
/// moved to `App`).
fn engine_install_bridge_msg(ev: EngineEvent) -> Message {
    match ev {
        EngineEvent::Installed { name, dir } => Message::EngineInstalled { name, dir },
        EngineEvent::Unavailable(msg) => Message::EngineUnavailable(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_resize_is_forwarded_to_every_screen() {
        // Regression (Hieu, 2026-11-26): on the phone Play rendered
        // the wide branch — small centred board, panel clipped away —
        // because its `window` was never the true size; a screen's own
        // resize subscription only starts when the screen becomes
        // visible. App owns the size and forwards it to every screen,
        // visible or not, so navigation can never show a stale layout.
        let (mut app, _) = App::new();
        assert_eq!(app.window, Size::new(1280.0, 800.0));
        let _ = app.update(Message::WindowResized(Size::new(390.0, 844.0)));
        assert_eq!(app.window, Size::new(390.0, 844.0));
        assert_eq!(app.home.window, Size::new(390.0, 844.0));
        assert_eq!(app.play.window, Size::new(390.0, 844.0));
        assert_eq!(app.learn.window, Size::new(390.0, 844.0));
        assert_eq!(app.practice.window, Size::new(390.0, 844.0));
    }

    #[test]
    fn prewarm_starts_at_startup_and_retry_is_blocked_while_running() {
        let (mut app, _) = App::new();
        // The startup pre-warm owns the single install receiver.
        assert!(app.install_rx.is_some());
        let first = app.install_rx.as_ref().expect("pre-warm").clone();
        // Retry while it is running must NOT spawn a second install:
        // the receiver stays the very same one.
        let _ = app.update(Message::Dialog(settings_dialog::Msg::RetryEngine));
        assert!(app.install_rx.is_some());
        assert!(Arc::ptr_eq(
            &first,
            app.install_rx.as_ref().expect("still installing")
        ));
    }

    #[test]
    fn install_outcome_clears_receiver_updates_name_and_frees_retry() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::EngineInstalled {
            name: "Pikafish 2026-09-06".to_string(),
            dir: std::path::PathBuf::from("/tmp"),
        });
        // Outcome consumed: the guard is free and the name updated.
        assert!(app.install_rx.is_none());
        assert_eq!(app.shared.engine_name, "Pikafish 2026-09-06");
        // Retry now spawns a fresh install.
        let _ = app.update(Message::Dialog(settings_dialog::Msg::RetryEngine));
        assert!(app.install_rx.is_some());
        // And an Unavailable outcome falls back to the builtin name.
        let _ = app.update(Message::EngineUnavailable(
            "Không tìm thấy Pikafish".to_string(),
        ));
        assert!(app.install_rx.is_none());
        assert_eq!(app.shared.engine_name, "AI tích hợp");
    }

    #[test]
    fn engine_log_lives_next_to_settings() {
        let shared = Shared::new();
        let log = shared.engine_log.as_ref().expect("log dir is writable");
        assert!(
            log.path()
                .ends_with(std::path::Path::new("xiangqi/logs/engine.log")),
            "log path is {}",
            log.path().display()
        );
    }

    #[test]
    fn back_to_menu_from_practice_resets_it() {
        // Back to menu is an exit (Hieu, 2026-11-14): re-entering
        // Practice must start from scratch, not resume the old game.
        use crate::screens::practice;
        let (mut app, _) = App::new();
        let _ = app.update(Message::Goto(Screen::Practice));
        assert_eq!(app.screen, Screen::Practice);
        assert_eq!(app.practice.phase(), practice::PracticePhase::Setup);
        // Start a game, so the phase is Playing.
        let _ = app.update(Message::Practice(practice::Msg::StartFromStart));
        assert_eq!(app.practice.phase(), practice::PracticePhase::Playing);
        // Back to the menu: the screen switches AND the Practice
        // state is a fresh one (Setup phase).
        let _ = app.update(Message::Practice(practice::Msg::Navigate(Screen::Home)));
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.practice.phase(), practice::PracticePhase::Setup);
    }

    #[test]
    fn back_to_menu_mid_lesson_opens_prompt_then_leaves() {
        // Hieu, 2026-11-14: back-to-menu looked dead once the lesson
        // was under way — the save prompt was a card at the top of
        // the scrollable panel, scrolled out of view. The prompt is
        // now a full-screen overlay, and the App flow must open it
        // instead of navigating, then leave on either answer.
        use crate::screens::learn;
        use crate::screens::learn_select;
        use xq_core::types::Color;
        let (mut app, _) = App::new();
        let _ = app.update(Message::Goto(Screen::LearnSelect));
        // Pick the first Red lesson: LearnSelect starts it and
        // navigates to Learn.
        let lesson = app
            .shared
            .book
            .by_learner(Color::Red)
            .into_iter()
            .next()
            .expect("a Red lesson")
            .clone();
        let _ = app.update(Message::LearnSelect(learn_select::Msg::SelectLesson(
            lesson.id.clone(),
        )));
        // Selecting only records the choice; StartLesson records the
        // pending choice and navigates, and the App starts the lesson
        // from it (the real UI flow).
        let _ = app.update(Message::LearnSelect(learn_select::Msg::StartLesson));
        let _ = app.update(Message::LearnSelect(learn_select::Msg::Navigate(
            Screen::Learn,
        )));
        assert_eq!(app.screen, Screen::Learn);
        assert!(!app.learn.save_prompt_open());
        // Play one learner move: the lesson is now unfinished.
        assert!(!app.learn.hints().is_empty(), "hints for the first turn");
        let mv = app.learn.hints()[0].mv;
        let _ = app.update(Message::Learn(learn::Msg::Board(
            crate::board::BoardMsg::SquareClicked(mv.from),
        )));
        let _ = app.update(Message::Learn(learn::Msg::Board(
            crate::board::BoardMsg::SquareClicked(mv.to),
        )));
        assert!(app.learn.needs_save_prompt(), "unfinished lesson");
        // Back to the menu: the prompt opens, the screen stays.
        let _ = app.update(Message::Learn(learn::Msg::Navigate(Screen::Home)));
        assert_eq!(app.screen, Screen::Learn);
        assert!(app.learn.save_prompt_open(), "prompt opened");
        // Answer "don't save": back at the menu, lesson reset.
        let _ = app.update(Message::Learn(learn::Msg::SavePromptDiscard));
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.learn.save_prompt_open());
        assert!(!app.learn.needs_save_prompt(), "lesson reset on exit");
    }

    #[test]
    fn learn_select_book_game_hands_the_lesson_to_play() {
        // Hieu, 2026-11-19: "Đánh với AI theo thế trận" — the
        // picked lesson starts a book game on the Play screen.
        use crate::screens::learn_select;
        use xq_core::types::Color;
        let (mut app, _) = App::new();
        let _ = app.update(Message::Goto(Screen::LearnSelect));
        let lesson = app
            .shared
            .book
            .by_learner(Color::Red)
            .into_iter()
            .next()
            .expect("a Red lesson")
            .clone();
        let _ = app.update(Message::LearnSelect(learn_select::Msg::SelectLesson(
            lesson.id.clone(),
        )));
        // Selecting only records the choice; StartBookGame records
        // the pending book choice and navigates, and the App starts
        // the book game from it (the real UI flow).
        let _ = app.update(Message::LearnSelect(learn_select::Msg::StartBookGame));
        let _ = app.update(Message::LearnSelect(learn_select::Msg::Navigate(
            Screen::Play,
        )));
        assert_eq!(app.screen, Screen::Play);
        assert_eq!(app.play.book_lesson_id(), Some(lesson.id.as_str()));
    }
}
