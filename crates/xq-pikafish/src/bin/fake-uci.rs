//! Fake UCI engine for tests (integration spec §9.2).
//!
//! Test-only binary, never shipped. It reads a script name from
//! `FAKE_UCI_SCRIPT` and behaves accordingly. Every line it receives is
//! appended to `FAKE_UCI_LOG` (when set) so tests can assert on the exact
//! commands the session sent; `EXITED` is appended on clean exit.
//!
//! Scripts:
//! - `ok`: well-behaved engine (default).
//! - `slow_ready`: answers `uci` but never `readyok`.
//! - `crash_after_go`: exits(1) on the first `go` (tracked in
//!   `FAKE_UCI_STATE_FILE`), behaves like `ok` afterwards.
//! - `crash_always`: exits(1) on every `go`.
//! - `illegal_move`: answers `bestmove a0a9` (illegal).
//! - `no_bestmove`: never answers `go`.
//! - `garbage`: valid replies with garbage lines mixed in.
//! - `no_rule60`: like `ok` but does not advertise `Rule60MaxPly` /
//!   `Sixty Move Rule`.
//! - `slow_go`: waits 2 s before answering `go`.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use xq_core::board::Position;
use xq_core::notation::parse_iccs;

fn log_path() -> Option<PathBuf> {
    arg_value("--log")
        .map(PathBuf::from)
        .or_else(|| std::env::var("FAKE_UCI_LOG").ok().map(PathBuf::from))
}

fn arg_value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn log_line(line: &str) {
    if let Some(path) = log_path() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn option_lines(script: &str) -> Vec<String> {
    let mut opts = vec![
        "option name EvalFile type string default <empty>".to_string(),
        "option name Threads type spin default 1 min 1 max 8".to_string(),
        "option name Hash type spin default 256 min 1 max 1024".to_string(),
        "option name MultiPV type spin default 1 min 1 max 5".to_string(),
        "option name UCI_LimitStrength type check default false".to_string(),
        "option name UCI_Elo type spin default 1280 min 1280 max 3133".to_string(),
        "option name Skill Level type spin default 20 min 0 max 20".to_string(),
        "option name Repetition Rule type combo default AsianRule var AsianRule var ChineseRule"
            .to_string(),
        "option name Ponder type check default false".to_string(),
    ];
    if script != "no_rule60" {
        opts.push("option name Sixty Move Rule type check default true".to_string());
        opts.push("option name Rule60MaxPly type spin default 120 min 0 max 1000".to_string());
    }
    opts
}

fn state_count() -> u64 {
    let Some(path) = arg_value("--state").or_else(|| std::env::var("FAKE_UCI_STATE_FILE").ok())
    else {
        return 0;
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn set_state_count(n: u64) {
    if let Some(path) = arg_value("--state").or_else(|| std::env::var("FAKE_UCI_STATE_FILE").ok()) {
        let _ = std::fs::write(path, n.to_string());
    }
}

fn main() {
    // Args win over env vars: a restarted engine must get the same
    // script/log/state as the first spawn, and env vars are racy when
    // tests run in parallel.
    let script = arg_value("--script")
        .or_else(|| std::env::var("FAKE_UCI_SCRIPT").ok())
        .unwrap_or_else(|| "ok".to_string());
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut pos = Position::startpos();
    let mut multipv = 1usize;
    for line in BufReader::new(stdin).lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        log_line(&line);
        let mut parts = line.split_whitespace();
        match parts.next().unwrap_or("") {
            "uci" => {
                println!("id name FakeUci 1.0");
                println!("id author Test");
                for o in option_lines(&script) {
                    println!("{o}");
                }
                println!("uciok");
                let _ = out.flush();
            }
            "isready" => {
                if script != "slow_ready" {
                    println!("readyok");
                    let _ = out.flush();
                }
            }
            "setoption" => {
                // `setoption name MultiPV value N`
                let rest: Vec<&str> = line.split_whitespace().collect();
                if rest.len() >= 5 && rest[2] == "MultiPV" {
                    multipv = rest[4].parse().unwrap_or(1);
                }
            }
            "ucinewgame" => {}
            "position" => {
                // `position fen <fen> [moves ...]`
                let rest: Vec<&str> = line.split_whitespace().collect();
                if rest.len() >= 2 && rest[1] == "fen" {
                    // FEN is 6 fields (board side - - halfmove fullmove);
                    // our FEN has exactly 6 fields.
                    if rest.len() >= 8 {
                        let fen = rest[2..8].join(" ");
                        if let Ok(p) = Position::from_fen(&fen) {
                            pos = p;
                        }
                        if let Some(at) = rest.iter().position(|t| *t == "moves") {
                            for tok in &rest[at + 1..] {
                                if let Ok(mv) = parse_iccs(tok) {
                                    if pos.legal_moves().contains(&mv) {
                                        pos.make_move(mv);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "go" => {
                if script == "crash_always" {
                    std::process::exit(1);
                }
                if script == "crash_after_go" {
                    let n = state_count();
                    set_state_count(n + 1);
                    if n == 0 {
                        std::process::exit(1);
                    }
                }
                if script == "no_bestmove" {
                    continue;
                }
                if script == "slow_go" {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
                // `go ... searchmoves <mv>`: answer exactly that move.
                let tokens: Vec<&str> = line.split_whitespace().collect();
                let searchmoves = tokens
                    .iter()
                    .position(|t| *t == "searchmoves")
                    .and_then(|i| tokens.get(i + 1).copied());
                let legal = pos.legal_moves();
                let best = match searchmoves {
                    Some(mv) => mv.to_string(),
                    None => legal
                        .first()
                        .map(|m| xq_core::notation::iccs(*m))
                        .unwrap_or_else(|| "h2e2".to_string()),
                };
                if script == "garbage" {
                    println!("hello world");
                    println!();
                    println!("nonsense line here");
                }
                let n_lines = if searchmoves.is_some() { 1 } else { multipv };
                for slot in 1..=n_lines {
                    let (score, pv_mv) = match slot {
                        1 => (35, best.clone()),
                        2 => (
                            10,
                            legal
                                .get(1)
                                .map(|m| xq_core::notation::iccs(*m))
                                .unwrap_or_else(|| best.clone()),
                        ),
                        _ => (
                            -5,
                            legal
                                .get(2)
                                .map(|m| xq_core::notation::iccs(*m))
                                .unwrap_or_else(|| best.clone()),
                        ),
                    };
                    println!(
                        "info depth 10 seldepth 12 multipv {slot} score cp {score} nodes 1000 nps 500000 time 10 pv {pv_mv}"
                    );
                }
                if script == "illegal_move" {
                    println!("bestmove a0a9");
                } else {
                    println!("bestmove {best}");
                }
                let _ = out.flush();
            }
            "stop" => {}
            "quit" => {
                log_line("EXITED");
                break;
            }
            _ => {}
        }
    }
}
