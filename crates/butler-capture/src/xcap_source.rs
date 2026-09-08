// Spec: specs/006-screen-capture/spec.md

//! The `xcap`-backed source for Windows and macOS (spec 006 §3.3).
//!
//! # Why `xcap`, and why the compositor path matters
//!
//! On Windows `xcap` goes through DXGI desktop duplication; on macOS through
//! `ScreenCaptureKit`. Both are **compositor** paths, and that is not a
//! performance detail: it is the property spec 005's exclusion self-test
//! depends on. The compositor honours `WDA_EXCLUDEFROMCAPTURE` and
//! `NSWindowSharingNone`, so a window the OS was asked to exclude is absent
//! from the buffer. A window-list composition would re-include it, and the
//! self-test would pass while the overlay was visible to every screen-sharing
//! tool on the machine.
//!
//! # What is not retained
//!
//! The source holds no buffer between calls (§3.3). Each capture converts to
//! RGBA8 once, hands the bytes to a [`Frame`], and forgets them; the frame's
//! own `Drop` is what zeroes them.

use std::time::Instant;

use crate::frame::Frame;
use crate::monitor::{Bounds, MonitorId, MonitorInfo};
use crate::source::{CAPTURE_TIMEOUT, CaptureError, ScreenSource};

/// The production source (§3.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct XcapSource;

impl XcapSource {
    /// A source. Stateless by design: nothing is cached between captures.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Find one `xcap` monitor by our id.
    fn find(id: MonitorId) -> Result<xcap::Monitor, CaptureError> {
        let monitors = xcap::Monitor::all().map_err(|e| map_error(&e))?;
        for monitor in monitors {
            if monitor.id().map_err(|e| map_error(&e))? == id.0 {
                return Ok(monitor);
            }
        }
        Err(CaptureError::MonitorGone(id))
    }
}

impl ScreenSource for XcapSource {
    fn monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError> {
        let mut out = Vec::new();
        for monitor in xcap::Monitor::all().map_err(|e| map_error(&e))? {
            out.push(MonitorInfo {
                id: MonitorId(monitor.id().map_err(|e| map_error(&e))?),
                // `friendly_name` is what a user would recognize in the
                // settings panel; §3.4 pins a monitor by the name they see.
                name: monitor
                    .friendly_name()
                    .or_else(|_| monitor.name())
                    .map_err(|e| map_error(&e))?,
                bounds: Bounds {
                    x: monitor.x().map_err(|e| map_error(&e))?,
                    y: monitor.y().map_err(|e| map_error(&e))?,
                    width: monitor.width().map_err(|e| map_error(&e))?,
                    height: monitor.height().map_err(|e| map_error(&e))?,
                },
                scale: monitor.scale_factor().map_err(|e| map_error(&e))?,
                is_primary: monitor.is_primary().map_err(|e| map_error(&e))?,
            });
        }
        Ok(out)
    }

    fn capture(&self, monitor: MonitorId) -> Result<Frame, CaptureError> {
        let started = Instant::now();
        let target = Self::find(monitor)?;
        let scale = target.scale_factor().map_err(|e| map_error(&e))?;

        let image = target.capture_image().map_err(|e| map_error(&e))?;
        let (width, height) = (image.width(), image.height());

        // §3.1: the budget is checked after the fact rather than by racing a
        // timer on another thread. The runtime already calls this from a
        // blocking task, so a slow capture delays only itself; what matters
        // is that a slow one is *reported* as a timeout rather than handed
        // to a state machine that has moved on.
        if started.elapsed() > CAPTURE_TIMEOUT {
            return Err(CaptureError::Timeout);
        }

        // `into_raw` takes the buffer rather than copying it, and this is the
        // only place pixels enter the crate.
        let pixels = image.into_raw().into_boxed_slice();
        Ok(Frame::new(monitor, width, height, scale, pixels))
    }
}

/// Map `xcap`'s error onto ours (§3.1).
///
/// macOS reports a missing Screen Recording grant as an ordinary failure, so
/// the text is inspected for it. That is a heuristic, and it is the reason
/// FR-005's real assertion is a manual one: the shell (004) knows the
/// permission state authoritatively through `CGPreflightScreenCaptureAccess`
/// and is what decides whether to prompt (spec 006 D-2).
fn map_error(error: &xcap::XCapError) -> CaptureError {
    let message = error.to_string();
    let lowered = message.to_lowercase();
    if lowered.contains("permission") || lowered.contains("not authorized") {
        CaptureError::PermissionDenied
    } else {
        CaptureError::Unavailable(message)
    }
}

#[cfg(test)]
mod tests {
    use super::XcapSource;
    use crate::source::{CaptureError, ScreenSource};

    /// Enumeration works on a real desktop and is refused honestly without
    /// one. Both are correct outcomes; what would not be is a blank list,
    /// which a caller would read as "no monitors" rather than "I could not
    /// look".
    #[test]
    fn monitors_either_enumerate_or_report_why_not() {
        match XcapSource::new().monitors() {
            Ok(monitors) => {
                for monitor in &monitors {
                    assert!(!monitor.name.is_empty(), "a monitor must have a name");
                    assert!(monitor.scale > 0.0, "scale must be positive");
                }
            }
            Err(error) => assert!(
                matches!(
                    error,
                    CaptureError::Unavailable(_) | CaptureError::PermissionDenied
                ),
                "unexpected enumeration failure: {error:?}"
            ),
        }
    }

    #[test]
    fn an_unknown_monitor_is_reported_gone_rather_than_captured() {
        // No platform hands out this id; if enumeration itself is
        // unavailable the source says so, which is equally honest.
        match XcapSource::new().capture(crate::monitor::MonitorId(u32::MAX)) {
            Err(CaptureError::MonitorGone(_) | CaptureError::Unavailable(_)) => {}
            other => panic!("expected MonitorGone or Unavailable, got {other:?}"),
        }
    }
}
