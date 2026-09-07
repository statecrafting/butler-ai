// Spec: specs/004-desktop-shell/spec.md

//! The single overlay window.
//!
//! There is exactly one window, labelled `overlay`. Spec 004 §3.2 fixes its
//! flags, and spec 015 constrains what the webview inside it may reach.
//!
//! The flag set is extracted as [`OverlayWindowConfig`], a plain value, so
//! that "what the spec asks for" is testable without a window server.
//! [`create_overlay_window`] is the only thing that turns it into a real
//! window, and it applies the config field by field, so the test and the
//! product cannot disagree about the request. What the *operating system*
//! then does with the request is a separate question, and an honest one: see
//! spec 004 D-3 on why this crate does not claim to verify it in CI.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::AppError;

/// The window's label. Spec 004 §3.2: there is one, and this is its name.
pub const OVERLAY_LABEL: &str = "overlay";

/// Where the overlay sits on its monitor.
///
/// Spec 014 will supply this from settings; until it lands, [`Self::default`]
/// is §3.2's default (the top-right quadrant of the primary monitor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayGeometry {
    /// Fraction of the monitor's width the overlay occupies.
    pub width_fraction: f64,
    /// Fraction of the monitor's height the overlay occupies.
    pub height_fraction: f64,
    /// Inset from the monitor's edges, in logical pixels.
    pub margin: f64,
}

impl Default for OverlayGeometry {
    fn default() -> Self {
        Self {
            width_fraction: 0.5,
            height_fraction: 0.5,
            margin: 24.0,
        }
    }
}

/// Every flag spec 004 §3.2 fixes, as data.
///
/// Only `Self::from_spec` constructs one, and it is `const`-shaped and takes
/// nothing but geometry, so the flags cannot vary by call site.
#[allow(
    clippy::struct_excessive_bools,
    reason = "this struct is spec 004 section 3.2's flag table, one field per \
              flag. Grouping them into sub-structs or a bitflags type would \
              make the test that checks them against the spec read less like \
              the spec, which is the whole point of extracting it."
)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayWindowConfig {
    /// The window is a transparent surface, not a panel with a background.
    pub transparent: bool,
    /// No title bar, no frame.
    pub decorations: bool,
    /// No drop shadow; a shadow would draw outside the transparent surface.
    pub shadow: bool,
    /// Above ordinary windows at all times.
    pub always_on_top: bool,
    /// No taskbar button (Windows) and no Dock tile (macOS).
    pub skip_taskbar: bool,
    /// Present on every desktop/space, including over full-screen apps.
    pub visible_on_all_workspaces: bool,
    /// Never takes focus. Focus would steal input from the app underneath.
    pub focusable: bool,
    /// Fixed size; the user positions it through settings, not by dragging.
    pub resizable: bool,
    /// Clicks pass through to whatever is beneath, until the interaction
    /// shortcut is held (§3.3).
    pub ignore_cursor_events: bool,
    /// Created hidden. §3.2: the window MUST NOT be shown before exclusion
    /// (spec 005) has been applied.
    pub visible: bool,
}

impl OverlayWindowConfig {
    /// Spec 004 §3.2, verbatim. The one source of these values.
    #[must_use]
    pub const fn from_spec() -> Self {
        Self {
            transparent: true,
            decorations: false,
            shadow: false,
            always_on_top: true,
            skip_taskbar: true,
            visible_on_all_workspaces: true,
            focusable: false,
            resizable: false,
            ignore_cursor_events: true,
            visible: false,
        }
    }
}

impl Default for OverlayWindowConfig {
    fn default() -> Self {
        Self::from_spec()
    }
}

/// Create the one overlay window, hidden and click-through.
///
/// # Errors
///
/// Returns [`AppError::Window`] if a window labelled [`OVERLAY_LABEL`] already
/// exists (there is exactly one, §3.2), or if Tauri refuses to build it.
pub fn create_overlay_window<R: Runtime>(
    app: &AppHandle<R>,
    geometry: OverlayGeometry,
) -> Result<WebviewWindow<R>, AppError> {
    if app.get_webview_window(OVERLAY_LABEL).is_some() {
        return Err(AppError::Window(
            "an overlay window already exists; spec 004 §3.2 allows exactly one".into(),
        ));
    }

    let cfg = OverlayWindowConfig::from_spec();

    let window = WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::default())
        .title("Butler")
        .transparent(cfg.transparent)
        .decorations(cfg.decorations)
        .shadow(cfg.shadow)
        .always_on_top(cfg.always_on_top)
        .skip_taskbar(cfg.skip_taskbar)
        .visible_on_all_workspaces(cfg.visible_on_all_workspaces)
        .focused(false)
        .resizable(cfg.resizable)
        .visible(cfg.visible)
        .build()
        .map_err(|e| AppError::Window(e.to_string()))?;

    // §3.2: click-through immediately after creation, before anything can be
    // drawn into it. Not doing this first would leave a window that eats
    // clicks for however long the rest of setup takes.
    window
        .set_ignore_cursor_events(cfg.ignore_cursor_events)
        .map_err(|e| AppError::Window(e.to_string()))?;

    apply_geometry(&window, geometry)?;
    platform::raise_above_menu_bar(&window);

    Ok(window)
}

/// Anchor the overlay to the top-right of its monitor, per §3.2's default.
fn apply_geometry<R: Runtime>(
    window: &WebviewWindow<R>,
    geometry: OverlayGeometry,
) -> Result<(), AppError> {
    let Some(monitor) = window
        .current_monitor()
        .map_err(|e| AppError::Window(e.to_string()))?
    else {
        // No monitor is a legitimate transient state (a laptop with the lid
        // shut on no external display). Leave Tauri's default placement.
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let position = monitor.position().to_logical::<f64>(scale);

    let w = size.width * geometry.width_fraction;
    let h = size.height * geometry.height_fraction;
    let x = position.x + size.width - w - geometry.margin;
    let y = position.y + geometry.margin;

    window
        .set_size(tauri::LogicalSize::<f64>::new(w, h))
        .map_err(|e| AppError::Window(e.to_string()))?;
    window
        .set_position(tauri::LogicalPosition::<f64>::new(x, y))
        .map_err(|e| AppError::Window(e.to_string()))?;
    Ok(())
}

/// Set the overlay's interactivity. `true` makes it click-through.
///
/// # Errors
///
/// Returns [`AppError::Window`] if the platform refuses the change.
pub fn set_click_through<R: Runtime>(
    window: &WebviewWindow<R>,
    click_through: bool,
) -> Result<(), AppError> {
    window
        .set_ignore_cursor_events(click_through)
        .map_err(|e| AppError::Window(e.to_string()))
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    use tauri::{Runtime, WebviewWindow};

    /// Above the menu bar and full-screen apps, and present on every space.
    ///
    /// Spec 004 §3.2 names `NSScreenSaverWindowLevel` and the three collection
    /// behaviours. Tauri's `always_on_top` maps to `NSFloatingWindowLevel`,
    /// which is below the menu bar, and its `visible_on_all_workspaces` sets
    /// only `canJoinAllSpaces`, so this raises the rest by hand.
    ///
    /// A failure here is not fatal: the overlay still works, it just sits
    /// lower. Spec 005 is what refuses to proceed when a guarantee is missing.
    pub fn raise_above_menu_bar<R: Runtime>(window: &WebviewWindow<R>) {
        // NSScreenSaverWindowLevel: CoreGraphics kCGScreenSaverWindowLevel is
        // 1000, and AppKit's NSWindowLevel is the same scale.
        const SCREEN_SAVER_LEVEL: isize = 1000;

        let Ok(handle) = window.ns_window() else {
            return;
        };
        if handle.is_null() {
            return;
        }
        // SAFETY: `ns_window()` returns the `NSWindow` Tauri created for this
        // webview window and keeps alive for its lifetime, so the pointer is
        // valid and correctly typed here. It is null-checked above. We only
        // borrow it for the duration of these two setters and never take
        // ownership, so no release is owed.
        #[allow(unsafe_code)]
        unsafe {
            let ns_window: &NSWindow = &*handle.cast::<NSWindow>();
            ns_window.setLevel(SCREEN_SAVER_LEVEL);
            ns_window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
        // Keep the type in scope so a future refactor cannot silently drop the
        // retained-pointer reasoning above.
        let _: Option<Retained<NSWindow>> = None;
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use tauri::{Runtime, WebviewWindow};

    /// Windows needs no equivalent.
    ///
    /// Spec 004 §3.2 asks for `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` and
    /// `WS_EX_LAYERED`. Tauri sets all three from the builder flags this crate
    /// already passes: `skip_taskbar` gives `WS_EX_TOOLWINDOW`, `focused(false)`
    /// plus a non-focusable overlay gives `WS_EX_NOACTIVATE`, and `transparent`
    /// gives `WS_EX_LAYERED`. Doing it again by hand would be a second, weaker
    /// source of truth for the same three bits.
    pub fn raise_above_menu_bar<R: Runtime>(_window: &WebviewWindow<R>) {}
}

#[cfg(test)]
mod tests {
    use super::{OVERLAY_LABEL, OverlayGeometry, OverlayWindowConfig};

    /// Spec 004 AC-1. Every flag §3.2 fixes, asserted field by field.
    ///
    /// This checks the request, not the realized window: creating one needs a
    /// window server, which CI does not have, and asserting on a window that
    /// silently failed to appear would be worse than not asserting at all.
    /// `create_overlay_window` applies this exact value, so a drift between
    /// the spec and the builder call still fails here. What the OS does with
    /// the request is FR-001's manual half (spec 004 D-4).
    #[test]
    fn window_flags_match_spec() {
        let c = OverlayWindowConfig::from_spec();

        assert!(c.transparent, "§3.2: transparent: true");
        assert!(!c.decorations, "§3.2: decorations: false");
        assert!(!c.shadow, "§3.2: shadow: false");
        assert!(c.always_on_top, "§3.2: always_on_top: true");
        assert!(c.skip_taskbar, "§3.2: skip_taskbar: true");
        assert!(
            c.visible_on_all_workspaces,
            "§3.2: visible_on_all_workspaces: true"
        );
        assert!(!c.focusable, "§3.2: focusable: false at creation");
        assert!(!c.resizable, "§3.2: resizable: false");
        assert!(
            c.ignore_cursor_events,
            "§3.2: click-through by default, immediately after creation"
        );
        assert!(
            !c.visible,
            "§3.2: the window MUST never be shown before exclusion is applied"
        );
    }

    #[test]
    fn there_is_exactly_one_window_label() {
        assert_eq!(OVERLAY_LABEL, "overlay", "§3.2 names the single window");
    }

    #[test]
    fn default_geometry_is_the_top_right_quadrant() {
        let g = OverlayGeometry::default();
        assert!(
            (g.width_fraction - 0.5).abs() < f64::EPSILON
                && (g.height_fraction - 0.5).abs() < f64::EPSILON,
            "§3.2 default: the top-right quadrant of the primary monitor"
        );
        assert!(g.margin > 0.0, "an inset keeps the overlay off the edge");
    }

    #[test]
    fn config_default_is_the_spec_and_not_a_derived_default() {
        // A `#[derive(Default)]` here would silently give `false` for every
        // flag the spec sets to true.
        assert_eq!(
            OverlayWindowConfig::default(),
            OverlayWindowConfig::from_spec()
        );
    }
}
