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

use std::time::Instant;

use butler_capture::FrameView;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::core::HSTRING;

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

        let result = engine
            .RecognizeAsync(&bitmap)
            .and_then(|op| op.get())
            .map_err(|e| OcrError::internal(&e.message()))?;

        // §3.1: reported as a timeout rather than handed to a state machine
        // that has moved on.
        if started.elapsed() > RECOGNIZE_TIMEOUT {
            return Err(OcrError::Timeout);
        }

        let mut lines = Vec::new();
        let ocr_lines = result
            .Lines()
            .map_err(|e| OcrError::internal(&e.message()))?;
        for line in &ocr_lines {
            let text = line
                .Text()
                .map_err(|e| OcrError::internal(&e.message()))?
                .to_string_lossy();
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

/// Create an engine for the requested language, falling back to the user's
/// profile languages (§3.3).
fn create_engine(opts: &RecognizeOptions) -> Result<OcrEngine, OcrError> {
    for tag in &opts.languages {
        let language = Language::CreateLanguage(&HSTRING::from(tag.as_str()))
            .map_err(|e| OcrError::internal(&e.message()))?;
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
    let buffer = CryptographicBuffer::CreateFromByteArray(bgra)
        .map_err(|e| OcrError::internal(&e.message()))?;

    SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        size.width.try_into().unwrap_or(i32::MAX),
        size.height.try_into().unwrap_or(i32::MAX),
    )
    .or_else(|_| {
        // Some builds require the alpha mode to be named explicitly.
        SoftwareBitmap::CreateCopyFromBuffer2(
            &buffer,
            BitmapPixelFormat::Bgra8,
            size.width.try_into().unwrap_or(i32::MAX),
            size.height.try_into().unwrap_or(i32::MAX),
            BitmapAlphaMode::Premultiplied,
        )
    })
    .map_err(|e| OcrError::internal(&e.message()))
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
    let sx = source.width as f32 / engine.width.max(1) as f32;
    let sy = source.height as f32 / engine.height.max(1) as f32;

    Rect {
        x: (left * sx).round().max(0.0) as u32,
        y: (top * sy).round().max(0.0) as u32,
        width: ((right - left) * sx).round().max(1.0) as u32,
        height: ((bottom - top) * sy).round().max(1.0) as u32,
    }
}
