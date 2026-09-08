// Spec: specs/005-capture-exclusion/spec.md

//! Windows exclusion: `SetWindowDisplayAffinity` (§3.2).
//!
//! # Why there is no fallback
//!
//! Builds older than 19041 offer only `WDA_MONITOR`, which paints a **black
//! rectangle** where the window is. §3.2 forbids falling back to it, and the
//! reason is worth stating plainly: a black rectangle in a shared screen is
//! more conspicuous than the overlay itself, and it tells the other party
//! both that something is being hidden and exactly where. `Unsupported` is
//! the honest answer, and §3.5 is what stops the app arming on it.

use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetWindowDisplayAffinity, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
};
use tauri::{Runtime, WebviewWindow};

use super::{ExclusionError, ExclusionMethod, ExclusionStatus};

/// Apply display affinity, and read it back (§3.2, FR-001).
pub fn apply<R: Runtime>(window: &WebviewWindow<R>) -> Result<ExclusionStatus, ExclusionError> {
    let handle = window
        .hwnd()
        .map_err(|e| ExclusionError::NoHandle(e.to_string()))?;
    let hwnd = HWND(handle.0.cast());

    // SAFETY: `hwnd` is the window handle Tauri created for this webview
    // window and keeps alive for its lifetime. Both calls are plain Win32
    // calls on it; the second writes through a pointer to a local that
    // outlives the call.
    #[allow(unsafe_code)]
    let applied = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };

    if let Err(error) = applied {
        // §3.2: an older build refuses this affinity. That is `Unsupported`,
        // never a fall back to the black box.
        return Ok(ExclusionStatus::Unsupported {
            reason: format!(
                "SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) refused: {}",
                error.message()
            ),
        });
    }

    // The read-back FR-001 asks for. A call that returned success but left
    // the affinity unchanged would otherwise be reported as `Applied`.
    //
    // `u32`, not `WINDOW_DISPLAY_AFFINITY`: the getter's out-parameter is the
    // raw value even though the setter takes the newtype. Verified against
    // the Windows target before this landed, in a scratch crate, because the
    // Tauri crate's build script cannot cross-compile from macOS (D-4).
    let mut readback = 0_u32;
    // SAFETY: as above; `readback` is a live local for the duration.
    #[allow(unsafe_code)]
    let read = unsafe { GetWindowDisplayAffinity(hwnd, &raw mut readback) };

    match read {
        Ok(()) if readback == WDA_EXCLUDEFROMCAPTURE.0 => Ok(ExclusionStatus::Applied {
            method: ExclusionMethod::WindowsDisplayAffinity,
        }),
        Ok(()) => Ok(ExclusionStatus::Unsupported {
            reason: format!("display affinity read back as {readback:#x}"),
        }),
        Err(error) => Ok(ExclusionStatus::Unsupported {
            reason: format!("GetWindowDisplayAffinity failed: {}", error.message()),
        }),
    }
}
