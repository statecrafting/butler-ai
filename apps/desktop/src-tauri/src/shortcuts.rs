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
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// The five actions §3.3 defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutAction {
    /// Arm or disarm the capture pipeline (`Event::Arm` / `Event::Disarm`).
    ArmDisarm,
    /// Toggle the overlay between interactive and click-through.
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
            ShortcutAction::Interact => "Interact (toggle)",
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
    /// `DismissAnswer` is the one action deliberately absent: §3.3 binds `Esc`
    /// *while interactive*, which is window-local. Registering `Esc` globally
    /// would swallow it from every other application on the machine.
    ///
    /// `Interact` is a **toggle**, not a hold (spec 004 D-9). §3.3 originally
    /// gave it `Cmd+Option` / `Ctrl+Alt`, a bare modifier combination that no
    /// global shortcut can express, and a genuinely *held* modifier would need
    /// the Input Monitoring permission §3.5 refuses to request. The toggle is
    /// registrable, keeps the `Mod+Shift+<key>` shape of the other three, and
    /// uses `Space` rather than the mnemonic `I`: on Windows `RegisterHotKey`
    /// intercepts before the focused app, so `Ctrl+Shift+I` as a global
    /// default would take developer tools away from every browser.
    #[must_use]
    pub fn defaults() -> Vec<ShortcutBinding> {
        #[cfg(target_os = "macos")]
        let (arm, interact, visibility, ask) = (
            "Cmd+Shift+B",
            "Cmd+Shift+Space",
            "Cmd+Shift+H",
            "Cmd+Shift+Enter",
        );
        #[cfg(not(target_os = "macos"))]
        let (arm, interact, visibility, ask) = (
            "Ctrl+Shift+B",
            "Ctrl+Shift+Space",
            "Ctrl+Shift+H",
            "Ctrl+Shift+Enter",
        );

        vec![
            ShortcutBinding {
                action: ShortcutAction::ArmDisarm,
                accelerator: arm,
            },
            ShortcutBinding {
                action: ShortcutAction::Interact,
                accelerator: interact,
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

/// Which action, if any, a fired shortcut belongs to.
///
/// The plugin installs one handler for every shortcut, so something has to
/// route the event. Routing compares parsed accelerators rather than strings:
/// `Cmd+Shift+B` and `Shift+Cmd+B` are the same chord, and when spec 014 makes
/// bindings user-editable the table changes without this lookup changing.
#[must_use]
pub fn action_for(fired: &Shortcut) -> Option<ShortcutAction> {
    ShortcutBinding::defaults()
        .into_iter()
        .find(|b| {
            b.accelerator
                .parse::<Shortcut>()
                .is_ok_and(|parsed| parsed == *fired)
        })
        .map(|b| b.action)
}

/// Whether a shortcut event is the edge that should act.
///
/// §3.3: the Interact toggle acts on **press** only. A global shortcut reports
/// press *and* release, so acting on both would flip interactivity twice per
/// keystroke and leave it exactly where it started; the shortcut would look
/// dead while working perfectly. This is a function rather than an inline
/// `matches!` so the rule is covered by a test.
#[must_use]
pub fn is_actionable(state: ShortcutState) -> bool {
    matches!(state, ShortcutState::Pressed)
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
    use super::{
        ShortcutAction, ShortcutBinding, ShortcutOutcome, ShortcutReport, action_for, is_actionable,
    };
    use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

    #[test]
    fn defaults_cover_every_globally_bindable_action() {
        let bindings = ShortcutBinding::defaults();
        let actions: Vec<_> = bindings.iter().map(|b| b.action).collect();

        for expected in [
            ShortcutAction::ArmDisarm,
            ShortcutAction::Interact,
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
    }

    /// Spec 004 D-9. The regression this guards is the one D-5 recorded: an
    /// accelerator that is only modifiers cannot be a global shortcut, so a
    /// future rebinding must not reintroduce one.
    #[test]
    fn interact_is_bound_to_a_registrable_toggle() {
        let interact = ShortcutBinding::defaults()
            .into_iter()
            .find(|b| b.action == ShortcutAction::Interact)
            .expect("§3.3 binds Interact as a toggle (D-9)");

        assert_eq!(
            interact.accelerator,
            if cfg!(target_os = "macos") {
                "Cmd+Shift+Space"
            } else {
                "Ctrl+Shift+Space"
            },
            "§3.3's table and this default are one decision (D-9)"
        );
        assert!(
            interact.accelerator.parse::<Shortcut>().is_ok(),
            "a bare modifier combination does not parse; D-5's whole point"
        );
        assert_eq!(
            interact.action.label(),
            "Interact (toggle)",
            "the tray says toggle, because §3.2 no longer allows a hold"
        );
    }

    /// §3.3: the toggle acts on press only. Acting on release as well would
    /// flip it twice per keystroke, so the shortcut would appear to do nothing.
    #[test]
    fn the_toggle_acts_on_press_only() {
        assert!(is_actionable(ShortcutState::Pressed));
        assert!(!is_actionable(ShortcutState::Released));
    }

    #[test]
    fn every_default_routes_back_to_its_own_action() {
        for b in ShortcutBinding::defaults() {
            let parsed: Shortcut = b
                .accelerator
                .parse()
                .expect("checked by every_default_accelerator_parses");
            assert_eq!(
                action_for(&parsed),
                Some(b.action),
                "the handler must route {} to {:?}",
                b.accelerator,
                b.action
            );
        }
    }

    #[test]
    fn an_unregistered_chord_routes_nowhere() {
        // The handler is global. A chord this app never bound must fall
        // through rather than land on whichever action happens to be first.
        let stray: Shortcut = "Ctrl+Alt+F19".parse().expect("parses as a chord");
        assert_eq!(action_for(&stray), None);
    }

    #[test]
    fn no_two_defaults_share_an_accelerator() {
        // Two actions on one chord would make routing order-dependent, and
        // the second registration would fail at runtime for no visible reason.
        let mut seen: Vec<Shortcut> = Vec::new();
        for b in ShortcutBinding::defaults() {
            let parsed: Shortcut = b.accelerator.parse().expect("parses");
            assert!(
                !seen.contains(&parsed),
                "{} is bound twice in §3.3's table",
                b.accelerator
            );
            seen.push(parsed);
        }
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
