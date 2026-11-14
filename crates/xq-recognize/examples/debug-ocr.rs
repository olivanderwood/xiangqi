//! Debug helper: run OCR on every detected piece crop and print the
//! YOLO label vs the OCR vote. Validates whether OCR fixes the
//! Horse/Elephant and General/Advisor confusions.
//!
//!   cargo run -p xq-recognize --example debug-ocr -- <image...>

use xq_recognize::board::physical_class_to_piece;
use xq_recognize::ocr::OcrVoter;
use xq_recognize::screenshot::ScreenshotRecognizer;
use xq_recognize::{find_models_dir, lib_path_for, ModelPaths, Recognizer};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: debug-ocr <image...>");
        std::process::exit(2);
    }
    let dir = find_models_dir().expect("recognize/ dir with models");
    let paths = ModelPaths::in_dir(&dir);
    let lib = lib_path_for(&dir);
    let lib = lib.exists().then_some(lib.as_path());
    let mut phys = Recognizer::load(&paths, lib).expect("load physical recognizer");
    let mut shot = ScreenshotRecognizer::load(&paths, lib).expect("load screenshot recognizer");
    let mut ocr = OcrVoter::load(&paths.ocr, lib).expect("load OCR voter");
    for path in args {
        let bytes = std::fs::read(&path).expect("read image");
        let img = image::load_from_memory(&bytes)
            .expect("decode image")
            .to_rgb8();
        println!("== {path}");
        for (name, dets) in [
            ("PHYSICAL", phys.detect(&bytes).map(|(d, _)| d).ok()),
            ("SCREENSHOT", shot.detect(&bytes).map(|(d, _)| d).ok()),
        ] {
            let Some(dets) = dets else {
                println!("   {name}: error");
                continue;
            };
            println!("   {name}: {} detections", dets.len());
            for d in &dets {
                let piece = physical_class_to_piece(d.class);
                let (w, h) = img.dimensions();
                let x1 = ((d.cx - d.w / 2.0).max(0.0) as u32).min(w);
                let y1 = ((d.cy - d.h / 2.0).max(0.0) as u32).min(h);
                let x2 = ((d.cx + d.w / 2.0).max(0.0) as u32).min(w);
                let y2 = ((d.cy + d.h / 2.0).max(0.0) as u32).min(h);
                if x2 <= x1 || y2 <= y1 {
                    continue;
                }
                let crop = image::imageops::crop_imm(&img, x1, y1, x2 - x1, y2 - y1).to_image();
                let vote = ocr.recognize_crop(&crop);
                if (d.cx - 172.0).abs() < 5.0 && (d.cy - 193.0).abs() < 5.0
                    || (d.cx - 297.0).abs() < 5.0 && (d.cy - 110.0).abs() < 5.0
                {
                    let top = ocr.debug_top(&crop);
                    eprintln!("     TOP5 at ({:.0},{:.0}): {:?}", d.cx, d.cy, top);
                }
                let yolo = piece
                    .map(|p| format!("{:?}/{:?}", p.color, p.kind))
                    .unwrap_or_else(|| format!("class {}", d.class));
                match vote {
                    Some((kind, conf)) => {
                        let agree = piece.is_some_and(|p| p.kind == kind);
                        println!(
                            "     ({:.0},{:.0}) YOLO {yolo} conf {:.2} | OCR {:?} conf {:.2} {}",
                            d.cx,
                            d.cy,
                            d.confidence,
                            kind,
                            conf,
                            if agree { "agree" } else { "DIFFERS" }
                        );
                    }
                    None => {
                        println!(
                            "     ({:.0},{:.0}) YOLO {yolo} conf {:.2} | OCR none",
                            d.cx, d.cy, d.confidence
                        );
                    }
                }
            }
        }
        println!();
    }
}
