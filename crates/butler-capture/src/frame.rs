// Spec: specs/006-screen-capture/spec.md

//! The frame, and the first link in the privacy chain.
//!
//! Spec 015 constrains this file. A [`Frame`] is the user's screen: their
//! mail, their bank, their employer's documents. Four properties keep it
//! from becoming anything else, and each is a type-level fact rather than a
//! convention:
//!
//! - **No `Serialize`.** There is no derive and no hand-written impl, so a
//!   frame cannot be turned into bytes by any code that takes a `Serialize`.
//! - **No `Clone`.** One frame, one owner, one drop. A clone would double the
//!   number of copies that have to be zeroed and halve the chance both are.
//! - **`Debug` without pixels.** Written by hand, so a `{:?}` in a log line
//!   prints dimensions and never content (spec 015 §3.5).
//! - **Zeroed on drop.** `Drop` overwrites the buffer before it is freed, so
//!   the screen does not survive in memory the allocator hands to someone
//!   else.
//!
//! `captured_at` is an [`Instant`] and never a wall-clock time. A frame that
//! carried "the user's screen looked like this at 14:32 on Tuesday" would be
//! a different artefact from one that carried "this was 400 ms ago".

use std::time::Instant;

use zeroize::Zeroize as _;

use crate::monitor::MonitorId;

/// One captured monitor (spec 006 §3.2).
///
/// Constructed only by a [`crate::source::ScreenSource`]; there is no public
/// constructor that takes pixels from anywhere else.
///
/// # FR-003: what cannot be done with one
///
/// The pixels are private, so no caller can take the buffer out and write it
/// somewhere:
///
/// ```compile_fail
/// use butler_capture::{MonitorId, test_support::frame_from_pattern};
/// let frame = frame_from_pattern(MonitorId(0), 2, 2, 1.0, 7);
/// // `pixels` is private: there is no path from a frame to a file.
/// let _stolen = frame.pixels;
/// ```
///
/// And a frame cannot be duplicated, so there is exactly one owner and
/// exactly one drop to zero it:
///
/// ```compile_fail
/// use butler_capture::{MonitorId, test_support::frame_from_pattern};
/// let frame = frame_from_pattern(MonitorId(0), 2, 2, 1.0, 7);
/// let _copy = frame.clone();
/// ```
///
/// The positive control, so a passing `compile_fail` above cannot be passing
/// because an import path is wrong:
///
/// ```
/// use butler_capture::{MonitorId, test_support::frame_from_pattern};
/// let frame = frame_from_pattern(MonitorId(0), 2, 2, 1.0, 7);
/// assert_eq!(frame.as_rgba().len(), 2 * 2 * 4);
/// ```
pub struct Frame {
    /// Which monitor this came from.
    pub monitor: MonitorId,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// The monitor's scale factor. The recognizer decides whether to
    /// downscale (§3.3).
    pub scale: f32,
    /// When it was captured, monotonically. Never wall-clock (§3.2).
    pub captured_at: Instant,
    /// RGBA8, row-major. Private, and the only way out is [`Frame::as_rgba`].
    pixels: Box<[u8]>,
}

impl Frame {
    /// Build a frame from pixels a source just captured.
    ///
    /// `pub(crate)` on purpose: the only callers are the sources in this
    /// crate, so there is no path by which pixels from elsewhere become a
    /// `Frame` and inherit its lifecycle guarantees without earning them.
    ///
    /// # Panics
    ///
    /// If `pixels` is not exactly `width * height * 4` bytes, which would
    /// mean the source and the buffer disagree about the geometry.
    #[must_use]
    pub(crate) fn new(
        monitor: MonitorId,
        width: u32,
        height: u32,
        scale: f32,
        pixels: Box<[u8]>,
    ) -> Self {
        let expected = (width as usize) * (height as usize) * 4;
        assert_eq!(
            pixels.len(),
            expected,
            "a frame's buffer must be exactly width * height * 4 bytes"
        );
        Self {
            monitor,
            width,
            height,
            scale,
            captured_at: Instant::now(),
            pixels,
        }
    }

    /// The pixels, borrowed. RGBA8, row-major.
    #[must_use]
    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// How many bytes the frame holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    /// Whether the frame holds no pixels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Borrow a rectangle for the recognizer (007) or the self-test (005).
    ///
    /// The view borrows, so it cannot outlive the frame and cannot be the
    /// thing that keeps a screen alive after its owner dropped it.
    ///
    /// # Errors
    ///
    /// [`CropError`] if the rectangle leaves the frame.
    pub fn crop(&self, rect: Rect) -> Result<FrameView<'_>, CropError> {
        if rect.width == 0 || rect.height == 0 {
            return Err(CropError::Empty);
        }
        let right = rect
            .x
            .checked_add(rect.width)
            .ok_or(CropError::OutOfBounds)?;
        let bottom = rect
            .y
            .checked_add(rect.height)
            .ok_or(CropError::OutOfBounds)?;
        if right > self.width || bottom > self.height {
            return Err(CropError::OutOfBounds);
        }
        Ok(FrameView { frame: self, rect })
    }
}

/// Dimensions only. Spec 015 §3.5: never the content, not even a sample.
impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("monitor", &self.monitor)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("scale", &self.scale)
            .field("bytes", &self.pixels.len())
            .finish_non_exhaustive()
    }
}

/// Zero the buffer before it is freed (§3.2, FR-004).
///
/// Without this the user's screen stays in the heap until something else
/// happens to overwrite it, which is a window an attacker with process
/// access does not have to work for.
impl Drop for Frame {
    fn drop(&mut self) {
        self.pixels.zeroize();
    }
}

/// A rectangle in frame pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Why a crop was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CropError {
    /// The rectangle leaves the frame.
    #[error("the rectangle leaves the frame")]
    OutOfBounds,
    /// The rectangle has no area.
    #[error("the rectangle is empty")]
    Empty,
}

/// A borrowed rectangle of a [`Frame`] (§3.2).
///
/// It cannot outlive the frame, by the lifetime, which is what stops a view
/// from being the thing that keeps a screen alive.
#[derive(Debug)]
pub struct FrameView<'a> {
    frame: &'a Frame,
    rect: Rect,
}

impl FrameView<'_> {
    /// The rectangle this view covers.
    #[must_use]
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// The view's rows, each `width * 4` bytes of RGBA8.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        let stride = self.frame.width as usize * 4;
        let start_x = self.rect.x as usize * 4;
        let end_x = start_x + self.rect.width as usize * 4;
        (self.rect.y..self.rect.y + self.rect.height).map(move |y| {
            let row = y as usize * stride;
            &self.frame.pixels[row + start_x..row + end_x]
        })
    }
}
