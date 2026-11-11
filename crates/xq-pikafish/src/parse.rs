//! Parsing of UCI output lines (integration spec §2.4).
//!
//! Pure functions, no I/O: one line in, one [`ParsedLine`] out. Unknown
//! keys are ignored so newer engine versions do not break the parser.

use xq_core::notation::parse_iccs;
use xq_core::types::Move;
use xq_engine::Score;

/// Bound flag of an `info` line: the score is a lower/upper bound, not an
/// exact value. Such lines are progress only, never a final result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    Lower,
    Upper,
}

/// One parsed `info` line (search progress or a final line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoLine {
    pub depth: u8,
    pub seldepth: u8,
    /// MultiPV slot, 1-based; defaults to 1 when the engine omits it.
    pub multipv: usize,
    pub score: Option<Score>,
    pub bound: Option<Bound>,
    pub nodes: u64,
    pub nps: u64,
    pub time_ms: u64,
    /// Principal variation; cut at the first unparseable move.
    pub pv: Vec<Move>,
}

impl Default for InfoLine {
    fn default() -> Self {
        Self {
            depth: 0,
            seldepth: 0,
            multipv: 1,
            score: None,
            bound: None,
            nodes: 0,
            nps: 0,
            time_ms: 0,
            pv: Vec::new(),
        }
    }
}

/// One `option name ...` line from the `uci` handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineOption {
    pub name: String,
    pub kind: String,
    pub default: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub vars: Vec<String>,
}

impl EngineOption {
    /// Does this option exist (by exact name) in a list from the handshake?
    #[must_use]
    pub fn exists<'a>(options: &'a [EngineOption], name: &str) -> Option<&'a EngineOption> {
        options.iter().find(|o| o.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedLine {
    Info(InfoLine),
    /// `info string ...`: free-form engine message, log only.
    InfoString(String),
    /// `bestmove <mv> [ponder <mv>]`; `None` for `(none)` / `0000`.
    BestMove(Option<Move>, Option<Move>),
    Option(EngineOption),
    /// `id name ...` (engine name/version) or `id author ...`.
    Id(String, String),
    UciOk,
    ReadyOk,
    /// Anything else (empty lines, garbage, unknown keywords).
    Other(String),
}

fn parse_num<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.parse::<T>().ok()
}

/// Parse one line of UCI output.
#[must_use]
pub fn parse_line(line: &str) -> ParsedLine {
    let line = line.trim();
    if line.is_empty() {
        return ParsedLine::Other(line.to_string());
    }
    let mut it = line.split_whitespace();
    let first = it.next().unwrap_or("");
    match first {
        "uciok" => ParsedLine::UciOk,
        "readyok" => ParsedLine::ReadyOk,
        "id" => {
            let key = it.next().unwrap_or("").to_string();
            let value = line.splitn(3, ' ').nth(2).unwrap_or("").trim().to_string();
            ParsedLine::Id(key, value)
        }
        "bestmove" => {
            let mv = it.next().unwrap_or("");
            let best = match mv {
                "(none)" | "0000" | "" => None,
                s => parse_iccs(s).ok(),
            };
            let ponder = if it.next() == Some("ponder") {
                it.next().and_then(|s| parse_iccs(s).ok())
            } else {
                None
            };
            ParsedLine::BestMove(best, ponder)
        }
        "option" => parse_option(line)
            .map_or_else(|| ParsedLine::Other(line.to_string()), ParsedLine::Option),
        "info" => parse_info(line),
        _ => ParsedLine::Other(line.to_string()),
    }
}

/// `option name <name> type <type> [default <v>] [min <n>] [max <n>]
/// [var <v> ...]` — the name may contain spaces.
fn parse_option(line: &str) -> Option<EngineOption> {
    // Token walk: `option name X Y type spin default ...`.
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 || tokens[0] != "option" || tokens[1] != "name" {
        return None;
    }
    // Name = tokens until `type`.
    let type_at = tokens.iter().position(|t| *t == "type")?;
    if type_at < 3 {
        return None;
    }
    let name = tokens[2..type_at].join(" ");
    let kind = tokens.get(type_at + 1).copied().unwrap_or("").to_string();
    let mut default = None;
    let mut min = None;
    let mut max = None;
    let mut vars = Vec::new();
    let mut i = type_at + 2;
    while i < tokens.len() {
        match tokens[i] {
            "default" => {
                default = tokens.get(i + 1).map(|s| (*s).to_string());
                i += 2;
            }
            "min" => {
                min = tokens.get(i + 1).and_then(|s| parse_num(s));
                i += 2;
            }
            "max" => {
                max = tokens.get(i + 1).and_then(|s| parse_num(s));
                i += 2;
            }
            "var" => {
                if let Some(v) = tokens.get(i + 1) {
                    vars.push((*v).to_string());
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(EngineOption {
        name,
        kind,
        default,
        min,
        max,
        vars,
    })
}

/// `info ...`: key/value walk; `pv` consumes the rest of the line.
fn parse_info(line: &str) -> ParsedLine {
    let rest = line.strip_prefix("info").unwrap_or("").trim();
    if let Some(msg) = rest.strip_prefix("string") {
        return ParsedLine::InfoString(msg.trim().to_string());
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut info = InfoLine::default();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                info.depth = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(0);
                i += 2;
            }
            "seldepth" => {
                info.seldepth = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(0);
                i += 2;
            }
            "multipv" => {
                info.multipv = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(1);
                i += 2;
            }
            "score" => {
                // `score cp <n> [lowerbound|upperbound]` or
                // `score mate <n> [lowerbound|upperbound]`.
                let kind = tokens.get(i + 1).copied().unwrap_or("");
                let value: Option<i32> = tokens.get(i + 2).and_then(|s| parse_num(s));
                if let Some(v) = value {
                    info.score = match kind {
                        "cp" => Some(Score::Cp(v)),
                        "mate" => Some(Score::Mate(v)),
                        _ => None,
                    };
                }
                i += 3;
                if i < tokens.len() {
                    match tokens[i] {
                        "lowerbound" => {
                            info.bound = Some(Bound::Lower);
                            i += 1;
                        }
                        "upperbound" => {
                            info.bound = Some(Bound::Upper);
                            i += 1;
                        }
                        _ => {}
                    }
                }
            }
            "nodes" => {
                info.nodes = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(0);
                i += 2;
            }
            "nps" => {
                info.nps = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(0);
                i += 2;
            }
            "time" => {
                info.time_ms = tokens.get(i + 1).and_then(|s| parse_num(s)).unwrap_or(0);
                i += 2;
            }
            "pv" => {
                for tok in &tokens[i + 1..] {
                    match parse_iccs(tok) {
                        Ok(mv) => info.pv.push(mv),
                        Err(_) => break,
                    }
                }
                break;
            }
            // Unknown keys (`hashfull`, `tbhits`, `wdl`, `currmove`,
            // `cpuload`, ...): skip the key and its value if any.
            _ => i += 1,
        }
    }
    ParsedLine::Info(info)
}
