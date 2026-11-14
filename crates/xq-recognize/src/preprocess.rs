//! Image preprocessing for the YOLOv8 models.
//!
//! Follows the Xiangqi Arena web code (`ChessRecognitionModel
//! .preprocessImage`): decode the photo, letterbox it into a
//! 640x640 square (grey `114` padding, aspect preserved), and
//! emit CHW float32 RGB in `[0, 1]`. The returned transform
//! (`scale`, `offset_x`, `offset_y`) maps the model frame back to
//! the original image: `orig = (model - offset) / scale`.

use crate::RecognizeError;

/// The model input size (YOLOv8 standard).
pub const INPUT_SIZE: u32 = 640;
/// The letterbox padding colour (upstream: grey 114).
const PAD: u8 = 114;

/// The letterboxed model input + the transform back to the
/// original image.
pub struct ModelInput {
    /// CHW float32 RGB, `[0, 1]`, `3 * 640 * 640` values.
    pub data: Vec<f32>,
    /// `model = orig * scale + offset`.
    pub scale: f32,
    /// X offset of the letterboxed image in the model frame.
    pub offset_x: f32,
    /// Y offset of the letterboxed image in the model frame.
    pub offset_y: f32,
    /// Original image width (pixels).
    pub orig_w: u32,
    /// Original image height (pixels).
    pub orig_h: u32,
}

/// Decode `bytes` (PNG/JPEG/WebP/...) and letterbox it into the
/// 640x640 model input.
pub fn preprocess(bytes: &[u8]) -> Result<ModelInput, RecognizeError> {
    let img =
        image::load_from_memory(bytes).map_err(|e| RecognizeError::BadImage(e.to_string()))?;
    let rgb = img.to_rgb8();
    let (orig_w, orig_h) = rgb.dimensions();
    if orig_w == 0 || orig_h == 0 {
        return Err(RecognizeError::BadImage("empty image".to_string()));
    }
    let scale = (INPUT_SIZE as f32 / orig_w as f32).min(INPUT_SIZE as f32 / orig_h as f32);
    let scaled_w = (orig_w as f32 * scale).round() as u32;
    let scaled_h = (orig_h as f32 * scale).round() as u32;
    let offset_x = (INPUT_SIZE - scaled_w) as f32 / 2.0;
    let offset_y = (INPUT_SIZE - scaled_h) as f32 / 2.0;
    let resized = image::imageops::resize(
        &rgb,
        scaled_w.max(1),
        scaled_h.max(1),
        image::imageops::FilterType::Triangle,
    );
    // Grey canvas, paste the resized image centred.
    let mut canvas =
        image::RgbImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgb([PAD, PAD, PAD]));
    image::imageops::replace(
        &mut canvas,
        &resized,
        offset_x.round() as i64,
        offset_y.round() as i64,
    );
    let size = (INPUT_SIZE * INPUT_SIZE) as usize;
    let mut data = vec![0f32; 3 * size];
    for (i, px) in canvas.pixels().enumerate() {
        data[i] = px[0] as f32 / 255.0;
        data[size + i] = px[1] as f32 / 255.0;
        data[2 * size + i] = px[2] as f32 / 255.0;
    }
    Ok(ModelInput {
        data,
        scale,
        offset_x,
        offset_y,
        orig_w,
        orig_h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x1 red image, encoded as PNG.
    fn tiny_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .expect("encode png");
        buf
    }

    #[test]
    fn preprocess_letterboxes_and_normalizes() {
        let input = preprocess(&tiny_png()).expect("preprocess");
        assert_eq!(input.data.len(), 3 * 640 * 640);
        assert_eq!(input.orig_w, 2);
        assert_eq!(input.orig_h, 1);
        // Scale: min(640/2, 640/1) = 320. Scaled 640x320, offset y 160.
        assert!((input.scale - 320.0).abs() < 1e-4);
        assert!((input.offset_x).abs() < 1e-4);
        assert!((input.offset_y - 160.0).abs() < 1e-4);
        // The top-left pixel is padding (grey 114/255), the centre
        // is the red image (1.0, 0.0, 0.0).
        let size = 640 * 640;
        assert!((input.data[0] - 114.0 / 255.0).abs() < 1e-6);
        let centre = 320 * 640 + 320;
        assert!((input.data[centre] - 1.0).abs() < 1e-6);
        assert!((input.data[size + centre]).abs() < 1e-6);
        assert!((input.data[2 * size + centre]).abs() < 1e-6);
    }

    #[test]
    fn preprocess_rejects_garbage() {
        assert!(preprocess(b"not an image").is_err());
        assert!(preprocess(&[]).is_err());
    }
}
