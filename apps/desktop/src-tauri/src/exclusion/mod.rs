// Spec: specs/005-capture-exclusion/spec.md

//! Keep the overlay out of the frame buffer, and **verify** it (spec 005).
//!
//! The product promise is that the overlay is invisible to screen sharing and
//! recording while fully visible to the person at the keyboard. Both
//! platforms expose a compositor-level switch for exactly that, built for
//! password managers and DRM surfaces.
//!
//! # Why this is a module and not two API calls
//!
//! **The switch is a request, not a guarantee.** Its effect depends on the OS
//! version, on which capture API the other party uses, and on Apple's
//! evolving `ScreenCaptureKit` behaviour. Constitution §VI is the rule this
//! module exists to obey: the product verifies rather than assumes, and
//! reports what it measured rather than what it asked for.
//!
//! **The pipeline captures the screen too** (spec 006). If the overlay's own
//! pixels were in the frame, the recognized text would contain the previous
//! answer, the change detector would fire on it, and the assistant would be
//! asked about its own output. Exclusion is a correctness property of the
//! pipeline as much as a privacy one, which is why §3.6 keeps a guard for the
//! case where it fails.

use std::time::Instant;

use butler_capture::ScreenSource;
use tauri::{Runtime, WebviewWindow};

pub mod selftest;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

pub use selftest::{SENTINEL_MATCH_THRESHOLD, SelfTestEvidence};

/// Which mechanism the platform offers (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExclusionMethod {
    /// `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`.
    WindowsDisplayAffinity,
    /// `NSWindow.sharingType = NSWindowSharingNone`.
    MacOsSharingNone,
}

/// What the product knows about its own visibility (§3.1).
///
/// Four states, not a boolean, and that is constitution §VI in a type:
/// "asked for and not yet confirmed" is a different thing from "confirmed",
/// and "the OS said yes but the camera says otherwise" is a third. The status
/// strip (spec 012) renders whichever of these is true, and the runtime (spec
/// 009) refuses to arm on two of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ExclusionStatus {
    /// Applied, and the self-test found no overlay pixels in a capture.
    Verified {
        /// How it was applied.
        method: ExclusionMethod,
        /// When it was last confirmed, monotonically.
        verified_at: Instant,
    },
    /// Applied; the self-test has not run yet. Transient during startup.
    Applied {
        /// How it was applied.
        method: ExclusionMethod,
    },
    /// The OS accepted the call and the self-test saw the overlay anyway.
    ///
    /// The worst of the four, and the one a boolean would have hidden.
    Compromised {
        /// How it was applied.
        method: ExclusionMethod,
        /// What the self-test measured.
        evidence: SelfTestEvidence,
    },
    /// The platform cannot exclude this window.
    Unsupported {
        /// Why, in the platform's own words.
        reason: String,
    },
    /// Nothing has been attempted yet. The state a fresh `AppState` holds,
    /// and never a state `apply_exclusion` or the self-test returns.
    #[default]
    Unknown,
}

impl ExclusionStatus {
    /// Whether the runtime may arm without the user's degraded-mode consent
    /// (§3.5).
    #[must_use]
    pub const fn permits_arming(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    /// A short name for a log line or a message. Never the reason string,
    /// which is the platform's words and can be long.
    #[must_use]
    pub const fn summary_name(&self) -> &'static str {
        match self {
            Self::Verified { .. } => "verified",
            Self::Applied { .. } => "applied but unverified",
            Self::Compromised { .. } => "compromised",
            Self::Unsupported { .. } => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    /// The wire summary spec 011 carries.
    #[must_use]
    pub const fn summary(&self) -> butler_core::ipc::ExclusionSummary {
        use butler_core::ipc::ExclusionSummary;
        match self {
            Self::Verified { .. } => ExclusionSummary::Verified,
            Self::Applied { .. } => ExclusionSummary::Applied,
            Self::Compromised { .. } => ExclusionSummary::Compromised,
            Self::Unsupported { .. } => ExclusionSummary::Unsupported,
            // The wire has no fifth value, and should not: "nothing has been
            // attempted" and "the platform cannot" are the same thing to a
            // user reading the status strip, and both refuse arming.
            Self::Unknown => ExclusionSummary::Unknown,
        }
    }
}

/// Why exclusion could not even be attempted (§3.1).
#[derive(Debug, thiserror::Error)]
pub enum ExclusionError {
    /// The platform window handle could not be obtained.
    #[error("no native window handle: {0}")]
    NoHandle(String),
}

/// Apply the platform's exclusion switch (§3.1).
///
/// **Idempotent**, and it must be: §3.1 requires re-application whenever the
/// window is recreated or re-shown, because some window managers reset
/// affinity across `SW_HIDE`/`SW_SHOW`. A caller that had to track whether it
/// had already applied would eventually get it wrong.
///
/// Returns [`ExclusionStatus::Applied`] on success: **not** `Verified`.
/// Nothing here has looked at a captured frame, and saying `Verified` before
/// the self-test has run would be the exact claim constitution §VI forbids.
///
/// # Errors
///
/// [`ExclusionError::NoHandle`] if the native window handle is unavailable.
pub fn apply_exclusion<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<ExclusionStatus, ExclusionError> {
    platform::apply(window)
}

/// Run the self-test and report what it measured (§3.4).
///
/// Never returns `Applied`: this function's whole purpose is to replace a
/// request with a measurement.
pub fn verify_exclusion<R: Runtime>(
    window: &WebviewWindow<R>,
    source: &dyn ScreenSource,
    applied: &ExclusionStatus,
) -> ExclusionStatus {
    selftest::run(window, source, applied)
}

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "windows")]
use windows as platform;

/// The platform that cannot exclude anything.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{ExclusionError, ExclusionStatus};
    use tauri::{Runtime, WebviewWindow};

    /// §3.1: `Unsupported` rather than a silent success. The window is then
    /// not shown unless the user accepted degraded mode (§3.5).
    pub fn apply<R: Runtime>(
        _window: &WebviewWindow<R>,
    ) -> Result<ExclusionStatus, ExclusionError> {
        Ok(ExclusionStatus::Unsupported {
            reason: "no compositor-level window exclusion on this platform".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ExclusionMethod, ExclusionStatus, SelfTestEvidence};
    use butler_core::ipc::ExclusionSummary;

    fn evidence(ratio: f64) -> SelfTestEvidence {
        SelfTestEvidence {
            match_ratio: ratio,
            sampled_pixels: 10_000,
        }
    }

    /// §3.5. Only a measurement permits arming. This is the honesty contract
    /// in one assertion: neither "the OS said yes" nor "we have not looked"
    /// is good enough.
    #[test]
    fn only_verified_permits_arming() {
        let verified = ExclusionStatus::Verified {
            method: ExclusionMethod::MacOsSharingNone,
            verified_at: std::time::Instant::now(),
        };
        assert!(verified.permits_arming());

        for status in [
            ExclusionStatus::Applied {
                method: ExclusionMethod::MacOsSharingNone,
            },
            ExclusionStatus::Compromised {
                method: ExclusionMethod::MacOsSharingNone,
                evidence: evidence(0.4),
            },
            ExclusionStatus::Unsupported {
                reason: "old build".to_owned(),
            },
        ] {
            assert!(
                !status.permits_arming(),
                "{status:?} must not permit arming without consent"
            );
        }
    }

    /// The wire summary keeps all four states distinct. Collapsing any two
    /// would let the status strip say something the product did not measure.
    #[test]
    fn every_status_has_its_own_wire_summary() {
        let summaries = [
            ExclusionStatus::Verified {
                method: ExclusionMethod::WindowsDisplayAffinity,
                verified_at: std::time::Instant::now(),
            }
            .summary(),
            ExclusionStatus::Applied {
                method: ExclusionMethod::WindowsDisplayAffinity,
            }
            .summary(),
            ExclusionStatus::Compromised {
                method: ExclusionMethod::WindowsDisplayAffinity,
                evidence: evidence(0.9),
            }
            .summary(),
            ExclusionStatus::Unsupported {
                reason: "x".to_owned(),
            }
            .summary(),
        ];

        assert_eq!(
            summaries,
            [
                ExclusionSummary::Verified,
                ExclusionSummary::Applied,
                ExclusionSummary::Compromised,
                ExclusionSummary::Unsupported,
            ]
        );
    }
}
