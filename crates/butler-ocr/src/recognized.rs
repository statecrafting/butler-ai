// Spec: specs/007-text-recognition/spec.md

//! What the recognizer produces (spec 007 §3.2).
//!
//! Spec 015 constrains this file. [`Recognized`] **is** the user's screen, as
//! text, so the rule is narrower than [`butler_capture::Frame`]'s: it may be
//! `Clone`, because the pipeline holds the previous one to compare against
//! (spec 008), but it must never be `Serialize`. It leaves the process only
//! as `RedactedText` inside one `InferenceRequest` (spec 010), after
//! redaction (spec 015 §3.2), and never as a whole.

use std::time::Instant;

pub use butler_capture::Rect;

/// A BCP-47 language tag, as the engines take them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageTag(pub String);

impl LanguageTag {
    /// The tag the engines default to when settings name none.
    #[must_use]
    pub fn english() -> Self {
        Self("en-US".to_owned())
    }

    /// The tag as the platform APIs want it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which engine produced a result (§3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineId {
    /// Apple Vision's `VNRecognizeTextRequest`.
    AppleVision,
    /// `Windows.Media.Ocr`.
    WindowsMediaOcr,
    /// No engine: the crate compiled for a platform butler-ai does not ship on.
    None,
}

/// A pixel size, for the normalization pass's geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// One recognized line, in frame coordinates (§3.2).
///
/// `bbox` is always in the *original* frame's pixels, even when the engine
/// saw a downscaled image (FR-005): a caller that had to know about the
/// downscale would be a caller that could get it wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The line's text, exactly as the engine returned it.
    pub text: String,
    /// Where it sits in the frame.
    pub bbox: Rect,
    /// The engine's confidence, 0.0 to 1.0.
    pub confidence: f32,
}

/// The recognizer's output (§3.2).
///
/// `Clone` but **not** `Serialize`, deliberately. There is no derive and no
/// hand-written impl anywhere in this crate.
#[derive(Clone, Debug)]
pub struct Recognized {
    /// The engine's lines, in the engine's own order and un-normalized.
    ///
    /// Kept because the exclusion self-test (005) and any future geometry
    /// consumer need boxes, and because normalization is lossy by design.
    pub lines: Vec<Line>,
    /// The normalized string (§3.4): what the change detector compares and
    /// what the prompt carries.
    pub text: String,
    /// Mean confidence across the lines that survived the confidence filter.
    pub mean_confidence: f32,
    /// Which engine produced it.
    pub engine: EngineId,
    /// When, monotonically. Never wall-clock, for the reason
    /// `butler_capture::Frame` gives.
    pub recognized_at: Instant,
}

impl Recognized {
    /// Assemble a result from engine lines, normalizing as it goes.
    #[must_use]
    pub fn from_lines(
        lines: Vec<Line>,
        frame: Size,
        engine: EngineId,
        min_confidence: f32,
    ) -> Self {
        let text = crate::normalize::normalize(&lines, frame, min_confidence);
        let kept: Vec<f32> = lines
            .iter()
            .filter(|line| line.confidence >= min_confidence)
            .map(|line| line.confidence)
            .collect();
        let mean_confidence = if kept.is_empty() {
            0.0
        } else {
            // Summed in `f64` and narrowed once. The engines report
            // confidence to two or three digits, so the narrowing is far
            // below what either of them means.
            let total: f64 = kept.iter().map(|c| f64::from(*c)).sum();
            let count = f64::from(u32::try_from(kept.len()).unwrap_or(u32::MAX));
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a mean of values in 0.0..=1.0 is in 0.0..=1.0, which \
                          every f32 represents exactly enough for a number the \
                          engines report to three digits"
            )]
            {
                (total / count) as f32
            }
        };
        Self {
            lines,
            text,
            mean_confidence,
            engine,
            recognized_at: Instant::now(),
        }
    }

    /// How many characters the normalized text holds.
    ///
    /// The one number the runtime traces (spec 019 emits `text_len`), because
    /// a length is not content.
    #[must_use]
    pub fn text_len(&self) -> usize {
        self.text.chars().count()
    }
}
