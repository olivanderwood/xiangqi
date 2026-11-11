//! Fake-engine behaviour (integration spec §9.2). Runs always; the real
//! Pikafish is only needed for the `#[ignore]`d integration tests.
//!
//! The worker-level fallback rows (`crash_twice_fallback`'s "worker
//! switches to Builtin" part and `stale_request_dropped`) are tested in
//! `xq-app`'s `ai_worker` unit tests with a mock engine, because the
//! worker lives in the binary crate `xq-app` and cannot be imported here.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use xq_core::board::Position;
use xq_core::notation::parse_iccs;
use xq_engine::{Engine, EngineRequest, SearchLimits};
use xq_pikafish::{EngineOptions, LocatedEngine, PikafishEngine, SessionTimeouts};

fn short_timeouts() -> SessionTimeouts {
    SessionTimeouts {
        uciok: Duration::from_millis(500),
        readyok: Duration::from_millis(500),
        stop_wait: Duration::from_millis(300),
        bestmove_slack: Duration::from_millis(300),
    }
}

struct Fixture {
    dir: PathBuf,
    log: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "xiangqi-pikafish-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tạo thư mục test");
        let log = dir.join("uci.log");
        let state = dir.join("state.txt");
        std::fs::write(&state, "0").expect("ghi state");
        // The engine requires an NNUE file to exist (even for the fake).
        std::fs::write(dir.join("pikafish.nnue"), b"fake").expect("ghi nnue giả");
        Self { dir, log, state }
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Number of lines exactly equal to `line` (e.g. `"uci"`, not the
    /// substring inside `"ucinewgame"`).
    fn count_lines(&self, line: &str) -> usize {
        self.log_text().lines().filter(|l| *l == line).count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn spawn(script: &str, fx: &Fixture, timeouts: SessionTimeouts) -> PikafishEngine {
    // Args, not env vars: a restarted engine must get the same script,
    // and env vars are racy when tests run in parallel.
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_fake-uci"));
    let located = LocatedEngine {
        exe,
        engine_dir: fx.dir.clone(),
        nnue: fx.dir.join("pikafish.nnue"),
    };
    let mut options = EngineOptions::new(fx.dir.join("pikafish.nnue"));
    options.threads = 1;
    options.hash_mb = 16;
    let args = vec![
        "--script".to_string(),
        script.to_string(),
        "--log".to_string(),
        fx.log.to_string_lossy().into_owned(),
        "--state".to_string(),
        fx.state.to_string_lossy().into_owned(),
    ];
    PikafishEngine::spawn_with_timeouts_and_args(located, options, timeouts, args)
        .expect("fake-uci khởi chạy được")
}

fn startpos_req(limits: SearchLimits) -> EngineRequest {
    EngineRequest::new(Position::startpos().to_fen(), Vec::new(), limits)
}

#[test]
fn handshake_ok() {
    let fx = Fixture::new("handshake-ok");
    let mut engine = spawn("ok", &fx, SessionTimeouts::default());
    assert_eq!(engine.name(), "FakeUci 1.0");
    assert!(engine.is_alive());
    engine.new_game().expect("new_game");
}

#[test]
fn handshake_timeout() {
    let fx = Fixture::new("handshake-timeout");
    let t0 = Instant::now();
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_fake-uci"));
    let located = LocatedEngine {
        exe,
        engine_dir: fx.dir.clone(),
        nnue: fx.dir.join("pikafish.nnue"),
    };
    let options = EngineOptions::new(fx.dir.join("pikafish.nnue"));
    let args = vec![
        "--script".to_string(),
        "slow_ready".to_string(),
        "--log".to_string(),
        fx.log.to_string_lossy().into_owned(),
        "--state".to_string(),
        fx.state.to_string_lossy().into_owned(),
    ];
    let err = match PikafishEngine::spawn_with_timeouts_and_args(
        located,
        options,
        short_timeouts(),
        args,
    ) {
        Ok(_) => panic!("slow_ready phải timeout"),
        Err(e) => e,
    };
    assert!(
        matches!(err, xq_engine::EngineError::Timeout("readyok")),
        "mong đợi Timeout(readyok), gặp {err:?}"
    );
    assert!(
        t0.elapsed() < Duration::from_secs(16),
        "timeout phải dưới 16 s"
    );
}

#[test]
fn crash_recovers_once() {
    let fx = Fixture::new("crash-once");
    let mut engine = spawn("crash_after_go", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(500));
    let line = engine
        .best_move(&req, &mut |_| {})
        .expect("khởi chạy lại 1 lần rồi đi được");
    assert!(Position::startpos().legal_moves().contains(&line.mv));
    // Two handshakes: the first session crashed, the second served.
    assert_eq!(fx.count_lines("uci"), 2);
}

#[test]
fn crash_twice_errors() {
    let fx = Fixture::new("crash-twice");
    let mut engine = spawn("crash_always", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(500));
    let err = engine
        .best_move(&req, &mut |_| {})
        .expect_err("crash 2 lần");
    assert!(
        matches!(err, xq_engine::EngineError::Crashed(_)),
        "mong đợi Crashed, gặp {err:?}"
    );
    // Initial spawn + exactly one restart.
    assert_eq!(fx.count_lines("uci"), 2);
}

#[test]
fn illegal_bestmove_rejected() {
    let fx = Fixture::new("illegal");
    let mut engine = spawn("illegal_move", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(500));
    let err = engine
        .best_move(&req, &mut |_| {})
        .expect_err("phải từ chối");
    assert!(
        matches!(err, xq_engine::EngineError::IllegalMove(_)),
        "mong đợi IllegalMove, gặp {err:?}"
    );
}

#[test]
fn no_bestmove_timeout() {
    let fx = Fixture::new("no-bestmove");
    let mut engine = spawn("no_bestmove", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(100));
    let t0 = Instant::now();
    let err = engine
        .best_move(&req, &mut |_| {})
        .expect_err("phải timeout");
    assert!(
        matches!(err, xq_engine::EngineError::Timeout("bestmove")),
        "mong đợi Timeout(bestmove), gặp {err:?}"
    );
    assert!(t0.elapsed() < Duration::from_secs(16));
}

#[test]
fn garbage_lines_ignored() {
    let fx = Fixture::new("garbage");
    let mut engine = spawn("garbage", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(500));
    let line = engine.best_move(&req, &mut |_| {}).expect("rác bị bỏ qua");
    assert!(Position::startpos().legal_moves().contains(&line.mv));
}

#[test]
fn unknown_option_skipped() {
    let fx = Fixture::new("no-rule60");
    let mut engine = spawn("no_rule60", &fx, short_timeouts());
    assert!(
        engine
            .missing_options()
            .contains(&"Rule60MaxPly".to_string()),
        "Rule60MaxPly phải bị bỏ qua, missing = {:?}",
        engine.missing_options()
    );
    let req = startpos_req(SearchLimits::time_ms(500));
    engine.best_move(&req, &mut |_| {}).expect("vẫn đi được");
}

#[test]
fn multipv_reset() {
    let fx = Fixture::new("multipv");
    let mut engine = spawn("ok", &fx, short_timeouts());
    let req = startpos_req(SearchLimits::time_ms(500));
    let lines = engine.analyze(&req, 3, &mut |_| {}).expect("analyze top 3");
    assert_eq!(lines.len(), 3);
    let moves: Vec<_> = lines.iter().map(|l| l.mv).collect();
    assert!(moves.windows(2).all(|w| w[0] != w[1]), "3 nước khác nhau");
    for w in lines.windows(2) {
        assert!(w[0].score.to_cp_clamped() >= w[1].score.to_cp_clamped());
    }
    engine.best_move(&req, &mut |_| {}).expect("đi tiếp được");
    let log = fx.log_text();
    let set3 = log
        .find("setoption name MultiPV value 3")
        .expect("MultiPV 3");
    let back1 = log
        .find("setoption name MultiPV value 1")
        .expect("MultiPV về 1");
    assert!(back1 > set3, "MultiPV phải trả về 1 sau analyze");
}

#[test]
fn position_command_format() {
    let fx = Fixture::new("position");
    let mut engine = spawn("ok", &fx, short_timeouts());
    let req = EngineRequest::new(
        Position::startpos().to_fen(),
        vec![parse_iccs("h2e2").unwrap(), parse_iccs("h9g7").unwrap()],
        SearchLimits::time_ms(500),
    );
    engine.best_move(&req, &mut |_| {}).expect("đi được");
    let want = format!(
        "position fen {} moves h2e2 h9g7",
        Position::startpos().to_fen()
    );
    assert!(
        fx.log_text().contains(&want),
        "log phải có: {want}\nlog thực tế:\n{}",
        fx.log_text()
    );
}

#[test]
fn quit_on_drop() {
    let fx = Fixture::new("quit");
    let engine = spawn("ok", &fx, short_timeouts());
    drop(engine);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if fx.log_text().contains("EXITED") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "tiến trình phải kết thúc trong 2 s"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
