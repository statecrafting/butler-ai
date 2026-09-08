// Spec: specs/005-capture-exclusion/spec.md

//! The self-test (spec 005 §3.4).
//!
//! Render a sentinel into the overlay, capture the monitor through the same
//! compositor path recording tools use (spec 006), and look for the
//! sentinel's colours in the overlay's rectangle. Finding them means the
//! exclusion did not take, whatever the OS reported.
//!
//! This is the measurement constitution §VI asks for. Everything else in this
//! module is a request; only this looks at a real frame.
//!
//! # The two halves
//!
//! **Detection is pure** and lives in [`match_ratio`]: given pixels and a
//! rectangle, how much of it looks like the sentinel. It is tested in both
//! directions on synthetic frames, on every target, which is what FR-003
//! means by an assertion that cannot pass vacuously.
//!
//! **Orchestration is not pure**: it needs a window, a webview that will
//! render the sentinel, and a screen to capture. That half is exercised on a
//! real desktop and is recorded as a deferred checklist row (D-3).

use std::time::Duration;

use butler_capture::{CaptureError, Frame, MonitorId, Rect, ScreenSource};
use butler_core::ipc::UiEvent;
use tauri::{Manager as _, Runtime, WebviewWindow};

use super::{ExclusionMethod, ExclusionStatus};

/// The sentinel's two colours, as RGB.
///
/// These must match `apps/desktop/src/components/Sentinel.tsx` exactly: the
/// overlay paints them and this file looks for them. Saturated magenta and
/// green appear in no other part of the UI, and neither is a colour a
/// wallpaper or a document is likely to produce across a whole rectangle.
pub const SENTINEL_RGB: [[u8; 3]; 2] = [[0xFF, 0x00, 0xFF], [0x00, 0xFF, 0x00]];

/// How far a captured pixel may drift and still count as the sentinel.
///
/// Capture paths rescale, and a downscaled checkerboard blends its two
/// colours at the boundaries. A tolerance this wide still excludes ordinary
/// content: no common UI or wallpaper colour sits within 24 of pure magenta
/// *and* differs from it in the way an interpolated sentinel does.
pub const CHANNEL_TOLERANCE: u8 = 24;

/// Above this fraction of matching pixels, the overlay was visible (§3.4).
///
/// Half a percent. Low enough that a partially visible overlay is caught,
/// high enough that a stray magenta pixel in the user's own content does not
/// declare a working exclusion broken.
pub const SENTINEL_MATCH_THRESHOLD: f64 = 0.005;

/// The alpha the sentinel is rendered at.
///
/// §3.4 asks for "the lowest alpha the capture path still resolves", so the
/// user sees the test for as little as possible. **This is full opacity**,
/// and deliberately, until someone measures the real floor on both platforms
/// (AC-3, D-2).
///
/// The two errors are not symmetric. Too high, and the user sees a flash for
/// one frame. Too low, and the capture cannot resolve the sentinel, the
/// self-test finds nothing, and it reports `Verified` for an overlay that is
/// plainly visible to the other party. One is a cosmetic defect; the other is
/// the product lying about the only thing it promises.
pub const SENTINEL_ALPHA: f32 = 1.0;

/// What the self-test measured (§3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfTestEvidence {
    /// The fraction of sampled pixels that matched the sentinel.
    pub match_ratio: f64,
    /// How many pixels were sampled.
    pub sampled_pixels: usize,
}

/// Whether one RGBA pixel is one of the sentinel's colours.
#[must_use]
pub fn is_sentinel(pixel: &[u8]) -> bool {
    let Some(rgb) = pixel.get(0..3) else {
        return false;
    };
    SENTINEL_RGB.iter().any(|target| {
        rgb.iter()
            .zip(target)
            .all(|(a, b)| a.abs_diff(*b) <= CHANNEL_TOLERANCE)
    })
}

/// The fraction of an RGBA8 buffer that looks like the sentinel (§3.4 step 3).
///
/// **The whole of the detection logic, and pure.** It takes a slice rather
/// than a `Frame` on purpose: spec 006 D-5 keeps `Frame`'s constructor closed
/// so arbitrary pixels cannot acquire its lifecycle guarantees, and a
/// `frame_from_pixels` added for this test would have undone exactly that.
/// Taking bytes instead lets FR-003's two directions be asserted on synthetic
/// buffers, on every target, without widening anything.
#[must_use]
pub fn match_ratio_rgba(pixels: &[u8]) -> SelfTestEvidence {
    let (matched, total) = count_matches(pixels);
    evidence_from(matched, total)
}

/// Matching and total pixel counts for one RGBA8 buffer.
fn count_matches(pixels: &[u8]) -> (usize, usize) {
    let mut matched = 0_usize;
    let mut total = 0_usize;
    for pixel in pixels.chunks_exact(4) {
        total += 1;
        if is_sentinel(pixel) {
            matched += 1;
        }
    }
    (matched, total)
}

/// Turn two counts into evidence. The one place the division happens.
fn evidence_from(matched: usize, total: usize) -> SelfTestEvidence {
    #[allow(
        clippy::cast_precision_loss,
        reason = "both are pixel counts of one screen rectangle, far inside \
                  f64's exact-integer range"
    )]
    let ratio = if total == 0 {
        0.0
    } else {
        matched as f64 / total as f64
    };

    SelfTestEvidence {
        match_ratio: ratio,
        sampled_pixels: total,
    }
}

/// [`match_ratio_rgba`] over the overlay's rectangle in a captured frame.
///
/// A rectangle that leaves the frame yields `0.0` sampled pixels rather than
/// a panic: a monitor reconfigured mid-test is a real event, and the honest
/// answer is "nothing was sampled", which [`verdict`] turns into a refusal
/// rather than a pass.
#[must_use]
pub fn match_ratio(frame: &Frame, rect: Rect) -> SelfTestEvidence {
    let Ok(view) = frame.crop(rect) else {
        return SelfTestEvidence {
            match_ratio: 0.0,
            sampled_pixels: 0,
        };
    };

    let mut matched = 0_usize;
    let mut total = 0_usize;
    for row in view.rows() {
        // Counting rather than averaging the rows' ratios: a mean of means is
        // not the mean unless every row is the same length, and the last row
        // of a crop need not be.
        let (row_matched, row_total) = count_matches(row);
        matched += row_matched;
        total += row_total;
    }
    evidence_from(matched, total)
}

/// Turn a measurement into a status (§3.4 step 3).
#[must_use]
pub fn verdict(method: ExclusionMethod, evidence: SelfTestEvidence) -> ExclusionStatus {
    if evidence.sampled_pixels == 0 {
        // Nothing was looked at, so nothing was verified. Saying `Verified`
        // here would be the vacuous pass FR-003 exists to rule out.
        return ExclusionStatus::Compromised { method, evidence };
    }
    if evidence.match_ratio > SENTINEL_MATCH_THRESHOLD {
        ExclusionStatus::Compromised { method, evidence }
    } else {
        ExclusionStatus::Verified {
            method,
            verified_at: std::time::Instant::now(),
        }
    }
}

/// How long to wait for the overlay to paint the sentinel (§3.4).
///
/// Solid writes the DOM synchronously, so the only remaining wait is paint:
/// two frames at 60 Hz is 33 ms, and this allows four. §3.4 budgets the whole
/// self-test at under a second, so the margin is affordable and the cost of
/// being too quick is a capture taken before the pattern is on screen, which
/// would report `Verified` for an overlay that is plainly visible.
const SENTINEL_PAINT_WAIT: Duration = Duration::from_millis(66);

/// Run the self-test end to end (§3.4).
///
/// Mount the sentinel, wait for paint, capture, sample, unmount. Every exit
/// path that could not complete a measurement returns `Compromised` rather
/// than `Verified`: the product does not claim what it did not check.
pub fn run<R: Runtime>(
    window: &WebviewWindow<R>,
    source: &dyn ScreenSource,
    applied: &ExclusionStatus,
) -> ExclusionStatus {
    let method = match applied {
        ExclusionStatus::Applied { method }
        | ExclusionStatus::Verified { method, .. }
        | ExclusionStatus::Compromised { method, .. } => *method,
        // Nothing was applied, so there is nothing to verify. §3.5 already
        // refuses arming on either.
        ExclusionStatus::Unsupported { .. } | ExclusionStatus::Unknown => {
            return applied.clone();
        }
    };

    let Some(rect) = overlay_rect(window) else {
        return ExclusionStatus::Compromised {
            method,
            evidence: SelfTestEvidence {
                match_ratio: 0.0,
                sampled_pixels: 0,
            },
        };
    };

    // §3.4 step 1: the overlay renders the sentinel. Spec 012's `Sentinel`
    // component is mounted by a `set_sentinel` command; spec 011's contract
    // does not carry one yet, so the sentinel is not yet driven from here.
    // See D-3.
    // §3.4 step 1. Without this the self-test looks for colours nobody is
    // painting, finds none, and reports `Verified` for every overlay there
    // will ever be. That is the vacuous pass FR-003 exists to rule out, and
    // it is why the sentinel is mounted here rather than assumed.
    if !set_sentinel(window, true) {
        return unverifiable(method);
    }
    std::thread::sleep(SENTINEL_PAINT_WAIT);

    // §3.4 step 2, through the same compositor path a recording uses.
    let captured = capture_overlay_monitor(source);

    // §3.4 step 4: restore the overlay before deciding anything, so a return
    // path cannot leave the pattern on screen.
    let _ = set_sentinel(window, false);

    let Ok(frame) = captured else {
        return unverifiable(method);
    };

    verdict(method, match_ratio(&frame, rect))
}

/// The status for "no measurement was possible".
///
/// Never `Verified`, and never `Applied` either: `Applied` means "asked for,
/// not yet checked", and by the time this is reached the check has been tried
/// and failed. Reporting the stronger refusal keeps §3.5's gate closed.
const fn unverifiable(method: ExclusionMethod) -> ExclusionStatus {
    ExclusionStatus::Compromised {
        method,
        evidence: SelfTestEvidence {
            match_ratio: 0.0,
            sampled_pixels: 0,
        },
    }
}

/// Ask the overlay to mount or unmount the sentinel. `false` if it could not
/// be told.
fn set_sentinel<R: Runtime>(window: &WebviewWindow<R>, on: bool) -> bool {
    crate::events::emit(window.app_handle(), &UiEvent::SelfTestSentinel { on }).is_ok()
}

/// The overlay's rectangle in the captured frame's pixels.
fn overlay_rect<R: Runtime>(window: &WebviewWindow<R>) -> Option<Rect> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(Rect {
        x: u32::try_from(position.x.max(0)).ok()?,
        y: u32::try_from(position.y.max(0)).ok()?,
        width: size.width,
        height: size.height,
    })
}

/// Capture the monitor the overlay is on.
fn capture_overlay_monitor(source: &dyn ScreenSource) -> Result<Frame, CaptureError> {
    let monitors = source.monitors()?;
    let target = monitors
        .iter()
        .find(|monitor| monitor.is_primary)
        .or_else(|| monitors.first())
        .map_or(MonitorId(0), |monitor| monitor.id);
    source.capture(target)
}

#[cfg(test)]
mod tests {
    use super::super::{ExclusionMethod, ExclusionStatus};
    use super::{
        CHANNEL_TOLERANCE, SENTINEL_ALPHA, SENTINEL_MATCH_THRESHOLD, SENTINEL_RGB,
        SelfTestEvidence, is_sentinel, match_ratio, match_ratio_rgba, verdict,
    };
    use butler_capture::{MonitorId, Rect, test_support::frame_from_pattern};

    const METHOD: ExclusionMethod = ExclusionMethod::MacOsSharingNone;

    /// A flat RGBA buffer of `count` pixels, the first `sentinels` of which
    /// are the sentinel's first colour and the rest a dark grey.
    fn buffer(count: usize, sentinels: usize) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(count * 4);
        for i in 0..count {
            let rgb = if i < sentinels {
                SENTINEL_RGB[0]
            } else {
                [0x20, 0x20, 0x20]
            };
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
        }
        pixels
    }

    #[test]
    fn the_sentinel_colours_match_the_overlay_component() {
        // Spec 012's `Sentinel.tsx` paints these two. If either side changes
        // alone, the self-test looks for a pattern nobody draws and reports
        // `Verified` for every overlay there will ever be.
        let component = std::fs::read_to_string("../src/components/Sentinel.tsx")
            .expect("spec 012's Sentinel component");
        assert!(
            component.contains("#ff00ff"),
            "magenta must be in the component"
        );
        assert!(
            component.contains("#00ff00"),
            "green must be in the component"
        );
        assert_eq!(SENTINEL_RGB[0], [0xFF, 0x00, 0xFF]);
        assert_eq!(SENTINEL_RGB[1], [0x00, 0xFF, 0x00]);
    }

    #[test]
    fn a_sentinel_pixel_is_recognized_within_tolerance() {
        assert!(is_sentinel(&[0xFF, 0x00, 0xFF, 0xFF]));
        assert!(is_sentinel(&[0x00, 0xFF, 0x00, 0xFF]));
        // Drifted by the tolerance, as a rescaled capture would be.
        assert!(is_sentinel(&[
            0xFF - CHANNEL_TOLERANCE,
            CHANNEL_TOLERANCE,
            0xFF - CHANNEL_TOLERANCE,
            0xFF
        ]));
    }

    #[test]
    fn ordinary_content_is_not_the_sentinel() {
        for pixel in [
            [0xFF, 0xFF, 0xFF, 0xFF], // white
            [0x00, 0x00, 0x00, 0xFF], // black
            [0x1E, 0x1E, 0x1E, 0xFF], // a dark editor
            [0x00, 0x7A, 0xCC, 0xFF], // a link blue
            [0xFF, 0x80, 0xFF, 0xFF], // pale pink: near magenta, still not it
        ] {
            assert!(!is_sentinel(&pixel), "{pixel:?} must not match");
        }
    }

    /// FR-003, the **found** direction: a capture containing the overlay is
    /// reported `Compromised`.
    #[test]
    fn fr_003_a_visible_overlay_is_detected() {
        let pixels = buffer(1024, 1024);
        let evidence = match_ratio_rgba(&pixels);

        assert_eq!(evidence.sampled_pixels, 1024);
        assert!(
            (evidence.match_ratio - 1.0).abs() < f64::EPSILON,
            "{evidence:?}"
        );
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Compromised { .. }
        ));
    }

    /// FR-003, the **absent** direction. Both are asserted, so neither can
    /// pass vacuously: a detector that always said "found" fails here, and
    /// one that always said "absent" fails above.
    #[test]
    fn fr_003_an_excluded_overlay_is_verified() {
        let pixels = buffer(1024, 0);
        let evidence = match_ratio_rgba(&pixels);

        assert_eq!(evidence.sampled_pixels, 1024);
        assert!(evidence.match_ratio.abs() < f64::EPSILON, "{evidence:?}");
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Verified { .. }
        ));
    }

    /// A capture that sampled nothing is **not** a pass. This is the failure
    /// mode the two-direction rule exists to catch: a self-test that looked
    /// at an empty rectangle and called it clean.
    #[test]
    fn nothing_sampled_is_never_verified() {
        let evidence = SelfTestEvidence {
            match_ratio: 0.0,
            sampled_pixels: 0,
        };
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Compromised { .. }
        ));
        assert_eq!(match_ratio_rgba(&[]).sampled_pixels, 0);
    }

    /// A rectangle outside the frame samples nothing rather than panicking:
    /// a monitor reconfigured mid-test is a real event.
    #[test]
    fn a_rectangle_outside_the_frame_samples_nothing() {
        let frame = frame_from_pattern(MonitorId(0), 8, 8, 1.0, 0x20);
        let outside = match_ratio(
            &frame,
            Rect {
                x: 100,
                y: 100,
                width: 8,
                height: 8,
            },
        );
        assert_eq!(outside.sampled_pixels, 0);
        assert!(matches!(
            verdict(METHOD, outside),
            ExclusionStatus::Compromised { .. }
        ));
    }

    /// An in-bounds crop of a frame with nothing sentinel-coloured verifies,
    /// which is the `Frame` path's own two directions meeting the slice's.
    #[test]
    fn an_in_bounds_crop_of_a_clean_frame_verifies() {
        let frame = frame_from_pattern(MonitorId(0), 16, 16, 1.0, 0x20);
        let evidence = match_ratio(
            &frame,
            Rect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
        );
        assert_eq!(evidence.sampled_pixels, 16 * 16);
        assert!(evidence.match_ratio.abs() < f64::EPSILON, "{evidence:?}");
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Verified { .. }
        ));
    }

    /// The threshold tolerates a stray pixel and catches a partial overlay.
    #[test]
    fn the_threshold_separates_noise_from_a_visible_overlay() {
        // One sentinel pixel in 16384: well under 0.5%.
        let evidence = match_ratio_rgba(&buffer(16_384, 1));
        assert!(
            evidence.match_ratio < SENTINEL_MATCH_THRESHOLD,
            "{evidence:?}"
        );
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Verified { .. }
        ));

        // A hundred: over 0.5%, and a hundred sentinel pixels on screen means
        // a corner of the overlay is being captured.
        let evidence = match_ratio_rgba(&buffer(16_384, 100));
        assert!(
            evidence.match_ratio > SENTINEL_MATCH_THRESHOLD,
            "{evidence:?}"
        );
        assert!(matches!(
            verdict(METHOD, evidence),
            ExclusionStatus::Compromised { .. }
        ));
    }

    /// AC-3. The alpha is a named constant with its reasoning beside it.
    #[test]
    fn ac_003_the_sentinel_alpha_is_recorded() {
        // Full opacity, until someone measures the real floor on both
        // platforms. The two errors are not symmetric: too high shows the
        // user a flash for one frame, too low makes the self-test report
        // `Verified` for an overlay the other party can plainly see.
        assert!((SENTINEL_ALPHA - 1.0).abs() < f32::EPSILON);
    }
}
