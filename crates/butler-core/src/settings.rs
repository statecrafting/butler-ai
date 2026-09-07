// Spec: specs/014-user-configuration/spec.md

//! The one configuration surface (spec 014).
//!
//! Every tunable another spec names lives here: the capture interval (006),
//! the similarity threshold and stability frames (008), the provider, model
//! and budget (010), the pacing rate (013), the shortcut accelerators (004
//! §3.3), the privacy toggles (015), and the overlay's geometry (012).
//!
//! Three properties make this worth centralizing rather than scattering.
//!
//! **Defaults are in code.** [`Settings::default`] is the documented
//! configuration; a missing file is not a different product.
//!
//! **Validation is pure and total.** [`Settings::validate`] returns *every*
//! problem, not the first, because a settings panel showing one error at a
//! time makes the user play twenty questions.
//!
//! **A patch is all or nothing.** [`Settings::apply`] produces a candidate
//! that is validated before it is stored, so a rejected patch leaves the
//! previous configuration exactly as it was. Half-applied settings would be a
//! configuration the user never chose and cannot see.
//!
//! There is no environment-variable surface (FR-005, and
//! `docs/architecture.md` D9). Configuration that no UI can show is
//! configuration nobody can audit.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The settings schema version. Bumped when a migration is needed (§3.1).
pub const SCHEMA_VERSION: u16 = 1;

// ---------------------------------------------------------------- the model

/// Which monitor to watch (spec 006).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MonitorSelector {
    /// Whichever monitor the OS calls primary.
    #[default]
    Primary,
    /// A zero-based index into the monitor list.
    Index {
        /// The index.
        index: u32,
    },
    /// A monitor by the name the OS reports.
    Named {
        /// The name.
        name: String,
    },
}

/// A sub-region of a monitor, in percentages of its bounds (spec 006).
///
/// Percentages rather than pixels so a region survives a resolution change
/// and a scale-factor change, neither of which the user thinks of as
/// "my region moved".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct RectPct {
    /// Left edge, 0.0 to 1.0.
    pub x: f64,
    /// Top edge, 0.0 to 1.0.
    pub y: f64,
    /// Width, 0.0 to 1.0.
    pub width: f64,
    /// Height, 0.0 to 1.0.
    pub height: f64,
}

/// What to capture and how often (spec 006).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct CaptureSettings {
    /// Which monitor.
    pub monitor: MonitorSelector,
    /// Milliseconds between capture attempts.
    pub interval_ms: u32,
    /// An optional sub-region of that monitor.
    pub region: Option<RectPct>,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            monitor: MonitorSelector::Primary,
            interval_ms: 2500,
            region: None,
        }
    }
}

/// When the screen counts as having changed (spec 008).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct DetectionSettings {
    /// Similarity above which two screens are "the same".
    pub threshold: f64,
    /// Consecutive frames a change must persist before it counts.
    pub stability_frames: u8,
    /// Cap on the text compared, so a huge screen cannot stall the loop.
    pub max_compare_chars: u32,
}

impl Default for DetectionSettings {
    fn default() -> Self {
        Self {
            threshold: 0.85,
            stability_frames: 2,
            max_compare_chars: 6000,
        }
    }
}

/// How hard the model should think (spec 010).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum Effort {
    /// Fastest, cheapest.
    Low,
    /// The default balance.
    #[default]
    Medium,
    /// Slowest, most thorough.
    High,
}

/// How much the answer should say (spec 010).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum AnswerStyle {
    /// A sentence or two. The overlay is small and the user is busy.
    #[default]
    Short,
    /// A paragraph.
    Normal,
    /// As much as the token cap allows.
    Detailed,
}

/// Spend caps (spec 010's `SpendGuard` reads these).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct BudgetSettings {
    /// Hard cap per rolling day, in US dollars.
    pub daily_usd: f64,
    /// Hard cap per rolling month, in US dollars.
    pub monthly_usd: f64,
    /// Refuse a request whose input would exceed this many tokens.
    pub max_input_tokens: u32,
}

impl Default for BudgetSettings {
    fn default() -> Self {
        Self {
            daily_usd: 2.0,
            monthly_usd: 20.0,
            max_input_tokens: 8000,
        }
    }
}

/// Which assistant, and how it is asked (spec 010).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct AssistantSettings {
    /// The provider's configured name.
    pub provider: String,
    /// The model id.
    pub model: String,
    /// How hard to think.
    pub effort: Effort,
    /// How much to say.
    pub answer_style: AnswerStyle,
    /// Cap on the answer's length.
    pub max_output_tokens: u32,
    /// A self-hosted gateway, if the user runs one. MUST be `https://`
    /// (spec 015 §3.3), and the UI shows it as "custom endpoint".
    pub endpoint_override: Option<String>,
    /// Spend caps.
    pub budget: BudgetSettings,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_owned(),
            model: "claude-opus-5".to_owned(),
            effort: Effort::Medium,
            answer_style: AnswerStyle::Short,
            max_output_tokens: 1024,
            endpoint_override: None,
            budget: BudgetSettings::default(),
        }
    }
}

/// How fast the answer is revealed (spec 013).
///
/// Spec 014 §3.1 names the field's type as spec 013's `PacingPolicy`, which
/// is phase 4 and does not exist. This carries the one value the summary
/// names, "pacing (words per minute)", so the user-facing setting exists now
/// and spec 013 decides how its policy reads it (D-2).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct PacingSettings {
    /// Reading pace, in words per minute.
    pub words_per_minute: u16,
}

impl Default for PacingSettings {
    fn default() -> Self {
        // Comfortable silent-reading pace for on-screen prose. Faster than
        // this and the overlay is a flicker; slower and it is a teleprompter.
        Self {
            words_per_minute: 300,
        }
    }
}

/// The four global accelerators (spec 004 §3.3).
///
/// Strings in Tauri's parser syntax, because that is what
/// `shortcuts::register_shortcuts` hands the OS. Validation checks they parse
/// there, not here: this crate has no Tauri.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct ShortcutSettings {
    /// Arm or disarm.
    pub arm_disarm: String,
    /// Toggle the overlay's interactivity.
    pub interact: String,
    /// Hide or show the overlay.
    pub toggle_visibility: String,
    /// Capture and ask immediately.
    pub ask_now: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        // Spec 004 §3.3's table, and D-9 for `interact`. `Cmd` is written
        // rather than the platform split, because Tauri's parser maps
        // `CmdOrCtrl` per platform and this crate cannot know which it is on.
        Self {
            arm_disarm: "CmdOrCtrl+Shift+B".to_owned(),
            interact: "CmdOrCtrl+Shift+Space".to_owned(),
            toggle_visibility: "CmdOrCtrl+Shift+H".to_owned(),
            ask_now: "CmdOrCtrl+Shift+Enter".to_owned(),
        }
    }
}

/// How much the logs say (spec 016 §3.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticsLevel {
    /// `warn` and above. The default: a privacy tool logs little.
    #[default]
    Minimal,
    /// `info`.
    Normal,
    /// `trace`, including every state transition by id.
    Verbose,
}

/// The privacy toggles (spec 015).
#[allow(
    clippy::struct_excessive_bools,
    reason = "this is spec 015's toggle table, one field per switch the user \
              can flip, and `OverlayWindowConfig` carries the same exemption \
              for the same reason. Grouping them into sub-structs or a \
              bitflags type would make the settings file and the panel read \
              less like the spec they mirror, which is the point of the \
              struct."
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct PrivacySettings {
    /// Whether `redact` removes secret-shaped content before a prompt leaves
    /// the process. Defaults on; turning it off is an explicit, warned
    /// choice (spec 015 §3.2).
    pub redaction_enabled: bool,
    /// Whether personal data (e-mail, phone, IBAN) is redacted too. Opt-in,
    /// because it is lossy on ordinary prose.
    pub redact_pii: bool,
    /// Whether the pipeline may arm when capture exclusion is not verified.
    /// Defaults **off**: degraded mode is opt-in and bannered (spec 004
    /// §3.2, spec 009).
    pub allow_degraded_mode: bool,
    /// How much the logs say.
    pub diagnostics_level: DiagnosticsLevel,
    /// Whether capture is restricted to `capture.region`.
    pub region_only: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            redaction_enabled: true,
            redact_pii: false,
            allow_degraded_mode: false,
            diagnostics_level: DiagnosticsLevel::Minimal,
            region_only: false,
        }
    }
}

/// Which corner the overlay sits in (spec 004 §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    /// Top left.
    TopLeft,
    /// Top right. The default (spec 004 §3.2).
    #[default]
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
}

/// The overlay's geometry (spec 004 §3.2, spec 012).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct WindowSettings {
    /// Which corner.
    pub anchor: Anchor,
    /// Width in logical pixels.
    pub width_px: u32,
    /// The tallest the overlay may grow before the answer scrolls.
    pub max_height_px: u32,
    /// Overall opacity of the plate.
    pub opacity: f64,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopRight,
            width_px: 420,
            max_height_px: 600,
            opacity: 0.92,
        }
    }
}

/// Light, dark, or follow the system (spec 012).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// Follow the OS.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Presentation (spec 012).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct UiSettings {
    /// Which theme.
    pub theme: Theme,
    /// Multiplier on the 14 px base size.
    pub font_scale: f64,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            font_scale: 1.0,
        }
    }
}

/// The whole configuration (spec 014 §3.1).
///
/// `deny_unknown_fields` is FR-004: a key nobody recognizes is a settings
/// file from a newer build, or a typo that would silently do nothing. Both
/// are worth refusing loudly. `default` is what makes a *missing* key fine,
/// so adding a field is not a breaking change to an existing file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    /// The schema version this file was written by.
    pub schema: u16,
    /// What to capture and how often.
    pub capture: CaptureSettings,
    /// When the screen counts as changed.
    pub detection: DetectionSettings,
    /// Which assistant and how it is asked.
    pub assistant: AssistantSettings,
    /// How fast the answer is revealed.
    pub pacing: PacingSettings,
    /// The global accelerators.
    pub shortcuts: ShortcutSettings,
    /// The privacy toggles.
    pub privacy: PrivacySettings,
    /// The overlay's geometry.
    pub window: WindowSettings,
    /// Presentation.
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            capture: CaptureSettings::default(),
            detection: DetectionSettings::default(),
            assistant: AssistantSettings::default(),
            pacing: PacingSettings::default(),
            shortcuts: ShortcutSettings::default(),
            privacy: PrivacySettings::default(),
            window: WindowSettings::default(),
            ui: UiSettings::default(),
        }
    }
}

// ----------------------------------------------------------- validation

/// Which field failed, and why (spec 014 §3.1).
///
/// A field path and a kind, never a rendered sentence: the settings panel
/// (§3.4) needs to highlight the control that is wrong, which it cannot do
/// from prose, and spec 015 §3.5 keeps content out of error text anyway. The
/// bound is carried so the UI can say what the range is without restating
/// this module's constants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct SettingsError {
    /// Dotted path to the field, for example `detection.threshold`.
    pub field: String,
    /// What is wrong with it.
    pub kind: SettingsErrorKind,
}

/// The closed set of things that can be wrong with a setting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SettingsErrorKind {
    /// A number outside its documented range.
    OutOfRange {
        /// The inclusive minimum.
        min: f64,
        /// The inclusive maximum.
        max: f64,
    },
    /// A string that must not be empty.
    Empty,
    /// An endpoint override that is not `https://` (spec 015 §3.3).
    NotHttps,
    /// A schema this build does not know how to read.
    UnknownSchema {
        /// The version found in the file.
        found: u16,
        /// The newest version this build understands.
        supported: u16,
    },
}

/// Range check helper. Pushes an error rather than returning one, because
/// `validate` reports every problem rather than the first.
fn check_range(errors: &mut Vec<SettingsError>, field: &str, value: f64, min: f64, max: f64) {
    if value < min || value > max || value.is_nan() {
        errors.push(SettingsError {
            field: field.to_owned(),
            kind: SettingsErrorKind::OutOfRange { min, max },
        });
    }
}

fn check_non_empty(errors: &mut Vec<SettingsError>, field: &str, value: &str) {
    if value.trim().is_empty() {
        errors.push(SettingsError {
            field: field.to_owned(),
            kind: SettingsErrorKind::Empty,
        });
    }
}

impl Settings {
    /// Every range spec 014 §3.1 documents, checked.
    ///
    /// # Errors
    ///
    /// Returns **all** the violations, not the first. A panel that reveals
    /// one error at a time makes the user guess how many are left.
    pub fn validate(&self) -> Result<(), Vec<SettingsError>> {
        let mut errors = Vec::new();

        if self.schema > SCHEMA_VERSION {
            errors.push(SettingsError {
                field: "schema".to_owned(),
                kind: SettingsErrorKind::UnknownSchema {
                    found: self.schema,
                    supported: SCHEMA_VERSION,
                },
            });
        }

        self.capture.validate_into(&mut errors);
        self.detection.validate_into(&mut errors);
        self.assistant.validate_into(&mut errors);
        self.pacing.validate_into(&mut errors);
        self.shortcuts.validate_into(&mut errors);
        self.window.validate_into(&mut errors);
        self.ui.validate_into(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Apply a patch, producing a candidate. Does **not** validate.
    ///
    /// The caller validates the result and keeps the previous value if it
    /// fails, which is what makes §3.1's "an invalid patch is rejected whole"
    /// true: nothing here mutates `self`.
    #[must_use]
    pub fn apply(&self, patch: &SettingsPatch) -> Self {
        let mut next = self.clone();
        patch.apply_to(&mut next);
        next
    }
}

impl CaptureSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        check_range(
            errors,
            "capture.interval_ms",
            f64::from(self.interval_ms),
            1000.0,
            10_000.0,
        );
        if let Some(region) = self.region {
            check_range(errors, "capture.region.x", region.x, 0.0, 1.0);
            check_range(errors, "capture.region.y", region.y, 0.0, 1.0);
            check_range(errors, "capture.region.width", region.width, 0.0, 1.0);
            check_range(errors, "capture.region.height", region.height, 0.0, 1.0);
        }
    }
}

impl DetectionSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        check_range(errors, "detection.threshold", self.threshold, 0.5, 0.99);
        check_range(
            errors,
            "detection.stability_frames",
            f64::from(self.stability_frames),
            1.0,
            5.0,
        );
        check_range(
            errors,
            "detection.max_compare_chars",
            f64::from(self.max_compare_chars),
            500.0,
            100_000.0,
        );
    }
}

impl AssistantSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        check_non_empty(errors, "assistant.provider", &self.provider);
        check_non_empty(errors, "assistant.model", &self.model);
        check_range(
            errors,
            "assistant.max_output_tokens",
            f64::from(self.max_output_tokens),
            64.0,
            8192.0,
        );
        check_range(
            errors,
            "assistant.budget.daily_usd",
            self.budget.daily_usd,
            0.0,
            1000.0,
        );
        check_range(
            errors,
            "assistant.budget.monthly_usd",
            self.budget.monthly_usd,
            0.0,
            10_000.0,
        );
        check_range(
            errors,
            "assistant.budget.max_input_tokens",
            f64::from(self.budget.max_input_tokens),
            500.0,
            200_000.0,
        );

        // Spec 015 §3.3: a self-hosted gateway is legitimate, plaintext is
        // not. The screen's text is what would travel over it.
        if let Some(endpoint) = &self.endpoint_override {
            if endpoint.trim().is_empty() {
                errors.push(SettingsError {
                    field: "assistant.endpoint_override".to_owned(),
                    kind: SettingsErrorKind::Empty,
                });
            } else if !endpoint.starts_with("https://") {
                errors.push(SettingsError {
                    field: "assistant.endpoint_override".to_owned(),
                    kind: SettingsErrorKind::NotHttps,
                });
            }
        }
    }
}

impl PacingSettings {
    // By value: the struct is two bytes, smaller than the reference to it.
    fn validate_into(self, errors: &mut Vec<SettingsError>) {
        check_range(
            errors,
            "pacing.words_per_minute",
            f64::from(self.words_per_minute),
            60.0,
            1200.0,
        );
    }
}

impl ShortcutSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        for (field, accelerator) in [
            ("shortcuts.arm_disarm", &self.arm_disarm),
            ("shortcuts.interact", &self.interact),
            ("shortcuts.toggle_visibility", &self.toggle_visibility),
            ("shortcuts.ask_now", &self.ask_now),
        ] {
            check_non_empty(errors, field, accelerator);
        }
    }
}

impl WindowSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        check_range(
            errors,
            "window.width_px",
            f64::from(self.width_px),
            200.0,
            2000.0,
        );
        check_range(
            errors,
            "window.max_height_px",
            f64::from(self.max_height_px),
            120.0,
            4000.0,
        );
        check_range(errors, "window.opacity", self.opacity, 0.2, 1.0);
    }
}

impl UiSettings {
    fn validate_into(&self, errors: &mut Vec<SettingsError>) {
        check_range(errors, "ui.font_scale", self.font_scale, 0.75, 2.0);
    }
}

// ---------------------------------------------------------------- the patch

/// Assign `target.field = value` for each field the patch carries.
///
/// Only the *names* pass through this macro. The struct definitions below are
/// written out longhand, because `specta`'s derive resolves field types by
/// path and cannot see through a macro's `ty` fragment: generating them threw
/// "Cannot get path from type `f64`" for every field.
macro_rules! fold {
    ($target:expr, $patch:expr, $($field:ident),+ $(,)?) => {$(
        if let Some(value) = $patch.$field.clone() {
            $target.$field = value;
        }
    )+};
}

/// Capture changes (spec 014 §3.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct CapturePatch {
    /// Which monitor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<MonitorSelector>,
    /// Milliseconds between captures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u32>,
}

/// Detection changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct DetectionPatch {
    /// Similarity threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// Consecutive frames a change must persist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability_frames: Option<u8>,
    /// Cap on compared text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_compare_chars: Option<u32>,
}

/// Assistant changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct AssistantPatch {
    /// Provider name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Thinking effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Answer length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_style: Option<AnswerStyle>,
    /// Answer token cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

/// Pacing changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct PacingPatch {
    /// Reading pace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words_per_minute: Option<u16>,
}

/// Shortcut changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct ShortcutPatch {
    /// Arm or disarm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arm_disarm: Option<String>,
    /// Interactivity toggle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interact: Option<String>,
    /// Visibility toggle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle_visibility: Option<String>,
    /// Ask now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_now: Option<String>,
}

/// Privacy changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct PrivacyPatch {
    /// Redaction on or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_enabled: Option<bool>,
    /// Personal-data redaction on or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_pii: Option<bool>,
    /// Degraded-mode consent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_degraded_mode: Option<bool>,
    /// Log verbosity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics_level: Option<DiagnosticsLevel>,
    /// Region-only capture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_only: Option<bool>,
}

/// Window changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct WindowPatch {
    /// Corner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    /// Width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_px: Option<u32>,
    /// Height cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height_px: Option<u32>,
    /// Plate opacity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
}

/// Presentation changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct UiPatch {
    /// Theme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// Font multiplier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_scale: Option<f64>,
}

/// A partial update (spec 014 §3.1).
///
/// Every field is optional, recursively, so the settings panel sends only
/// what the user touched. `skip_serializing_if` keeps an untouched patch to
/// `{}` on the wire rather than a tree of nulls, which matters because this
/// crosses the IPC boundary on every `UpdateSettings`.
///
/// Applied by [`Settings::apply`], which returns a *candidate*; the caller
/// validates it and discards it whole if it fails (§3.1: no partial
/// application).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct SettingsPatch {
    /// Capture changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<CapturePatch>,
    /// Detection changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection: Option<DetectionPatch>,
    /// Assistant changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant: Option<AssistantPatch>,
    /// Pacing changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pacing: Option<PacingPatch>,
    /// Shortcut changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<ShortcutPatch>,
    /// Privacy changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy: Option<PrivacyPatch>,
    /// Window changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPatch>,
    /// Presentation changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiPatch>,
}

impl SettingsPatch {
    /// Fold every present field into `target`.
    ///
    /// Three things are deliberately **not** patchable here. `schema` belongs
    /// to whatever wrote the file, and letting a UI patch it would let the
    /// overlay claim a migration that never ran. `capture.region` and
    /// `assistant.endpoint_override` are `Option` in the model, so "absent
    /// from the patch" and "set to none" would be the same JSON; clearing
    /// either needs its own command rather than an ambiguity.
    fn apply_to(&self, target: &mut Settings) {
        if let Some(patch) = &self.capture {
            fold!(target.capture, patch, monitor, interval_ms);
        }
        if let Some(patch) = &self.detection {
            fold!(
                target.detection,
                patch,
                threshold,
                stability_frames,
                max_compare_chars
            );
        }
        if let Some(patch) = &self.assistant {
            fold!(
                target.assistant,
                patch,
                provider,
                model,
                effort,
                answer_style,
                max_output_tokens
            );
        }
        if let Some(patch) = &self.pacing {
            fold!(target.pacing, patch, words_per_minute);
        }
        if let Some(patch) = &self.shortcuts {
            fold!(
                target.shortcuts,
                patch,
                arm_disarm,
                interact,
                toggle_visibility,
                ask_now
            );
        }
        if let Some(patch) = &self.privacy {
            fold!(
                target.privacy,
                patch,
                redaction_enabled,
                redact_pii,
                allow_degraded_mode,
                diagnostics_level,
                region_only
            );
        }
        if let Some(patch) = &self.window {
            fold!(
                target.window,
                patch,
                anchor,
                width_px,
                max_height_px,
                opacity
            );
        }
        if let Some(patch) = &self.ui {
            fold!(target.ui, patch, theme, font_scale);
        }
    }
}
