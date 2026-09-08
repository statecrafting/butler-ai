// Spec: specs/007-text-recognition/spec.md

//! Apple Vision (spec 007 §3.3).
//!
//! `VNRecognizeTextRequest` on a `CGImage` built from the frame view. The
//! engine is the operating system's: no model ships with butler-ai, nothing
//! is downloaded, and no pixels leave the process.
//!
//! # `unsafe`, and where it is
//!
//! Spec 001 FR-005 denies `unsafe` workspace-wide and allows it in a platform
//! crate under `#[allow(unsafe_code)]` on the smallest possible block with a
//! `// SAFETY:` note. Three blocks here need it: building the `CGImage`,
//! running the request, and reading the observations back. Each is annotated
//! where it sits.

use std::time::Instant;

use butler_capture::FrameView;
use objc2::AnyThread as _;
use objc2::rc::Retained;
use objc2_core_foundation::CGRect;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
};
use objc2_foundation::{NSArray, NSString};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequestTextRecognitionLevel};

use crate::recognized::{EngineId, Line, Recognized, Rect, Size};
use crate::recognizer::{
    OcrError, RECOGNIZE_TIMEOUT, RecognizeOptions, TextRecognizer, engine_scale, resample_rgba,
};

/// `VNRequestTextRecognitionLevelAccurate`.
const LEVEL_ACCURATE: isize = 0;
/// `VNRequestTextRecognitionLevelFast`.
const LEVEL_FAST: isize = 1;

/// The macOS recognizer (§3.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct AppleVisionRecognizer;

impl AppleVisionRecognizer {
    /// A recognizer. Stateless: Vision holds the model, not this type.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextRecognizer for AppleVisionRecognizer {
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

        // §3.3 and FR-005. The engine sees at most `MAX_ENGINE_SIDE`; the
        // factor is what puts the boxes back in frame coordinates below.
        let (engine_size, factor) = engine_scale(source);
        let rgba = resample_rgba(view, source, engine_size);

        let image = build_cg_image(&rgba, engine_size)?;
        let request = build_request(opts);

        // SAFETY: `image` is a live `CGImage` this function owns for the
        // duration of the call, and the handler borrows it only while
        // performing. The empty options dictionary is what Vision expects
        // when no orientation hint is supplied.
        #[allow(unsafe_code)]
        let handler = unsafe {
            VNImageRequestHandler::initWithCGImage_options(
                VNImageRequestHandler::alloc(),
                &image,
                &objc2_foundation::NSDictionary::new(),
            )
        };

        // Two upcasts: VNRecognizeTextRequest -> VNImageBasedRequest ->
        // VNRequest, which is what the handler takes.
        let requests = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(
            request.clone(),
        ))]);
        handler.performRequests_error(&requests).map_err(
            |e: Retained<objc2_foundation::NSError>| {
                OcrError::internal(&e.localizedDescription().to_string())
            },
        )?;

        // §3.1: a slow engine is reported as a timeout rather than handed to
        // a state machine that has moved on. Checked after the fact because
        // Vision has no cancellation that would beat its own work.
        if started.elapsed() > RECOGNIZE_TIMEOUT {
            return Err(OcrError::Timeout);
        }

        let lines = read_observations(&request, source, factor);
        Ok(Recognized::from_lines(
            lines,
            source,
            EngineId::AppleVision,
            opts.min_confidence,
        ))
    }
}

/// Configure the request from the caller's options (§3.3).
fn build_request(opts: &RecognizeOptions) -> Retained<VNRecognizeTextRequest> {
    let request = VNRecognizeTextRequest::new();

    request.setRecognitionLevel(VNRequestTextRecognitionLevel(if opts.fast {
        LEVEL_FAST
    } else {
        LEVEL_ACCURATE
    }));
    // §3.3: language correction on. The engine's dictionary fixes the
    // character confusions that would otherwise make two captures of one
    // screen normalize differently.
    request.setUsesLanguageCorrection(true);

    if !opts.languages.is_empty() {
        let tags: Vec<Retained<NSString>> = opts
            .languages
            .iter()
            .map(|tag| NSString::from_str(tag.as_str()))
            .collect();
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&tags));
    }

    request
}

/// Build a `CGImage` over an RGBA8 buffer.
fn build_cg_image(
    rgba: &[u8],
    size: Size,
) -> Result<objc2_core_foundation::CFRetained<CGImage>, OcrError> {
    use objc2_core_graphics::CGImageAlphaInfo;

    // SAFETY: `with_data` copies nothing and borrows the pointer for the
    // provider's lifetime, so the buffer must outlive the image. `rgba` is
    // owned by the caller for the whole of `recognize`, and the image is
    // dropped before it. A null release callback is correct precisely because
    // the provider does not own the bytes.
    #[allow(unsafe_code)]
    let provider = unsafe {
        CGDataProvider::with_data(std::ptr::null_mut(), rgba.as_ptr().cast(), rgba.len(), None)
    }
    .ok_or_else(|| OcrError::internal("CGDataProviderCreateWithData returned null"))?;

    let space = CGColorSpace::new_device_rgb()
        .ok_or_else(|| OcrError::internal("CGColorSpaceCreateDeviceRGB returned null"))?;

    // SAFETY: the geometry matches the buffer exactly: 8 bits per component,
    // 32 per pixel, `width * 4` bytes per row, over `width * height * 4`
    // bytes. `decode` is null, which means the identity map.
    #[allow(unsafe_code)]
    let image = unsafe {
        CGImage::new(
            size.width as usize,
            size.height as usize,
            8,
            32,
            size.width as usize * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0),
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| OcrError::internal("CGImageCreate returned null"))?;

    Ok(image)
}

/// Read the request's observations back as lines in frame coordinates.
///
/// Vision reports boxes in a normalized, bottom-left origin space. The frame
/// is top-left origin in pixels, so `y` is flipped here; a caller that had to
/// remember that would be a caller that eventually forgot.
fn read_observations(request: &VNRecognizeTextRequest, frame: Size, _factor: f64) -> Vec<Line> {
    let Some(results) = request.results() else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    for observation in &results {
        // SAFETY: `topCandidates` is a plain Objective-C call on a live
        // observation owned by the results array, which outlives this loop.
        #[allow(unsafe_code)]
        let candidates = observation.topCandidates(1);
        let Some(best) = candidates.firstObject() else {
            continue;
        };

        let text = best.string().to_string();
        if text.is_empty() {
            continue;
        }

        // SAFETY: as above; `boundingBox` and `confidence` are property
        // reads on a live observation.
        #[allow(unsafe_code)]
        let (bbox, confidence) = unsafe { (observation.boundingBox(), observation.confidence()) };

        lines.push(Line {
            text,
            bbox: normalized_to_frame(bbox, frame),
            confidence,
        });
    }
    lines
}

/// Vision's normalized, bottom-left-origin rect to frame pixels.
fn normalized_to_frame(bbox: CGRect, frame: Size) -> Rect {
    let width = f64::from(frame.width);
    let height = f64::from(frame.height);

    Rect {
        x: to_px((bbox.origin.x * width).max(0.0)),
        // Flip: Vision's origin is bottom-left, the frame's is top-left.
        y: to_px(((1.0 - bbox.origin.y - bbox.size.height) * height).max(0.0)),
        width: to_px((bbox.size.width * width).max(1.0)),
        height: to_px((bbox.size.height * height).max(1.0)),
    }
}

/// Round a frame-space coordinate to a pixel.
///
/// One place for the narrowing, with one reason: the inputs are Vision's
/// normalized coordinates multiplied by a frame dimension, so they are
/// bounded by the frame and cannot approach `u32::MAX`. Clamping rather than
/// wrapping means a pathological observation gives a wrong box rather than a
/// wrapped one.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded by the frame's own dimensions and clamped at both ends"
)]
fn to_px(value: f64) -> u32 {
    value.round().clamp(0.0, f64::from(u32::MAX)) as u32
}
