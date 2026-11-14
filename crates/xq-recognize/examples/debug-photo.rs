//! Debug helper: run both recognizers (physical + screenshot) on
//! image files and print every detection + the grid mapping.
//! Not shipped in the app.
//!
//!   cargo run -p xq-recognize --example debug-photo -- <image...>
//!   cargo run -p xq-recognize --example debug-photo -- --cal x1,y1 x2,y2 x3,y3 x4,y4 px,py <image...>
//!
//! `--cal` passes a board calibration (4 corners + centre pawn) so
//! the grid is mapped exactly instead of estimated.

use xq_recognize::board::{
    detections_to_grid, grid_to_position, physical_class_to_piece, BoardCalibration,
};
use xq_recognize::screenshot::ScreenshotRecognizer;
use xq_recognize::{find_models_dir, lib_path_for, ModelPaths, Recognizer};

fn parse_point(s: &str) -> (f32, f32) {
    let (x, y) = s.split_once(',').expect("point is x,y");
    (
        x.parse().expect("x is a number"),
        y.parse().expect("y is a number"),
    )
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Optional `--cal` + 5 points (4 corners, then centre pawn).
    let mut cal: Option<BoardCalibration> = None;
    if args.first().is_some_and(|a| a == "--cal") {
        args.remove(0);
        let pts: Vec<(f32, f32)> = args.drain(..5).map(|s| parse_point(&s)).collect();
        cal = Some(BoardCalibration {
            corners: [pts[0], pts[1], pts[2], pts[3]],
            pawn: pts[4],
        });
    }
    if args.is_empty() {
        eprintln!("usage: debug-photo [--cal x1,y1 x2,y2 x3,y3 x4,y4 px,py] <image...>");
        std::process::exit(2);
    }
    let dir = find_models_dir().expect("recognize/ dir with models");
    let paths = ModelPaths::in_dir(&dir);
    let lib = lib_path_for(&dir);
    let lib = lib.exists().then_some(lib.as_path());
    let mut phys = Recognizer::load(&paths, lib).expect("load physical recognizer");
    let mut shot = ScreenshotRecognizer::load(&paths, lib).expect("load screenshot recognizer");
    for path in args {
        let bytes = std::fs::read(&path).expect("read image");
        println!("== {path} ({} bytes)", bytes.len());
        // Physical pipeline.
        match phys.detect(&bytes) {
            Ok((dets, max_conf)) => {
                println!(
                    "   PHYSICAL: {} detections, max conf {:.2}",
                    dets.len(),
                    max_conf
                );
                print_grid(&dets, cal.as_ref());
            }
            Err(e) => println!("   PHYSICAL ERROR: {e}"),
        }
        // Screenshot pipeline.
        match shot.detect(&bytes) {
            Ok((dets, max_conf)) => {
                println!(
                    "   SCREENSHOT: {} detections, max conf {:.2}",
                    dets.len(),
                    max_conf
                );
                print_grid(&dets, cal.as_ref());
            }
            Err(e) => println!("   SCREENSHOT ERROR: {e}"),
        }
        println!();
    }
}

fn print_grid(dets: &[xq_recognize::Detection], cal: Option<&BoardCalibration>) {
    for d in dets {
        let piece = physical_class_to_piece(d.class);
        println!(
            "     class {:2} {:?} conf {:.2} at ({:.0},{:.0})",
            d.class, piece, d.confidence, d.cx, d.cy
        );
    }
    let grid = detections_to_grid(dets, cal);
    let mut n = 0;
    for (r, row) in grid.iter().enumerate() {
        for (c, p) in row.iter().enumerate() {
            if let Some(p) = p {
                n += 1;
                println!("     grid row {r} col {c}: {}", p.fen_char());
            }
        }
    }
    println!("     pieces on grid: {n}");
    match grid_to_position(&grid) {
        Ok(pos) => println!("     GRID FEN: {}", pos.to_fen()),
        Err(e) => println!("     GRID ERROR: {e}"),
    }
}
