//! YOLOv8 output parsing + non-maximum suppression.
//!
//! Follows the Xiangqi Arena web code (`index.html`, class
//! `ChessRecognitionModel`): the model outputs one row per anchor,
//! `[cx, cy, w, h, class0..classN]` (or the transposed layout),
//! in the letterboxed 640x640 coordinate frame. Detections below
//! the confidence threshold are dropped, the rest are unpadded +
//! unscaled back to the original image, and NMS (IoU) removes
//! overlapping boxes of the same class.

/// One piece detection, in the *original* image's pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    /// Centre x (original image pixels).
    pub cx: f32,
    /// Centre y (original image pixels).
    pub cy: f32,
    /// Width (original image pixels).
    pub w: f32,
    /// Height (original image pixels).
    pub h: f32,
    /// Class index (0..num_classes).
    pub class: usize,
    /// Confidence (max class score).
    pub confidence: f32,
}

/// Confidence below this is dropped (upstream: 0.25).
pub const CONF_THRESHOLD: f32 = 0.25;
/// IoU above this suppresses the lower-confidence box (upstream: 0.45).
pub const IOU_THRESHOLD: f32 = 0.45;

/// The letterbox transform + original image size, for
/// un-projecting detections back to the original image.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// `model = orig * scale + offset`.
    pub scale: f32,
    /// X offset of the letterboxed image in the model frame.
    pub offset_x: f32,
    /// Y offset of the letterboxed image in the model frame.
    pub offset_y: f32,
    /// Original image width (pixels).
    pub orig_w: f32,
    /// Original image height (pixels).
    pub orig_h: f32,
}

/// Parse a YOLOv8 output tensor into detections, in the original
/// image's coordinates.
///
/// `data` is the flat output (row-major), `dims` its shape
/// (`[1, C, N]` or `[1, N, C]`, `C = 4 + num_classes`).
/// `frame` is the letterbox transform that maps the 640x640 model
/// input back to the original image (see `preprocess`).
pub fn parse_yolo_output(
    data: &[f32],
    dims: &[usize],
    num_classes: usize,
    frame: &Frame,
) -> Vec<Detection> {
    let Some((layout, n)) = yolo_layout(dims, num_classes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..n {
        let (x, y, w, h, scores) = match layout {
            Layout::RowMajor => {
                // [1, N, C]: row i holds cx, cy, w, h, then the classes.
                let off = i * (4 + num_classes);
                (
                    data[off],
                    data[off + 1],
                    data[off + 2],
                    data[off + 3],
                    data[off + 4..off + 4 + num_classes].to_vec(),
                )
            }
            Layout::ChannelMajor => {
                // [1, C, N]: channel c holds the c-th value of all rows.
                let scores: Vec<f32> = (0..num_classes).map(|c| data[n * (4 + c) + i]).collect();
                (
                    data[i],
                    data[n + i],
                    data[2 * n + i],
                    data[3 * n + i],
                    scores,
                )
            }
        };
        // Best class + its score.
        let mut best = 0usize;
        let mut best_score = 0f32;
        for (c, &s) in scores.iter().enumerate() {
            if s > best_score {
                best_score = s;
                best = c;
            }
        }
        if best_score < CONF_THRESHOLD {
            continue;
        }
        // Unpad + unscale back to the original image.
        let cx = (x - frame.offset_x) / frame.scale;
        let cy = (y - frame.offset_y) / frame.scale;
        let w = w / frame.scale;
        let h = h / frame.scale;
        if cx < 0.0 || cx > frame.orig_w || cy < 0.0 || cy > frame.orig_h || w <= 0.0 || h <= 0.0 {
            continue;
        }
        out.push(Detection {
            cx,
            cy,
            w,
            h,
            class: best,
            confidence: best_score,
        });
    }
    non_max_suppression(out)
}

/// YOLOv8 output layout: which dimension holds the channels
/// (`4 + num_classes`) and which holds the detection rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layout {
    /// `[1, C, N]`: channel-major (the physical model's output).
    ChannelMajor,
    /// `[1, N, C]`: row-major.
    RowMajor,
}

/// The output layout + number of detection rows, from the output
/// shape. `None` when the shape is not a 3-D YOLOv8 output. The
/// channel count (`4 + num_classes`) identifies the layout — not
/// the relative size of the dimensions, which breaks for tiny
/// test tensors.
fn yolo_layout(dims: &[usize], num_classes: usize) -> Option<(Layout, usize)> {
    if dims.len() != 3 || dims[0] != 1 {
        return None;
    }
    let channels = 4 + num_classes;
    if dims[1] == channels {
        Some((Layout::ChannelMajor, dims[2]))
    } else if dims[2] == channels {
        Some((Layout::RowMajor, dims[1]))
    } else {
        None
    }
}

/// Intersection-over-union of two boxes (centre + size).
fn iou(a: &Detection, b: &Detection) -> f32 {
    let ax0 = a.cx - a.w / 2.0;
    let ay0 = a.cy - a.h / 2.0;
    let ax1 = a.cx + a.w / 2.0;
    let ay1 = a.cy + a.h / 2.0;
    let bx0 = b.cx - b.w / 2.0;
    let by0 = b.cy - b.h / 2.0;
    let bx1 = b.cx + b.w / 2.0;
    let by1 = b.cy + b.h / 2.0;
    let ix0 = ax0.max(bx0);
    let iy0 = ay0.max(by0);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    let iw = (ix1 - ix0).max(0.0);
    let ih = (iy1 - iy0).max(0.0);
    let inter = iw * ih;
    let area_a = a.w * a.h;
    let area_b = b.w * b.h;
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Non-maximum suppression: keep the highest-confidence box, drop
/// any same-class box whose IoU with a kept box exceeds the
/// threshold. Input is sorted by confidence first (highest first).
pub fn non_max_suppression(mut detections: Vec<Detection>) -> Vec<Detection> {
    detections.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<Detection> = Vec::new();
    for det in detections {
        let overlaps = kept
            .iter()
            .any(|k| k.class == det.class && iou(k, &det) > IOU_THRESHOLD);
        if !overlaps {
            kept.push(det);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `[1, C, N]` output with one strong detection (class 0,
    /// conf 0.9) at the centre of a 640x640 frame, and one weak
    /// detection (class 1, conf 0.1) that must be dropped.
    fn yolo_output() -> (Vec<f32>, Vec<usize>) {
        // C = 4 + 2 classes = 6, N = 2 rows.
        let n = 2;
        let mut data = vec![0f32; 6 * n];
        // Row 0: cx=320, cy=320, w=60, h=60, class0=0.9, class1=0.05.
        data[0] = 320.0;
        data[n] = 320.0;
        data[2 * n] = 60.0;
        data[3 * n] = 60.0;
        data[4 * n] = 0.9;
        data[5 * n] = 0.05;
        // Row 1: cx=100, cy=100, w=50, h=50, class0=0.05, class1=0.1.
        data[1] = 100.0;
        data[n + 1] = 100.0;
        data[2 * n + 1] = 50.0;
        data[3 * n + 1] = 50.0;
        data[4 * n + 1] = 0.05;
        data[5 * n + 1] = 0.1;
        (data, vec![1, 6, 2])
    }

    #[test]
    fn parse_keeps_confident_detection_and_drops_weak() {
        let (data, dims) = yolo_output();
        // No letterbox (scale 1, offset 0), original 640x640.
        let frame = Frame {
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            orig_w: 640.0,
            orig_h: 640.0,
        };
        let dets = parse_yolo_output(&data, &dims, 2, &frame);
        assert_eq!(dets.len(), 1);
        assert_eq!(dets[0].class, 0);
        assert!((dets[0].confidence - 0.9).abs() < 1e-6);
        assert!((dets[0].cx - 320.0).abs() < 1e-6);
        assert!((dets[0].cy - 320.0).abs() < 1e-6);
    }

    #[test]
    fn parse_unpads_and_unscales() {
        let (data, dims) = yolo_output();
        // Letterbox: scale 0.5, offset (10, 20) in the 640 frame.
        // Model cx 320 -> orig (320-10)/0.5 = 620.
        let frame = Frame {
            scale: 0.5,
            offset_x: 10.0,
            offset_y: 20.0,
            orig_w: 1280.0,
            orig_h: 1280.0,
        };
        let dets = parse_yolo_output(&data, &dims, 2, &frame);
        assert_eq!(dets.len(), 1);
        assert!((dets[0].cx - 620.0).abs() < 1e-4);
        assert!((dets[0].cy - 600.0).abs() < 1e-4);
        assert!((dets[0].w - 120.0).abs() < 1e-4);
    }

    #[test]
    fn nms_drops_overlapping_same_class_box() {
        let a = Detection {
            cx: 100.0,
            cy: 100.0,
            w: 60.0,
            h: 60.0,
            class: 0,
            confidence: 0.9,
        };
        let mut b = a;
        b.cx = 110.0;
        b.confidence = 0.8;
        // Same class, heavily overlapping: only the stronger survives.
        assert_eq!(non_max_suppression(vec![a, b]).len(), 1);
        // Different class: both survive.
        b.class = 1;
        assert_eq!(non_max_suppression(vec![a, b]).len(), 2);
    }
}
