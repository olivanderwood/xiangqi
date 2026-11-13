//! All UI strings live here (§7.5) — no hardcoded strings in screens.
//! Two languages: Vietnamese (default) and English (Hieu, 2026-11-12).
//! The current language is a process-wide setting (see `set_language`);
//! `t(key)` returns the string for it, so call sites never change.
//! Chinese characters appear ONLY on the board pieces; every UI string
//! and every move name is Vietnamese or English.
//!
//! Templates use `{n}` / `{name}` placeholders; screens substitute values
//! with `str::replace`, the template itself stays in this file.

use std::sync::atomic::{AtomicU8, Ordering};

use xq_core::analysis::Grade;

/// UI language.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    /// Tiếng Việt (default).
    #[default]
    Vietnamese,
    English,
}

impl Language {
    /// Parse the persisted language name (`"vi"` / `"en"`).
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "en" => Language::English,
            _ => Language::Vietnamese,
        }
    }
}

/// Current UI language (process-wide). 0 = Vietnamese, 1 = English.
static LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// Set the UI language (called once at startup from saved settings,
/// and whenever the user changes it in Settings).
pub fn set_language(lang: Language) {
    LANGUAGE.store(
        match lang {
            Language::Vietnamese => 0,
            Language::English => 1,
        },
        Ordering::Relaxed,
    );
}

/// Current UI language.
#[must_use]
pub fn language() -> Language {
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => Language::English,
        _ => Language::Vietnamese,
    }
}

// NOTE (integration): not every key is constructed yet — the
// Settings/About dialog keys (P4) and a few screen strings are still
// literals or land with their screens. The enum is the full string
// table ported from egui; the allow keeps the gate green until the
// last consumer lands, instead of deleting keys P4 needs.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    AppTitle,
    AppSubtitle,
    PlayVsAi,
    LearnOpenings,
    Settings,
    About,
    NewGame,
    Undo,
    Hint,
    DrawOffer,
    Resign,
    FlipBoard,
    Thinking,
    ThinkingDepth,
    Check,
    BackToMenu,
    Close,
    Start,
    Cancel,
    Side,
    SideRed,
    SideBlack,
    SideRandom,
    TwoPlayers,
    Difficulty,
    DiffEasy,
    DiffMedium,
    DiffHard,
    DiffMaster,
    StartGameTitle,
    RedToMove,
    BlackToMove,
    RedSide,
    BlackSide,
    MovesTitle,
    GameInfo,
    HintsUsed,
    AiAcceptedDraw,
    AiDeclinedDraw,
    ResultTitle,
    ReviewGame,
    Reviewing,
    BackToEnd,
    LearnFirst,
    LearnSecond,
    NoLessonsYet,
    LevelStars,
    CompletedMark,
    IntroTitle,
    StartLesson,
    TutorTitle,
    HideHints,
    ShowHints,
    RetryMove,
    ContinueMove,
    OffBookTitle,
    OffBookBody,
    BookMoveShouldBe,
    OpeningCompleteTitle,
    CorrectBookMoves,
    MistakesAndInaccuracies,
    EngineScore,
    StopAndSummary,
    ContinueWithAi,
    SummaryTitle,
    LearnAgain,
    OtherLesson,
    ViewPosition,
    ProgressMoves,
    BestDirections,
    AdvantageScore,
    SoundOn,
    NotationLabel,
    NotationVn,
    NotationEn,
    LanguageLabel,
    LanguageVi,
    LanguageEn,
    AnimateLabel,
    AboutBody,
    AboutFonts,
    AboutNoSound,
    VsAiContinueNote,
    YourTurnLearn,
    NoHintAvailable,
    OpponentChooseTitle,
    OpponentChooseMsg,
    NextMovesTitle,
    NextMovesOpponent,
    NextMovesFollow,
    LessonNotFound,
    GradeLabel,
    EngineSection,
    EngineInUse,
    EngineThreads,
    EngineHash,
    EngineHashAuto,
    EngineCustomElo,
    DiffCustom,
    EngineDiversity,
    EngineTournament,
    EnginePath,
    EngineRetry,
    EngineInstalling,
    EngineTraceLog,
    AboutEngine,
    AboutEngineBody,
    AboutNnue,
    OpenEngineFolder,
    OpenEngineLog,
    ChessDbTitle,
    ChessDbLoading,
    ChessDbNoData,
    ChessDbWinRate,
    ChessDbBestMove,
    // --- Self practice mode (Hieu, 2026-11-12) ---
    PracticeTitle,
    PracticeDifficulty,
    PracticeStartPos,
    PracticeFromFen,
    PracticeFromImage,
    PracticeFenLabel,
    PracticeFenHint,
    PracticeLoadFen,
    PracticeFenInvalid,
    PracticePickImage,
    PracticeImageNote,
    PracticeImageNotFound,
    PracticeImageNotRecognized,
    PracticeImageRef,
    PracticeEditorTitle,
    PracticeEditorHint,
    PracticeUsePosition,
    PracticeEditorInvalid,
    PracticeSideToMove,
    PracticeClearBoard,
    PracticeEraser,
    PracticeSuggestions,
    PracticeSuggestChessDb,
    PracticeSuggestEngine,
    PracticeThinking,
    PracticeNoSuggestions,
    PracticeGradeBest,
    PracticeGradeGood,
    PracticeGradeAcceptable,
    PracticeGradeInaccurate,
    PracticeGradeBlunder,
    PracticeLastMove,
    PracticeRedWins,
    PracticeBlackWins,
    PracticeDraw,
    PracticeReasonCheckmate,
    PracticeReasonStalemate,
    PracticeReasonRepetition,
    PracticeBackToSetup,
    // --- P4: Settings / About dialogs + screen literals ---
    PlayerYou,
    PlayYourTurn,
    PlayWaitingForYou,
    HomePlayDesc,
    HomeLearnDesc,
    HomePracticeDesc,
    HomeVersion,
    HomeLearnButton,
    HomePracticeButton,
    LearnSelectSubtitle,
    GradeBook,
    GradeBest,
    GradeGood,
    GradeAcceptable,
    GradeInaccurate,
    GradeBlunder,
    AboutPikafishVersion,
    AboutBook,
    AboutSoundNote,
    AboutChessDbNote,
    SettingsEngineApplyNote,
}

/// UI string for `key` in the current language (see `set_language`).
#[must_use]
pub fn t(key: Key) -> &'static str {
    match language() {
        Language::Vietnamese => t_vi(key),
        Language::English => t_en(key),
    }
}

/// Vietnamese strings.
fn t_vi(key: Key) -> &'static str {
    match key {
        Key::AppTitle => "Cờ Tướng",
        Key::AppSubtitle => "Đấu với máy • Học chơi khai cuộc",
        Key::PlayVsAi => "Đấu với AI",
        Key::LearnOpenings => "Học chơi khai cuộc",
        Key::Settings => "Cài đặt",
        Key::About => "Giới thiệu",
        Key::NewGame => "Ván mới",
        Key::Undo => "Hoàn nước",
        Key::Hint => "Gợi ý",
        Key::DrawOffer => "Xin hòa",
        Key::Resign => "Xin thua",
        Key::FlipBoard => "Lật bàn",
        Key::Thinking => "Máy đang nghĩ…",
        Key::ThinkingDepth => "Máy đang nghĩ… (độ sâu {n})",
        Key::Check => "Chiếu!",
        Key::BackToMenu => "Về menu",
        Key::Close => "Đóng",
        Key::Start => "Bắt đầu",
        Key::Cancel => "Hủy",
        Key::Side => "Bên",
        Key::SideRed => "Đỏ đi trước",
        Key::SideBlack => "Đen",
        Key::SideRandom => "Ngẫu nhiên",
        Key::TwoPlayers => "2 người chơi",
        Key::Difficulty => "Mức độ",
        Key::DiffEasy => "Dễ",
        Key::DiffMedium => "Trung bình",
        Key::DiffHard => "Khó",
        Key::DiffMaster => "Cao thủ",
        Key::StartGameTitle => "Bắt đầu ván mới",
        Key::RedToMove => "Lượt Đỏ đi",
        Key::BlackToMove => "Lượt Đen đi",
        Key::RedSide => "Đỏ",
        Key::BlackSide => "Đen",
        Key::MovesTitle => "Danh sách nước",
        Key::GameInfo => "Thông tin ván",
        Key::HintsUsed => "Đã dùng {n} gợi ý",
        Key::AiAcceptedDraw => "AI đồng ý hòa.",
        Key::AiDeclinedDraw => "AI từ chối hòa.",
        Key::ResultTitle => "Kết quả ván đấu",
        Key::ReviewGame => "Xem lại ván",
        Key::Reviewing => "Đang xem lại nước {n}/{m}",
        Key::BackToEnd => "Về cuối ván",
        Key::LearnFirst => "Đi tiên (cầm Đỏ)",
        Key::LearnSecond => "Đi hậu (cầm Đen)",
        Key::NoLessonsYet => "Chưa có bài học cho bên này.",
        Key::LevelStars => "Mức {n}/3",
        Key::CompletedMark => "✓ Đã hoàn thành",
        Key::IntroTitle => "Giới thiệu bài",
        Key::StartLesson => "Bắt đầu",
        Key::TutorTitle => "Gia sư",
        Key::HideHints => "Ẩn gợi ý",
        Key::ShowHints => "Xem gợi ý",
        Key::RetryMove => "Đi lại",
        Key::ContinueMove => "Tiếp tục",
        Key::OffBookTitle => "Nước ngoài sách",
        Key::OffBookBody => "Nước này không có trong sách khai cuộc.",
        Key::BookMoveShouldBe => "Nước sách nên đi: {name}",
        Key::OpeningCompleteTitle => "Hoàn thành khai cuộc",
        Key::CorrectBookMoves => "Đúng sách: {n}/{m}",
        Key::MistakesAndInaccuracies => "Sai lầm: {n} • Thiếu chính xác: {m}",
        Key::EngineScore => "Điểm engine thế cờ cuối: {n}",
        Key::StopAndSummary => "Dừng và xem tổng kết",
        Key::ContinueWithAi => "Đánh tiếp với AI",
        Key::SummaryTitle => "Tổng kết bài học",
        Key::LearnAgain => "Học lại bài này",
        Key::OtherLesson => "Bài khác",
        Key::ViewPosition => "Bấm một dòng để xem thế cờ lúc đó.",
        Key::ProgressMoves => "Nước {n} / khoảng {m}",
        Key::BestDirections => "Các hướng đi tốt nhất",
        Key::AdvantageScore => "Điểm lợi thế",
        Key::SoundOn => "Âm thanh",
        Key::NotationLabel => "Ký hiệu nước đi",
        Key::NotationVn => "Việt Nam (P2-5)",
        Key::AnimateLabel => "Hiệu ứng chuyển quân",
        Key::AboutBody => "Cờ Tướng AI {n} — game cờ tướng viết bằng Rust.",
        Key::AboutFonts => "Font chữ Hán: Noto Serif CJK SC (SIL Open Font License 1.1, xem assets/fonts/OFL.txt). Font giao diện: Noto Sans (OFL 1.1).",
        Key::AboutNoSound => "Âm thanh hiện chỉ là công tắc lưu trữ; chưa có phát âm thanh trong bản này.",
        Key::VsAiContinueNote => "Ván tiếp tục từ bài học — gợi ý tự động bật.",
        Key::YourTurnLearn => "Đến lượt bạn. Chọn một nước trong gợi ý, hoặc tự đi.",
        Key::NoHintAvailable => "Chưa có gợi ý.",
        Key::OpponentChooseTitle => "Đối thủ đi biến nào?",
        Key::OpponentChooseMsg => "Đến lượt đối thủ — bấm một biến để đối thủ đi biến đó.",
        Key::NextMovesTitle => "Nước tiếp theo theo sách",
        Key::NextMovesOpponent => "Đối thủ sẽ đi {name}",
        Key::NextMovesFollow => "Sau đó bạn có thể đi:",
        Key::LessonNotFound => "Không tìm thấy bài học.",
        Key::GradeLabel => "Chấm",
        Key::EngineSection => "Engine cờ (Pikafish)",
        Key::EngineInUse => "Engine đang dùng: {n}",
        Key::EngineThreads => "Số luồng",
        Key::EngineHash => "Bộ nhớ Hash (MB)",
        Key::EngineHashAuto => "Tự động (theo RAM)",
        Key::EngineCustomElo => "Elo tùy chỉnh",
        Key::DiffCustom => "Tùy chỉnh (Elo)",
        Key::EngineDiversity => "Đa dạng nước đi (Cao thủ)",
        Key::EngineTournament => "Chế độ thi đấu (tắt đa dạng)",
        Key::EnginePath => "Đường dẫn Pikafish (để trống = tự tìm)",
        Key::EngineRetry => "Thử lại Pikafish",
        Key::EngineInstalling => "Đang cài Pikafish…",
        Key::EngineTraceLog => "Ghi chi tiết UCI vào engine.log",
        Key::AboutEngine => "Engine Pikafish {n} — GPLv3, mã nguồn: github.com/official-pikafish/Pikafish.",
        Key::AboutEngineBody => "AI mặc định là Pikafish (mạnh nhất hiện nay cho cờ tướng). Khi không chạy được Pikafish, game tự dùng AI tích hợp.",
        Key::AboutNnue => "Mạng nơ-ron pikafish.nnue chỉ dùng phi thương mại (xem NNUE-License).",
        Key::OpenEngineFolder => "Mở thư mục engine",
        Key::OpenEngineLog => "Mở engine.log",
        Key::ChessDbTitle => "Điểm ChessDB",
        Key::ChessDbLoading => "Đang lấy điểm ChessDB…",
        Key::ChessDbNoData => "ChessDB chưa có dữ liệu cho thế cờ này",
        Key::ChessDbWinRate => "Đỏ thắng {n}% • Hòa {m}% • Thua {k}%",
        Key::ChessDbBestMove => "Nước tốt nhất: {name}",
        Key::NotationEn => "Tiếng Anh (C2-5)",
        Key::LanguageLabel => "Ngôn ngữ",
        Key::LanguageVi => "Tiếng Việt",
        Key::LanguageEn => "English",
        // --- Tự luyện tập ---
        Key::PracticeTitle => "Tự luyện tập",
        Key::PracticeDifficulty => "Độ khó (Pikafish)",
        Key::PracticeStartPos => "Thế cờ ban đầu",
        Key::PracticeFromFen => "Từ FEN",
        Key::PracticeFromImage => "Từ ảnh bàn cờ",
        Key::PracticeFenLabel => "Dán FEN của thế cờ",
        Key::PracticeFenHint => "Dán FEN vào đây…",
        Key::PracticeLoadFen => "Tải thế cờ",
        Key::PracticeFenInvalid => "FEN không hợp lệ.",
        Key::PracticePickImage => "Tải ảnh từ thiết bị",
        Key::PracticeImageNote => "Ảnh chỉ dùng làm tham chiếu — app chưa tự đọc được thế cờ từ ảnh (cần mô hình nhận diện riêng). Dùng bộ soạn bên dưới để đặt quân theo ảnh.",
        Key::PracticeImageNotFound => "Không tìm thấy file ảnh.",
        Key::PracticeImageNotRecognized => "Không nhận diện được ảnh này. Hãy dùng bộ soạn thế cờ bên dưới để dựng lại thế cờ từ ảnh.",
        Key::PracticeImageRef => "Ảnh tham chiếu",
        Key::PracticeEditorTitle => "Soạn thế cờ theo ảnh",
        Key::PracticeEditorHint => "Chọn một quân rồi bấm vào ô để đặt; bấm vào quân trên bàn để xóa.",
        Key::PracticeUsePosition => "Dùng thế cờ này",
        Key::PracticeEditorInvalid => "Thế cờ chưa hợp lệ: cần đủ 2 Tướng và 2 Tướng không được đối mặt.",
        Key::PracticeSideToMove => "Bên đi",
        Key::PracticeClearBoard => "Xóa bàn",
        Key::PracticeEraser => "Tẩy",
        Key::PracticeSuggestions => "Gợi ý nước đi",
        Key::PracticeSuggestChessDb => "Gợi ý (ChessDB)",
        Key::PracticeSuggestEngine => "Gợi ý (Pikafish)",
        Key::PracticeThinking => "Đang tính gợi ý…",
        Key::PracticeNoSuggestions => "Chưa có gợi ý cho thế cờ này.",
        Key::PracticeGradeBest => "Nước tốt nhất",
        Key::PracticeGradeGood => "Tốt",
        Key::PracticeGradeAcceptable => "Chấp nhận được",
        Key::PracticeGradeInaccurate => "Thiếu chính xác",
        Key::PracticeGradeBlunder => "Sai lầm",
        Key::PracticeLastMove => "Nước vừa đi",
        Key::PracticeRedWins => "Đỏ thắng",
        Key::PracticeBlackWins => "Đen thắng",
        Key::PracticeDraw => "Hòa",
        Key::PracticeReasonCheckmate => "chiếu hết",
        Key::PracticeReasonStalemate => "hết nước đi",
        Key::PracticeReasonRepetition => "lặp nước",
        Key::PracticeBackToSetup => "Đổi thế cờ",
        // --- P4: Settings / About dialogs + screen literals ---
        Key::PlayerYou => "Bạn",
        Key::PlayYourTurn => "Đến lượt bạn",
        Key::PlayWaitingForYou => "Chờ bạn",
        Key::HomePlayDesc => "Đấu với Pikafish ở mức độ bạn chọn. Có thể hoàn nước và xin gợi ý.",
        Key::HomeLearnDesc => "Chọn Tiên hoặc Hậu, máy làm đối thủ và gợi ý các nước đi theo khai cuộc.",
        Key::HomePracticeDesc => "Tự đi quân cho cả hai bên trên bàn cờ, không có máy đối thủ.",
        Key::HomeVersion => "Phiên bản {n} · Pikafish 2026-09-06",
        Key::HomeLearnButton => "Vào học",
        Key::HomePracticeButton => "Vào luyện tập",
        Key::LearnSelectSubtitle => "Máy sẽ là đối thủ và gợi ý các nước đi theo khai cuộc.",
        Key::GradeBook => "Đúng sách",
        Key::GradeBest => "Nước tốt nhất",
        Key::GradeGood => "Tốt",
        Key::GradeAcceptable => "Chấp nhận được",
        Key::GradeInaccurate => "Thiếu chính xác",
        Key::GradeBlunder => "Sai lầm",
        Key::AboutPikafishVersion => "Engine: Pikafish 2026-09-06",
        Key::AboutBook => "Sách khai cuộc: {n} bài.",
        Key::AboutSoundNote => "Âm thanh: tiếng đi / ăn / chiếu / chiếu hết được tổng hợp bằng code và phát khi bật âm thanh trong Cài đặt.",
        Key::AboutChessDbNote => "Điểm ChessDB: hệ thống chấm điểm riêng qua mạng (chessdb.cn); không có mạng thì hiện “không khả dụng”.",
        Key::SettingsEngineApplyNote => "Thay đổi threads / hash / đường dẫn có hiệu lực khi bấm “Thử lại Pikafish” (hoặc khởi động lại app).",
    }
}

/// English strings.
fn t_en(key: Key) -> &'static str {
    match key {
        Key::AppTitle => "Xiangqi",
        Key::AppSubtitle => "Play vs AI • Learn openings",
        Key::PlayVsAi => "Play vs AI",
        Key::LearnOpenings => "Learn openings",
        Key::Settings => "Settings",
        Key::About => "About",
        Key::NewGame => "New game",
        Key::Undo => "Undo",
        Key::Hint => "Hint",
        Key::DrawOffer => "Offer draw",
        Key::Resign => "Resign",
        Key::FlipBoard => "Flip board",
        Key::Thinking => "AI is thinking…",
        Key::ThinkingDepth => "AI is thinking… (depth {n})",
        Key::Check => "Check!",
        Key::BackToMenu => "Back to menu",
        Key::Close => "Close",
        Key::Start => "Start",
        Key::Cancel => "Cancel",
        Key::Side => "Side",
        Key::SideRed => "Red first",
        Key::SideBlack => "Black",
        Key::SideRandom => "Random",
        Key::TwoPlayers => "2 players",
        Key::Difficulty => "Difficulty",
        Key::DiffEasy => "Easy",
        Key::DiffMedium => "Medium",
        Key::DiffHard => "Hard",
        Key::DiffMaster => "Master",
        Key::StartGameTitle => "Start a new game",
        Key::RedToMove => "Red to move",
        Key::BlackToMove => "Black to move",
        Key::RedSide => "Red",
        Key::BlackSide => "Black",
        Key::MovesTitle => "Move list",
        Key::GameInfo => "Game info",
        Key::HintsUsed => "Used {n} hints",
        Key::AiAcceptedDraw => "AI accepted the draw.",
        Key::AiDeclinedDraw => "AI declined the draw.",
        Key::ResultTitle => "Game result",
        Key::ReviewGame => "Review game",
        Key::Reviewing => "Reviewing move {n}/{m}",
        Key::BackToEnd => "Back to end",
        Key::LearnFirst => "First player (Red)",
        Key::LearnSecond => "Second player (Black)",
        Key::NoLessonsYet => "No lessons for this side yet.",
        Key::LevelStars => "Level {n}/3",
        Key::CompletedMark => "✓ Completed",
        Key::IntroTitle => "Lesson intro",
        Key::StartLesson => "Start",
        Key::TutorTitle => "Tutor",
        Key::HideHints => "Hide hints",
        Key::ShowHints => "Show hints",
        Key::RetryMove => "Retry",
        Key::ContinueMove => "Continue",
        Key::OffBookTitle => "Out-of-book move",
        Key::OffBookBody => "This move is not in the opening book.",
        Key::BookMoveShouldBe => "Book move: {name}",
        Key::OpeningCompleteTitle => "Opening complete",
        Key::CorrectBookMoves => "Book moves: {n}/{m}",
        Key::MistakesAndInaccuracies => "Mistakes: {n} • Inaccuracies: {m}",
        Key::EngineScore => "Engine score at the end: {n}",
        Key::StopAndSummary => "Stop and view summary",
        Key::ContinueWithAi => "Continue vs AI",
        Key::SummaryTitle => "Lesson summary",
        Key::LearnAgain => "Learn this lesson again",
        Key::OtherLesson => "Another lesson",
        Key::ViewPosition => "Tap a line to view the position.",
        Key::ProgressMoves => "Move {n} / about {m}",
        Key::BestDirections => "Best lines",
        Key::AdvantageScore => "Advantage",
        Key::SoundOn => "Sound",
        Key::NotationLabel => "Move notation",
        Key::NotationVn => "Vietnamese (P2-5)",
        Key::NotationEn => "English (C2-5)",
        Key::LanguageLabel => "Language",
        Key::LanguageVi => "Tiếng Việt",
        Key::LanguageEn => "English",
        Key::AnimateLabel => "Piece slide animation",
        Key::AboutBody => "Xiangqi AI {n} — a Xiangqi game written in Rust.",
        Key::AboutFonts => "Chinese fonts: Noto Serif CJK SC (SIL Open Font License 1.1, see assets/fonts/OFL.txt). UI font: Noto Sans (OFL 1.1).",
        Key::AboutNoSound => "Sound is currently only a saved toggle; this build has no audio playback yet.",
        Key::VsAiContinueNote => "Continuing from a lesson — auto hints on.",
        Key::YourTurnLearn => "Your turn. Pick a suggested move, or play your own.",
        Key::NoHintAvailable => "No hint yet.",
        Key::OpponentChooseTitle => "Which line does the opponent play?",
        Key::OpponentChooseMsg => "Opponent to move — tap a line for the opponent to play it.",
        Key::NextMovesTitle => "Next moves from the book",
        Key::NextMovesOpponent => "Opponent will play {name}",
        Key::NextMovesFollow => "Then you can play:",
        Key::LessonNotFound => "Lesson not found.",
        Key::GradeLabel => "Grade",
        Key::EngineSection => "Engine (Pikafish)",
        Key::EngineInUse => "Engine in use: {n}",
        Key::EngineThreads => "Threads",
        Key::EngineHash => "Hash memory (MB)",
        Key::EngineHashAuto => "Auto (by RAM)",
        Key::EngineCustomElo => "Custom Elo",
        Key::DiffCustom => "Custom (Elo)",
        Key::EngineDiversity => "Varied moves (Master)",
        Key::EngineTournament => "Tournament mode (variety off)",
        Key::EnginePath => "Pikafish path (empty = auto)",
        Key::EngineRetry => "Retry Pikafish",
        Key::EngineInstalling => "Installing Pikafish…",
        Key::EngineTraceLog => "Log UCI details to engine.log",
        Key::AboutEngine => "Pikafish engine {n} — GPLv3, source: github.com/official-pikafish/Pikafish.",
        Key::AboutEngineBody => "The default AI is Pikafish (the strongest Xiangqi engine today). When Pikafish cannot run, the game falls back to the built-in AI.",
        Key::AboutNnue => "The pikafish.nnue network is for non-commercial use only (see NNUE-License).",
        Key::OpenEngineFolder => "Open engine folder",
        Key::OpenEngineLog => "Open engine.log",
        Key::ChessDbTitle => "ChessDB score",
        Key::ChessDbLoading => "Loading ChessDB score…",
        Key::ChessDbNoData => "ChessDB has no data for this position",
        Key::ChessDbWinRate => "Red wins {n}% • Draw {m}% • Loses {k}%",
        Key::ChessDbBestMove => "Best move: {name}",
        // --- Self practice ---
        Key::PracticeTitle => "Self practice",
        Key::PracticeDifficulty => "Difficulty (Pikafish)",
        Key::PracticeStartPos => "Starting position",
        Key::PracticeFromFen => "From FEN",
        Key::PracticeFromImage => "From a board photo",
        Key::PracticeFenLabel => "Paste the position FEN",
        Key::PracticeFenHint => "Paste a FEN here…",
        Key::PracticeLoadFen => "Load position",
        Key::PracticeFenInvalid => "Invalid FEN.",
        Key::PracticePickImage => "Pick an image from the device",
        Key::PracticeImageNote => "The photo is only a reference — the app cannot read a position from a photo yet (it needs a separate vision model). Use the editor below to place the pieces from the photo.",
        Key::PracticeImageNotFound => "Image file not found.",
        Key::PracticeImageNotRecognized => "This image could not be recognised. Use the board editor below to set up the position from the photo.",
        Key::PracticeImageRef => "Reference photo",
        Key::PracticeEditorTitle => "Set up the position from the photo",
        Key::PracticeEditorHint => "Pick a piece, then tap a square to place it; tap a piece on the board to remove it.",
        Key::PracticeUsePosition => "Use this position",
        Key::PracticeEditorInvalid => "Position not valid: both Generals are required and they must not face each other.",
        Key::PracticeSideToMove => "Side to move",
        Key::PracticeClearBoard => "Clear board",
        Key::PracticeEraser => "Eraser",
        Key::PracticeSuggestions => "Suggested moves",
        Key::PracticeSuggestChessDb => "Suggestions (ChessDB)",
        Key::PracticeSuggestEngine => "Suggestions (Pikafish)",
        Key::PracticeThinking => "Calculating suggestions…",
        Key::PracticeNoSuggestions => "No suggestions for this position.",
        Key::PracticeGradeBest => "Best move",
        Key::PracticeGradeGood => "Good",
        Key::PracticeGradeAcceptable => "Acceptable",
        Key::PracticeGradeInaccurate => "Inaccurate",
        Key::PracticeGradeBlunder => "Blunder",
        Key::PracticeLastMove => "Last move",
        Key::PracticeRedWins => "Red wins",
        Key::PracticeBlackWins => "Black wins",
        Key::PracticeDraw => "Draw",
        Key::PracticeReasonCheckmate => "checkmate",
        Key::PracticeReasonStalemate => "stalemate",
        Key::PracticeReasonRepetition => "repetition",
        Key::PracticeBackToSetup => "Change position",
        // --- P4: Settings / About dialogs + screen literals ---
        Key::PlayerYou => "You",
        Key::PlayYourTurn => "Your turn",
        Key::PlayWaitingForYou => "Waiting for you",
        Key::HomePlayDesc => "Play Pikafish at the level you choose. Undo moves and ask for hints.",
        Key::HomeLearnDesc => "Play Red or Black — the AI is your opponent and suggests book moves.",
        Key::HomePracticeDesc => "Move the pieces for both sides yourself, with no AI opponent.",
        Key::HomeVersion => "Version {n} · Pikafish 2026-09-06",
        Key::HomeLearnButton => "Start learning",
        Key::HomePracticeButton => "Start practice",
        Key::LearnSelectSubtitle => "The AI plays as your opponent and suggests book moves.",
        Key::GradeBook => "Book move",
        Key::GradeBest => "Best move",
        Key::GradeGood => "Good",
        Key::GradeAcceptable => "Acceptable",
        Key::GradeInaccurate => "Inaccurate",
        Key::GradeBlunder => "Blunder",
        Key::AboutPikafishVersion => "Engine: Pikafish 2026-09-06",
        Key::AboutBook => "Opening book: {n} lessons.",
        Key::AboutSoundNote => "Sound: move / capture / check / checkmate tones are synthesized in code and play when sound is on in Settings.",
        Key::AboutChessDbNote => "ChessDB score: a separate scoring system over the network (chessdb.cn); without a network it shows “unavailable”.",
        Key::SettingsEngineApplyNote => "Threads / hash / path changes apply when you press “Retry Pikafish” (or restart the app).",
    }
}

/// Difficulty index (0..=3) to its Vietnamese name.
#[must_use]
pub fn difficulty_name(d: u8) -> &'static str {
    match d {
        0 => t(Key::DiffEasy),
        2 => t(Key::DiffHard),
        3 => t(Key::DiffMaster),
        4 => t(Key::DiffCustom),
        _ => t(Key::DiffMedium),
    }
}

/// Localised label of a move [`Grade`] in the current language
/// (P4: replaces the Vietnamese-only `Grade::label_vi` at the
/// display sites in Play / Learn / Summary; the Tutor's stored
/// explanations stay as-is — they are lesson data, not UI chrome).
#[must_use]
pub fn grade_label(grade: Grade) -> &'static str {
    match grade {
        Grade::Book => t(Key::GradeBook),
        Grade::Best => t(Key::GradeBest),
        Grade::Good => t(Key::GradeGood),
        Grade::Acceptable => t(Key::GradeAcceptable),
        Grade::Inaccurate => t(Key::GradeInaccurate),
        Grade::Blunder => t(Key::GradeBlunder),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p4_keys_have_both_languages() {
        for (key, vi, en) in [
            (Key::PlayerYou, "Bạn", "You"),
            (Key::PlayYourTurn, "Đến lượt bạn", "Your turn"),
            (Key::PlayWaitingForYou, "Chờ bạn", "Waiting for you"),
            (
                Key::LearnSelectSubtitle,
                "Máy sẽ là đối thủ và gợi ý các nước đi theo khai cuộc.",
                "The AI plays as your opponent and suggests book moves.",
            ),
            (Key::GradeBook, "Đúng sách", "Book move"),
            (Key::GradeBest, "Nước tốt nhất", "Best move"),
            (Key::GradeGood, "Tốt", "Good"),
            (Key::GradeAcceptable, "Chấp nhận được", "Acceptable"),
            (Key::GradeInaccurate, "Thiếu chính xác", "Inaccurate"),
            (Key::GradeBlunder, "Sai lầm", "Blunder"),
            (
                Key::AboutPikafishVersion,
                "Engine: Pikafish 2026-09-06",
                "Engine: Pikafish 2026-09-06",
            ),
        ] {
            set_language(Language::Vietnamese);
            assert_eq!(t(key), vi, "{key:?} vi");
            set_language(Language::English);
            assert_eq!(t(key), en, "{key:?} en");
        }
        set_language(Language::Vietnamese);
    }

    #[test]
    fn grade_label_follows_language() {
        set_language(Language::Vietnamese);
        assert_eq!(grade_label(Grade::Book), "Đúng sách");
        assert_eq!(grade_label(Grade::Blunder), "Sai lầm");
        set_language(Language::English);
        assert_eq!(grade_label(Grade::Book), "Book move");
        assert_eq!(grade_label(Grade::Blunder), "Blunder");
        set_language(Language::Vietnamese);
    }

    #[test]
    fn home_version_template_carries_pikafish_version() {
        set_language(Language::Vietnamese);
        assert_eq!(
            t(Key::HomeVersion).replace("{n}", "0.1.0"),
            "Phiên bản 0.1.0 · Pikafish 2026-09-06"
        );
        set_language(Language::English);
        assert_eq!(
            t(Key::HomeVersion).replace("{n}", "0.1.0"),
            "Version 0.1.0 · Pikafish 2026-09-06"
        );
        set_language(Language::Vietnamese);
    }
}
