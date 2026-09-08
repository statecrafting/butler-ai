// Spec: specs/007-text-recognition/spec.md

//! On-device text recognition (spec 007).
//!
//! The pipeline reasons about text, not pixels, and both target platforms
//! ship a capable, hardware-accelerated OCR engine with no download. Using
//! theirs rather than bundling Tesseract or an ONNX model means a smaller
//! binary, no model licensing, no GPU dependency of our own, and engines that
//! improve with the OS (§1).
//!
//! Recognition runs **entirely on device**. FR-004 is the mechanical form of
//! that promise: no networking crate appears in this crate's tree.
//!
//! # The part that matters most
//!
//! [`normalize`] is not incidental tidying. Engine output varies frame to
//! frame on a screen nobody touched: line order shifts with layout
//! heuristics, whitespace differs, confidence flickers on anti-aliased text.
//! Without a deterministic pass, spec 008's change detector fires on noise
//! and the product asks the model a question every few seconds. It is pure,
//! has no platform code, and its fixtures run on every CI target (§2).

pub mod normalize;
pub mod recognized;
pub mod recognizer;

#[cfg(target_os = "macos")]
pub mod apple_vision;

#[cfg(target_os = "windows")]
pub mod windows_ocr;

pub use normalize::{DEFAULT_MIN_CONFIDENCE, normalize};
pub use recognized::{EngineId, LanguageTag, Line, Recognized, Rect, Size};
pub use recognizer::{
    MAX_ENGINE_SIDE, NullRecognizer, OcrError, RECOGNIZE_TIMEOUT, RecognizeOptions, TextRecognizer,
    engine_id, engine_scale,
};

#[cfg(target_os = "macos")]
pub use apple_vision::AppleVisionRecognizer;

#[cfg(target_os = "windows")]
pub use windows_ocr::WindowsOcrRecognizer;

/// The recognizer this build ships with.
///
/// One name for "the OS engine on a shipped platform, an honest refusal
/// everywhere else", so callers do not each write the same `cfg`.
#[cfg(target_os = "macos")]
#[must_use]
pub fn platform_recognizer() -> AppleVisionRecognizer {
    AppleVisionRecognizer::new()
}

/// The recognizer this build ships with (see the macOS version).
#[cfg(target_os = "windows")]
#[must_use]
pub fn platform_recognizer() -> WindowsOcrRecognizer {
    WindowsOcrRecognizer::new()
}

/// The recognizer this build ships with (see the macOS version).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[must_use]
pub fn platform_recognizer() -> NullRecognizer {
    NullRecognizer
}
