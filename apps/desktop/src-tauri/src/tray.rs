// Spec: specs/004-desktop-shell/spec.md

//! The tray icon, which is the app's only chrome.
//!
//! Spec 004 §3.4. The overlay has no title bar, no Dock tile and no taskbar
//! button, so when it is idle the tray is the *only* evidence the app is
//! running. That makes two things load-bearing: the icon must reflect the
//! runtime state honestly, and the menu must be able to reach every action,
//! including the ones whose shortcut failed to register (§3.3).

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};

use crate::AppError;
use crate::shortcuts::ShortcutReport;

/// What the tray icon is currently saying.
///
/// The pipeline's own states are spec 009's; this is the small projection of
/// them the tray can draw, named here so the tray does not depend on the
/// reducer's type before spec 019 wires them together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrayState {
    /// Not watching the screen.
    #[default]
    Disarmed,
    /// Watching, nothing in flight.
    ArmedIdle,
    /// A question is in flight.
    Inferencing,
    /// Running, but capture exclusion is not verified (spec 005).
    Degraded,
    /// Stopped on an error, waiting to retry.
    Fault,
}

impl TrayState {
    /// The tooltip text, which is the only place the state is spelled out.
    #[must_use]
    pub fn tooltip(self) -> &'static str {
        match self {
            TrayState::Disarmed => "Butler: disarmed",
            TrayState::ArmedIdle => "Butler: armed",
            TrayState::Inferencing => "Butler: thinking",
            TrayState::Degraded => "Butler: degraded (overlay may be captured)",
            TrayState::Fault => "Butler: error, retrying",
        }
    }
}

/// The tray menu's item ids. Stable strings, because the event handler
/// matches on them and spec 011 will route them to the runtime.
pub mod ids {
    /// Arm or disarm the pipeline.
    pub const ARM_DISARM: &str = "arm_disarm";
    /// Show or hide the overlay.
    pub const TOGGLE_OVERLAY: &str = "toggle_overlay";
    /// Capture and ask immediately.
    pub const ASK_NOW: &str = "ask_now";
    /// Open the settings panel.
    pub const SETTINGS: &str = "settings";
    /// Re-run the capture-exclusion self-test (spec 005).
    pub const SELF_TEST: &str = "self_test";
    /// Write a diagnostics bundle (spec 016).
    pub const DIAGNOSTICS: &str = "diagnostics";
    /// Quit.
    pub const QUIT: &str = "quit";
}

/// Build the tray icon and its menu (§3.4).
///
/// # Errors
///
/// Returns [`AppError::Tray`] if the menu or icon cannot be built, which on
/// both platforms means there is no usable tray and therefore no chrome.
pub fn build_tray<R: Runtime>(app: &AppHandle<R>, report: &ShortcutReport) -> Result<(), AppError> {
    let menu = build_menu(app, report)?;

    let mut builder = TrayIconBuilder::with_id("butler-tray")
        .menu(&menu)
        .tooltip(TrayState::default().tooltip())
        .show_menu_on_left_click(true);

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder
        .build(app)
        .map(|_| ())
        .map_err(|e| AppError::Tray(e.to_string()))
}

/// The menu §3.4 specifies, plus a disabled section naming any shortcut that
/// failed to register (§3.3).
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    report: &ShortcutReport,
) -> Result<Menu<R>, AppError> {
    let e = |err: tauri::Error| AppError::Tray(err.to_string());

    let arm = MenuItem::with_id(app, ids::ARM_DISARM, "Arm", true, None::<&str>).map_err(e)?;
    let overlay = MenuItem::with_id(app, ids::TOGGLE_OVERLAY, "Hide overlay", true, None::<&str>)
        .map_err(e)?;
    let ask = MenuItem::with_id(app, ids::ASK_NOW, "Ask now", true, None::<&str>).map_err(e)?;
    let settings =
        MenuItem::with_id(app, ids::SETTINGS, "Settings...", true, None::<&str>).map_err(e)?;
    let self_test = MenuItem::with_id(
        app,
        ids::SELF_TEST,
        "Run exclusion self-test",
        true,
        None::<&str>,
    )
    .map_err(e)?;
    let diagnostics = MenuItem::with_id(
        app,
        ids::DIAGNOSTICS,
        "Save diagnostics bundle...",
        true,
        None::<&str>,
    )
    .map_err(e)?;
    let quit = MenuItem::with_id(app, ids::QUIT, "Quit Butler", true, None::<&str>).map_err(e)?;
    let sep = PredefinedMenuItem::separator(app).map_err(e)?;

    let menu = Menu::new(app).map_err(e)?;
    menu.append(&arm).map_err(e)?;
    menu.append(&overlay).map_err(e)?;
    menu.append(&ask).map_err(e)?;
    menu.append(&sep).map_err(e)?;
    menu.append(&settings).map_err(e)?;
    menu.append(&self_test).map_err(e)?;
    menu.append(&diagnostics).map_err(e)?;

    // §3.3: a shortcut the OS refused is invisible otherwise. The user would
    // press it, nothing would happen, and there would be nowhere to find out
    // why. A disabled submenu says so without offering a false action.
    let failures = report.failures();
    if !failures.is_empty() {
        let unavailable = Submenu::new(app, "Shortcuts unavailable", true).map_err(e)?;
        for outcome in failures {
            let text = format!("{} ({})", outcome.action.label(), outcome.accelerator);
            let item = MenuItem::new(app, text, false, None::<&str>).map_err(e)?;
            unavailable.append(&item).map_err(e)?;
        }
        menu.append(&sep).map_err(e)?;
        menu.append(&unavailable).map_err(e)?;
    }

    menu.append(&sep).map_err(e)?;
    menu.append(&quit).map_err(e)?;
    Ok(menu)
}
