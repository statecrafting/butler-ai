// Spec: specs/007-text-recognition/spec.md

//! The recognition trait (spec 007 §3.1).

use butler_capture::FrameView;

use crate::recognized::{EngineId, LanguageTag, Recognized, Size};

/// How long recognition may take before it is a timeout (§3.1).
pub const RECOGNIZE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// The longest side an engine is given (§3.3).
///
/// Beyond this both engines degrade rather than refuse, so a view is
/// downscaled with a box filter first and the factor is recorded, so `bbox`
/// values stay in frame coordinates (FR-005).
pub const MAX_ENGINE_SIDE: u32 = 4096;

/// What the caller asks for (§3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct RecognizeOptions {
    /// Which languages to try. Whatever the OS has installed; this crate
    /// never downloads a pack (§6).
    pub languages: Vec<LanguageTag>,
    /// Trade accuracy for speed.
    pub fast: bool,
    /// Lines below this confidence are dropped by normalization (§3.4).
    pub min_confidence: f32,
}

impl Default for RecognizeOptions {
    fn default() -> Self {
        Self {
            languages: vec![LanguageTag::english()],
            fast: false,
            min_confidence: crate::normalize::DEFAULT_MIN_CONFIDENCE,
        }
    }
}

/// Why recognition failed (§3.1).
///
/// **No variant carries text or pixels.** `Internal` holds a sanitized
/// platform message: the engine's own words about its own state, never
/// anything it read (spec 015 §3.5).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OcrError {
    /// Recognition did not finish within [`RECOGNIZE_TIMEOUT`].
    #[error("recognition timed out")]
    Timeout,
    /// The platform has no OCR engine available.
    #[error("no OCR engine available")]
    EngineUnavailable,
    /// The engine has no model for a requested language.
    #[error("language {0} is not supported by the installed engine")]
    LanguageUnsupported(String),
    /// Anything else the platform reported.
    #[error("recognition failed: {0}")]
    Internal(String),
}

impl OcrError {
    /// Build an `Internal` from a platform message, sanitized (§3.1).
    ///
    /// Engine messages are the OS's, not the user's, but they are
    /// pass-through strings and this is the one place they enter the process.
    /// Truncating and stripping control characters keeps a pathological
    /// message from becoming a log line nobody can read, and keeps the
    /// variant honest about being a *kind* with a hint rather than a payload.
    #[must_use]
    pub fn internal(message: &str) -> Self {
        const MAX: usize = 200;
        let cleaned: String = message
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX)
            .collect();
        Self::Internal(cleaned)
    }
}

/// Turn a view of a frame into text (§3.1).
pub trait TextRecognizer: Send + Sync {
    /// Recognize the text in `view`.
    ///
    /// # Errors
    ///
    /// [`OcrError`] if the engine is unavailable, a language is missing, the
    /// engine fails, or recognition exceeds [`RECOGNIZE_TIMEOUT`].
    fn recognize(
        &self,
        view: &FrameView<'_>,
        opts: &RecognizeOptions,
    ) -> Result<Recognized, OcrError>;
}

/// A recognizer for targets butler-ai does not ship on (§2).
///
/// It refuses rather than returning empty text. An empty `Recognized` would
/// look to spec 008 like a screen with nothing on it, which is a claim; this
/// is an admission.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullRecognizer;

impl TextRecognizer for NullRecognizer {
    fn recognize(
        &self,
        _view: &FrameView<'_>,
        _opts: &RecognizeOptions,
    ) -> Result<Recognized, OcrError> {
        Err(OcrError::EngineUnavailable)
    }
}

/// The size a view is given to the engine at, and the factor applied
/// (§3.3, FR-005).
///
/// Pure, so the downscale decision is testable on every target even though
/// the resampling itself lives in the platform modules.
#[must_use]
pub fn engine_scale(view: Size) -> (Size, f64) {
    let longest = view.width.max(view.height);
    if longest <= MAX_ENGINE_SIDE || longest == 0 {
        return (view, 1.0);
    }
    let factor = f64::from(MAX_ENGINE_SIDE) / f64::from(longest);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "each product is a screen dimension scaled *down* toward \
                  MAX_ENGINE_SIDE, so it is positive and far below u32::MAX; \
                  `max(1.0)` rules out the zero a degenerate view could give."
    )]
    let scaled = Size {
        width: (f64::from(view.width) * factor).round().max(1.0) as u32,
        height: (f64::from(view.height) * factor).round().max(1.0) as u32,
    };
    (scaled, factor)
}

/// Copy a view into a contiguous RGBA8 buffer, downscaling if asked (§3.3).
///
/// A **box filter**: each destination pixel is the average of the source
/// pixels that map to it. Nearest-neighbour would alias text into noise at
/// exactly the moment the image is large enough to need scaling, which is the
/// moment the text is smallest.
///
/// Shared by both engines rather than written twice. The two platforms differ
/// only in channel order, and `swap_rb` is that difference; duplicating the
/// filter would have been two places for a resampling bug to live, and only
/// one of them testable on any given host.
#[must_use]
pub fn resample_rgba(view: &FrameView<'_>, source: Size, target: Size) -> Vec<u8> {
    let rows: Vec<&[u8]> = view.rows().collect();

    if source == target {
        let mut out = Vec::with_capacity(rows.len() * source.width as usize * 4);
        for row in rows {
            out.extend_from_slice(row);
        }
        return out;
    }

    let mut out = vec![0_u8; target.width as usize * target.height as usize * 4];
    let x_ratio = f64::from(source.width) / f64::from(target.width.max(1));
    let y_ratio = f64::from(source.height) / f64::from(target.height.max(1));

    for ty in 0..target.height as usize {
        let (y0, y1) = span(ty, y_ratio, rows.len());
        for tx in 0..target.width as usize {
            let (x0, x1) = span(tx, x_ratio, source.width as usize);

            let mut sums = [0_u32; 4];
            let mut count = 0_u32;
            for row in rows.iter().take(y1).skip(y0) {
                for x in x0..x1 {
                    let base = x * 4;
                    if let Some(pixel) = row.get(base..base + 4) {
                        for (channel, sum) in sums.iter_mut().enumerate() {
                            *sum += u32::from(pixel[channel]);
                        }
                        count += 1;
                    }
                }
            }

            if count > 0 {
                let base = (ty * target.width as usize + tx) * 4;
                for (channel, sum) in sums.iter().enumerate() {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "a mean of u8 values is a u8"
                    )]
                    {
                        out[base + channel] = (*sum / count) as u8;
                    }
                }
            }
        }
    }
    out
}

/// The half-open source span one destination index covers.
///
/// Always at least one pixel wide, so a destination pixel can never average
/// nothing and come out black.
fn span(index: usize, ratio: f64, limit: usize) -> (usize, usize) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "index and ratio are both bounded by screen dimensions, so \
                  the products are far below both f64's exact-integer range \
                  and usize::MAX, and neither can be negative"
    )]
    let (start, end) = {
        let start = (index as f64 * ratio) as usize;
        let end = ((index + 1) as f64 * ratio) as usize;
        (start, end)
    };
    (start, end.min(limit).max(start + 1))
}

/// Swap the red and blue channels in place.
///
/// The frame is RGBA8 (spec 006 §3.2); `Windows.Media.Ocr` wants BGRA8. This
/// is the whole of the difference between the two engines' input paths.
pub fn swap_rb(buffer: &mut [u8]) {
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

/// The engine this build recognizes with.
#[must_use]
pub const fn engine_id() -> EngineId {
    #[cfg(target_os = "macos")]
    {
        EngineId::AppleVision
    }
    #[cfg(target_os = "windows")]
    {
        EngineId::WindowsMediaOcr
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        EngineId::None
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_ENGINE_SIDE, OcrError, RECOGNIZE_TIMEOUT, RecognizeOptions, engine_scale};
    use crate::recognized::Size;

    #[test]
    fn the_budget_is_the_one_the_spec_names() {
        assert_eq!(RECOGNIZE_TIMEOUT, std::time::Duration::from_millis(1500));
        assert_eq!(MAX_ENGINE_SIDE, 4096);
    }

    #[test]
    fn the_defaults_are_the_ones_the_spec_names() {
        let opts = RecognizeOptions::default();
        assert_eq!(opts.languages.len(), 1);
        assert!(!opts.fast);
        assert!((opts.min_confidence - 0.5).abs() < f32::EPSILON);
    }

    /// FR-005: a view within the budget is untouched; one beyond it is
    /// scaled so the longest side lands exactly on the cap.
    #[test]
    fn fr_005_only_an_oversized_view_is_downscaled() {
        let small = Size {
            width: 1920,
            height: 1080,
        };
        let (size, factor) = engine_scale(small);
        assert_eq!(size, small);
        assert!((factor - 1.0).abs() < f64::EPSILON);

        let huge = Size {
            width: 8192,
            height: 4096,
        };
        let (size, factor) = engine_scale(huge);
        assert_eq!(size.width, MAX_ENGINE_SIDE);
        assert_eq!(size.height, 2048);
        assert!((factor - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_degenerate_view_does_not_divide_by_zero() {
        let empty = Size {
            width: 0,
            height: 0,
        };
        let (size, factor) = engine_scale(empty);
        assert_eq!(size, empty);
        assert!((factor - 1.0).abs() < f64::EPSILON);
    }

    /// The shared resampler copies a view faithfully when no scaling is
    /// needed. Shared by both engines, so this covers the Windows path on a
    /// macOS host too, which is the point of having one of them.
    #[test]
    fn an_unscaled_view_is_copied_verbatim() {
        use butler_capture::{MonitorId, Rect, test_support::frame_from_pattern};

        let frame = frame_from_pattern(MonitorId(0), 4, 3, 1.0, 0x5A);
        let view = frame
            .crop(Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 3,
            })
            .expect("full-frame crop");
        let size = Size {
            width: 4,
            height: 3,
        };

        let out = super::resample_rgba(&view, size, size);
        assert_eq!(out.len(), 4 * 3 * 4);
        assert!(out.iter().all(|b| *b == 0x5A));
    }

    /// A halved view averages, and every destination pixel gets a value: a
    /// span that covered nothing would come out black, which on text is the
    /// difference between smaller words and no words.
    #[test]
    fn a_downscaled_view_averages_and_leaves_no_hole() {
        use butler_capture::{MonitorId, Rect, test_support::frame_from_pattern};

        let frame = frame_from_pattern(MonitorId(0), 8, 8, 1.0, 0x40);
        let view = frame
            .crop(Rect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            })
            .expect("full-frame crop");

        let out = super::resample_rgba(
            &view,
            Size {
                width: 8,
                height: 8,
            },
            Size {
                width: 4,
                height: 4,
            },
        );
        assert_eq!(out.len(), 4 * 4 * 4);
        assert!(
            out.iter().all(|b| *b == 0x40),
            "a uniform source must average to the same uniform value"
        );
    }

    /// The one difference between the two engines' input paths.
    #[test]
    fn swap_rb_exchanges_the_outer_channels_only() {
        let mut pixels = vec![1_u8, 2, 3, 4, 5, 6, 7, 8];
        super::swap_rb(&mut pixels);
        assert_eq!(pixels, vec![3, 2, 1, 4, 7, 6, 5, 8]);
    }

    /// §3.1: an engine message is a hint, not a payload.
    #[test]
    fn an_internal_message_is_sanitized_and_bounded() {
        let noisy = format!("bad\u{0}thing\n{}", "x".repeat(500));
        let OcrError::Internal(message) = OcrError::internal(&noisy) else {
            panic!("internal() must build Internal");
        };
        assert!(message.chars().count() <= 200);
        assert!(!message.contains('\u{0}'));
        assert!(!message.contains('\n'));
    }
}
