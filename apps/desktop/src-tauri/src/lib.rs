// Spec: specs/004-desktop-shell/spec.md

//! The Tauri v2 application: `butler-desktop`.
//!
//! Spec 004 is the host every other desktop-side spec extends, and it is thin
//! on purpose. It creates the window, registers the inputs, builds the tray,
//! and asks the OS for what the product needs. It contains **no pipeline
//! logic**: the decisions live in `butler_core`'s pure reducer (spec 009) and
//! the runtime that executes them is spec 019's.
//!
//! # What is here, and what arrives later
//!
//! [`window`], [`shortcuts`], [`tray`], [`permissions`] and [`app_state`] are
//! this spec's. The setup hook in [`run`] is written in the order §3.1
//! requires, with the steps that belong to specs not yet built marked in
//! place, so the ordering constraint is visible rather than remembered:
//! logging (016), settings (014), exclusion (005) and the runtime (019).
//!
//! # `unsafe`
//!
//! Denied workspace-wide (spec 001 FR-005). The two exceptions in this crate
//! are the macOS window level in [`window`] and the CoreGraphics permission
//! calls in [`permissions`], each `#[allow(unsafe_code)]` on the smallest
//! possible block with a `// SAFETY:` note.

pub mod app_state;
pub mod permissions;
pub mod shortcuts;
pub mod tray;
pub mod window;

use tauri::Manager;

pub use app_state::AppState;
pub use window::{OVERLAY_LABEL, OverlayGeometry, OverlayWindowConfig, create_overlay_window};

/// What can go wrong in the shell itself.
///
/// Deliberately small and stringly-typed at the boundary: these are OS
/// failures with no useful structure, and spec 015 §3.5 forbids putting
/// anything from the screen into a log line, so an error here carries a kind
/// and a platform message, never content.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The overlay window could not be created or configured.
    #[error("overlay window: {0}")]
    Window(String),
    /// The tray icon or its menu could not be built.
    #[error("tray: {0}")]
    Tray(String),
    /// The setup hook failed before the app could run.
    #[error("setup: {0}")]
    Setup(String),
}

/// Build and run the application.
///
/// # Panics
///
/// Panics only if Tauri itself cannot start, which is not recoverable: there
/// is no UI in which to report it.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();

            // §3.1 fixes this order. Logging (016) and settings (014) come
            // first so that everything after them is observable and
            // configured; they are no-ops until those specs land.

            // 3. Create the overlay window, hidden and click-through.
            let _overlay = create_overlay_window(&handle, OverlayGeometry::default())?;

            // 4. Apply capture exclusion and record its verified status
            //    (spec 005). Until it lands the window stays hidden: §3.2
            //    forbids showing it before exclusion has been applied, and
            //    showing it here "for now" is exactly the shortcut that would
            //    make a degraded state invisible.

            // 5. Register shortcuts. Failures are reported, not fatal (§3.3).
            let report = shortcuts::register_shortcuts(&handle);
            for failure in report.failures() {
                eprintln!(
                    "shortcut unavailable: {} ({}): {}",
                    failure.action.label(),
                    failure.accelerator,
                    failure.error.as_deref().unwrap_or("unknown")
                );
            }
            handle
                .state::<AppState>()
                .set_shortcut_report(report.clone());

            // 6. Build the tray, which is the only chrome (§3.4).
            tray::build_tray(&handle, &report)?;

            // 7. Start the runtime disarmed (spec 019).

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri failed to start");
}
