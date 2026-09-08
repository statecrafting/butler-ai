// Spec: specs/004-desktop-shell/spec.md

//! The one piece of shared state the shell hands to everything else.
//!
//! Spec 004 §3.1: `AppState` is `Send + Sync` and exposes only typed
//! accessors. It is deliberately small. The shell owns the OS surface; the
//! pipeline's state lives in `butler_core`'s reducer (spec 009), and the
//! runtime that drives it is spec 019's.
//!
//! Fields arrive with the specs that own them: the settings handle with 014,
//! the runtime handle with 019, the exclusion status with 005. What is here
//! now is what the shell itself needs, and the accessors are shaped so those
//! additions do not change the call sites that already exist.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use butler_core::settings::Settings;

use crate::exclusion::ExclusionStatus;

use crate::shortcuts::ShortcutReport;

/// Shared, typed application state.
///
/// `Send + Sync` by construction: every field is either atomic or behind a
/// `Mutex`, and nothing here holds a window handle, which is not `Send` on
/// either platform.
#[derive(Debug, Default)]
pub struct AppState {
    /// Whether the overlay is currently interactive (click-through off).
    ///
    /// Atomic rather than `Mutex` because the interaction shortcut toggles it
    /// on the OS event thread while the UI reads it (§3.3).
    interactive: AtomicBool,
    /// Whether the overlay is hidden by the visibility shortcut.
    hidden: AtomicBool,
    /// What happened when shortcuts were registered.
    ///
    /// §3.3: a registration failure is surfaced in the tray menu and logged,
    /// and the app continues, so the outcome has to be readable later.
    shortcuts: Mutex<ShortcutReport>,
    /// What capture exclusion last reported (spec 005).
    ///
    /// Never persisted: §3.5 says it is recomputed every launch, because a
    /// remembered `Verified` from a previous OS version is exactly the claim
    /// constitution §VI forbids.
    exclusion: Mutex<ExclusionStatus>,
    /// The current configuration (spec 014).
    ///
    /// The settings handle §3.1 names. One in-memory copy, replaced whole on
    /// a successful `UpdateSettings`: every reader sees a configuration that
    /// validated, never a half-applied one.
    settings: Mutex<Settings>,
}

impl AppState {
    /// A fresh state: not interactive, not hidden, no shortcuts registered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the overlay currently accepts clicks.
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.interactive.load(Ordering::Acquire)
    }

    /// Record that the overlay's interactivity changed.
    pub fn set_interactive(&self, interactive: bool) {
        self.interactive.store(interactive, Ordering::Release);
    }

    /// Whether the overlay is hidden by the visibility shortcut.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.hidden.load(Ordering::Acquire)
    }

    /// Record that the overlay was hidden or shown.
    pub fn set_hidden(&self, hidden: bool) {
        self.hidden.store(hidden, Ordering::Release);
    }

    /// Replace the shortcut registration report.
    pub fn set_shortcut_report(&self, report: ShortcutReport) {
        *self
            .shortcuts
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = report;
    }

    /// What capture exclusion last reported (spec 005 §3.5).
    #[must_use]
    pub fn exclusion(&self) -> ExclusionStatus {
        self.exclusion
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Record what the exclusion call or the self-test found.
    pub fn set_exclusion(&self, status: ExclusionStatus) {
        *self
            .exclusion
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = status;
    }

    /// The current configuration, cloned (spec 014).
    ///
    /// A poisoned lock returns the inner value rather than panicking, as for
    /// the shortcut report: this is read from command handlers, and a panic
    /// there would take out the IPC seam.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replace the configuration with one that has already validated.
    pub fn set_settings(&self, settings: Settings) {
        *self.settings.lock().unwrap_or_else(PoisonError::into_inner) = settings;
    }

    /// The shortcut registration report, cloned.
    ///
    /// A poisoned lock returns the inner value rather than panicking: this is
    /// read from the tray menu, and a panic there would take down the only
    /// chrome the app has.
    #[must_use]
    pub fn shortcut_report(&self) -> ShortcutReport {
        self.shortcuts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
