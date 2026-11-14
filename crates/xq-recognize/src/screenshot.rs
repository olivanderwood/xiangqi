//! Board recognition from a screenshot (a digital board), offline,
//! via ONNX.
//!
//! Follows the Xiangqi Arena web code (`OnlineGameRecognitionModel`):
//! a single-class YOLOv8 detector finds the piece bounding boxes,
//! a 64x64 classifier names the piece kind, and a colour check on
//! the crop names the side. The physical model (`Recognizer`) cannot
//! read screenshots — its training data is photos of real boards —
//! so screenshots go through this pipeline instead.
//!
//! Models (already fetched by `tools/fetch-recognize.sh`):
//! - `online_xiangqi_piece_detector.onnx`: YOLOv8, input `images`
//!   `[1, 3, 640, 640]` letterboxed, output `[1, 5, 8400]`
//!   (4 bbox + 1 confidence).
//! - `online_xiangqi_classifier.onnx`: input `images` `[1, 3, 64, 64]`,
//!   output 7 class probabilities, in the order advisor, rook,
//!   horse, pawn, king, cannon, elephant.
//!
//! The colour check scores the centre 50% of the crop: a pixel is
//! red when R clearly exceeds G and B, black when it is dark and
//! near-grey; the higher total wins.

use crate::board::{self, BoardCalibration, Grid};
use crate::preprocess::{self, INPUT_SIZE};
use crate::yolo::{self, Detection};
use crate::RecognizeError;
use std::path::Path;
use xq_core::board::Position;
use xq_core::types::{Color, Piece, PieceKind};

/// The classifier's 7 classes, in model order.
pub const CLASSIFIER_CLASSES: usize = 7;
/// A classifier result below this max probability is dropped:
/// the detector fires on round shapes that are not pieces (board
/// markings, arrows, a second box on the same piece), and the
/// classifier is unsure about them. Upstream keeps every argmax;
/// the threshold is what keeps a stray low-confidence box from
/// landing on an illegal square and voiding the whole position.
pub const CLASSIFIER_CONF_THRESHOLD: f32 = 0.7;
/// The classifier input size.
pub const CLASSIFIER_SIZE: u32 = 64;
/// How much the detector box is expanded before cropping, so the
/// character is not cut off (upstream: 15%).
const CROP_EXPAND: f32 = 0.15;

/// Map a classifier class index to a piece kind. `None` for an
/// out-of-range index. Upstream order: advisor, rook, horse, pawn,
/// king, cannon, elephant.
pub fn classifier_class_to_kind(class: usize) -> Option<PieceKind> {
    match class {
        0 => Some(PieceKind::Advisor),
        1 => Some(PieceKind::Rook),
        2 => Some(PieceKind::Horse),
        3 => Some(PieceKind::Pawn),
        4 => Some(PieceKind::King),
        5 => Some(PieceKind::Cannon),
        6 => Some(PieceKind::Elephant),
        _ => None,
    }
}

/// The colour of a piece, from the centre 50% of its 64x64 crop.
/// Red when a pixel's R clearly exceeds G and B (scored by
/// intensity); black when it is dark and near-grey. The higher
/// total wins; a tie is black (the darker side).
pub fn determine_color(rgb: &image::RgbImage) -> Color {
    let (w, h) = rgb.dimensions();
    let x0 = w / 4;
    let y0 = h / 4;
    let x1 = x0 + w / 2;
    let y1 = y0 + h / 2;
    let mut red_score = 0i32;
    let mut black_score = 0i32;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = rgb.get_pixel(x, y);
            let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
            let brightness = (r + g + b) / 3;
            if r > g + 30 && r > b + 30 {
                red_score += if r > 150 {
                    3
                } else if r > 100 {
                    2
                } else {
                    1
                };
            } else if brightness < 80 && (r - g).abs() < 20 && (g - b).abs() < 20 {
                black_score += 3;
            } else if brightness < 100 {
                black_score += 1;
            }
        }
    }
    if red_score > black_score {
        Color::Red
    } else {
        Color::Black
    }
}

/// A loaded screenshot recognizer: the piece detector + the piece
/// classifier. Created once per process and reused for every
/// screenshot; inference is synchronous, so the app runs it on a
/// worker thread, never on the UI thread.
pub struct ScreenshotRecognizer {
    detector: ort::session::Session,
    classifier: ort::session::Session,
    detector_input: String,
    detector_output: String,
    classifier_input: String,
    classifier_output: String,
    /// OCR voter (PP-OCRv6-tiny). `None` when the model file is
    /// absent — the classifier alone decides then.
    ocr: Option<crate::ocr::OcrVoter>,
}

impl ScreenshotRecognizer {
    /// Load the detector + classifier from `paths`. `lib_path` is
    /// the ONNX Runtime library; `None` loads it from the system
    /// search path.
    pub fn load(
        paths: &crate::ModelPaths,
        lib_path: Option<&Path>,
    ) -> Result<Self, RecognizeError> {
        let env = crate::environment_with(lib_path)?;
        let detector = ort::session::Session::builder(env)?.commit_from_file(&paths.detector)?;
        let classifier =
            ort::session::Session::builder(env)?.commit_from_file(&paths.classifier)?;
        let detector_input = detector
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "images".to_string());
        let detector_output = detector
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "output0".to_string());
        let classifier_input = classifier
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "images".to_string());
        let classifier_output = classifier
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "output0".to_string());
        // The OCR voter is optional: if its model file is missing
        // (e.g. an older recognize/ dir), recognition still works.
        let ocr = crate::ocr::OcrVoter::load(&paths.ocr, lib_path).ok();
        Ok(Self {
            detector,
            classifier,
            detector_input,
            detector_output,
            classifier_input,
            classifier_output,
            ocr,
        })
    }

    /// Detect pieces in `bytes` (a screenshot) and return them
    /// (original image coordinates) plus the strongest detector
    /// score seen (before the threshold), for diagnostics.
    pub fn detect(&mut self, bytes: &[u8]) -> Result<(Vec<Detection>, f32), RecognizeError> {
        let img =
            image::load_from_memory(bytes).map_err(|e| RecognizeError::BadImage(e.to_string()))?;
        let rgb = img.to_rgb8();
        // Step 1: the detector finds the piece boxes. The detector
        // outputs borrow the session, so parse them inside a block
        // and only keep the owned detections.
        let (boxes, max_confidence) = {
            let input = preprocess::preprocess(bytes)?;
            let tensor = ort::value::Tensor::from_array((
                [1usize, 3, INPUT_SIZE as usize, INPUT_SIZE as usize],
                input.data,
            ))?;
            let outputs = self
                .detector
                .run(ort::inputs![self.detector_input.as_str() => tensor])?;
            let output = outputs.get(&self.detector_output).ok_or_else(|| {
                RecognizeError::Ort(format!("missing output {}", self.detector_output))
            })?;
            let (shape, data) = output.try_extract_tensor::<f32>()?;
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            let frame = yolo::Frame {
                scale: input.scale,
                offset_x: input.offset_x,
                offset_y: input.offset_y,
                orig_w: input.orig_w as f32,
                orig_h: input.orig_h as f32,
            };
            // 1 class: the "class score" is the detector confidence.
            let result = yolo::parse_yolo_output(data, &dims, 1, &frame);
            (result.detections, result.max_confidence)
        };
        // Step 2: classify each box + determine its colour.
        // The classifier's max probability is the detection
        // confidence (upstream); a box the classifier is unsure
        // about is dropped instead of landing on a wrong square.
        let mut detections = Vec::new();
        for b in boxes {
            let Some((piece, class_conf)) = self.classify_box(&rgb, &b)? else {
                continue;
            };
            if class_conf < CLASSIFIER_CONF_THRESHOLD {
                continue;
            }
            detections.push(Detection {
                cx: b.cx,
                cy: b.cy,
                w: b.w,
                h: b.h,
                class: piece_class(piece),
                confidence: class_conf,
            });
        }
        // Cross-class NMS: the detector fires two boxes on one
        // piece (or on an arrow/highlight) and the classifier names
        // them differently, so same-class NMS lets both through.
        // The higher-confidence box wins.
        let detections = yolo::non_max_suppression_cross_class(detections);
        Ok((detections, max_confidence))
    }

    /// Classify one detector box: crop (expanded 15%), resize to
    /// 64x64, run the classifier for the kind, and check the crop
    /// colour for the side. Returns `None` when the box is not a
    /// usable piece (empty crop or unknown class); otherwise the
    /// piece + the classifier's max probability.
    fn classify_box(
        &mut self,
        rgb: &image::RgbImage,
        b: &Detection,
    ) -> Result<Option<(Piece, f32)>, RecognizeError> {
        let (w, h) = rgb.dimensions();
        let expand_w = b.w * CROP_EXPAND;
        let expand_h = b.h * CROP_EXPAND;
        let x1 = ((b.cx - b.w / 2.0 - expand_w).max(0.0) as u32).min(w);
        let y1 = ((b.cy - b.h / 2.0 - expand_h).max(0.0) as u32).min(h);
        let x2 = ((b.cx + b.w / 2.0 + expand_w).max(0.0) as u32).min(w);
        let y2 = ((b.cy + b.h / 2.0 + expand_h).max(0.0) as u32).min(h);
        if x2 <= x1 || y2 <= y1 {
            return Ok(None);
        }
        let crop = image::imageops::crop_imm(rgb, x1, y1, x2 - x1, y2 - y1).to_image();
        let resized = image::imageops::resize(
            &crop,
            CLASSIFIER_SIZE,
            CLASSIFIER_SIZE,
            image::imageops::FilterType::Triangle,
        );
        // Classifier input: CHW float32 RGB in [0, 1].
        let size = (CLASSIFIER_SIZE * CLASSIFIER_SIZE) as usize;
        let mut data = vec![0f32; 3 * size];
        for (i, px) in resized.pixels().enumerate() {
            data[i] = px[0] as f32 / 255.0;
            data[size + i] = px[1] as f32 / 255.0;
            data[2 * size + i] = px[2] as f32 / 255.0;
        }
        let tensor = ort::value::Tensor::from_array((
            [
                1usize,
                3,
                CLASSIFIER_SIZE as usize,
                CLASSIFIER_SIZE as usize,
            ],
            data,
        ))?;
        let outputs = self
            .classifier
            .run(ort::inputs![self.classifier_input.as_str() => tensor])?;
        let output = outputs.get(&self.classifier_output).ok_or_else(|| {
            RecognizeError::Ort(format!("missing output {}", self.classifier_output))
        })?;
        let (shape, probs) = output.try_extract_tensor::<f32>()?;
        let _ = shape;
        let mut best = 0usize;
        let mut best_score = 0f32;
        for (c, &s) in probs.iter().enumerate() {
            if s > best_score {
                best_score = s;
                best = c;
            }
        }
        let Some(kind) = classifier_class_to_kind(best) else {
            return Ok(None);
        };
        let color = determine_color(&resized);
        let mut piece = Piece::new(color, kind);
        // OCR vote: the 64x64 classifier confuses look-alike glyphs
        // (Horse vs Elephant, General vs Advisor). OCR reads the
        // actual character, so when it is confident (>= 0.9) and
        // disagrees, its kind wins. Colour still comes from the
        // HSV check above, never from OCR.
        if let Some(ocr) = self.ocr.as_mut() {
            if let Some((ocr_kind, ocr_conf)) = ocr.recognize_crop(&crop) {
                if ocr_conf >= crate::ocr::VOTE_THRESHOLD && ocr_kind != kind {
                    piece = Piece::new(color, ocr_kind);
                }
            }
        }
        Ok(Some((piece, best_score)))
    }

    /// Recognise a board from a screenshot: detect + classify +
    /// map to squares. Returns the position with Red to move (the
    /// screenshot does not say whose turn it is — the app asks the
    /// user after recognition). Fails when too few pieces are
    /// found or the generals are missing (see `recognize_grid`
    /// for the partial result).
    pub fn recognize(
        &mut self,
        bytes: &[u8],
        calibration: Option<&BoardCalibration>,
    ) -> Result<Position, RecognizeError> {
        let grid = self.recognize_grid(bytes, calibration)?;
        board::grid_to_position(&grid)
    }

    /// Recognise a screenshot into a 10x9 grid (row 0 = image top),
    /// even when the generals are missing — the caller fills the
    /// editor from the grid and the user fixes the rest. Returns
    /// an error only when no piece at all was detected.
    pub fn recognize_grid(
        &mut self,
        bytes: &[u8],
        calibration: Option<&BoardCalibration>,
    ) -> Result<Grid, RecognizeError> {
        let (detections, max_confidence) = self.detect(bytes)?;
        if detections.is_empty() {
            return Err(RecognizeError::NoPosition(format!(
                "no pieces detected (strongest score {:.2}, threshold {:.2})",
                max_confidence,
                yolo::CONF_THRESHOLD
            )));
        }
        Ok(board::detections_to_grid(&detections, calibration))
    }
}

/// The class index a `Piece` would have in the physical model's
/// 14 classes, so a screenshot detection can reuse the shared
/// `Detection` (which carries a class index). Kept in sync with
/// `board::physical_class_to_piece` — note Black has rook (11)
/// before cannon (12), unlike Red (cannon 4 before rook 5).
fn piece_class(piece: Piece) -> usize {
    match (piece.color, piece.kind) {
        (Color::Red, PieceKind::King) => 0,
        (Color::Red, PieceKind::Advisor) => 1,
        (Color::Red, PieceKind::Elephant) => 2,
        (Color::Red, PieceKind::Horse) => 3,
        (Color::Red, PieceKind::Cannon) => 4,
        (Color::Red, PieceKind::Rook) => 5,
        (Color::Red, PieceKind::Pawn) => 6,
        (Color::Black, PieceKind::King) => 7,
        (Color::Black, PieceKind::Advisor) => 8,
        (Color::Black, PieceKind::Elephant) => 9,
        (Color::Black, PieceKind::Horse) => 10,
        (Color::Black, PieceKind::Rook) => 11,
        (Color::Black, PieceKind::Cannon) => 12,
        (Color::Black, PieceKind::Pawn) => 13,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_classes_map_to_kinds() {
        assert_eq!(classifier_class_to_kind(0), Some(PieceKind::Advisor));
        assert_eq!(classifier_class_to_kind(1), Some(PieceKind::Rook));
        assert_eq!(classifier_class_to_kind(2), Some(PieceKind::Horse));
        assert_eq!(classifier_class_to_kind(3), Some(PieceKind::Pawn));
        assert_eq!(classifier_class_to_kind(4), Some(PieceKind::King));
        assert_eq!(classifier_class_to_kind(5), Some(PieceKind::Cannon));
        assert_eq!(classifier_class_to_kind(6), Some(PieceKind::Elephant));
        assert_eq!(classifier_class_to_kind(7), None);
    }

    #[test]
    fn color_red_beats_black_on_a_red_crop() {
        // A 64x64 crop that is red in the centre.
        let mut img = image::RgbImage::from_pixel(64, 64, image::Rgb([240, 230, 220]));
        for y in 16..48 {
            for x in 16..48 {
                img.put_pixel(x, y, image::Rgb([200, 40, 40]));
            }
        }
        assert_eq!(determine_color(&img), Color::Red);
    }

    #[test]
    fn color_black_beats_red_on_a_dark_crop() {
        let mut img = image::RgbImage::from_pixel(64, 64, image::Rgb([240, 230, 220]));
        for y in 16..48 {
            for x in 16..48 {
                img.put_pixel(x, y, image::Rgb([30, 30, 30]));
            }
        }
        assert_eq!(determine_color(&img), Color::Black);
    }

    #[test]
    fn piece_class_round_trips_through_physical_map() {
        // A screenshot detection's class must map back to the same
        // piece through the physical model's table.
        for color in [Color::Red, Color::Black] {
            for kind in [
                PieceKind::King,
                PieceKind::Advisor,
                PieceKind::Elephant,
                PieceKind::Horse,
                PieceKind::Cannon,
                PieceKind::Rook,
                PieceKind::Pawn,
            ] {
                let piece = Piece::new(color, kind);
                let class = piece_class(piece);
                assert_eq!(board::physical_class_to_piece(class), Some(piece));
            }
        }
    }
}
