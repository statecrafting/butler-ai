// Spec: specs/006-screen-capture/spec.md

//! Snapshot capture of one monitor (spec 006).
//!
//! This crate knows how to take **one picture, now**, and nothing else. When
//! to take it, how often, and what to do when it fails are the state
//! machine's (spec 009), which is why nothing here depends on that crate and
//! why the machine can be tested without a screen.
//!
//! The other reason the crate is this small is spec 015: the privacy boundary
//! needs exactly one type to constrain, and [`frame::Frame`] is it.
//!
//! # Platforms
//!
//! `xcap` backs [`xcap_source::XcapSource`] on Windows and macOS. Everywhere
//! else the crate still compiles, to the trait and
//! [`source::NullSource`], so this crate's lifecycle tests and
//! `butler-core`'s suite run on a Linux runner (§2).

pub mod frame;
pub mod monitor;
pub mod source;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub mod xcap_source;

pub use frame::{CropError, Frame, FrameView, Rect};
pub use monitor::{Bounds, MonitorId, MonitorInfo};
pub use source::{CAPTURE_TIMEOUT, CaptureError, NullSource, ScreenSource};

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use xcap_source::XcapSource;

/// Constructors for the lifecycle tests, and nothing else.
///
/// `Frame::new` is `pub(crate)` so that nothing outside this crate can turn
/// arbitrary pixels into a `Frame` and inherit its guarantees without earning
/// them (§3.2). `tests/frame_lifecycle.rs` is an integration test and so is
/// *outside*, which is why this exists.
///
/// It is deliberately not a hole in that rule: the function **generates** its
/// buffer from a single byte and takes no pixels from the caller, so it
/// cannot be used to smuggle a real screen into a `Frame`.
#[doc(hidden)]
pub mod test_support {
    use crate::frame::Frame;
    use crate::monitor::MonitorId;

    /// A frame of `width * height` pixels, every byte `byte`.
    #[must_use]
    pub fn frame_from_pattern(
        monitor: MonitorId,
        width: u32,
        height: u32,
        scale: f32,
        byte: u8,
    ) -> Frame {
        let pixels = vec![byte; (width as usize) * (height as usize) * 4].into_boxed_slice();
        Frame::new(monitor, width, height, scale, pixels)
    }
}

/// The source this build ships with.
///
/// One name for "the real one on a shipped platform, the honest refusal
/// everywhere else", so callers do not each write the same `cfg`.
#[cfg(any(target_os = "windows", target_os = "macos"))]
#[must_use]
pub fn platform_source() -> XcapSource {
    XcapSource::new()
}

/// The source this build ships with (see the shipped-platform version).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[must_use]
pub fn platform_source() -> NullSource {
    NullSource
}
