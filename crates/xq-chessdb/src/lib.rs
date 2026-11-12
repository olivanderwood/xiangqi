//! ChessDB cloud database client — a **separate** live scoring system.
//!
//! The app already scores positions with its own engines (Pikafish / the
//! builtin engine) and with the opening book. ChessDB is an independent
//! second opinion: after every move the app asks ChessDB for the new
//! position's evaluation and shows it in its own UI section, in both
//! Play-vs-AI and Learn mode (Hieu, 2026-11-11).
//!
//! Design notes:
//! - One `queryall` call is enough: it returns the top moves with their
//!   scores and win/draw/loss rates, and the best move's score equals the
//!   `queryscore` position score (verified on all 503 scored positions
//!   of `data/vietcotuong/chessdb-eval.json` — 0 mismatches). So the
//!   client never calls `queryscore`.
//! - ChessDB scores and win/draw/loss rates are from **Red's**
//!   perspective (verified on the same eval file: positions with Black
//!   to move are overwhelmingly negative, i.e. good for Black). This
//!   matches the app's advantage display, so the score can be shown
//!   as-is with the same formatting.
//! - ChessDB appends a NUL byte to its responses; it must be stripped
//!   before parsing (Rust's `trim` does not treat NUL as whitespace).
//! - The service is free with no key. Calls are one per position change
//!   (a move happens at most every few seconds), and the app caches
//!   results per FEN in the worker, so this stays polite. A position
//!   ChessDB does not know returns an empty evaluation, not an error.
//! - The app is otherwise offline; this is the only part that needs a
//!   network. Every failure (no network, timeout, bad status, bad
//!   body) surfaces as [`ChessDbError`] and the UI shows "unavailable"
//!   instead of crashing.

use std::time::Duration;

/// ChessDB API endpoint (plain HTTP; the service has no HTTPS).
pub const CHESSDB_URL: &str = "http://www.chessdb.cn/chessdb.php";

/// How long a ChessDB request may take before it counts as failed.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// One move from ChessDB `queryall`:
/// `move:b2e2,score:1,rank:2,note:! (45-02),winrate:1.87,drawrate:96.40,lossrate:1.73`.
/// Scores and rates are from Red's perspective.
#[derive(Clone, Debug, PartialEq)]
pub struct ChessDbMove {
    /// The move in ICCS (e.g. `h2e2`).
    pub mv: String,
    /// Centipawn score from Red's perspective.
    pub score: i32,
    /// Rank among the top moves (0 = best).
    pub rank: i32,
    /// Red's win rate in percent.
    pub winrate: f32,
    /// Draw rate in percent.
    pub drawrate: f32,
    /// Red's loss rate in percent.
    pub lossrate: f32,
    /// ChessDB's note, e.g. `! (45-02)` (the `!` marks a book move).
    pub note: String,
    /// ECCO code hidden in the note's parentheses, e.g. `45-02`.
    /// This is ChessDB's numeric code system, different from the
    /// letter+number codes (c04, d29) VietCoTuong uses.
    pub ecco: Option<String>,
}

/// ChessDB's evaluation of one position.
#[derive(Clone, Debug, PartialEq)]
pub struct ChessDbEval {
    /// The FEN that was evaluated.
    pub fen: String,
    /// Position score in centipawns from Red's perspective (the best
    /// move's score). `None` when ChessDB has no data for the position.
    pub score: Option<i32>,
    /// Top moves at this position (up to 4, best first).
    pub top: Vec<ChessDbMove>,
}

/// What can go wrong when asking ChessDB. "No data for this position"
/// is NOT an error: it is an evaluation with an empty `top` list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChessDbError {
    /// No network / DNS / connection failure, or a timeout.
    Network(String),
    /// The server answered with a non-2xx status.
    HttpStatus(u16),
    /// The body could not be understood.
    Parse(String),
}

impl std::fmt::Display for ChessDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChessDbError::Network(msg) => write!(f, "không kết nối được ChessDB: {msg}"),
            ChessDbError::HttpStatus(code) => write!(f, "ChessDB trả về HTTP {code}"),
            ChessDbError::Parse(msg) => write!(f, "không đọc được trả lời ChessDB: {msg}"),
        }
    }
}

impl std::error::Error for ChessDbError {}

/// A ChessDB client. Cheap to clone (the agent is internally shared);
/// create one per worker thread.
#[derive(Clone, Debug)]
pub struct ChessDbClient {
    agent: ureq::Agent,
}

impl Default for ChessDbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ChessDbClient {
    /// Build a client with the standard timeout.
    ///
    /// The proxy (if any) comes from the `http_proxy` / `HTTP_PROXY`
    /// environment variables, the same ones curl uses: on a machine
    /// with direct internet there is no proxy and the client connects
    /// directly; behind a corporate egress proxy the client goes
    /// through it. This matters because direct connections are blocked
    /// in some environments (the dev VM can only reach ChessDB via its
    /// egress proxy). No proxy is ever hardcoded.
    #[must_use]
    pub fn new() -> Self {
        let mut builder = ureq::AgentBuilder::new().timeout(TIMEOUT);
        if let Some(proxy) = proxy_from_env() {
            builder = builder.proxy(proxy);
        }
        Self {
            agent: builder.build(),
        }
    }

    /// Ask ChessDB `queryall` for one FEN and return its evaluation.
    /// Returns an evaluation with an empty `top` list when ChessDB has
    /// no data for the position; returns `Err` only on network, HTTP
    /// or parse failures.
    pub fn query_all(&self, fen: &str) -> Result<ChessDbEval, ChessDbError> {
        let resp = self
            .agent
            .get(CHESSDB_URL)
            .query("action", "queryall")
            .query("board", fen)
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => ChessDbError::HttpStatus(code),
                ureq::Error::Transport(t) => ChessDbError::Network(t.to_string()),
            })?;
        let text = resp
            .into_string()
            .map_err(|e| ChessDbError::Parse(e.to_string()))?;
        let top = parse_queryall(&text)?;
        Ok(ChessDbEval {
            fen: fen.to_string(),
            score: top.first().map(|m| m.score),
            top,
        })
    }
}

/// Read the HTTP proxy from the environment (`http_proxy`, falling
/// back to `HTTP_PROXY`), the same variables curl honours. Returns
/// `None` when no proxy is configured (direct connection).
fn proxy_from_env() -> Option<ureq::Proxy> {
    let url = std::env::var("http_proxy")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .ok()?;
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    ureq::Proxy::new(url).ok()
}

/// Parse a `queryall` response body into the top moves. Public so the
/// app and the tests can exercise the parser without a network.
///
/// The body is `|`-separated move entries; each entry is
/// comma-separated `key:value` fields. ChessDB appends a NUL byte,
/// which is stripped first. `unknown`, `invalid board` and an empty
/// body all mean "no data" and parse to an empty list.
pub fn parse_queryall(body: &str) -> Result<Vec<ChessDbMove>, ChessDbError> {
    let text = body.replace('\0', "");
    let text = text.trim();
    if text.is_empty() || text == "unknown" || text == "invalid board" {
        return Ok(Vec::new());
    }
    let mut moves: Vec<ChessDbMove> = text.split('|').filter_map(parse_chessdb_move).collect();
    if moves.is_empty() {
        return Err(ChessDbError::Parse(format!(
            "không có nước nào đọc được từ: {}",
            text.chars().take(80).collect::<String>()
        )));
    }
    // ChessDB lists best first; keep the top 4.
    moves.truncate(4);
    Ok(moves)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `queryall` response at the start position (captured from
    /// the live service; the trailing NUL is part of the real format).
    const STARTPOS: &str = "move:b2e2,score:1,rank:2,note:! (45-02),winrate:1.87,drawrate:96.40,lossrate:1.73|move:h2e2,score:1,rank:2,note:! (45-02),winrate:1.87,drawrate:96.40,lossrate:1.73|move:c3c4,score:1,rank:2,note:! (44-02),winrate:1.87,drawrate:96.40,lossrate:1.73|move:g3g4,score:1,rank:2,note:! (44-02),winrate:1.87,drawrate:96.40,lossrate:1.73\0";

    #[test]
    fn parses_real_startpos_response() {
        let moves = parse_queryall(STARTPOS).expect("parse được");
        assert_eq!(moves.len(), 4);
        assert_eq!(moves[0].mv, "b2e2");
        assert_eq!(moves[0].score, 1);
        assert_eq!(moves[0].rank, 2);
        assert!((moves[0].winrate - 1.87).abs() < 0.01);
        assert!((moves[0].drawrate - 96.40).abs() < 0.01);
        assert!((moves[0].lossrate - 1.73).abs() < 0.01);
        assert_eq!(moves[0].note, "! (45-02)");
        assert_eq!(moves[0].ecco.as_deref(), Some("45-02"));
        // The NUL byte is stripped, so the last move still parses.
        assert_eq!(moves[3].mv, "g3g4");
        assert_eq!(moves[3].ecco.as_deref(), Some("44-02"));
    }

    #[test]
    fn parses_bad_move_with_negative_score() {
        let body = "move:h9g7,score:-1,rank:0,note:! (35-09),winrate:0.08,drawrate:70.60,lossrate:29.32|move:c6c5,score:-513,rank:10,note:?,winrate:0.00,drawrate:0.00,lossrate:100.00";
        let moves = parse_queryall(body).expect("parse được");
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].score, -1);
        assert_eq!(moves[1].mv, "c6c5");
        assert_eq!(moves[1].score, -513);
        assert!((moves[1].lossrate - 100.0).abs() < 0.01);
        assert_eq!(moves[1].ecco, None);
    }

    #[test]
    fn unknown_position_is_empty_not_error() {
        assert_eq!(parse_queryall("unknown").expect("ok"), Vec::new());
        assert_eq!(parse_queryall("invalid board").expect("ok"), Vec::new());
        assert_eq!(parse_queryall("").expect("ok"), Vec::new());
        assert_eq!(parse_queryall("  \n").expect("ok"), Vec::new());
        // NUL-only body is also empty.
        assert_eq!(parse_queryall("\0").expect("ok"), Vec::new());
    }

    #[test]
    fn garbage_body_is_parse_error() {
        assert!(matches!(
            parse_queryall("hello world"),
            Err(ChessDbError::Parse(_))
        ));
    }

    #[test]
    fn keeps_top_four_only() {
        let one = "move:a0a1,score:1,rank:0,note:!,winrate:1.0,drawrate:98.0,lossrate:1.0";
        let body = [one; 6].join("|");
        let moves = parse_queryall(&body).expect("parse được");
        assert_eq!(moves.len(), 4);
    }

    /// Live check against the real service. Ignored by default: it
    /// needs a network and the public service. Run explicitly with
    /// `cargo test -p xq-chessdb -- --ignored`.
    #[test]
    #[ignore]
    fn live_queryall_startpos() {
        let client = ChessDbClient::new();
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        let eval = client.query_all(fen).expect("live queryall");
        assert_eq!(eval.fen, fen);
        assert!(!eval.top.is_empty(), "startpos phải có nước");
        assert_eq!(eval.score, eval.top.first().map(|m| m.score));
    }
}
