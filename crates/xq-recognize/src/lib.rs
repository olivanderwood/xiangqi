//! Board recognition from a photo or screenshot, offline, via ONNX.
//!
//! Models from [Xiangqi Arena](https://github.com/dffge552/xiangqi-pwa-offline)
//! (`dffge552`, MIT — see `models/README.md` for the attribution):
//! - `Entity_chess_recognition_model.onnx` (12 MB): unified YOLOv8
//!   detector for a photo of a physical board. 14 classes (7 piece
//!   kinds x red/black). Input `images` `[1, 3, 640, 640]`, RGB 0-1,
//!   letterboxed. Output YOLOv8 `[1, 18, 8400]` (4 bbox + 14 classes).
//! - `online_xiangqi_piece_detector.onnx` (12 MB): single-class
//!   YOLOv8 detector for a screenshot. Input `images` `[1, 3, 640, 640]`.
//!   Output `[1, 5, 8400]` (4 bbox + 1 confidence).
//! - `online_xiangqi_classifier.onnx` (5.7 MB): piece-kind
//!   classifier for a 64x64 crop. Input `images` `[1, 3, 64, 64]`.
//!   Output 7 class probabilities (advisor, rook, horse, pawn, king,
//!   cannon, bishop); the side (red/black) comes from a colour check
//!   on the crop, not from the model.
//!
//! Both models need the board's 4 corners + the centre pawn to map
//! detections to squares (see `BoardCalibration`); without them the
//! grid is only estimated. The app asks the user to mark them on the
//! photo after the first detection pass.

use std::path::{Path, PathBuf};

pub mod board;
pub mod ocr;
pub mod preprocess;
pub mod screenshot;
pub mod yolo;

pub use board::{BoardCalibration, Grid};
use xq_core::board::Position;
pub use yolo::Detection;

/// Where the ONNX model files live.
#[derive(Clone, Debug)]
pub struct ModelPaths {
    /// Unified physical-board model.
    pub physical: PathBuf,
    /// Screenshot piece detector.
    pub detector: PathBuf,
    /// Screenshot piece-kind classifier.
    pub classifier: PathBuf,
    /// OCR voter (PP-OCRv6-tiny rec). May be absent — the voter is
    /// skipped then and YOLO alone decides.
    pub ocr: PathBuf,
}

impl ModelPaths {
    /// The model files inside `dir`.
    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            physical: dir.join("Entity_chess_recognition_model.onnx"),
            detector: dir.join("online_xiangqi_piece_detector.onnx"),
            classifier: dir.join("online_xiangqi_classifier.onnx"),
            ocr: dir.join("PP-OCRv6_tiny_rec.onnx"),
        }
    }
}

/// What can go wrong while recognising a board.
#[derive(Debug)]
pub enum RecognizeError {
    /// The ONNX Runtime (or a model file) failed: message.
    Ort(String),
    /// The image bytes could not be decoded.
    BadImage(String),
    /// The board could not be read from the detections (too few
    /// pieces, no generals, pieces off the board, ...).
    NoPosition(String),
}

impl std::fmt::Display for RecognizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecognizeError::Ort(msg) => write!(f, "ONNX: {msg}"),
            RecognizeError::BadImage(msg) => write!(f, "image: {msg}"),
            RecognizeError::NoPosition(msg) => write!(f, "no position: {msg}"),
        }
    }
}

impl std::error::Error for RecognizeError {}

impl From<ort::Error> for RecognizeError {
    fn from(e: ort::Error) -> Self {
        RecognizeError::Ort(e.to_string())
    }
}

/// The single ONNX Runtime environment for this process: ONNX
/// Runtime allows only one environment per process, so it is
/// created once (lazily) and shared by every session. The init
/// result is stored so a failure is reported, not panicked on,
/// every time. `lib_path` is the ONNX Runtime library
/// (`libonnxruntime.so` / `.dylib` / `.dll`); when given, the
/// library is loaded from there instead of the system search path
/// (`ORT_DYLIB_PATH` / system library).
pub(crate) fn environment_with(
    lib_path: Option<&Path>,
) -> Result<&'static ort::environment::Environment, RecognizeError> {
    static ENV: std::sync::OnceLock<Result<ort::environment::Environment, String>> =
        std::sync::OnceLock::new();
    let env = ENV.get_or_init(|| {
        let builder: Result<ort::environment::EnvironmentBuilder, String> = match lib_path {
            Some(path) => ort::init_from(path).map_err(|e| e.to_string()),
            None => Ok(ort::init()),
        };
        match builder {
            Ok(b) => b
                .with_name("xq-recognize")
                .build()
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        }
    });
    match env {
        Ok(env) => Ok(env),
        Err(msg) => Err(RecognizeError::Ort(msg.clone())),
    }
}

/// The single ONNX Runtime environment, loading the library from
/// the system search path.
fn environment() -> Result<&'static ort::environment::Environment, RecognizeError> {
    environment_with(None)
}

/// Load an ONNX model and return a one-line summary of its
/// input/output names and shapes, for diagnostics and tests.
pub fn model_info(path: &Path) -> Result<String, RecognizeError> {
    let env = environment()?;
    let session = ort::session::Session::builder(env)?.commit_from_file(path)?;
    let mut out = String::new();
    for input in session.inputs() {
        out.push_str(&format!("in {} {:?}; ", input.name(), input.dtype()));
    }
    for output in session.outputs() {
        out.push_str(&format!("out {} {:?}; ", output.name(), output.dtype()));
    }
    Ok(out)
}

/// The ONNX Runtime library file name for this platform.
pub fn lib_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// The ONNX Runtime library path inside a models directory.
pub fn lib_path_for(dir: &Path) -> PathBuf {
    dir.join(lib_file_name())
}

/// Find the `recognize/` directory (models + ONNX Runtime library):
/// next to the running executable first, then the current working
/// directory (dev runs from the repo root). Returns `None` when
/// neither has the physical model — recognition is then unavailable
/// and the app falls back to the manual editor.
pub fn find_models_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("recognize"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("recognize"));
    }
    candidates
        .into_iter()
        .find(|d| d.join("Entity_chess_recognition_model.onnx").exists())
}

/// A loaded board recognizer: the physical-board YOLOv8 model.
/// Created once per process (it holds the ONNX sessions) and
/// reused for every photo; inference itself is synchronous, so the
/// app runs it on a worker thread, never on the UI thread.
pub struct Recognizer {
    session: ort::session::Session,
    input_name: String,
    output_name: String,
    /// OCR voter (PP-OCRv6-tiny). `None` when the model file is
    /// absent — YOLO alone decides then.
    ocr: Option<ocr::OcrVoter>,
}

impl Recognizer {
    /// Load the physical-board model from `paths.physical`.
    /// `lib_path` is the ONNX Runtime library; `None` loads it
    /// from the system search path.
    pub fn load(paths: &ModelPaths, lib_path: Option<&Path>) -> Result<Self, RecognizeError> {
        let env = environment_with(lib_path)?;
        let session = ort::session::Session::builder(env)?.commit_from_file(&paths.physical)?;
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "images".to_string());
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "output0".to_string());
        let ocr = ocr::OcrVoter::load(&paths.ocr, lib_path).ok();
        Ok(Self {
            session,
            input_name,
            output_name,
            ocr,
        })
    }

    /// Recognise a board from `bytes` (PNG/JPEG/WebP/...).
    /// `calibration` (4 corners + centre pawn, in the original
    /// image's pixels) maps detections to squares; `None` estimates
    /// the board from the detections (best-effort). Returns the
    /// position with Red to move — the photo does not say whose
    /// turn it is, so the app asks the user after recognition.
    /// Fails when too few pieces are found or the generals are
    /// missing (see `recognize_grid` for the partial result).
    pub fn recognize(
        &mut self,
        bytes: &[u8],
        calibration: Option<&BoardCalibration>,
    ) -> Result<Position, RecognizeError> {
        let grid = self.recognize_grid(bytes, calibration)?;
        board::grid_to_position(&grid)
    }

    /// Recognise a photo into a 10x9 grid (row 0 = image top),
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

    /// Detect pieces in `bytes` and return them (original image
    /// coordinates) plus the strongest class score seen (before
    /// the threshold), for diagnostics and for callers that want
    /// the raw detections instead of a finished position.
    pub fn detect(&mut self, bytes: &[u8]) -> Result<(Vec<Detection>, f32), RecognizeError> {
        let input = preprocess::preprocess(bytes)?;
        let tensor = ort::value::Tensor::from_array((
            [
                1usize,
                3,
                preprocess::INPUT_SIZE as usize,
                preprocess::INPUT_SIZE as usize,
            ],
            input.data,
        ))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])?;
        let output = outputs
            .get(&self.output_name)
            .ok_or_else(|| RecognizeError::Ort(format!("missing output {}", self.output_name)))?;
        let (shape, data) = output.try_extract_tensor::<f32>()?;
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        let frame = yolo::Frame {
            scale: input.scale,
            offset_x: input.offset_x,
            offset_y: input.offset_y,
            orig_w: input.orig_w as f32,
            orig_h: input.orig_h as f32,
        };
        let result = yolo::parse_yolo_output(data, &dims, board::PHYSICAL_CLASSES, &frame);
        let mut detections = result.detections;
        // OCR vote: YOLO reads the whole board at 640 px, so each
        // glyph is ~30 px and look-alikes (Horse vs Elephant,
        // General vs Advisor) get confused — on Hieu's two photos
        // OCR corrected 6 such errors the rule repair could not
        // catch. When OCR is confident (>= 0.9) and disagrees, its
        // kind wins; the colour stays YOLO's.
        if let Some(ocr) = self.ocr.as_mut() {
            if let Ok(img) = image::load_from_memory(bytes).map(|i| i.to_rgb8()) {
                let (w, h) = img.dimensions();
                for det in &mut detections {
                    let Some(piece) = board::physical_class_to_piece(det.class) else {
                        continue;
                    };
                    let x1 = ((det.cx - det.w / 2.0).max(0.0) as u32).min(w);
                    let y1 = ((det.cy - det.h / 2.0).max(0.0) as u32).min(h);
                    let x2 = ((det.cx + det.w / 2.0).max(0.0) as u32).min(w);
                    let y2 = ((det.cy + det.h / 2.0).max(0.0) as u32).min(h);
                    if x2 <= x1 || y2 <= y1 {
                        continue;
                    }
                    let crop = image::imageops::crop_imm(&img, x1, y1, x2 - x1, y2 - y1).to_image();
                    if let Some((ocr_kind, ocr_conf)) = ocr.recognize_crop(&crop) {
                        if ocr_conf >= ocr::VOTE_THRESHOLD && ocr_kind != piece.kind {
                            let corrected = xq_core::types::Piece::new(piece.color, ocr_kind);
                            det.class = board::piece_to_physical_class(corrected);
                        }
                    }
                }
            }
        }
        Ok((detections, result.max_confidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The models, fetched by `tools/fetch-recognize.sh` into the
    /// workspace `recognize/` dir (not committed). Falls back to
    /// the cloned upstream repo in /tmp, if present.
    fn model_dir() -> PathBuf {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../recognize");
        if workspace
            .join("Entity_chess_recognition_model.onnx")
            .exists()
        {
            workspace
        } else {
            PathBuf::from("/tmp/xiangqi-pwa-offline")
        }
    }

    #[test]
    fn physical_model_loads_and_reports_yolov8_io() {
        let paths = ModelPaths::in_dir(model_dir());
        assert!(paths.physical.exists(), "physical model missing");
        let info = model_info(&paths.physical).expect("load physical model");
        // Input `images`, YOLOv8 output (18 = 4 bbox + 14 classes).
        assert!(info.contains("images"), "input name: {info}");
        assert!(info.contains("Float32"), "input type: {info}");
        // The output is a 3-D tensor with 18 channels (or 8400 rows).
        assert!(
            info.contains("18") || info.contains("8400"),
            "output dims: {info}"
        );
    }

    #[test]
    fn detector_and_classifier_load() {
        let paths = ModelPaths::in_dir(model_dir());
        assert!(paths.detector.exists(), "detector model missing");
        assert!(paths.classifier.exists(), "classifier model missing");
        let det = model_info(&paths.detector).expect("load detector");
        assert!(det.contains("images"), "detector input: {det}");
        let cls = model_info(&paths.classifier).expect("load classifier");
        assert!(cls.contains("images"), "classifier input: {cls}");
    }
}
