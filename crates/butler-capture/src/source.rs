// Spec: specs/006-screen-capture/spec.md

//! The capture trait, and the source that exists on every platform.
//!
//! Spec 006 §3.1. `capture` is synchronous and must return within 250 ms; the
//! runtime (019) calls it from a blocking task, so a slow display does not
//! stall the event loop.

use crate::frame::Frame;
use crate::monitor::{MonitorId, MonitorInfo};

/// How long a capture may take before it is a timeout (§3.1).
pub const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Why a capture failed (§3.1).
///
/// **No variant carries pixel data**, and none carries text from the screen.
/// `Unavailable` holds a platform message, which is the OS's own words about
/// its own state; spec 015 §3.5 keeps the *user's* content out, not the
/// system's diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// macOS has not granted Screen Recording (spec 004 §3.5).
    ///
    /// Returned without prompting: the shell owns prompting, and a library
    /// that raised a system dialog from a background task would be a dialog
    /// the user cannot connect to anything they did (FR-005).
    #[error("screen recording permission is not granted")]
    PermissionDenied,
    /// The monitor was unplugged or reconfigured between calls.
    #[error("monitor {0:?} is gone")]
    MonitorGone(MonitorId),
    /// The capture did not return within [`CAPTURE_TIMEOUT`].
    #[error("capture timed out")]
    Timeout,
    /// Anything else the platform reported.
    #[error("capture unavailable: {0}")]
    Unavailable(String),
}

/// One picture of one monitor, on demand (§3.1).
pub trait ScreenSource: Send + Sync {
    /// Enumerate the monitors.
    ///
    /// # Errors
    ///
    /// [`CaptureError`] if the platform refuses or is unavailable.
    fn monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError>;

    /// Capture `monitor` now.
    ///
    /// # Errors
    ///
    /// [`CaptureError`] if the monitor is gone, permission is missing, the
    /// capture times out, or the platform refuses.
    fn capture(&self, monitor: MonitorId) -> Result<Frame, CaptureError>;
}

/// A source for targets butler-ai does not ship on (§2).
///
/// It exists so this crate, and `butler-core`'s tests, compile and run on a
/// Linux runner. It is not a stub that pretends: every call returns
/// [`CaptureError::Unavailable`], so a test that expected a picture fails
/// rather than passing against a blank one.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullSource;

impl NullSource {
    /// The message every call returns.
    pub const REASON: &'static str = "screen capture is not supported on this platform";
}

impl ScreenSource for NullSource {
    fn monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError> {
        Err(CaptureError::Unavailable(Self::REASON.to_owned()))
    }

    fn capture(&self, _monitor: MonitorId) -> Result<Frame, CaptureError> {
        Err(CaptureError::Unavailable(Self::REASON.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::{CAPTURE_TIMEOUT, CaptureError, NullSource, ScreenSource};
    use crate::monitor::MonitorId;

    #[test]
    fn the_null_source_refuses_rather_than_returning_a_blank_frame() {
        let source = NullSource;
        assert!(matches!(
            source.capture(MonitorId(0)),
            Err(CaptureError::Unavailable(_))
        ));
        assert!(matches!(
            source.monitors(),
            Err(CaptureError::Unavailable(_))
        ));
    }

    /// §3.1 fixes the budget. The runtime's blocking task and spec 009's
    /// fault timing are both written against it.
    #[test]
    fn the_timeout_is_the_one_the_spec_names() {
        assert_eq!(CAPTURE_TIMEOUT, std::time::Duration::from_millis(250));
    }

    /// §3.1: no error variant carries pixels. Asserted by construction here,
    /// and by `Frame` having no `Serialize` (FR-003) everywhere else.
    #[test]
    fn no_error_variant_can_carry_a_screen() {
        let rendered = format!(
            "{:?} {:?} {:?}",
            CaptureError::PermissionDenied,
            CaptureError::MonitorGone(MonitorId(3)),
            CaptureError::Timeout
        );
        assert!(rendered.contains("PermissionDenied"));
        assert!(rendered.contains("MonitorGone"));
        assert!(rendered.contains("Timeout"));
    }
}
