// Spec: specs/004-desktop-shell/spec.md

//! Screen-recording permission, which only macOS asks for.
//!
//! Spec 004 §3.5. Two rules shape this module. The app asks **once per
//! launch** and then stops, because a loop-prompting app trains the user to
//! dismiss the dialog. And it never asks for Accessibility, because it never
//! synthesizes input; requesting a permission the product does not use is
//! exactly the pattern a privacy-first tool should not have.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether capture is permitted, and if not, whether asking is worthwhile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    /// Capture may proceed.
    Granted,
    /// Not granted, and this launch has not asked yet.
    Denied,
    /// Not granted, and this launch has already asked once (§3.5).
    DeniedAlreadyRequested,
}

impl PermissionState {
    /// Whether the pipeline may arm.
    #[must_use]
    pub fn is_granted(self) -> bool {
        matches!(self, PermissionState::Granted)
    }
}

/// One request per launch (§3.5), across every thread that might arm.
static REQUESTED_THIS_LAUNCH: AtomicBool = AtomicBool::new(false);

/// Whether screen capture is currently permitted, without prompting.
#[must_use]
pub fn screen_capture_state() -> PermissionState {
    if platform::preflight() {
        return PermissionState::Granted;
    }
    if REQUESTED_THIS_LAUNCH.load(Ordering::Acquire) {
        PermissionState::DeniedAlreadyRequested
    } else {
        PermissionState::Denied
    }
}

/// Ask for screen-recording permission, at most once per launch.
///
/// Returns the state afterwards. On macOS the system dialog appears only the
/// first time the binary asks; afterwards the user must go to System Settings,
/// which is what the onboarding panel (spec 012) links to.
pub fn request_screen_capture() -> PermissionState {
    if platform::preflight() {
        return PermissionState::Granted;
    }
    // `swap` rather than load-then-store: two threads arming at once must
    // produce one request, not two (§3.5 "one request per launch").
    if REQUESTED_THIS_LAUNCH.swap(true, Ordering::AcqRel) {
        return PermissionState::DeniedAlreadyRequested;
    }
    platform::request();
    if platform::preflight() {
        PermissionState::Granted
    } else {
        PermissionState::DeniedAlreadyRequested
    }
}

#[cfg(target_os = "macos")]
mod platform {
    // These two live in CoreGraphics and have no crate binding worth adding a
    // dependency for; the signatures are stable public API since macOS 10.15.
    #[allow(unsafe_code)]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    /// Whether the TCC record already grants capture. Never prompts.
    pub fn preflight() -> bool {
        // SAFETY: a nullary CoreGraphics call with no arguments to validate
        // and no pointer to own. It reads the process's TCC state and returns
        // a plain bool; it cannot fail or block.
        #[allow(unsafe_code)]
        unsafe {
            CGPreflightScreenCaptureAccess()
        }
    }

    /// Ask the system to prompt. No-op if the user already answered.
    pub fn request() {
        // SAFETY: as `preflight`. The return value is the immediate answer,
        // which is not useful here (the dialog is asynchronous), so the
        // caller re-checks with `preflight` instead.
        #[allow(unsafe_code)]
        unsafe {
            let _ = CGRequestScreenCaptureAccess();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    /// Windows needs no permission for desktop duplication (§3.5).
    pub fn preflight() -> bool {
        true
    }
    /// Nothing to request.
    pub fn request() {}
}

#[cfg(test)]
mod tests {
    use super::PermissionState;

    #[test]
    fn only_granted_permits_arming() {
        assert!(PermissionState::Granted.is_granted());
        assert!(!PermissionState::Denied.is_granted());
        assert!(!PermissionState::DeniedAlreadyRequested.is_granted());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn windows_needs_no_permission() {
        // §3.5: desktop duplication requires no grant, so the state is
        // Granted unconditionally and arming is never gated on a prompt.
        assert_eq!(super::screen_capture_state(), PermissionState::Granted);
    }
}
