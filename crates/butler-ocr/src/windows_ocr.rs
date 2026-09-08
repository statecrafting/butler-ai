// Spec: specs/007-text-recognition/spec.md

//! `Windows.Media.Ocr` (spec 007 §3.3).
//!
//! The engine is the operating system's: no model ships with butler-ai and
//! nothing is downloaded. The app uses whatever language packs the user
//! already has and surfaces [`OcrError::LanguageUnsupported`] when it has
//! none for a requested tag (§6).
//!
//! # No confidence
//!
//! `Windows.Media.Ocr` reports **no confidence**, per line or per word. §3.2's
//! `Line::confidence` therefore carries `1.0` here, and §3.4 rule 1's
//! confidence filter is inert on this platform. Spec 007 D-3 records what
//! that costs and why inventing a number would be worse.

use std::time::{Duration, Instant};

use butler_capture::FrameView;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::core::{Error as WinError, HSTRING};
use windows_future::{AsyncStatus, IAsyncOperation};

use crate::recognized::{EngineId, Line, Recognized, Rect, Size};
use crate::recognizer::{
    OcrError, RECOGNIZE_TIMEOUT, RecognizeOptions, TextRecognizer, engine_scale, resample_rgba,
    swap_rb,
};

/// The confidence reported for every line (see the module note).
const ASSUMED_CONFIDENCE: f32 = 1.0;

/// The Windows recognizer (§3.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsOcrRecognizer;

impl WindowsOcrRecognizer {
    /// A recognizer. Stateless: the engine is created per call, because a
    /// language change in settings must take effect without a restart.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextRecognizer for WindowsOcrRecognizer {
    fn recognize(
        &self,
        view: &FrameView<'_>,
        opts: &RecognizeOptions,
    ) -> Result<Recognized, OcrError> {
        let started = Instant::now();
        let rect = view.rect();
        let source = Size {
            width: rect.width,
            height: rect.height,
        };

        // §3.3 and FR-005.
        let (engine_size, _factor) = engine_scale(source);
        let mut bgra = resample_rgba(view, source, engine_size);
        swap_rb(&mut bgra);

        let engine = create_engine(opts)?;
        let bitmap = build_bitmap(&bgra, engine_size)?;

        let operation = engine.RecognizeAsync(&bitmap).map_err(win)?;
        let remaining = RECOGNIZE_TIMEOUT.saturating_sub(started.elapsed());
        let result = await_operation(&operation, remaining)?;

        let mut lines = Vec::new();
        let ocr_lines = result.Lines().map_err(win)?;
        for line in &ocr_lines {
            let text = line.Text().map_err(win)?.to_string_lossy();
            if text.is_empty() {
                continue;
            }
            lines.push(Line {
                text,
                bbox: line_bounds(&line, source, engine_size),
                confidence: ASSUMED_CONFIDENCE,
            });
        }

        Ok(Recognized::from_lines(
            lines,
            source,
            EngineId::WindowsMediaOcr,
            opts.min_confidence,
        ))
    }
}

/// Map a `windows` error into ours, sanitized (§3.1).
#[allow(
    clippy::needless_pass_by_value,
    reason = "used as `map_err(win)`, which hands the error over by value. \
              Taking a reference would force a closure at every one of the \
              call sites this function exists to remove."
)]
fn win(error: WinError) -> OcrError {
    OcrError::internal(&error.message())
}

/// Block until the operation finishes, or until the budget runs out (§3.1).
///
/// `windows-future` offers no blocking accessor: an `IAsyncOperation` is
/// awaited, and this crate's trait is synchronous by design because the
/// runtime already calls it from a blocking task (spec 019). Polling `Status`
/// is the supported way to wait without an executor, and it makes §3.1's
/// budget a real deadline rather than a check made after the engine already
/// took as long as it liked.
///
/// The poll interval is deliberately short relative to the budget: 5 ms is
/// under half a percent of 1500 ms, so the wait costs at most that much
/// beyond the engine's own time.
fn await_operation(
    operation: &IAsyncOperation<OcrResult>,
    budget: Duration,
) -> Result<OcrResult, OcrError> {
    const POLL: Duration = Duration::from_millis(5);
    let deadline = Instant::now() + budget;

    loop {
        match operation.Status().map_err(win)? {
            AsyncStatus::Completed => return operation.GetResults().map_err(win),
            AsyncStatus::Error => {
                return Err(operation
                    .ErrorCode()
                    .map_or_else(win, |code| OcrError::internal(&format!("{code:?}"))));
            }
            AsyncStatus::Canceled => return Err(OcrError::internal("recognition was cancelled")),
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(OcrError::Timeout);
        }
        std::thread::sleep(POLL);
    }
}

/// Create an engine for the requested language, falling back to the user's
/// profile languages (§3.3).
fn create_engine(opts: &RecognizeOptions) -> Result<OcrEngine, OcrError> {
    for tag in &opts.languages {
        let language = Language::CreateLanguage(&HSTRING::from(tag.as_str())).map_err(win)?;
        if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
            return Ok(engine);
        }
    }

    // §3.3's fallback. A user whose profile language differs from the app's
    // default still gets recognition rather than an error.
    OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
        opts.languages
            .first()
            .map_or(OcrError::EngineUnavailable, |tag| {
                OcrError::LanguageUnsupported(tag.as_str().to_owned())
            })
    })
}

/// Build a BGRA8 `SoftwareBitmap` over the resampled buffer.
fn build_bitmap(bgra: &[u8], size: Size) -> Result<SoftwareBitmap, OcrError> {
    let buffer = CryptographicBuffer::CreateFromByteArray(bgra).map_err(win)?;

    SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        i32::try_from(size.width).unwrap_or(i32::MAX),
        i32::try_from(size.height).unwrap_or(i32::MAX),
    )
    .map_err(win)
}

/// The union of a line's word boxes, in original frame pixels (FR-005).
///
/// `OcrLine` has no bounding box of its own; only its words do. The union is
/// the line's extent, which is what §3.4's reading order needs.
fn line_bounds(line: &windows::Media::Ocr::OcrLine, source: Size, engine: Size) -> Rect {
    let Ok(words) = line.Words() else {
        return Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
    };

    let (mut left, mut top) = (f32::MAX, f32::MAX);
    let (mut right, mut bottom) = (0.0_f32, 0.0_f32);
    let mut any = false;

    for word in &words {
        if let Ok(rect) = word.BoundingRect() {
            left = left.min(rect.X);
            top = top.min(rect.Y);
            right = right.max(rect.X + rect.Width);
            bottom = bottom.max(rect.Y + rect.Height);
            any = true;
        }
    }

    if !any {
        return Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
    }

    // Back to frame coordinates: the engine saw a possibly downscaled image.
    let sx = f64::from(source.width) / f64::from(engine.width.max(1));
    let sy = f64::from(source.height) / f64::from(engine.height.max(1));

    Rect {
        x: to_px(f64::from(left) * sx),
        y: to_px(f64::from(top) * sy),
        width: to_px(f64::from(right - left) * sx).max(1),
        height: to_px(f64::from(bottom - top) * sy).max(1),
    }
}

/// Round an engine coordinate to a frame pixel.
///
/// One place for the narrowing, for the reason the macOS path gives: the
/// inputs are bounded by the frame's own dimensions, and clamping means a
/// pathological box is wrong rather than wrapped.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded by the frame's dimensions and clamped at both ends"
)]
fn to_px(value: f64) -> u32 {
    value.round().clamp(0.0, f64::from(u32::MAX)) as u32
}
