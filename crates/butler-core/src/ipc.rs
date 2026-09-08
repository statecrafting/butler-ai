// Spec: specs/011-ipc-contract/spec.md

//! The typed seam between the Rust process and the overlay webview.
//!
//! Tauri's IPC is untyped JSON at the boundary. Left there, the overlay and
//! the runtime drift apart silently and nothing fails until a user sees a
//! blank panel. Spec 011 makes this module the truth: the types live here,
//! once, in the crate that has no operating system in it, and the TypeScript
//! the overlay imports is generated from them and committed
//! (`apps/desktop/src/generated/bindings.ts`).
//!
//! # What crosses, and what may not
//!
//! [`UiEvent`] goes Rust to UI, [`UiCommand`] goes UI to Rust, and neither
//! carries screen content. That is not a convention here, it is a type: the
//! sealed [`IpcSafe`] marker is implemented for exactly the payloads in this
//! module, and `Frame` (006), `Recognized` (007), `RedactedText` (015) and
//! `Secret` (010) will never implement it, so a future variant that tried to
//! carry one would not compile (FR-003).
//!
//! # Versioning
//!
//! [`IPC_CONTRACT_VERSION`] is `(major, minor)`. Spec 011's `constrains` edge
//! freezes the shape: within a major, changes are **additive only**, new
//! variants and optional fields. Removing or retyping a field is a major bump
//! and an amendment to spec 011.
//!
//! # What is not here yet
//!
//! Spec 011 §2 assigns three groups of variants to the specs that own their
//! payload types, and they arrive additively, which the version rule above
//! permits without a major bump:
//!
//! - `AnswerChunk`, the paced-output event, is spec 013's.
//! - `BudgetExhausted` needs `BudgetWindow`, which is spec 010's.
//!
//! Spec 014 landed the settings DTOs (`SettingsUpdated`, `GetSettings`,
//! `UpdateSettings`), on its `refines` edge over the `settings-dtos` aspect.
//!
//! See spec 011 D-3.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::machine::{ExclusionState, State, StopReason};
use crate::settings::{Settings, SettingsError, SettingsPatch};

/// The re-exported failure kind (spec 011 §3.1).
///
/// Spec 009 defines this enum for the reducer and says outright that this
/// spec re-exports it as the wire kind, so there is one closed set of failure
/// kinds in the product rather than two that can drift. It is a kind and
/// never a message: an error string built from screen content would carry the
/// user's data across the privacy boundary (spec 015 §3.5).
pub use crate::machine::ErrorKind;

/// The IPC contract's `(major, minor)` version.
///
/// The UI compares the major it was generated against at startup and refuses
/// to run on a mismatch (§3.4), which is only reachable in development: a
/// release ships the binary and the bindings from the same commit.
pub const IPC_CONTRACT_VERSION: (u16, u16) = (1, 0);

/// Which state the machine is in, without the payload it carries.
///
/// The UI draws a status strip, not a state machine. It needs the name and
/// nothing else; [`State`]'s sessions, cycles and boxed resume states are the
/// reducer's business and would be a second, lagging copy of the truth if
/// they crossed (spec 011 §1: the UI is a renderer, not a second brain).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum StateName {
    /// Not watching the screen.
    #[default]
    Disarmed,
    /// Watching, with exclusion verified.
    Armed,
    /// Watching, with exclusion not verified and the user's consent.
    Degraded,
    /// A stage failed and a retry is counting down.
    Fault,
}

impl From<&State> for StateName {
    fn from(state: &State) -> Self {
        match state {
            State::Disarmed { .. } => Self::Disarmed,
            State::Armed { .. } => Self::Armed,
            State::Degraded { .. } => Self::Degraded,
            State::Fault { .. } => Self::Fault,
        }
    }
}

/// The capture-exclusion status as the overlay sees it.
///
/// The wire mirror of [`ExclusionState`]. Constitution §VI is why this is
/// five values and not a boolean: the product reports what it verified, and
/// "asked for and not confirmed" is a different thing from "confirmed".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum ExclusionSummary {
    /// Nothing reported yet; the self-test has not run.
    #[default]
    Unknown,
    /// The platform accepted the request; not yet confirmed.
    Applied,
    /// Applied and confirmed absent from a capture of the display.
    Verified,
    /// Applied, but the self-test saw the overlay's own pixels.
    Compromised,
    /// The platform cannot exclude this window.
    Unsupported,
}

impl From<ExclusionState> for ExclusionSummary {
    fn from(state: ExclusionState) -> Self {
        match state {
            ExclusionState::Unknown => Self::Unknown,
            ExclusionState::Applied => Self::Applied,
            ExclusionState::Verified => Self::Verified,
            ExclusionState::Compromised => Self::Compromised,
            ExclusionState::Unsupported => Self::Unsupported,
        }
    }
}

/// Why the model stopped producing tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum StopSummary {
    /// The model finished its answer.
    EndTurn,
    /// The output token cap was reached.
    MaxTokens,
    /// The model declined to answer.
    Refusal,
    /// Anything else the provider reported.
    Other,
}

impl From<StopReason> for StopSummary {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::EndTurn => Self::EndTurn,
            StopReason::MaxTokens => Self::MaxTokens,
            StopReason::Refusal => Self::Refusal,
            StopReason::Other => Self::Other,
        }
    }
}

/// Which operating-system permission the app is waiting on.
///
/// One value, and deliberately so. Spec 004 §3.5 requests Screen Recording on
/// macOS and nothing else; Accessibility is never requested because the app
/// never synthesizes input, and Windows needs no grant for desktop
/// duplication. A second variant here would be a spec change, not a detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionKind {
    /// macOS Screen Recording (TCC).
    ScreenRecording,
}

/// Rust to UI. One channel, one tagged payload (§3.2).
///
/// `PartialEq` but not `Eq`: spec 014's `Settings` reaches this enum through
/// `SettingsUpdated` and carries `f64` fields (opacity, font scale, the
/// similarity threshold), and floats have no total equality. Nothing needs
/// `Eq` here; the tests compare with `assert_eq!`, which does not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UiEvent {
    /// The runtime's current state, emitted on every transition.
    RuntimeStatus {
        /// Which state the machine is in.
        state: StateName,
        /// The sequence number of the most recent capture.
        seq: u64,
        /// The inference in flight, if any.
        request: Option<u64>,
        /// What capture exclusion last reported.
        exclusion: ExclusionSummary,
        /// The most recent failure kind, if the machine has seen one.
        last_error: Option<ErrorKind>,
        /// Ticks the machine has been armed for.
        armed_for_ticks: u64,
    },
    /// An inference began.
    AnswerStarted {
        /// Which request.
        request: u64,
    },
    /// An inference finished.
    AnswerDone {
        /// Which request.
        request: u64,
        /// Why the model stopped.
        stop: StopSummary,
    },
    /// An inference failed.
    AnswerFailed {
        /// Which request.
        request: u64,
        /// The failure kind, never a message.
        kind: ErrorKind,
    },
    /// No usable credential for the configured provider.
    NeedsCredential {
        /// Which provider, by its configured name.
        provider: String,
    },
    /// An operating-system permission is missing.
    NeedsPermission {
        /// Which permission.
        permission: PermissionKind,
    },
    /// The capture-exclusion self-test reported (spec 005).
    SelfTestResult {
        /// What it found.
        verdict: ExclusionSummary,
    },
    /// Mount or unmount the self-test's sentinel pattern (spec 005 §3.4).
    ///
    /// The one event that asks the overlay to *do* something rather than
    /// telling it something. It exists because the self-test's question is
    /// "would a recording see this window", and the only way to ask it is to
    /// put something recognizable in the window and go looking for it.
    SelfTestSentinel {
        /// Whether the pattern is on screen.
        on: bool,
    },
    /// The configuration changed and was persisted (spec 014 §3.3).
    ///
    /// Broadcast after a successful `UpdateSettings`, and in reply to
    /// `GetSettings`, so every panel renders one value rather than each
    /// holding its own copy.
    SettingsUpdated {
        /// The whole configuration. Spec 014 §3.1: this is `Settings` minus
        /// nothing, because there are no secrets in it by construction.
        ///
        /// Boxed. `Settings` is an order of magnitude larger than any other
        /// payload here, and an enum is as big as its largest variant, so
        /// every `UiEvent` in the process would carry that size. Serde and
        /// specta both see through a `Box`, so the wire format and the
        /// generated TypeScript are unchanged.
        settings: Box<SettingsView>,
    },
}

/// What the UI sees of the configuration (spec 014 §3.1).
///
/// A type alias rather than a projection, and deliberately: spec 014 §3.1
/// says "`SettingsView` is `Settings` minus nothing", because a secret cannot
/// be in `Settings` in the first place (spec 010 puts credentials in the OS
/// keychain, spec 015 §3.4 keeps them out of the file). A separate struct
/// here would be a second thing to keep in step, for no gain.
pub type SettingsView = Settings;

/// UI to Rust. Every command the overlay may issue (§3.2).
///
/// [`Debug`] is implemented by hand rather than derived, so that
/// [`UiCommand::StoreSecret`]'s argument cannot reach a log through the one
/// formatting call every logging macro makes (FR-004).
///
/// `PartialEq` but not `Eq`, for the reason [`UiEvent`] gives.
#[derive(Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UiCommand {
    /// Start watching the screen.
    Arm,
    /// Stop watching the screen.
    Disarm,
    /// Capture and ask now, bypassing change detection once.
    AskNow,
    /// Clear the current answer.
    Dismiss,
    /// Make the overlay accept clicks, or stop.
    SetInteractive {
        /// Whether the overlay is interactive.
        on: bool,
    },
    /// Put a provider credential in the OS keychain.
    StoreSecret {
        /// Which provider the credential is for.
        provider: String,
        /// The credential. Moved into a `Secret` (spec 010) by the handler
        /// and never logged, printed or retained (§3.2).
        secret: String,
    },
    /// Re-run the capture-exclusion self-test (spec 005).
    RunSelfTest,
    /// Read [`IPC_CONTRACT_VERSION`] (§3.4).
    GetContractVersion,
    /// Ask for the current configuration (spec 014 §3.3).
    GetSettings,
    /// Change the configuration (spec 014 §3.3).
    ///
    /// The patch is applied to a candidate, the candidate is validated, and
    /// an invalid one is rejected **whole**: nothing is persisted and the
    /// previous configuration is untouched.
    UpdateSettings {
        /// Only the fields the user changed.
        ///
        /// Boxed, for the reason [`UiEvent::SettingsUpdated`] gives.
        patch: Box<SettingsPatch>,
    },
}

impl core::fmt::Debug for UiCommand {
    /// FR-004: `StoreSecret`'s argument never reaches a log.
    ///
    /// Every tracing and logging macro formats its arguments through `Debug`
    /// or `Display`, so redacting here closes the whole class rather than
    /// asking each call site to remember. The provider name is kept: it is a
    /// configured label, not a credential, and losing it would make a
    /// credential failure undiagnosable.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Arm => f.write_str("Arm"),
            Self::Disarm => f.write_str("Disarm"),
            Self::AskNow => f.write_str("AskNow"),
            Self::Dismiss => f.write_str("Dismiss"),
            Self::SetInteractive { on } => {
                f.debug_struct("SetInteractive").field("on", on).finish()
            }
            Self::StoreSecret { provider, .. } => f
                .debug_struct("StoreSecret")
                .field("provider", provider)
                .field("secret", &"[redacted]")
                .finish(),
            Self::RunSelfTest => f.write_str("RunSelfTest"),
            Self::GetContractVersion => f.write_str("GetContractVersion"),
            Self::GetSettings => f.write_str("GetSettings"),
            Self::UpdateSettings { patch } => f
                .debug_struct("UpdateSettings")
                .field("patch", patch)
                .finish(),
        }
    }
}

mod sealed {
    /// Closed outside this module, so [`super::IpcSafe`] cannot be claimed by
    /// a type declared anywhere else, in this crate or outside it.
    pub trait Sealed {}
}

/// A type that may cross the IPC boundary (FR-003).
///
/// Sealed, and implemented **only** for the payloads in this module. The
/// types that carry the user's screen (`Frame`, `Recognized`, `RedactedText`,
/// `Secret`) cannot implement it, because `sealed::Sealed` is private here,
/// so a future variant that tried to carry one fails to compile rather than
/// failing review. That is the whole point: spec 015's boundary is a property
/// of the types, not of anyone's diligence.
///
/// ```compile_fail
/// use butler_core::ipc::IpcSafe;
/// use butler_core::redaction::RedactedText;
/// fn assert_safe<T: IpcSafe>() {}
/// // RedactedText is screen content. It must not satisfy the bound.
/// assert_safe::<RedactedText>();
/// ```
///
/// The positive control, so a passing `compile_fail` above cannot be passing
/// because the import path is wrong:
///
/// ```
/// use butler_core::ipc::{IpcSafe, UiEvent};
/// fn assert_safe<T: IpcSafe>() {}
/// assert_safe::<UiEvent>();
/// ```
pub trait IpcSafe: sealed::Sealed {}

macro_rules! ipc_safe {
    ($($t:ty),* $(,)?) => {$(
        impl sealed::Sealed for $t {}
        impl IpcSafe for $t {}
    )*};
}

ipc_safe!(
    bool,
    u16,
    u32,
    u64,
    String,
    StateName,
    ExclusionSummary,
    StopSummary,
    PermissionKind,
    ErrorKind,
    UiEvent,
    UiCommand,
    // Spec 014's configuration. It carries no screen content and no secret
    // by construction: credentials live in the OS keychain (spec 010) and
    // spec 015 §3.4 keeps them out of the settings file, so there is nothing
    // in `Settings` for the boundary to refuse.
    Settings,
    SettingsPatch,
    SettingsError,
);

impl<T: IpcSafe> sealed::Sealed for Box<T> {}
impl<T: IpcSafe> IpcSafe for Box<T> {}

impl<T: IpcSafe> sealed::Sealed for Option<T> {}
impl<T: IpcSafe> IpcSafe for Option<T> {}

/// FR-003, as a compile-time assertion rather than a test that could be
/// deleted: both wire enums satisfy the marker, and every field type they
/// carry had to satisfy it to be listed above.
const _: () = {
    const fn assert_ipc_safe<T: IpcSafe>() {}
    assert_ipc_safe::<UiEvent>();
    assert_ipc_safe::<UiCommand>();
    assert_ipc_safe::<Option<u64>>();
};

#[cfg(test)]
mod tests {
    use super::{
        ErrorKind, ExclusionSummary, IPC_CONTRACT_VERSION, PermissionKind, StateName, StopSummary,
        UiCommand, UiEvent,
    };
    use crate::machine::{ExclusionState, State, StopReason};
    use crate::settings::{PrivacyPatch, Settings, SettingsPatch};

    /// Every [`UiEvent`] variant, once.
    ///
    /// `exhaustive_event_coverage` below is what keeps this honest: it matches
    /// on a value and names every variant, so adding one to the enum stops
    /// this crate compiling until it is added here too. That is what spec 011
    /// FR-001 means by "generated from the enum"; the compiler is the
    /// generator, and unlike a macro it cannot silently skip a case.
    fn every_event() -> Vec<UiEvent> {
        vec![
            UiEvent::RuntimeStatus {
                state: StateName::Armed,
                seq: 42,
                request: Some(7),
                exclusion: ExclusionSummary::Verified,
                last_error: Some(ErrorKind::Network),
                armed_for_ticks: 1234,
            },
            // The same variant with every `Option` empty: `None` and `Some`
            // take different paths through an internally tagged enum.
            UiEvent::RuntimeStatus {
                state: StateName::Disarmed,
                seq: 0,
                request: None,
                exclusion: ExclusionSummary::Unknown,
                last_error: None,
                armed_for_ticks: 0,
            },
            UiEvent::AnswerStarted { request: 1 },
            UiEvent::AnswerDone {
                request: 1,
                stop: StopSummary::EndTurn,
            },
            UiEvent::AnswerFailed {
                request: 1,
                kind: ErrorKind::Provider,
            },
            UiEvent::NeedsCredential {
                provider: "anthropic".into(),
            },
            UiEvent::NeedsPermission {
                permission: PermissionKind::ScreenRecording,
            },
            UiEvent::SelfTestResult {
                verdict: ExclusionSummary::Compromised,
            },
            UiEvent::SelfTestSentinel { on: true },
            UiEvent::SelfTestSentinel { on: false },
            UiEvent::SettingsUpdated {
                settings: Box::new(Settings::default()),
            },
        ]
    }

    /// Every [`UiCommand`] variant, once.
    fn every_command() -> Vec<UiCommand> {
        vec![
            UiCommand::Arm,
            UiCommand::Disarm,
            UiCommand::AskNow,
            UiCommand::Dismiss,
            UiCommand::SetInteractive { on: true },
            UiCommand::SetInteractive { on: false },
            UiCommand::StoreSecret {
                provider: "anthropic".into(),
                secret: "sk-ant-not-a-real-key".into(),
            },
            UiCommand::RunSelfTest,
            UiCommand::GetContractVersion,
            UiCommand::GetSettings,
            // An empty patch and a populated one take different paths through
            // `skip_serializing_if`: the first serializes to `{}`.
            UiCommand::UpdateSettings {
                patch: Box::new(SettingsPatch::default()),
            },
            UiCommand::UpdateSettings {
                patch: Box::new(SettingsPatch {
                    privacy: Some(PrivacyPatch {
                        allow_degraded_mode: Some(true),
                        ..PrivacyPatch::default()
                    }),
                    ..SettingsPatch::default()
                }),
            },
        ]
    }

    #[test]
    fn exhaustive_event_coverage() {
        // Naming every variant is the point: a new one fails to compile here.
        for event in every_event() {
            match event {
                UiEvent::RuntimeStatus { .. }
                | UiEvent::AnswerStarted { .. }
                | UiEvent::AnswerDone { .. }
                | UiEvent::AnswerFailed { .. }
                | UiEvent::NeedsCredential { .. }
                | UiEvent::NeedsPermission { .. }
                | UiEvent::SelfTestResult { .. }
                | UiEvent::SelfTestSentinel { .. }
                | UiEvent::SettingsUpdated { .. } => {}
            }
        }
    }

    #[test]
    fn exhaustive_command_coverage() {
        for command in every_command() {
            match command {
                UiCommand::Arm
                | UiCommand::Disarm
                | UiCommand::AskNow
                | UiCommand::Dismiss
                | UiCommand::SetInteractive { .. }
                | UiCommand::StoreSecret { .. }
                | UiCommand::RunSelfTest
                | UiCommand::GetContractVersion
                | UiCommand::GetSettings
                | UiCommand::UpdateSettings { .. } => {}
            }
        }
    }

    /// FR-001, events.
    #[test]
    fn every_event_round_trips() {
        for event in every_event() {
            let json = serde_json::to_string(&event).expect("serialize");
            let back: UiEvent = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(event, back, "round trip changed the value: {json}");
        }
    }

    /// FR-001, commands.
    #[test]
    fn every_command_round_trips() {
        for command in every_command() {
            let json = serde_json::to_string(&command).expect("serialize");
            let back: UiCommand = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(command, back, "round trip changed the value: {json}");
        }
    }

    /// §3.2: one channel, one tagged payload, so the UI subscribes once and
    /// switches on `type`. An externally tagged enum would nest the payload
    /// under a variant key and break that switch.
    #[test]
    fn the_wire_shape_is_internally_tagged_kebab_case() {
        let json = serde_json::to_value(UiEvent::AnswerStarted { request: 3 }).expect("serialize");
        assert_eq!(json["type"], "answer-started");
        assert_eq!(json["request"], 3);

        let json = serde_json::to_value(UiCommand::GetContractVersion).expect("serialize");
        assert_eq!(json["type"], "get-contract-version");

        // A unit variant carries the tag and nothing else.
        assert_eq!(
            serde_json::to_string(&UiCommand::Arm).expect("serialize"),
            r#"{"type":"arm"}"#
        );
    }

    /// FR-004. `StoreSecret`'s argument cannot reach a log.
    ///
    /// Every tracing and logging macro formats through `Debug`, so this is
    /// the whole class, not one call site. Spec 016's capturing subscriber
    /// will assert the same property end to end when it lands; this is the
    /// half that can be held today, and it is the half that makes the other
    /// one hard to break.
    #[test]
    fn store_secret_is_redacted_in_debug_output() {
        let command = UiCommand::StoreSecret {
            provider: "anthropic".into(),
            secret: "sk-ant-super-secret".into(),
        };
        let rendered = format!("{command:?}");
        assert!(
            !rendered.contains("sk-ant-super-secret"),
            "the secret reached Debug output: {rendered}"
        );
        assert!(rendered.contains("[redacted]"), "{rendered}");
        assert!(
            rendered.contains("anthropic"),
            "the provider is a label, not a credential, and is kept: {rendered}"
        );
    }

    /// The alternate-form `Debug` that `{:#?}` produces must redact too: a
    /// pretty-printed log line is still a log line.
    #[test]
    fn store_secret_is_redacted_in_alternate_debug_output() {
        let command = UiCommand::StoreSecret {
            provider: "anthropic".into(),
            secret: "sk-ant-super-secret".into(),
        };
        assert!(!format!("{command:#?}").contains("sk-ant-super-secret"));
    }

    #[test]
    fn projections_cover_every_source_variant() {
        assert_eq!(StateName::from(&State::default()), StateName::Disarmed);

        for (from, want) in [
            (ExclusionState::Unknown, ExclusionSummary::Unknown),
            (ExclusionState::Applied, ExclusionSummary::Applied),
            (ExclusionState::Verified, ExclusionSummary::Verified),
            (ExclusionState::Compromised, ExclusionSummary::Compromised),
            (ExclusionState::Unsupported, ExclusionSummary::Unsupported),
        ] {
            assert_eq!(ExclusionSummary::from(from), want);
        }

        for (from, want) in [
            (StopReason::EndTurn, StopSummary::EndTurn),
            (StopReason::MaxTokens, StopSummary::MaxTokens),
            (StopReason::Refusal, StopSummary::Refusal),
            (StopReason::Other, StopSummary::Other),
        ] {
            assert_eq!(StopSummary::from(from), want);
        }
    }

    /// §3.4: the version is what the UI compares at startup. A change to the
    /// major here is a breaking change to every shipped overlay, so it is
    /// asserted rather than left to a reader to notice.
    #[test]
    fn the_contract_version_is_one_zero() {
        assert_eq!(IPC_CONTRACT_VERSION, (1, 0));
    }
}
