// Spec: specs/014-user-configuration/spec.md

//! Spec 014's functional requirements for the settings model.
//!
//! FR-003 (the golden TOML) lives with the store in `butler-desktop`, because
//! that is the crate that owns the serialization format; spec 009 §2 keeps
//! `toml` out of `butler-core`'s dependency budget.

use butler_core::settings::{
    Anchor, AnswerStyle, CapturePatch, DetectionPatch, DiagnosticsLevel, Effort, MonitorSelector,
    PrivacyPatch, RectPct, SCHEMA_VERSION, Settings, SettingsError, SettingsErrorKind,
    SettingsPatch, Theme, WindowPatch,
};

/// FR-001. The shipped configuration is a valid one.
#[test]
fn fr_001_the_defaults_validate() {
    assert_eq!(
        Settings::default().validate(),
        Ok(()),
        "the defaults are what a user gets with no file; they cannot be invalid"
    );
}

#[test]
fn the_defaults_are_the_ones_the_spec_documents() {
    let settings = Settings::default();

    assert_eq!(settings.schema, SCHEMA_VERSION);
    assert_eq!(settings.capture.monitor, MonitorSelector::Primary);
    assert_eq!(settings.capture.interval_ms, 2500);
    assert_eq!(settings.capture.region, None);
    assert!((settings.detection.threshold - 0.85).abs() < f64::EPSILON);
    assert_eq!(settings.detection.stability_frames, 2);
    assert_eq!(settings.detection.max_compare_chars, 6000);
    assert_eq!(settings.assistant.provider, "anthropic");
    assert_eq!(settings.assistant.model, "claude-opus-5");
    assert_eq!(settings.assistant.effort, Effort::Medium);
    assert_eq!(settings.assistant.answer_style, AnswerStyle::Short);
    assert_eq!(settings.assistant.max_output_tokens, 1024);
    assert_eq!(settings.assistant.endpoint_override, None);
    assert_eq!(settings.window.anchor, Anchor::TopRight);
    assert_eq!(settings.window.width_px, 420);
    assert_eq!(settings.window.max_height_px, 600);
    assert_eq!(settings.ui.theme, Theme::System);

    // Spec 015 and spec 004 §3.2: the two that must default this way, or the
    // product is not the one the constitution describes.
    assert!(
        settings.privacy.redaction_enabled,
        "spec 015 §3.2: redaction is on unless the user turns it off"
    );
    assert!(
        !settings.privacy.allow_degraded_mode,
        "spec 004 §3.2: degraded mode is opt-in, never the default"
    );
    assert_eq!(
        settings.privacy.diagnostics_level,
        DiagnosticsLevel::Minimal
    );
}

/// FR-002: every documented range, at below-min, min, max and above-max.
///
/// Table-driven because the interesting failure is a bound that is checked
/// with the wrong comparison, and that only shows up when both edges are
/// tested. Each row mutates one field of an otherwise-default `Settings`, so
/// a verdict can only be about the field under test.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the length is the table: one row per documented range in \
              section 3.1. Splitting it would put the rows somewhere other \
              than beside the loop that drives them, which is what makes it \
              readable as the spec's list."
)]
fn fr_002_every_documented_range_is_enforced() {
    struct Case {
        field: &'static str,
        set: fn(&mut Settings, f64),
        min: f64,
        max: f64,
        /// Whether the field is stored as an integer.
        ///
        /// Load-bearing, not decoration. The table drives every field through
        /// `f64` so one row shape covers all of them, but an integer field
        /// truncates: `stability_frames` has range 1..=5, and a step of 0.04
        /// made "above max" 5.04, which is 5 once cast to `u8`. The row then
        /// asserted that a valid value is rejected, and the test caught it.
        /// An integral field steps by one.
        integral: bool,
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the table drives integer fields through f64 so one row shape \
                  covers every range in section 3.1. Values are the documented \
                  bounds and their neighbours, all far inside each integer \
                  type, so no cast here can lose information."
    )]
    let cases = [
        Case {
            field: "capture.interval_ms",
            set: |s, v| s.capture.interval_ms = v as u32,
            min: 1000.0,
            max: 10_000.0,
            integral: true,
        },
        Case {
            field: "detection.threshold",
            set: |s, v| s.detection.threshold = v,
            min: 0.5,
            max: 0.99,
            integral: false,
        },
        Case {
            field: "detection.stability_frames",
            set: |s, v| s.detection.stability_frames = v as u8,
            min: 1.0,
            max: 5.0,
            integral: true,
        },
        Case {
            field: "detection.max_compare_chars",
            set: |s, v| s.detection.max_compare_chars = v as u32,
            min: 500.0,
            max: 100_000.0,
            integral: true,
        },
        Case {
            field: "assistant.max_output_tokens",
            set: |s, v| s.assistant.max_output_tokens = v as u32,
            min: 64.0,
            max: 8192.0,
            integral: true,
        },
        Case {
            field: "pacing.words_per_minute",
            set: |s, v| s.pacing.words_per_minute = v as u16,
            min: 60.0,
            max: 1200.0,
            integral: true,
        },
        Case {
            field: "window.width_px",
            set: |s, v| s.window.width_px = v as u32,
            min: 200.0,
            max: 2000.0,
            integral: true,
        },
        Case {
            field: "window.max_height_px",
            set: |s, v| s.window.max_height_px = v as u32,
            min: 120.0,
            max: 4000.0,
            integral: true,
        },
        Case {
            field: "window.opacity",
            set: |s, v| s.window.opacity = v,
            min: 0.2,
            max: 1.0,
            integral: false,
        },
        Case {
            field: "ui.font_scale",
            set: |s, v| s.ui.font_scale = v,
            min: 0.75,
            max: 2.0,
            integral: false,
        },
    ];

    for case in &cases {
        // Strictly outside the range on both sides. An integer field needs a
        // whole step, or the cast puts the value back inside.
        let step = if case.integral {
            1.0
        } else {
            ((case.max - case.min) / 100.0).max(0.01)
        };

        for (value, want_ok, edge) in [
            (case.min - step, false, "below min"),
            (case.min, true, "min"),
            (case.max, true, "max"),
            (case.max + step, false, "above max"),
        ] {
            let mut settings = Settings::default();
            (case.set)(&mut settings, value);
            let verdict = settings.validate();

            if want_ok {
                assert_eq!(
                    verdict,
                    Ok(()),
                    "{} at its {edge} ({value}) should be accepted",
                    case.field
                );
            } else {
                let errors = verdict.expect_err(&format!(
                    "{} {edge} ({value}) should be rejected",
                    case.field
                ));
                assert!(
                    errors.iter().any(|e| e.field == case.field),
                    "{} {edge} was rejected, but not for that field: {errors:?}",
                    case.field
                );
            }
        }
    }
}

/// `validate` reports every problem, not the first: a panel that reveals one
/// error at a time makes the user guess how many are left.
#[test]
fn validation_reports_every_problem_at_once() {
    let mut settings = Settings::default();
    settings.capture.interval_ms = 10;
    settings.detection.threshold = 2.0;
    settings.window.opacity = 0.0;
    settings.assistant.provider = "  ".to_owned();

    let errors = settings.validate().expect_err("four fields are wrong");
    let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();

    for field in [
        "capture.interval_ms",
        "detection.threshold",
        "window.opacity",
        "assistant.provider",
    ] {
        assert!(fields.contains(&field), "{field} missing from {fields:?}");
    }
}

/// Spec 015 §3.3: a self-hosted gateway is legitimate; plaintext is not.
#[test]
fn an_endpoint_override_must_be_https() {
    let mut settings = Settings::default();

    settings.assistant.endpoint_override = Some("https://gateway.example".to_owned());
    assert_eq!(settings.validate(), Ok(()), "https is allowed");

    settings.assistant.endpoint_override = Some("http://gateway.example".to_owned());
    let errors = settings.validate().expect_err("plaintext must be refused");
    assert_eq!(
        errors,
        vec![SettingsError {
            field: "assistant.endpoint_override".to_owned(),
            kind: SettingsErrorKind::NotHttps,
        }]
    );
}

/// §3.1: a schema newer than this build is refused rather than guessed at.
#[test]
fn a_newer_schema_is_refused() {
    let settings = Settings {
        schema: SCHEMA_VERSION + 1,
        ..Settings::default()
    };

    let errors = settings.validate().expect_err("a future schema is refused");
    assert_eq!(
        errors,
        vec![SettingsError {
            field: "schema".to_owned(),
            kind: SettingsErrorKind::UnknownSchema {
                found: SCHEMA_VERSION + 1,
                supported: SCHEMA_VERSION,
            },
        }]
    );
}

/// A region outside the monitor is refused per component, so the panel can
/// point at the corner that is wrong.
#[test]
fn a_region_outside_the_monitor_is_refused() {
    let mut settings = Settings::default();
    settings.capture.region = Some(RectPct {
        x: 0.1,
        y: 0.1,
        width: 1.5,
        height: 0.4,
    });

    let errors = settings.validate().expect_err("width above 1.0");
    assert!(errors.iter().any(|e| e.field == "capture.region.width"));
}

// -------------------------------------------------------------------- patches

#[test]
fn a_patch_touches_only_what_it_carries() {
    let before = Settings::default();
    let after = before.apply(&SettingsPatch {
        detection: Some(DetectionPatch {
            threshold: Some(0.9),
            ..DetectionPatch::default()
        }),
        ..SettingsPatch::default()
    });

    assert!((after.detection.threshold - 0.9).abs() < f64::EPSILON);
    assert_eq!(
        after.detection.stability_frames,
        before.detection.stability_frames
    );
    assert_eq!(after.capture, before.capture);
    assert_eq!(after.assistant, before.assistant);
    assert_eq!(after.privacy, before.privacy);
}

#[test]
fn an_empty_patch_changes_nothing() {
    let before = Settings::default();
    assert_eq!(before.apply(&SettingsPatch::default()), before);
}

/// §3.1: an invalid patch is rejected **whole**. `apply` produces a candidate
/// and never mutates the original, which is what makes that possible: the
/// caller validates the candidate and drops it.
#[test]
fn an_invalid_patch_leaves_the_original_untouched() {
    let current = Settings::default();

    let candidate = current.apply(&SettingsPatch {
        capture: Some(CapturePatch {
            interval_ms: Some(10),
            monitor: Some(MonitorSelector::Index { index: 1 }),
        }),
        ..SettingsPatch::default()
    });

    assert!(
        candidate.validate().is_err(),
        "10 ms is below the documented minimum"
    );
    // The monitor change was valid and the interval was not. Neither is kept,
    // because the caller discards the candidate whole.
    assert_eq!(current, Settings::default());
    assert_eq!(current.capture.monitor, MonitorSelector::Primary);
}

#[test]
fn a_patch_can_turn_on_degraded_mode_because_the_user_asked() {
    let after = Settings::default().apply(&SettingsPatch {
        privacy: Some(PrivacyPatch {
            allow_degraded_mode: Some(true),
            ..PrivacyPatch::default()
        }),
        ..SettingsPatch::default()
    });

    assert!(after.privacy.allow_degraded_mode);
    assert_eq!(after.validate(), Ok(()));
    // The default is still off; consent is per-installation, not per-build.
    assert!(!Settings::default().privacy.allow_degraded_mode);
}

#[test]
fn a_window_patch_survives_a_round_trip_through_json() {
    let patch = SettingsPatch {
        window: Some(WindowPatch {
            anchor: Some(Anchor::BottomLeft),
            opacity: Some(0.5),
            ..WindowPatch::default()
        }),
        ..SettingsPatch::default()
    };

    let json = serde_json::to_string(&patch).expect("serialize");
    // `skip_serializing_if` keeps the untouched fields off the wire entirely.
    assert!(!json.contains("width_px"), "{json}");
    assert!(!json.contains("capture"), "{json}");

    let back: SettingsPatch = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, patch);
}

/// FR-004's model half: an unknown key is refused rather than ignored. The
/// store's half (back up the file, start with defaults) is tested with the
/// store, which is the thing that does it.
#[test]
fn fr_004_an_unknown_key_is_refused() {
    let json = r#"{"schema":1,"nonsense":true}"#;
    let parsed: Result<Settings, _> = serde_json::from_str(json);
    assert!(
        parsed.is_err(),
        "deny_unknown_fields must refuse a key nobody recognizes"
    );
}

#[test]
fn a_missing_key_is_filled_from_the_defaults() {
    // The other half of `deny_unknown_fields, default`: adding a field is not
    // a breaking change to a file written by an older build.
    let parsed: Settings = serde_json::from_str(r#"{"schema":1}"#).expect("partial file");
    assert_eq!(parsed, Settings::default());
}

// ------------------------------------------------- AC-2: the documented table

/// AC-2. `docs/architecture.md` §8 is generated from `Settings::default()`
/// and diffed, exactly as spec 009 AC-2 does for the state diagram: the doc
/// is never hand-edited, so it cannot drift from the code it describes.
mod documented_defaults {
    use super::Settings;

    const ARCHITECTURE_PATH: &str = "../../docs/architecture.md";
    const DOC_SECTION: &str = "## 8. Settings defaults (spec 014)";
    const FENCE: &str = "<!-- generated: settings-defaults -->";

    /// Flatten the default settings to `path = value` rows.
    ///
    /// Rendered from the serialized form rather than by listing fields by
    /// hand, so a field added to `Settings` appears in the document without
    /// anyone remembering to add it. Serialization is what the store writes,
    /// so the table shows what a user would actually find in their file.
    fn rows() -> Vec<(String, String)> {
        let value = serde_json::to_value(Settings::default()).expect("serialize the defaults");
        let mut out = Vec::new();
        flatten(String::new(), &value, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn flatten(prefix: String, value: &serde_json::Value, out: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    flatten(path, child, out);
                }
            }
            serde_json::Value::Null => out.push((prefix, "none".to_owned())),
            other => out.push((prefix, format!("`{other}`"))),
        }
    }

    fn render() -> String {
        use std::fmt::Write as _;

        let mut table = String::from("\n| Setting | Default |\n|---|---|\n");
        for (field, value) in rows() {
            // `write!` into the buffer rather than `push_str(&format!(..))`:
            // one allocation instead of one per row.
            let _ = writeln!(table, "| `{field}` | {value} |");
        }
        table
    }

    /// The span between the two generated markers.
    fn span(doc: &str) -> (usize, usize) {
        let section = doc
            .find(DOC_SECTION)
            .unwrap_or_else(|| panic!("docs/architecture.md has no `{DOC_SECTION}` heading"));
        let open = section
            + doc[section..]
                .find(FENCE)
                .unwrap_or_else(|| panic!("no `{FENCE}` marker under `{DOC_SECTION}`"))
            + FENCE.len();
        let close = open
            + doc[open..]
                .find(FENCE)
                .unwrap_or_else(|| panic!("`{FENCE}` is not closed under `{DOC_SECTION}`"));
        (open, close)
    }

    #[test]
    fn ac_002_the_document_matches_the_defaults() {
        let doc = std::fs::read_to_string(ARCHITECTURE_PATH)
            .unwrap_or_else(|e| panic!("cannot read {ARCHITECTURE_PATH}: {e}"));
        let (open, close) = span(&doc);
        assert_eq!(
            &doc[open..close],
            render(),
            "docs/architecture.md §8 is stale. Regenerate it:\n  \
             cargo test -p butler-core --test settings -- --ignored regenerate"
        );
    }

    #[test]
    #[ignore = "writes docs/architecture.md; run explicitly after changing a default"]
    fn regenerate_settings_defaults() {
        let doc = std::fs::read_to_string(ARCHITECTURE_PATH)
            .unwrap_or_else(|e| panic!("cannot read {ARCHITECTURE_PATH}: {e}"));
        let (open, close) = span(&doc);
        let next = format!("{}{}{}", &doc[..open], render(), &doc[close..]);
        std::fs::write(ARCHITECTURE_PATH, next).expect("cannot write the architecture document");
    }
}
