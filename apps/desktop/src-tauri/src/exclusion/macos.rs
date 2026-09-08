// Spec: specs/005-capture-exclusion/spec.md

//! macOS exclusion: `NSWindow.sharingType = NSWindowSharingNone` (§3.3).
//!
//! This excludes the window from `CGWindowListCreateImage`, `SCStream` and
//! the legacy `CGDisplayStream` paths that honour sharing type. Whether the
//! *running* OS still honours it is not something this file can know, which
//! is what the self-test is for: Apple's behaviour has changed across
//! releases, and a spec that assumed otherwise would be making a promise on
//! Apple's behalf.
//!
//! The Screen Recording permission (spec 004 §3.5) is needed for the
//! self-test's capture, not for exclusion itself. A user who has denied it
//! gets `Applied` and never `Verified`, which is the honest outcome: the
//! product cannot check, so it does not claim.

use objc2_app_kit::{NSWindow, NSWindowSharingType};
use tauri::{Runtime, WebviewWindow};

use super::{ExclusionError, ExclusionMethod, ExclusionStatus};

/// Set the sharing type, and read it back (§3.3, FR-002).
///
/// The read-back is the point. `setSharingType` returns nothing, so without
/// reading the property afterwards this function would report success for a
/// call the window server may have ignored.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>) -> Result<ExclusionStatus, ExclusionError> {
    let handle = window
        .ns_window()
        .map_err(|e| ExclusionError::NoHandle(e.to_string()))?;
    if handle.is_null() {
        return Err(ExclusionError::NoHandle("ns_window() was null".to_owned()));
    }

    // SAFETY: `ns_window()` returns the `NSWindow` Tauri created for this
    // webview window and keeps alive for its lifetime, so the pointer is
    // valid and correctly typed here; it is null-checked above. The borrow
    // lasts only for the setter and the read-back, and no ownership is taken,
    // so no release is owed.
    #[allow(unsafe_code)]
    let readback = unsafe {
        let ns_window: &NSWindow = &*handle.cast::<NSWindow>();
        ns_window.setSharingType(NSWindowSharingType::None);
        ns_window.sharingType()
    };

    if readback == NSWindowSharingType::None {
        Ok(ExclusionStatus::Applied {
            method: ExclusionMethod::MacOsSharingNone,
        })
    } else {
        // The window server declined. Reporting `Unsupported` rather than
        // `Applied` is what keeps §3.5's gate meaningful.
        Ok(ExclusionStatus::Unsupported {
            reason: format!("the window server kept sharingType {readback:?}"),
        })
    }
}
