// Spec: specs/011-ipc-contract/spec.md

//! UI to Rust: one Tauri command per [`UiCommand`] variant.
//!
//! Spec 011 §3.2. Each command delegates to [`AppState`] or, once spec 019
//! lands it, to the runtime, and returns a typed `Result<T, ErrorKind>` so
//! the overlay never parses an error string.
//!
//! # Why some of these fail
//!
//! Two of the eight are servable today. `set_interactive` is this crate's own
//! window property, and `get_contract_version` is a constant. The other six
//! drive machinery that phase 2 has not built: arming, asking and dismissing
//! need the runtime (spec 019), the self-test needs capture exclusion (spec
//! 005), and storing a credential needs the keychain-backed `Secret` (spec
//! 010).
//!
//! They are **registered and they fail**, rather than being absent or
//! silently succeeding. Absent, and the overlay would get Tauri's own
//! "command not found", which is not in the contract. Silently succeeding,
//! and the overlay would show an armed pipeline that is not running, which is
//! worse than an error: spec 004 §3.2 and constitution §VI are both about not
//! reporting a capability the product does not have.
//!
//! [`UiCommand`]: butler_core::ipc::UiCommand

use tauri::{AppHandle, Manager, Runtime, State};

use crate::app_state::AppState;
use crate::window::{self, OVERLAY_LABEL};
use butler_core::ipc::{ErrorKind, IPC_CONTRACT_VERSION, SettingsView};
use butler_core::settings::SettingsPatch;

use crate::events;
use crate::settings_store::SettingsStore;

/// The contract version the UI checks at startup (§3.4).
///
/// Returned as a two-element tuple, which the generated TypeScript renders as
/// `[number, number]`. The UI compares only the major: a minor difference is
/// additive by the `constrains` edge on `ipc.rs` and therefore compatible.
#[tauri::command]
#[specta::specta]
#[must_use]
pub const fn get_contract_version() -> (u16, u16) {
    IPC_CONTRACT_VERSION
}

/// Make the overlay accept clicks, or stop (§3.3's Interact toggle).
///
/// # Errors
///
/// [`ErrorKind::Internal`] if the overlay window is gone, or if the platform
/// refuses the change. The stored flag is updated only after the platform
/// accepts it: recording a value the window is not actually in would leave
/// the next toggle flipping away from a state that never existed, and the
/// overlay stuck eating clicks with no way back.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::needless_pass_by_value,
    reason = "`#[tauri::command]` extracts `AppHandle` and `State` from the \
              invoke context and hands them over by value; the macro does not \
              generate a call that could pass a reference. `AppHandle` is a \
              cheap clone of a handle and `State` is a borrow already, so \
              there is nothing to save here even if the signature were ours."
)]
pub fn set_interactive<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    on: bool,
) -> Result<(), ErrorKind> {
    let overlay = app
        .get_webview_window(OVERLAY_LABEL)
        .ok_or(ErrorKind::Internal)?;
    window::set_click_through(&overlay, !on).map_err(|_| ErrorKind::Internal)?;
    state.set_interactive(on);
    Ok(())
}

/// Start watching the screen.
///
/// # Errors
///
/// Always [`ErrorKind::Internal`] until spec 019 supplies the runtime.
#[tauri::command]
#[specta::specta]
pub const fn arm() -> Result<(), ErrorKind> {
    Err(ErrorKind::Internal)
}

/// Stop watching the screen.
///
/// # Errors
///
/// Always [`ErrorKind::Internal`] until spec 019 supplies the runtime.
#[tauri::command]
#[specta::specta]
pub const fn disarm() -> Result<(), ErrorKind> {
    Err(ErrorKind::Internal)
}

/// Capture and ask now, bypassing change detection once.
///
/// # Errors
///
/// Always [`ErrorKind::Internal`] until spec 019 supplies the runtime.
#[tauri::command]
#[specta::specta]
pub const fn ask_now() -> Result<(), ErrorKind> {
    Err(ErrorKind::Internal)
}

/// Clear the current answer.
///
/// # Errors
///
/// Always [`ErrorKind::Internal`] until spec 019 supplies the runtime.
#[tauri::command]
#[specta::specta]
pub const fn dismiss() -> Result<(), ErrorKind> {
    Err(ErrorKind::Internal)
}

/// Re-run the capture-exclusion self-test.
///
/// # Errors
///
/// Always [`ErrorKind::Internal`] until spec 005 supplies the self-test.
#[tauri::command]
#[specta::specta]
pub const fn run_self_test() -> Result<(), ErrorKind> {
    Err(ErrorKind::Internal)
}

/// Put a provider credential in the OS keychain.
///
/// Spec 011 §3.2 and spec 015 §3.1: the credential is taken by value, is
/// never logged, printed or returned, and is dropped before this function
/// returns. `provider` is a configured label and is not a credential.
///
/// The keychain write and the zeroizing `Secret` that owns it are spec 010's.
/// Until then this refuses rather than accepting a credential it has nowhere
/// safe to put: accepting and discarding one would tell the user their key
/// was stored when it was not.
///
/// # Errors
///
/// Always [`ErrorKind::Credential`] until spec 010 supplies the secret store.
#[tauri::command]
#[specta::specta]
pub fn store_secret(provider: String, secret: String) -> Result<(), ErrorKind> {
    // Named `_secret` and immediately dropped. Not logged, not traced, not
    // echoed back in the error. Spec 010 replaces this body with the move
    // into a zeroizing `Secret` and the keychain write.
    drop(secret);
    drop(provider);
    Err(ErrorKind::Credential)
}

/// Read the current configuration (spec 014 §3.3).
///
/// Serves from the in-memory copy rather than re-reading the file: the app is
/// the only writer (spec 014 §3.2), so the file cannot be ahead of memory,
/// and opening a settings panel should not touch the disk.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::needless_pass_by_value,
    reason = "`#[tauri::command]` hands `State` over by value; the macro does \
              not generate a call that could pass a reference."
)]
#[must_use]
pub fn get_settings(state: State<'_, AppState>) -> SettingsView {
    state.settings()
}

/// Change the configuration (spec 014 §3.3).
///
/// Apply, validate, persist, broadcast. The order matters and the failure
/// behaviour matters more: an invalid patch is rejected **whole**, so nothing
/// is written and the in-memory copy is untouched. A half-applied
/// configuration would be one the user never chose and cannot see.
///
/// Spec 019 will take a `SettingsChanged` event from here and spec 004 will
/// re-register the shortcuts when they change. Neither exists yet, so this
/// stops after the broadcast.
///
/// # Errors
///
/// [`ErrorKind::Internal`] if the patch does not validate, or if the file
/// cannot be written. The typed field list is not on the wire: `ErrorKind` is
/// a closed enum by spec 011 §3.1 and widening it is that spec's call
/// (spec 014 D-4).
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::needless_pass_by_value,
    reason = "`#[tauri::command]` hands `AppHandle` and `State` over by value."
)]
pub fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<SettingsView, ErrorKind> {
    let candidate = state.settings().apply(&patch);
    candidate.validate().map_err(|_| ErrorKind::Internal)?;

    let store = SettingsStore::platform().map_err(|_| ErrorKind::Internal)?;
    store.save(&candidate).map_err(|_| ErrorKind::Internal)?;

    state.set_settings(candidate.clone());

    // Best effort: the caller already has the new value as the return, so a
    // webview that has gone away costs it nothing.
    let _ = events::emit(
        &app,
        &butler_core::ipc::UiEvent::SettingsUpdated {
            settings: Box::new(candidate.clone()),
        },
    );

    Ok(candidate)
}

/// The one `tauri-specta` builder (§3.2, §3.3).
///
/// Both callers use this function: [`crate::run`] installs
/// `invoke_handler()` from it, and the `export-bindings` binary renders
/// TypeScript from it. One source means the handler the app installs and the
/// bindings the overlay imports cannot describe different contracts, which is
/// the whole point of spec 011.
///
/// [`UiEvent`] is registered with `typ` rather than `events`, because §3.2
/// puts every event on one channel with a `type` tag. `tauri-specta`'s event
/// machinery would give each variant its own Tauri event name, which would
/// put the variant list in the webview as a list of `listen` calls: exactly
/// the second source of truth this spec exists to remove.
///
#[must_use]
pub fn builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_contract_version,
            set_interactive::<tauri::Wry>,
            arm,
            disarm,
            ask_now,
            dismiss,
            run_self_test,
            store_secret,
            get_settings,
            update_settings::<tauri::Wry>,
        ])
        .typ::<butler_core::ipc::UiEvent>()
}

#[cfg(test)]
mod tests {
    use super::{arm, ask_now, disarm, dismiss, get_contract_version, run_self_test, store_secret};
    use butler_core::ipc::{ErrorKind, IPC_CONTRACT_VERSION};

    #[test]
    fn the_version_command_returns_the_contract_constant() {
        assert_eq!(get_contract_version(), IPC_CONTRACT_VERSION);
    }

    /// The six unbuilt commands fail rather than succeeding quietly.
    ///
    /// A `Ok(())` here would be the product claiming a capability it does not
    /// have, which is the one thing constitution §VI rules out. When 019, 005
    /// and 010 land, each of these tests changes with the command it covers.
    #[test]
    fn unbuilt_commands_refuse_rather_than_pretend() {
        assert_eq!(arm(), Err(ErrorKind::Internal));
        assert_eq!(disarm(), Err(ErrorKind::Internal));
        assert_eq!(ask_now(), Err(ErrorKind::Internal));
        assert_eq!(dismiss(), Err(ErrorKind::Internal));
        assert_eq!(run_self_test(), Err(ErrorKind::Internal));
    }

    /// Spec 015: a credential is refused, not accepted and dropped. The user
    /// must not be told a key was stored when there is nowhere to store it.
    #[test]
    fn store_secret_refuses_until_there_is_a_keychain() {
        assert_eq!(
            store_secret("anthropic".into(), "sk-ant-not-a-real-key".into()),
            Err(ErrorKind::Credential)
        );
    }

    /// FR-002: the exporter's output is byte-identical across runs.
    ///
    /// Not run on Windows, and the reason is a linker one rather than a
    /// behavioural one: calling `builder()` makes `tauri::Wry` *live* in this
    /// test binary, which imports `webview2-com`'s entry points.
    /// `tauri-build` puts `WebView2Loader.dll` beside `target/debug/`, and a
    /// test binary runs from `target/debug/deps/`, so the process fails to
    /// start with `STATUS_ENTRYPOINT_NOT_FOUND` before any test runs. The
    /// property is platform-independent and macOS checks it; the genuinely
    /// cross-platform half of FR-002 is CI's, which runs the exporter on both
    /// platforms and fails on a non-empty `git diff`. See spec 011 D-8.
    #[cfg(not(windows))]
    ///
    /// This is the half that can fail on one machine. The other half, "and
    /// across platforms", is CI's: `jobs.rust` re-runs the exporter on macOS
    /// and Windows and fails on a non-empty `git diff`, so a platform-varying
    /// render shows up as a dirty tree there rather than as a passing test
    /// here. Render order is the risk this covers: a `HashMap` anywhere in
    /// the type collection would make two runs in one process differ.
    #[test]
    fn the_export_is_deterministic() {
        let render = || {
            super::builder()
                .export_str(
                    specta_typescript::Typescript::default()
                        .bigint(specta_typescript::BigIntExportBehavior::Number),
                )
                .expect("render")
        };
        assert_eq!(render(), render(), "two renders in one process differed");
    }

    /// §3.2: every variant of the contract is reachable from the UI.
    ///
    /// `collect_commands!` is a macro, so a command that exists but is left
    /// out of it compiles, registers nothing, and fails at runtime with
    /// Tauri's own "command not found", which is not in the contract. This
    /// counts what the builder actually collected against the eight variants
    /// `UiCommand` currently has.
    ///
    /// Not run on Windows, for the linker reason above (spec 011 D-8).
    #[cfg(not(windows))]
    #[test]
    fn every_command_variant_is_registered() {
        let exported = super::builder()
            .export_str(
                specta_typescript::Typescript::default()
                    .bigint(specta_typescript::BigIntExportBehavior::Number),
            )
            .expect("render");
        for name in [
            "getContractVersion",
            "setInteractive",
            "arm",
            "disarm",
            "askNow",
            "dismiss",
            "runSelfTest",
            "storeSecret",
        ] {
            assert!(
                exported.contains(&format!("async {name}(")),
                "`{name}` is not in the generated bindings, so the overlay cannot call it"
            );
        }
    }
}
