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

/// A ChessDB client. Cheap to clone (it only holds the proxy config
/// read once at creation); create one per worker thread.
///
/// The client speaks plain HTTP/1.1 over `std::net::TcpStream`
/// directly — no HTTP client dependency. ChessDB is plain HTTP, and
/// the app must not crash on Windows when the network is up: the
/// previous `ureq`-based client was implicated in a 0xc0000409
/// (STATUS_STACK_BUFFER_OVERRUN) crash on entering either board
/// screen whenever the network was available (offline = no crash,
/// Hieu, 2026-11-12). A minimal hand-rolled client leaves nothing
/// outside this crate to go wrong.
#[derive(Clone, Debug)]
pub struct ChessDbClient {
    proxy: Option<ProxyConfig>,
}

/// An HTTP proxy from the environment: host/port to connect to, plus
/// an optional `Proxy-Authorization: Basic ...` value.
#[derive(Clone, Debug)]
struct ProxyConfig {
    host: String,
    port: u16,
    /// Base64 of `user:password`, when the proxy URL carries credentials.
    auth: Option<String>,
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
        Self {
            proxy: proxy_from_env(),
        }
    }

    /// Ask ChessDB `queryall` for one FEN and return its evaluation.
    /// Returns an evaluation with an empty `top` list when ChessDB has
    /// no data for the position; returns `Err` only on network, HTTP
    /// or parse failures.
    pub fn query_all(&self, fen: &str) -> Result<ChessDbEval, ChessDbError> {
        let query = format!("action=queryall&board={}", percent_encode(fen));
        let (status, body) = http_get(
            CHESSDB_HOST,
            CHESSDB_PORT,
            CHESSDB_PATH,
            &query,
            self.proxy.as_ref(),
        )?;
        if !(200..300).contains(&status) {
            return Err(ChessDbError::HttpStatus(status));
        }
        let top = parse_queryall(&body)?;
        Ok(ChessDbEval {
            fen: fen.to_string(),
            score: top.first().map(|m| m.score),
            top,
        })
    }
}

/// ChessDB host / port / path (plain HTTP; the service has no HTTPS).
const CHESSDB_HOST: &str = "www.chessdb.cn";
const CHESSDB_PORT: u16 = 80;
const CHESSDB_PATH: &str = "/chessdb.php";

/// Percent-encode a query value (RFC 3986 unreserved set kept as-is;
/// everything else becomes `%XX`, uppercase hex). The FEN contains
/// spaces and `/`, both of which must be encoded.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal base64 (standard alphabet, with `=` padding) for the
/// `Proxy-Authorization: Basic ...` header. Only used for proxy
/// credentials from the environment.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Read the HTTP proxy from the environment (`http_proxy`, falling
/// back to `HTTP_PROXY`), the same variables curl honours. Returns
/// `None` when no proxy is configured (direct connection). The URL
/// may carry credentials (`http://user:pass@host:port`); they are
/// turned into a Basic auth value, never logged or stored elsewhere.
fn proxy_from_env() -> Option<ProxyConfig> {
    let url = std::env::var("http_proxy")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .ok()?;
    parse_proxy_url(url.trim())
}

/// Parse a proxy URL (`http://[user:pass@]host[:port][/...]`) into a
/// [`ProxyConfig`]. Returns `None` for an empty or unusable URL.
fn parse_proxy_url(url: &str) -> Option<ProxyConfig> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    // Strip the scheme (`http://`); this client only speaks HTTP.
    let rest = url.strip_prefix("http://").unwrap_or(url);
    // Split off `user:pass@` when present.
    let (auth, hostport) = match rest.rsplit_once('@') {
        Some((userinfo, hostport)) => (Some(base64_encode(userinfo.as_bytes())), hostport),
        None => (None, rest),
    };
    // Split off a trailing `/...` path, if any.
    let hostport = hostport.split('/').next().unwrap_or(hostport);
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()?),
        None => (hostport.to_string(), 80),
    };
    if host.is_empty() {
        return None;
    }
    Some(ProxyConfig { host, port, auth })
}

/// One plain HTTP/1.1 GET. Returns `(status, body)`.
///
/// With a proxy, the client connects to the proxy and sends the
/// absolute URI (the standard HTTP-proxy request form); without one
/// it connects to the host directly and sends the origin form. The
/// body is read by `Content-Length`, by chunked transfer encoding,
/// or — when neither is given — until the server closes the
/// connection. Read/write timeouts are [`TIMEOUT`], so a hung server
/// cannot block the worker thread forever.
fn http_get(
    host: &str,
    port: u16,
    path: &str,
    query: &str,
    proxy: Option<&ProxyConfig>,
) -> Result<(u16, String), ChessDbError> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let (connect_host, connect_port, request_target) = match proxy {
        Some(p) => (
            p.host.as_str(),
            p.port,
            format!("http://{host}:{port}{path}?{query}"),
        ),
        None => (host, port, format!("{path}?{query}")),
    };
    let mut stream = TcpStream::connect((connect_host, connect_port))
        .map_err(|e| ChessDbError::Network(e.to_string()))?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| ChessDbError::Network(e.to_string()))?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| ChessDbError::Network(e.to_string()))?;

    let mut request = format!(
        "GET {request_target} HTTP/1.1\r\nHost: {host}:{port}\r\nUser-Agent: xiangqi-app/0.1.0\r\nAccept: */*\r\nConnection: close\r\n"
    );
    if let Some(p) = proxy {
        if let Some(auth) = &p.auth {
            request.push_str(&format!("Proxy-Authorization: Basic {auth}\r\n"));
        }
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| ChessDbError::Network(e.to_string()))?;

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| ChessDbError::Network(e.to_string()))?;
    parse_http_response(&raw)
}

/// Split a raw HTTP/1.1 response into `(status, body)`. Handles
/// `Content-Length`, `Transfer-Encoding: chunked`, and bodies that
/// simply run to the end of the connection.
fn parse_http_response(raw: &[u8]) -> Result<(u16, String), ChessDbError> {
    // Headers end at the first blank line (`\r\n\r\n`).
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| ChessDbError::Parse("trả lời HTTP không có đầu mục".to_string()))?;
    let head = String::from_utf8_lossy(&raw[..header_end]).into_owned();
    let mut lines = head.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| ChessDbError::Parse("trả lời HTTP trống".to_string()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            ChessDbError::Parse(format!("không đọc được trạng thái HTTP: {status_line}"))
        })?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let (name, value) = match line.split_once(':') {
            Some((n, v)) => (n.trim().to_ascii_lowercase(), v.trim()),
            None => continue,
        };
        if name == "content-length" {
            content_length = value.parse().ok();
        } else if name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked") {
            chunked = true;
        }
    }

    let body_bytes = &raw[header_end + 4..];
    let body: Vec<u8> = if chunked {
        decode_chunked(body_bytes)?
    } else if let Some(len) = content_length {
        body_bytes
            .get(..len.min(body_bytes.len()))
            .unwrap_or(&[])
            .to_vec()
    } else {
        body_bytes.to_vec()
    };
    let text = String::from_utf8_lossy(&body).into_owned();
    Ok((status, text))
}

/// Decode a chunked transfer-encoded body (`<hex-len>\r\n<data>\r\n`,
/// repeated, terminated by a zero-length chunk).
fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, ChessDbError> {
    let mut out = Vec::new();
    loop {
        let line_end = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| ChessDbError::Parse("chunk HTTP không hợp lệ".to_string()))?;
        let len_str = std::str::from_utf8(&data[..line_end])
            .map_err(|_| ChessDbError::Parse("độ dài chunk không phải ASCII".to_string()))?;
        // Chunk extensions (`;...`) are ignored.
        let len_str = len_str.split(';').next().unwrap_or(len_str).trim();
        let len = usize::from_str_radix(len_str, 16)
            .map_err(|_| ChessDbError::Parse(format!("độ dài chunk sai: {len_str}")))?;
        if len == 0 {
            break;
        }
        let chunk_start = line_end + 2;
        let chunk_end = chunk_start + len;
        if chunk_end > data.len() {
            return Err(ChessDbError::Parse(
                "chunk HTTP dài hơn phần thân còn lại".to_string(),
            ));
        }
        out.extend_from_slice(&data[chunk_start..chunk_end]);
        // Skip the chunk data and its trailing `\r\n`.
        data = &data[chunk_end + 2..];
    }
    Ok(out)
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

    #[test]
    fn percent_encode_fen() {
        // The startpos FEN: spaces and `/` must be encoded, `-` kept.
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        let enc = percent_encode(fen);
        assert!(!enc.contains(' '), "không còn khoảng trắng");
        assert!(!enc.contains('/'), "không còn dấu /");
        assert!(enc.contains("%20"), "khoảng trắng thành %20");
        assert!(enc.contains("%2F"), "/ thành %2F");
        assert!(enc.contains("w%20-%20-%20"), "phần sau FEN giữ nguyên dạng");
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn proxy_url_parsing() {
        let p = parse_proxy_url("http://proxy.example:8080").expect("parse được");
        assert_eq!(p.host, "proxy.example");
        assert_eq!(p.port, 8080);
        assert_eq!(p.auth, None);

        let p = parse_proxy_url("http://user:pass@hatch-egress-proxy:3128").expect("parse được");
        assert_eq!(p.host, "hatch-egress-proxy");
        assert_eq!(p.port, 3128);
        assert_eq!(p.auth.as_deref(), Some("dXNlcjpwYXNz"));

        // No port -> 80; no scheme -> still parsed; empty -> None.
        let p = parse_proxy_url("http://proxy.example").expect("parse được");
        assert_eq!(p.port, 80);
        let p = parse_proxy_url("proxy.example:3128").expect("parse được");
        assert_eq!(p.host, "proxy.example");
        assert!(parse_proxy_url("").is_none());
        assert!(parse_proxy_url("http://").is_none());
    }

    #[test]
    fn http_response_content_length() {
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhelloEXTRA";
        let (status, body) = parse_http_response(raw).expect("parse được");
        assert_eq!(status, 200);
        assert_eq!(body, "hello");
    }

    #[test]
    fn http_response_chunked() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let (status, body) = parse_http_response(raw).expect("parse được");
        assert_eq!(status, 200);
        assert_eq!(body, "hello world");
    }

    #[test]
    fn http_response_reads_to_close() {
        let raw = b"HTTP/1.1 404 Not Found\r\n\r\nunknown";
        let (status, body) = parse_http_response(raw).expect("parse được");
        assert_eq!(status, 404);
        assert_eq!(body, "unknown");
    }

    #[test]
    fn http_response_bad_status_is_parse_error() {
        let raw = b"GARBAGE\r\n\r\n";
        assert!(matches!(
            parse_http_response(raw),
            Err(ChessDbError::Parse(_))
        ));
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
