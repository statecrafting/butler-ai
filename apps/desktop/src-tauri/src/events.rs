// Spec: specs/011-ipc-contract/spec.md

//! Rust to UI: one channel, one tagged payload.
//!
//! Spec 011 §3.2 fixes the shape. Every [`UiEvent`] goes out on the single
//! channel [`EVENT_CHANNEL`], serialized with its `type` tag, so the overlay
//! subscribes **once** and switches on that tag. The alternative, a Tauri
//! event name per variant, would put the contract's variant list in two
//! places: the enum here and a list of `listen` calls in the webview, which
//! is exactly the drift spec 011 exists to prevent.

use tauri::{AppHandle, Emitter, Runtime};

use crate::AppError;
use butler_core::ipc::UiEvent;

/// The one event channel (§3.2).
///
/// The `butler://` prefix keeps it out of Tauri's own namespace, so a future
/// Tauri event can never collide with the product's.
pub const EVENT_CHANNEL: &str = "butler://event";

/// Emit one [`UiEvent`] to the overlay.
///
/// # Errors
///
/// Returns [`AppError::Ipc`] if the payload cannot be serialized or the
/// webview is gone. Neither is recoverable here: the caller is the runtime
/// (spec 019), and its policy for a UI that stopped listening is its own.
pub fn emit<R: Runtime>(app: &AppHandle<R>, event: &UiEvent) -> Result<(), AppError> {
    app.emit(EVENT_CHANNEL, event)
        .map_err(|e| AppError::Ipc(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::EVENT_CHANNEL;

    /// §3.2 names the channel, and the overlay's `listen` call has to match it
    /// exactly. A typo on either side is silence, not an error, so the string
    /// is pinned here and imported from the generated bindings there.
    #[test]
    fn the_channel_is_the_one_the_spec_names() {
        assert_eq!(EVENT_CHANNEL, "butler://event");
    }
}
