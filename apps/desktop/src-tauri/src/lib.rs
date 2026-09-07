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

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

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

/// Route a fired global shortcut to its effect (§3.3).
///
/// The plugin installs one handler for every shortcut, so this is where the
/// table in [`shortcuts`] turns into behaviour. Exactly one of the four
/// registered actions is this spec's own: `Interact` flips this crate's window
/// between click-through and interactive. The other three drive the pipeline,
/// whose runtime (spec 019) and command layer (spec 011) do not exist yet, so
/// they are matched by name and left visibly unhandled. A catch-all `_ => {}`
/// here would let a future action be swallowed without anyone noticing.
fn on_shortcut<R: Runtime>(app: &AppHandle<R>, fired: &Shortcut, state: ShortcutState) {
    // §3.3: the toggle acts on press only. The plugin reports both edges, and
    // acting on each would flip interactivity twice per keystroke.
    if !shortcuts::is_actionable(state) {
        return;
    }
    let Some(action) = shortcuts::action_for(fired) else {
        return;
    };

    #[allow(
        clippy::match_same_arms,
        reason = "the three unhandled arms have the same empty body today but \
                  are three different decisions, owned by three different \
                  specs: 019 supplies the runtime events, 005 is what makes \
                  showing the overlay legal under section 3.2, and 012 binds \
                  Esc in the webview. Merging them into one arm, or into a \
                  catch-all, is what would let a fifth action be swallowed \
                  silently when it is added."
    )]
    match action {
        shortcuts::ShortcutAction::Interact => toggle_interactive(app),

        // Spec 019 (`Event::Arm` / `Event::Disarm`, `Event::ForceCapture`).
        shortcuts::ShortcutAction::ArmDisarm | shortcuts::ShortcutAction::AskNow => {}

        // §3.2 forbids showing the overlay before capture exclusion (spec 005)
        // has been applied, and 005 has not landed, so there is no state in
        // which showing it here would be correct. This arrives with 005.
        shortcuts::ShortcutAction::ToggleVisibility => {}

        // Window-local while interactive, never registered globally, so the
        // handler cannot receive it. Spec 012 binds it in the webview.
        shortcuts::ShortcutAction::DismissAnswer => {}
    }
}

/// Flip the overlay between click-through and interactive (§3.2, §3.3).
///
/// The stored flag is updated only after the platform accepts the change: if
/// `set_ignore_cursor_events` fails and we recorded the new value anyway, the
/// next press would toggle away from a state the window was never in, and the
/// overlay would be stuck eating clicks with no way back.
fn toggle_interactive<R: Runtime>(app: &AppHandle<R>) {
    let Some(overlay) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    let state = app.state::<AppState>();
    let interactive = !state.is_interactive();

    match window::set_click_through(&overlay, !interactive) {
        Ok(()) => state.set_interactive(interactive),
        Err(e) => eprintln!("interact toggle refused by the platform: {e}"),
    }
}

/// Build and run the application.
///
/// # Panics
///
/// Panics only if Tauri itself cannot start, which is not recoverable: there
/// is no UI in which to report it.
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, fired, event| on_shortcut(app, fired, event.state))
                .build(),
        )
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();

            // §3.1 fixes this order. Logging (016) and settings (014) come
            // first so that everything after them is observable and
            // configured; they are no-ops until those specs land.

            // §3.2, FR-004: become an accessory app before any window exists,
            // so a Dock tile never appears even for the moment it would take
            // to create one. A failure is reported and not fatal, matching
            // `raise_above_menu_bar`: the app still works, it is just more
            // visible than the spec wants. Nothing here is a privacy
            // guarantee; spec 005 owns the one that is.
            #[cfg(target_os = "macos")]
            if let Err(e) = handle.set_activation_policy(window::MACOS_ACTIVATION_POLICY) {
                eprintln!("could not become an accessory app (§3.2): {e}");
            }

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
