// Spec: specs/004-desktop-shell/spec.md

//! Global shortcuts.
//!
//! Spec 004 §3.3 fixes five actions and their per-platform defaults. All are
//! rebindable through settings (spec 014); until that lands,
//! [`ShortcutBinding::defaults`] is the table from the spec.
//!
//! Registration is best-effort by design: a shortcut can be taken by another
//! app, and §3.3 says the failure is surfaced in the tray and logged while the
//! app continues. So [`register_shortcuts`] returns a [`ShortcutReport`]
//! rather than an error, and the caller decides what to show.

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// The five actions §3.3 defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutAction {
    /// Arm or disarm the capture pipeline (`Event::Arm` / `Event::Disarm`).
    ArmDisarm,
    /// Hold to make the overlay interactive; release to go click-through.
    Interact,
    /// Hide or show the overlay without disarming.
    ToggleVisibility,
    /// Clear the current answer.
    DismissAnswer,
    /// Capture and ask now, bypassing change detection once.
    AskNow,
}

impl ShortcutAction {
    /// A stable, human-readable name for the tray menu and logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ShortcutAction::ArmDisarm => "Arm / disarm",
            ShortcutAction::Interact => "Interact (hold)",
            ShortcutAction::ToggleVisibility => "Toggle overlay visibility",
            ShortcutAction::DismissAnswer => "Dismiss answer",
            ShortcutAction::AskNow => "Ask now",
        }
    }
}

/// An action and the accelerator bound to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutBinding {
    /// What the shortcut does.
    pub action: ShortcutAction,
    /// The accelerator, in Tauri's parser syntax.
    pub accelerator: &'static str,
}

impl ShortcutBinding {
    /// The globally registrable subset of spec 004 §3.3's table.
    ///
    /// Two of the five actions are deliberately absent, for different reasons.
    ///
    /// `DismissAnswer` is window-local: §3.3 binds `Esc` *while interactive*.
    /// Registering `Esc` globally would swallow it from every other app.
    ///
    /// `Interact` is **unresolved, and left unbound rather than guessed**.
    /// §3.3 gives it `Cmd+Option` / `Ctrl+Alt`, which is a bare modifier
    /// combination. A global shortcut needs a non-modifier key, so that string
    /// does not parse and cannot be registered. Implementing a genuinely
    /// *held* modifier means monitoring global key events, which on macOS
    /// requires Input Monitoring or Accessibility, and §3.5 says the app must
    /// not request Accessibility. §3.2 does allow "held **or toggled**", so a
    /// registrable toggle is within the spec, but choosing its accelerator is
    /// picking a user-facing default the spec does not state. See spec 004 D-5.
    #[must_use]
    pub fn defaults() -> Vec<ShortcutBinding> {
        #[cfg(target_os = "macos")]
        let (arm, visibility, ask) = ("Cmd+Shift+B", "Cmd+Shift+H", "Cmd+Shift+Enter");
        #[cfg(not(target_os = "macos"))]
        let (arm, visibility, ask) = ("Ctrl+Shift+B", "Ctrl+Shift+H", "Ctrl+Shift+Enter");

        vec![
            ShortcutBinding {
                action: ShortcutAction::ArmDisarm,
                accelerator: arm,
            },
            ShortcutBinding {
                action: ShortcutAction::ToggleVisibility,
                accelerator: visibility,
            },
            ShortcutBinding {
                action: ShortcutAction::AskNow,
                accelerator: ask,
            },
        ]
    }
}

/// What happened to one binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutOutcome {
    /// The action that was being bound.
    pub action: ShortcutAction,
    /// The accelerator that was attempted.
    pub accelerator: String,
    /// `None` on success; the reason on failure.
    pub error: Option<String>,
}

impl ShortcutOutcome {
    /// Whether the binding is live.
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.error.is_none()
    }
}

/// The outcome of a whole registration pass (§3.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShortcutReport {
    /// One entry per attempted binding, in the order attempted.
    pub outcomes: Vec<ShortcutOutcome>,
}

impl ShortcutReport {
    /// The bindings that failed, for the tray menu and the log.
    #[must_use]
    pub fn failures(&self) -> Vec<&ShortcutOutcome> {
        self.outcomes
            .iter()
            .filter(|o| !o.is_registered())
            .collect()
    }

    /// Whether every binding is live.
    #[must_use]
    pub fn all_registered(&self) -> bool {
        self.outcomes.iter().all(ShortcutOutcome::is_registered)
    }
}

/// Register every default shortcut, continuing past failures.
///
/// §3.3: "Registration failure for any shortcut MUST be surfaced in the tray
/// menu and logged; the app continues." So this returns a report instead of
/// failing, and a shortcut that will not parse is reported the same way as one
/// the OS refused: from the user's side they are the same problem.
pub fn register_shortcuts<R: Runtime>(app: &AppHandle<R>) -> ShortcutReport {
    let mut outcomes = Vec::new();

    for binding in ShortcutBinding::defaults() {
        let error = match binding.accelerator.parse::<Shortcut>() {
            Err(e) => Some(format!("unparsable accelerator: {e}")),
            Ok(shortcut) => app
                .global_shortcut()
                .register(shortcut)
                .err()
                .map(|e| e.to_string()),
        };
        outcomes.push(ShortcutOutcome {
            action: binding.action,
            accelerator: binding.accelerator.to_owned(),
            error,
        });
    }

    ShortcutReport { outcomes }
}

#[cfg(test)]
mod tests {
    use super::{ShortcutAction, ShortcutBinding, ShortcutOutcome, ShortcutReport};

    #[test]
    fn defaults_cover_every_globally_bindable_action() {
        let bindings = ShortcutBinding::defaults();
        let actions: Vec<_> = bindings.iter().map(|b| b.action).collect();

        for expected in [
            ShortcutAction::ArmDisarm,
            ShortcutAction::ToggleVisibility,
            ShortcutAction::AskNow,
        ] {
            assert!(actions.contains(&expected), "§3.3 binds {expected:?}");
        }
        assert!(
            !actions.contains(&ShortcutAction::DismissAnswer),
            "Esc is window-local while interactive; a global binding would \
             swallow it from every other app"
        );
        assert!(
            !actions.contains(&ShortcutAction::Interact),
            "§3.3's Cmd+Option is a bare modifier and cannot be a global \
             shortcut; left unbound pending spec 004 D-5, not guessed at"
        );
    }

    #[test]
    fn defaults_use_the_platform_modifier() {
        let bindings = ShortcutBinding::defaults();
        let modifier = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        for b in bindings {
            assert!(
                b.accelerator.starts_with(modifier),
                "§3.3: {:?} should use {modifier} on this platform, got {}",
                b.action,
                b.accelerator
            );
        }
    }

    #[test]
    fn every_default_accelerator_parses() {
        // A default that will not parse is a bug here, not a user problem, so
        // it must fail in the test suite rather than at the tray menu.
        for b in ShortcutBinding::defaults() {
            assert!(
                b.accelerator
                    .parse::<tauri_plugin_global_shortcut::Shortcut>()
                    .is_ok(),
                "§3.3 default for {:?} does not parse: {}",
                b.action,
                b.accelerator
            );
        }
    }

    #[test]
    fn a_failed_registration_is_reported_and_not_fatal() {
        let report = ShortcutReport {
            outcomes: vec![
                ShortcutOutcome {
                    action: ShortcutAction::ArmDisarm,
                    accelerator: "Cmd+Shift+B".into(),
                    error: None,
                },
                ShortcutOutcome {
                    action: ShortcutAction::AskNow,
                    accelerator: "Cmd+Shift+Enter".into(),
                    error: Some("already registered by another app".into()),
                },
            ],
        };
        assert!(!report.all_registered());
        assert_eq!(report.failures().len(), 1, "§3.3: failures are surfaced");
        assert_eq!(report.failures()[0].action, ShortcutAction::AskNow);
    }

    #[test]
    fn an_empty_report_is_trivially_all_registered() {
        assert!(ShortcutReport::default().all_registered());
    }
}
