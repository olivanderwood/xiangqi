//! OCR voter: PP-OCRv6-tiny recognition-only (Apache-2.0, PaddlePaddle),
//! run on the centre crop of a detected piece as a second opinion.
//!
//! YOLO reads the whole board at 640 px, so each glyph is only ~30 px
//! and the model confuses similar characters (Horse 馬 vs Elephant
//! 相/象, General 帥/將 vs Advisor 仕/士). OCR was trained on printed
//! Chinese text, so it has strong priors for exactly these glyphs —
//! but only as a *voter*, never a replacement: it fails on
//! calligraphic/blurred discs where YOLO succeeds.
//!
//! The recogniser takes `[1, 3, 48, W]` (height 48, width dynamic)
//! and returns CTC logits `[1, T, 6905]` (6,904 dict entries + blank
//! at index 0). Only the 13 dict glyphs that name a Xiangqi kind are
//! whitelisted; the 3 missing red/black variants (傌, 砲, 俥) are not
//! in the tiny dict and read as their standard forms (馬, 炮, 車),
//! which map to the same kind. Colour is determined separately (the
//! screenshot pipeline's HSV check / the YOLO class), never by OCR.
//!
//! The crop and its 180° rotation are both tried (the far side's
//! pieces are upside-down) and the higher-confidence valid character
//! wins.

use image::imageops::FilterType;
use image::{DynamicImage, RgbImage};
use ort::session::Session;
use ort::value::Tensor;
use xq_core::types::PieceKind;

use crate::{environment_with, RecognizeError};

/// The OCR recogniser's input height (PP-OCR resizes to height 48).
const OCR_HEIGHT: u32 = 48;
/// Max input width (PP-OCRv6 default); wider crops are scaled down.
const OCR_MAX_WIDTH: u32 = 320;
/// The crop is centred on the piece; this much of the YOLO box is
/// kept (drops the rim and neighbouring pieces).
const CROP_FRACTION: f32 = 0.65;

/// The 13 dict glyphs that name a Xiangqi kind, as (CTC class index,
/// kind). Class index = dict index + 1 (blank is 0). Found by reading
/// `inference.yml` from PaddlePaddle/PP-OCRv6_tiny_rec_onnx.
const GLYPH_CLASSES: &[(usize, PieceKind)] = &[
    (2169, PieceKind::King),     // 帥 (red general)
    (2007, PieceKind::King),     // 將 (black general)
    (767, PieceKind::Advisor),   // 仕 (red advisor)
    (1721, PieceKind::Advisor),  // 士 (black advisor)
    (4139, PieceKind::Elephant), // 相 (red elephant)
    (5645, PieceKind::Elephant), // 象 (black elephant)
    (6583, PieceKind::Horse),    // 馬 (horse)
    (6590, PieceKind::Horse),    // 马 (horse, simplified)
    (3672, PieceKind::Cannon),   // 炮 (cannon)
    (5815, PieceKind::Rook),     // 車 (rook)
    (5827, PieceKind::Rook),     // 车 (rook, simplified)
    (1001, PieceKind::Pawn),     // 兵 (red pawn)
    (1200, PieceKind::Pawn),     // 卒 (black pawn)
];

/// Map a CTC class index to the piece kind it names, if it is one
/// of the whitelisted glyphs.
fn class_to_kind(class: usize) -> Option<PieceKind> {
    GLYPH_CLASSES
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, k)| *k)
}

/// The OCR vote overrides YOLO/the classifier only when the OCR
/// confidence is at least this. Empirically OCR reads the glyph at
/// 0.85–1.00 when it is right (verified on Hieu's two photos, where
/// it corrected 6 YOLO errors the rule repair could not catch), and
/// the model is near-silent below ~0.5 on unreadable crops.
pub const VOTE_THRESHOLD: f32 = 0.9;

/// The OCR voter: a loaded PP-OCRv6-tiny recognition session.
pub struct OcrVoter {
    session: Session,
    input_name: String,
    output_name: String,
}

impl OcrVoter {
    /// Load the OCR model from `path` (an .onnx file), using the
    /// shared ORT environment (or `lib_path` for a dynamic library).
    pub fn load(
        path: &std::path::Path,
        lib_path: Option<&std::path::Path>,
    ) -> Result<Self, RecognizeError> {
        let env = environment_with(lib_path)?;
        let session = Session::builder(env)?.commit_from_file(path)?;
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "x".to_string());
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_default();
        Ok(Self {
            session,
            input_name,
            output_name,
        })
    }

    /// Read the glyph on a piece crop (the RGB image of one piece,
    /// already cropped to the YOLO box). Returns the kind + the
    /// CTC confidence of the winning character, or `None` when no
    /// whitelisted glyph is read. Both the crop and its 180° rotation
    /// are tried; the higher-confidence result wins.
    pub fn recognize_crop(&mut self, crop: &RgbImage) -> Option<(PieceKind, f32)> {
        let a = self.recognize_one(crop);
        let rotated = image::imageops::rotate180(crop);
        let b = self.recognize_one(&rotated);
        match (a, b) {
            (Some((ka, ca)), Some((kb, cb))) => {
                if ca >= cb {
                    Some((ka, ca))
                } else {
                    Some((kb, cb))
                }
            }
            (Some(x), None) => Some(x),
            (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }

    /// Debug: the top-5 (class index, logit) at the step where the
    /// winning glyph was read, for one oriented crop. Used by the
    /// debug-ocr example to inspect the raw CTC distribution.
    pub fn debug_top(&mut self, crop: &RgbImage) -> Vec<(usize, f32)> {
        let (w, h) = crop.dimensions();
        let cw = ((w as f32 * CROP_FRACTION) as u32).max(8);
        let ch = ((h as f32 * CROP_FRACTION) as u32).max(8);
        let x0 = (w - cw) / 2;
        let y0 = (h - ch) / 2;
        let centre = image::imageops::crop_imm(crop, x0, y0, cw, ch).to_image();
        let resized = image::imageops::resize(
            &DynamicImage::ImageRgb8(centre),
            (OCR_HEIGHT * cw / ch).clamp(8, OCR_MAX_WIDTH),
            OCR_HEIGHT,
            FilterType::Lanczos3,
        );
        let (rw, rh) = resized.dimensions();
        let mut data = Vec::with_capacity((3 * rw * rh) as usize);
        for c in 0..3 {
            for y in 0..rh {
                for x in 0..rw {
                    let px = resized.get_pixel(x, y);
                    data.push((px[c] as f32 / 255.0 - 0.5) / 0.5);
                }
            }
        }
        let tensor = Tensor::from_array(([1, 3, rh as usize, rw as usize], data));
        let Ok(tensor) = tensor else { return vec![] };
        let Ok(outputs) = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
        else {
            return vec![];
        };
        let Some(value) = outputs.get(&self.output_name) else {
            return vec![];
        };
        let Ok((shape, logits)) = value.try_extract_tensor::<f32>() else {
            return vec![];
        };
        if shape.len() != 3 {
            return vec![];
        }
        let (t, classes) = (shape[1] as usize, shape[2] as usize);
        // Find the step whose argmax is a whitelisted glyph.
        let mut prev: Option<usize> = None;
        for step in 0..t {
            let base = step * classes;
            let (mut argmax, mut maxv) = (0usize, f32::NEG_INFINITY);
            for c in 0..classes {
                let v = logits[base + c];
                if v > maxv {
                    maxv = v;
                    argmax = c;
                }
            }
            let is_repeat = prev == Some(argmax);
            prev = Some(argmax);
            if argmax == 0 || is_repeat {
                continue;
            }
            if class_to_kind(argmax).is_some() {
                let mut top: Vec<(usize, f32)> =
                    (0..classes).map(|c| (c, logits[base + c])).collect();
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                top.truncate(5);
                return top;
            }
        }
        vec![]
    }

    /// Run the recogniser on one oriented crop.
    fn recognize_one(&mut self, crop: &RgbImage) -> Option<(PieceKind, f32)> {
        // Centre-crop to drop the rim, then resize to height 48.
        let (w, h) = crop.dimensions();
        let cw = ((w as f32 * CROP_FRACTION) as u32).max(8);
        let ch = ((h as f32 * CROP_FRACTION) as u32).max(8);
        let x0 = (w - cw) / 2;
        let y0 = (h - ch) / 2;
        let centre = image::imageops::crop_imm(crop, x0, y0, cw, ch).to_image();
        let resized = image::imageops::resize(
            &DynamicImage::ImageRgb8(centre),
            // Keep the glyph's aspect ratio at height 48 (a square
            // glyph stays ~48 wide); PP-OCR caps the width at 320.
            (OCR_HEIGHT * cw / ch).clamp(8, OCR_MAX_WIDTH),
            OCR_HEIGHT,
            FilterType::Lanczos3,
        );
        let (rw, rh) = resized.dimensions();
        // PP-OCR normalises to [-1, 1] ((x/255 - 0.5) / 0.5), CHW.
        let mut data = Vec::with_capacity((3 * rw * rh) as usize);
        for c in 0..3 {
            for y in 0..rh {
                for x in 0..rw {
                    let px = resized.get_pixel(x, y);
                    data.push((px[c] as f32 / 255.0 - 0.5) / 0.5);
                }
            }
        }
        let tensor = Tensor::from_array(([1, 3, rh as usize, rw as usize], data)).ok()?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .ok()?;
        let value = outputs.get(&self.output_name)?;
        let (shape, logits) = value.try_extract_tensor::<f32>().ok()?;
        // Logits are [1, T, 6905]; CTC-decode: argmax per step,
        // collapse repeats, drop blanks (0). The first whitelisted
        // glyph wins, with its step probability as confidence.
        // NOTE: the ONNX model already outputs a probability
        // distribution (softmax applied), so the argmax value IS the
        // confidence — do not softmax again (that dilutes it to ~0.17).
        if shape.len() != 3 {
            return None;
        }
        let (t, classes) = (shape[1] as usize, shape[2] as usize);
        let mut prev: Option<usize> = None;
        let mut best: Option<(PieceKind, f32)> = None;
        for step in 0..t {
            let base = step * classes;
            let (mut argmax, mut maxv) = (0usize, f32::NEG_INFINITY);
            for c in 0..classes {
                let v = logits[base + c];
                if v > maxv {
                    maxv = v;
                    argmax = c;
                }
            }
            let is_repeat = prev == Some(argmax);
            prev = Some(argmax);
            if argmax == 0 || is_repeat {
                continue;
            }
            if let Some(kind) = class_to_kind(argmax) {
                if best.is_none_or(|(_, bc)| maxv > bc) {
                    best = Some((kind, maxv));
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_classes_map_to_kinds() {
        assert_eq!(class_to_kind(2169), Some(PieceKind::King));
        assert_eq!(class_to_kind(2007), Some(PieceKind::King));
        assert_eq!(class_to_kind(767), Some(PieceKind::Advisor));
        assert_eq!(class_to_kind(4139), Some(PieceKind::Elephant));
        assert_eq!(class_to_kind(6583), Some(PieceKind::Horse));
        assert_eq!(class_to_kind(3672), Some(PieceKind::Cannon));
        assert_eq!(class_to_kind(5815), Some(PieceKind::Rook));
        assert_eq!(class_to_kind(1001), Some(PieceKind::Pawn));
        // Not a Xiangqi glyph: no kind.
        assert_eq!(class_to_kind(0), None);
        assert_eq!(class_to_kind(1), None);
        assert_eq!(class_to_kind(6904), None);
    }
}
