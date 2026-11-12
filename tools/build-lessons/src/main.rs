//! `build-lessons`: build per-ECCO opening trees from VietCoTuong games.
//!
//! Usage:
//!   build-lessons [--games <games.jsonl>] [--out <ecco-trees.json>]
//!                 [--plies N] [--min-games N]
//!
//! Reads the VietCoTuong game dump (`games.jsonl`, one JSON object per
//! line: ICCS moves, `initial_fen` in the site's H/E convention,
//! `opening_ecco` + `opening_name`), replays every game through
//! `xq-core` (validating that every move is legal), and builds one move
//! tree per ECCO code. For each code it extracts the main line — the
//! most-played move at every node — plus the top moves at each node
//! with their game counts. The output (`ecco-trees.json`) is the
//! foundation for the data-driven `lessons-src.json`: the main line
//! becomes the lesson prefix (real games, not a hand-written guess),
//! and the top moves are what gen-book's Pikafish branching and the
//! ChessDB cross-check compare against.
//!
//! The FEN convention differs: VietCoTuong writes `H` for Horse and
//! `E` for Elephant, the project uses `N`/`B` (Fairy-Stockfish). The
//! board part is converted before parsing; the other FEN fields never
//! contain those letters, so a whole-string replace is safe.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use xq_core::board::Position;
use xq_core::notation::{parse_iccs, render_cn};

/// One game from `games.jsonl`. Unknown fields are ignored; missing
/// optional metadata defaults to empty.
#[derive(Deserialize)]
struct Game {
    uuid: String,
    #[serde(default)]
    result: String,
    #[serde(default)]
    opening_ecco: String,
    #[serde(default)]
    opening_name: String,
    #[serde(default)]
    initial_fen: String,
    #[serde(default)]
    moves: Vec<String>,
}

/// One node of a per-ECCO move tree. `count` is the number of games
/// that reached this node (i.e. played this exact prefix); `children`
/// maps the next ICCS move to its node.
#[derive(Default, Serialize)]
struct TreeNode {
    count: usize,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    children: HashMap<String, TreeNode>,
}

/// One step of a main line.
#[derive(Serialize)]
struct MainLineStep {
    /// ICCS coordinates, e.g. `h2e2`.
    iccs: String,
    /// Chinese notation, e.g. `炮二平五`.
    cn: String,
    /// Games that played this move at this node.
    count: usize,
    /// Games that reached this node.
    total: usize,
}

/// One ECCO code's tree summary.
#[derive(Serialize)]
struct EccoTree {
    code: String,
    /// Most common `opening_name` in the group.
    name: String,
    games: usize,
    /// Most-played move at every node, up to `--plies`.
    main_line: Vec<MainLineStep>,
    /// Top moves (ICCS, count) at each main-line node, most-played
    /// first, up to 4. The main-line move is always first.
    top_moves: Vec<Vec<(String, usize)>>,
}

#[derive(Serialize)]
struct Output {
    source: String,
    plies: usize,
    games_read: usize,
    games_ok: usize,
    games_failed: usize,
    /// Games skipped because they start mid-position (book excerpts).
    nonstandard_start: usize,
    /// Failure reason -> count (e.g. `bad_fen`, `illegal_move`).
    failed_reasons: HashMap<String, usize>,
    ecco: Vec<EccoTree>,
}

struct Args {
    games: PathBuf,
    out: PathBuf,
    plies: usize,
    min_games: usize,
    /// Also evaluate the main-line nodes with ChessDB (queryall) and
    /// write the results here. Unset = skip the (slow, online) step.
    chessdb_out: Option<PathBuf>,
    /// Max ChessDB positions to query (0 = all). For testing.
    chessdb_limit: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        games: PathBuf::from("data/vietcotuong/games.jsonl"),
        out: PathBuf::from("data/vietcotuong/ecco-trees.json"),
        plies: 12,
        min_games: 0,
        chessdb_out: None,
        chessdb_limit: 0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--games" => {
                args.games = PathBuf::from(it.next().ok_or("--games cần một đường dẫn")?);
            }
            "--out" => {
                args.out = PathBuf::from(it.next().ok_or("--out cần một đường dẫn")?);
            }
            "--plies" => {
                args.plies = it
                    .next()
                    .ok_or("--plies cần một số")?
                    .parse()
                    .map_err(|_| "--plies phải là số")?;
            }
            "--min-games" => {
                args.min_games = it
                    .next()
                    .ok_or("--min-games cần một số")?
                    .parse()
                    .map_err(|_| "--min-games phải là số")?;
            }
            "--chessdb-out" => {
                args.chessdb_out = Some(PathBuf::from(
                    it.next().ok_or("--chessdb-out cần một đường dẫn")?,
                ));
            }
            "--chessdb-limit" => {
                args.chessdb_limit = it
                    .next()
                    .ok_or("--chessdb-limit cần một số")?
                    .parse()
                    .map_err(|_| "--chessdb-limit phải là số")?;
            }
            other => return Err(format!("build-lessons: không rõ đối số {other}")),
        }
    }
    Ok(args)
}

/// One move from ChessDB `queryall`: `move:b2e2,score:1,rank:2,
/// note:! (45-02),winrate:1.87,drawrate:96.40,lossrate:1.73`.
/// The ECCO code hides in the note's parentheses, e.g. `45-02`.
#[derive(Clone, Serialize, Deserialize)]
struct ChessDbMove {
    mv: String,
    score: i32,
    rank: i32,
    winrate: f32,
    drawrate: f32,
    lossrate: f32,
    note: String,
    ecco: Option<String>,
}

/// ChessDB's evaluation of one main-line node.
#[derive(Clone, Serialize, Deserialize)]
struct ChessDbEval {
    ecco: String,
    /// Plies from the start (0 = startpos).
    ply: usize,
    fen: String,
    /// The real-game main-line move from this node (ICCS), if any.
    game_move: Option<String>,
    /// ChessDB's top moves at this node (up to 4, best first).
    top: Vec<ChessDbMove>,
    /// ChessDB's score of the position (`queryscore`), if obtained.
    score: Option<i32>,
}

fn parse_chessdb_move(entry: &str) -> Option<ChessDbMove> {
    let mut mv = None;
    let mut score = 0i32;
    let mut rank = 0i32;
    let mut winrate = 0f32;
    let mut drawrate = 0f32;
    let mut lossrate = 0f32;
    let mut note = String::new();
    for field in entry.split(',') {
        let (k, v) = field.split_once(':')?;
        match k {
            "move" => mv = Some(v.to_string()),
            "score" => score = v.parse().ok()?,
            "rank" => rank = v.parse().ok()?,
            "winrate" => winrate = v.parse().ok()?,
            "drawrate" => drawrate = v.parse().ok()?,
            "lossrate" => lossrate = v.parse().ok()?,
            "note" => note = v.to_string(),
            _ => {}
        }
    }
    let mv = mv?;
    // ECCO code inside the note's parentheses: "! (45-02)" -> "45-02".
    let ecco = note
        .split('(')
        .nth(1)
        .and_then(|s| s.split(')').next())
        .map(str::to_string);
    Some(ChessDbMove {
        mv,
        score,
        rank,
        winrate,
        drawrate,
        lossrate,
        note,
        ecco,
    })
}

/// Query ChessDB `queryall` for one FEN via curl (no new dependency;
/// curl ships on Linux/macOS/Windows 10+). Returns the top 4 moves.
/// ChessDB is a free service with no key: be polite (the caller
/// sleeps between queries) and stop on any error.
fn query_chessdb(fen: &str) -> Result<Vec<ChessDbMove>, String> {
    let out = std::process::Command::new("curl")
        .args([
            "-s",
            "-G",
            "--data-urlencode",
            &format!("board={fen}"),
            "http://www.chessdb.cn/chessdb.php?action=queryall",
        ])
        .output()
        .map_err(|e| format!("chạy curl lỗi: {e}"))?;
    if !out.status.success() {
        return Err(format!("curl trả về {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // ChessDB appends a NUL byte; strip it so the last field parses.
    let text = text.replace('\0', "");
    let text = text.trim();
    if text.is_empty() || text == "unknown" || text == "invalid board" {
        return Ok(Vec::new());
    }
    let mut moves: Vec<ChessDbMove> = text.split('|').filter_map(parse_chessdb_move).collect();
    // ChessDB lists best first; keep the top 4.
    moves.truncate(4);
    Ok(moves)
}

fn query_chessdb_score(fen: &str) -> Option<i32> {
    let out = std::process::Command::new("curl")
        .args([
            "-s",
            "-G",
            "--data-urlencode",
            &format!("board={fen}"),
            "http://www.chessdb.cn/chessdb.php?action=queryscore",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // ChessDB appends a NUL byte to the response; strip it (Rust's
    // trim does not treat NUL as whitespace, so "1\0" would not parse).
    let text = text.replace('\0', "");
    text.trim()
        .strip_prefix("eval:")
        .and_then(|s| s.parse().ok())
}

/// VietCoTuong FEN uses H/E for Horse/Elephant; the project uses N/B.
fn convert_fen(fen: &str) -> String {
    fen.chars()
        .map(|c| match c {
            'H' => 'N',
            'E' => 'B',
            'h' => 'n',
            'e' => 'b',
            other => other,
        })
        .collect()
}

/// The standard starting board (FEN board part, project N/B
/// convention). Only games starting here go into the opening tree:
/// the other ~2% start mid-position (book excerpts/traps) and their
/// move sequences are not comparable to full games from move 1.
const START_BOARD: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR";

fn run(args: &Args) -> Result<(), String> {
    let text = fs::read_to_string(&args.games)
        .map_err(|e| format!("không đọc được {}: {e}", args.games.display()))?;
    // code -> root node; code -> most common opening name + its count.
    let mut trees: HashMap<String, TreeNode> = HashMap::new();
    let mut names: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut games_read = 0usize;
    let mut games_ok = 0usize;
    let mut games_failed = 0usize;
    let mut nonstandard_start = 0usize;
    let mut failed_reasons: HashMap<String, usize> = HashMap::new();
    // Replay each game once, building the trees. The tree only keeps
    // the opening phase (up to --plies plies); deeper moves are
    // middlegame and do not belong in the opening tree.
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        games_read += 1;
        let game: Game = match serde_json::from_str(line) {
            Ok(g) => g,
            Err(_) => {
                games_failed += 1;
                *failed_reasons.entry("bad_json".into()).or_insert(0) += 1;
                continue;
            }
        };
        let code = if game.opening_ecco.is_empty() {
            "unknown".to_string()
        } else {
            game.opening_ecco.clone()
        };
        let fen = convert_fen(&game.initial_fen);
        // Skip games that do not start from the standard position:
        // their move sequences are not comparable to full games.
        // (Board part only — move counters may differ.)
        if fen.split(' ').next() != Some(START_BOARD) {
            nonstandard_start += 1;
            continue;
        }
        let mut pos = match Position::from_fen(&fen) {
            Ok(p) => p,
            Err(_) => {
                games_failed += 1;
                *failed_reasons.entry("bad_fen".into()).or_insert(0) += 1;
                continue;
            }
        };
        // Replay, collecting the CN form of each move for the tree
        // walk. A game fails if any move is illegal or unparseable.
        let mut played: Vec<String> = Vec::new();
        let mut bad: Option<&'static str> = None;
        for s in game.moves.iter().take(args.plies) {
            let mv = match parse_iccs(s) {
                Ok(m) => m,
                Err(_) => {
                    bad = Some("bad_iccs");
                    break;
                }
            };
            if !pos.legal_moves().contains(&mv) {
                bad = Some("illegal_move");
                break;
            }
            pos.make_move(mv);
            played.push(s.clone());
        }
        if let Some(reason) = bad {
            games_failed += 1;
            *failed_reasons.entry(reason.into()).or_insert(0) += 1;
            let _ = lineno;
            continue;
        }
        games_ok += 1;
        *names
            .entry(code.clone())
            .or_default()
            .entry(game.opening_name.clone())
            .or_insert(0) += 1;
        let root = trees.entry(code).or_default();
        root.count += 1;
        let mut node = root;
        for s in &played {
            node = node.children.entry(s.clone()).or_default();
            node.count += 1;
        }
        let _ = game.uuid;
        let _ = game.result;
    }
    // Extract the main line per code: most-played child at each node.
    // The CN form is re-derived by replaying the main line from the
    // start position (all games in a code share the standard start;
    // a custom initial FEN is re-checked here and the code is skipped
    // if its main line cannot be replayed from startpos).
    let mut ecco_out: Vec<EccoTree> = Vec::new();
    for (code, root) in &trees {
        if root.count < args.min_games {
            continue;
        }
        let name = names
            .get(code)
            .and_then(|m| m.iter().max_by_key(|(_, n)| *n).map(|(s, _)| s.clone()))
            .unwrap_or_default();
        // Replay the main line from the standard start to render CN.
        // (VietCoTuong games start from the standard position; if a
        // code's main line ever failed here it would mean the tree was
        // built from a non-standard start, which the replay above
        // would already have caught per game.)
        let mut pos = Position::startpos();
        let mut main_line: Vec<MainLineStep> = Vec::new();
        let mut top_moves: Vec<Vec<(String, usize)>> = Vec::new();
        let mut node = root;
        let mut ok = true;
        for _ in 0..args.plies {
            if node.children.is_empty() {
                break;
            }
            let mut ranked: Vec<(&String, &TreeNode)> = node.children.iter().collect();
            ranked.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
            let total = node.count;
            top_moves.push(
                ranked
                    .iter()
                    .take(4)
                    .map(|(s, n)| ((*s).clone(), n.count))
                    .collect(),
            );
            let (best_iccs, best_node) = ranked[0];
            let mv = match parse_iccs(best_iccs) {
                Ok(m) => m,
                Err(_) => {
                    ok = false;
                    break;
                }
            };
            if !pos.legal_moves().contains(&mv) {
                ok = false;
                break;
            }
            let cn = render_cn(&pos, mv);
            pos.make_move(mv);
            main_line.push(MainLineStep {
                iccs: best_iccs.clone(),
                cn,
                count: best_node.count,
                total,
            });
            node = best_node;
        }
        if !ok || main_line.is_empty() {
            continue;
        }
        ecco_out.push(EccoTree {
            code: code.clone(),
            name,
            games: root.count,
            main_line,
            top_moves,
        });
    }
    ecco_out.sort_by(|a, b| b.games.cmp(&a.games).then(a.code.cmp(&b.code)));
    let out = Output {
        source: "VietCoTuong Explore (games.jsonl)".to_string(),
        plies: args.plies,
        games_read,
        games_ok,
        games_failed,
        nonstandard_start,
        failed_reasons,
        ecco: ecco_out,
    };
    let json = serde_json::to_string_pretty(&out).map_err(|e| format!("ghi JSON lỗi: {e}"))?;
    fs::write(&args.out, json)
        .map_err(|e| format!("không ghi được {}: {e}", args.out.display()))?;
    println!(
        "build-lessons: đọc {} ván, ok {} ván, lỗi {} ván, {} ván bắt đầu giữa chừng (bỏ qua); {} mã ECCO -> {}",
        games_read, games_ok, games_failed, nonstandard_start, out.ecco.len(), args.out.display()
    );
    if let Some(chessdb_path) = &args.chessdb_out {
        eval_chessdb(&out.ecco, chessdb_path, args.chessdb_limit)?;
    }
    Ok(())
}

/// Evaluate every main-line node with ChessDB: replay each code's main
/// line from the start, query `queryall` (+ `queryscore`) at each
/// node, and write the results. Polite: 400 ms between queries; a
/// failed query leaves the node with an empty `top` instead of
/// aborting the whole run.
/// Evaluate every main-line node with ChessDB: replay each code's main
/// line from the start, query `queryall` (+ `queryscore`) at each
/// node, and write the results. Polite: 400 ms between queries; a
/// failed query leaves the node with an empty `top` instead of
/// aborting the whole run. Resumable: positions already in the output
/// file (matched by FEN) are reused, not re-queried; the file is
/// rewritten after every position so an interrupted run loses at most
/// the position in flight.
fn eval_chessdb(ecco: &[EccoTree], path: &PathBuf, limit: usize) -> Result<(), String> {
    // Load already-evaluated positions (FEN -> eval) for resume.
    let mut cache: HashMap<String, ChessDbEval> = HashMap::new();
    if let Ok(text) = fs::read_to_string(path) {
        if let Ok(prev) = serde_json::from_str::<Vec<ChessDbEval>>(&text) {
            for e in prev {
                cache.insert(e.fen.clone(), e);
            }
        }
    }
    let save = |evals: &[ChessDbEval]| -> Result<(), String> {
        let json = serde_json::to_string_pretty(evals).map_err(|e| format!("ghi JSON lỗi: {e}"))?;
        fs::write(path, json).map_err(|e| format!("không ghi được {}: {e}", path.display()))
    };
    let mut evals: Vec<ChessDbEval> = Vec::new();
    let mut queried = 0usize;
    let mut reused = 0usize;
    'outer: for tree in ecco {
        let mut pos = Position::startpos();
        // Ply 0 = the start position (no game move yet).
        for ply in 0..=tree.main_line.len() {
            if limit > 0 && queried + reused >= limit {
                break 'outer;
            }
            let fen = pos.to_fen();
            let game_move = tree.main_line.get(ply).map(|s| s.iccs.clone());
            if let Some(cached) = cache.get(&fen) {
                // Same FEN, same evaluation — reuse it (but keep this
                // code/ply's identity).
                let mut e = cached.clone();
                e.ecco = tree.code.clone();
                e.ply = ply;
                e.game_move = game_move;
                evals.push(e);
                reused += 1;
            } else {
                let top = match query_chessdb(&fen) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("chessdb: {} ply {}: {e} (bỏ qua nút này)", tree.code, ply);
                        Vec::new()
                    }
                };
                let score = query_chessdb_score(&fen);
                evals.push(ChessDbEval {
                    ecco: tree.code.clone(),
                    ply,
                    fen,
                    game_move,
                    top,
                    score,
                });
                queried += 1;
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
            save(&evals)?;
            // Advance to the next node with the real-game main-line move.
            if let Some(step) = tree.main_line.get(ply) {
                let mv = parse_iccs(&step.iccs)
                    .map_err(|_| format!("{}: nước {} không parse được", tree.code, step.iccs))?;
                if !pos.legal_moves().contains(&mv) {
                    break;
                }
                pos.make_move(mv);
            }
        }
    }
    println!(
        "build-lessons: đánh giá ChessDB xong (mới {queried}, dùng lại {reused}) -> {}",
        path.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match parse_args().and_then(|a| run(&a)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("build-lessons: {e}");
            ExitCode::FAILURE
        }
    }
}
