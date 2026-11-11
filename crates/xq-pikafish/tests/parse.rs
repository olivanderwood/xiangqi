//! UCI output parsing (integration spec §9.1). Runs always, no engine
//! needed.

use xq_core::notation::parse_iccs;
use xq_core::types::Move;
use xq_engine::Score;
use xq_pikafish::parse::{parse_line, Bound, ParsedLine};

fn mv(s: &str) -> Move {
    parse_iccs(s).unwrap()
}

#[test]
fn info_cp() {
    let ParsedLine::Info(info) =
        parse_line("info depth 10 multipv 1 score cp 35 nodes 100 pv h2e2 h9g7")
    else {
        panic!("mong đợi Info");
    };
    assert_eq!(info.depth, 10);
    assert_eq!(info.multipv, 1);
    assert_eq!(info.score, Some(Score::Cp(35)));
    assert_eq!(info.nodes, 100);
    assert_eq!(info.pv, vec![mv("h2e2"), mv("h9g7")]);
}

#[test]
fn info_mate_pos_neg() {
    let ParsedLine::Info(pos) =
        parse_line("info depth 22 multipv 1 score mate 3 nodes 100 pv b0b9")
    else {
        panic!("mong đợi Info");
    };
    assert_eq!(pos.score, Some(Score::Mate(3)));
    let ParsedLine::Info(neg) =
        parse_line("info depth 22 multipv 1 score mate -2 nodes 100 pv b0b9")
    else {
        panic!("mong đợi Info");
    };
    assert_eq!(neg.score, Some(Score::Mate(-2)));
}

#[test]
fn info_bound_ignored() {
    let ParsedLine::Info(info) =
        parse_line("info depth 15 multipv 1 score cp 40 lowerbound nodes 1000 nps 5000 pv h2e2")
    else {
        panic!("mong đợi Info");
    };
    assert_eq!(info.bound, Some(Bound::Lower));
    assert_eq!(info.score, Some(Score::Cp(40)));
}

#[test]
fn info_unknown_keys() {
    let ParsedLine::Info(info) = parse_line(
        "info depth 10 multipv 1 score cp 35 hashfull 5 tbhits 0 wdl 500 300 200 nodes 100 pv h2e2",
    ) else {
        panic!("mong đợi Info");
    };
    assert_eq!(info.score, Some(Score::Cp(35)));
    assert_eq!(info.nodes, 100);
    assert_eq!(info.pv, vec![mv("h2e2")]);
}

#[test]
fn info_multipv_default() {
    let ParsedLine::Info(info) = parse_line("info depth 10 score cp 35 nodes 100 pv h2e2") else {
        panic!("mong đợi Info");
    };
    assert_eq!(info.multipv, 1);
}

#[test]
fn info_string() {
    assert!(matches!(
        parse_line("info string NNUE evaluation using pikafish.nnue enabled"),
        ParsedLine::InfoString(_)
    ));
}

#[test]
fn bestmove_variants() {
    assert_eq!(
        parse_line("bestmove h2e2"),
        ParsedLine::BestMove(Some(mv("h2e2")), None)
    );
    assert_eq!(
        parse_line("bestmove h2e2 ponder h9g7"),
        ParsedLine::BestMove(Some(mv("h2e2")), Some(mv("h9g7")))
    );
    assert_eq!(
        parse_line("bestmove (none)"),
        ParsedLine::BestMove(None, None)
    );
    assert_eq!(
        parse_line("bestmove 0000"),
        ParsedLine::BestMove(None, None)
    );
}

#[test]
fn option_lines() {
    let ParsedLine::Option(o) =
        parse_line("option name UCI_Elo type spin default 1280 min 1280 max 3133")
    else {
        panic!("mong đợi Option");
    };
    assert_eq!(o.name, "UCI_Elo");
    assert_eq!(o.kind, "spin");
    assert_eq!(o.default.as_deref(), Some("1280"));
    assert_eq!(o.min, Some(1280));
    assert_eq!(o.max, Some(3133));
}

#[test]
fn option_names_with_spaces() {
    let ParsedLine::Option(o) = parse_line(
        "option name Repetition Rule type combo default AsianRule var AsianRule var ChineseRule",
    ) else {
        panic!("mong đợi Option");
    };
    assert_eq!(o.name, "Repetition Rule");
    assert_eq!(o.kind, "combo");
    assert_eq!(o.vars, vec!["AsianRule", "ChineseRule"]);
}

#[test]
fn pv_cut_on_invalid() {
    let ParsedLine::Info(info) =
        parse_line("info depth 10 multipv 1 score cp 35 nodes 100 pv h2e2 zz99 h9g7")
    else {
        panic!("mong đợi Info");
    };
    assert_eq!(info.pv, vec![mv("h2e2")]);
}
