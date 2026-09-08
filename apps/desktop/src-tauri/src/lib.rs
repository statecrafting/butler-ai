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
pub mod commands;
pub mod diagnostics;
pub mod events;
pub mod exclusion;
pub mod logging;
pub mod permissions;
pub mod runtime;
pub mod settings_store;
pub mod shortcuts;
pub mod tray;
pub mod window;

use butler_core::settings::DiagnosticsLevel;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

pub use app_state::AppState;
pub use window::{OVERLAY_LABEL, OverlayGeometry, OverlayWindowConfig, create_overlay_window};

/// Keeps the logging guard alive for the process's lifetime.
///
/// Managed rather than dropped on the floor: dropping the guard stops the
/// non-blocking writer's thread, and the log would go quiet with no error
/// anywhere. Wrapping it makes that a type Tauri holds rather than a local
/// that a future edit could shorten the life of.
#[derive(Debug)]
pub struct LogHandle(pub Option<logging::Guard>);

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
    /// An IPC payload could not be delivered to the overlay (spec 011).
    #[error("ipc: {0}")]
    Ipc(String),
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
    // Spec 011 §3.2: one builder is the source of both the command handler
    // installed here and the TypeScript the overlay imports. The exporter
    // binary calls the same function, so the two cannot describe different
    // contracts.
    let ipc = commands::builder();

    tauri::Builder::default()
        .invoke_handler(ipc.invoke_handler())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, fired, event| on_shortcut(app, fired, event.state))
                .build(),
        )
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();

            // §3.1 fixes this order.

            // 1. Logging (spec 016), before anything else, so everything
            //    after it is observable. The level is the shipped default
            //    here rather than the user's: settings have not been read
            //    yet, and reading them first would mean the read itself was
            //    unobservable. A level change takes effect next launch.
            let log_guard = match logging::init(DiagnosticsLevel::default()) {
                Ok(guard) => {
                    logging::install_panic_hook();
                    Some(guard)
                }
                Err(e) => {
                    eprintln!("logging unavailable, continuing without it: {e}");
                    None
                }
            };
            handle.manage(LogHandle(log_guard));

            // 2. Load settings (spec 014). A missing file is the defaults; an
            //    unreadable one is moved aside and the defaults are used, so
            //    a bad file cannot stop the app from starting. The outcome is
            //    reported rather than swallowed: spec 014 §3.2 wants the user
            //    told once, and until spec 012 has a notice for it, stderr is
            //    where "told" happens.
            match settings_store::SettingsStore::platform().and_then(|store| store.load()) {
                Ok((settings, outcome)) => {
                    if let settings_store::LoadOutcome::Recovered { backup } = &outcome {
                        eprintln!(
                            "settings unreadable; defaults in use, previous file kept at {}",
                            backup.display()
                        );
                    }
                    handle.state::<AppState>().set_settings(settings);
                }
                Err(e) => eprintln!("settings could not be read, using defaults: {e}"),
            }

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
            let overlay = create_overlay_window(&handle, OverlayGeometry::default())?;

            // 4. Apply capture exclusion, verify it, and only then decide
            //    whether the overlay may be seen at all (spec 005 §2, §3.5).
            //
            //    The order is the whole point. `apply_exclusion` returns
            //    `Applied`, never `Verified`: nothing has looked at a frame
            //    yet. The self-test is what replaces that request with a
            //    measurement, and `show_if_permitted` is the only path that
            //    turns the overlay on.
            let applied = match exclusion::apply_exclusion(&overlay) {
                Ok(status) => status,
                Err(e) => {
                    eprintln!("capture exclusion could not be applied: {e}");
                    exclusion::ExclusionStatus::Unsupported {
                        reason: e.to_string(),
                    }
                }
            };

            let source = butler_capture::platform_source();
            let status = exclusion::verify_exclusion(&overlay, &source, &applied);
            // §3.5: the status reaches the overlay's status strip, and the
            // strip is what the user reads before trusting the product.
            let _ = events::emit(
                &handle,
                &butler_core::ipc::UiEvent::SelfTestResult {
                    verdict: status.summary(),
                },
            );
            let state = handle.state::<AppState>();
            state.set_exclusion(status.clone());

            let allow_degraded = state.settings().privacy.allow_degraded_mode;
            match window::show_if_permitted(&overlay, &status, allow_degraded) {
                Ok(true) => {}
                Ok(false) => eprintln!(
                    "the overlay stays hidden: capture exclusion is {}, and degraded mode is off",
                    status.summary_name()
                ),
                Err(e) => eprintln!("the overlay could not be shown: {e}"),
            }

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
