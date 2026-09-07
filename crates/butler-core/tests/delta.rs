// Spec: specs/008-change-detection/spec.md

//! The change detector's functional requirements.
//!
//! Everything lives in a `delta` module so that spec 008 AC-1's command,
//! `cargo test -p butler-core delta::`, selects exactly these tests. Each test
//! is named for the requirement it discharges.

mod delta {
    use butler_core::delta::{
        ChangeDetector, DEFAULT_MAX_COMPARE_CHARS, DEFAULT_STABILITY_FRAMES, DEFAULT_THRESHOLD,
        DetectorInput, LevenshteinDetector, Verdict,
    };
    use butler_core::machine;
    use proptest::prelude::{ProptestConfig, any, prop_assert, proptest};
    use proptest::test_runner::FileFailurePersistence;

    // ----------------------------------------------------------------- setup

    /// The reference configuration: spec 008 §3.2's documented defaults.
    fn detector() -> LevenshteinDetector {
        LevenshteinDetector::new(
            DEFAULT_THRESHOLD,
            DEFAULT_STABILITY_FRAMES,
            DEFAULT_MAX_COMPARE_CHARS,
        )
    }

    /// No exclusion, no force: the ordinary capture cycle.
    fn plain() -> DetectorInput<'static> {
        DetectorInput::default()
    }

    /// A realistic screen: 40 numbered lines of prose.
    fn screen(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("line {i}: the quick brown fox jumps over the lazy dog"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn is_unchanged(v: Verdict) -> bool {
        matches!(v, Verdict::Unchanged { .. })
    }
    fn is_changed(v: Verdict) -> bool {
        matches!(v, Verdict::Changed { .. })
    }
    fn is_pending(v: Verdict) -> bool {
        matches!(v, Verdict::Pending { .. })
    }

    // ------------------------------------------------------------------ FR-001

    #[test]
    fn fr_001_identical_input_is_unchanged_at_similarity_one() {
        let mut d = detector();
        let text = screen(40);
        d.commit(&text);

        let v = d.evaluate(&text, &plain());

        assert!(is_unchanged(v), "identical text must be Unchanged, got {v:?}");
        assert!(
            (v.similarity() - 1.0).abs() < f32::EPSILON,
            "identical text must score exactly 1.0, got {}",
            v.similarity()
        );
    }

    #[test]
    fn fr_001_empty_against_empty_is_unchanged_not_a_division_by_zero() {
        let mut d = detector();
        let v = d.evaluate("", &plain());
        assert!(is_unchanged(v), "empty vs empty must be Unchanged, got {v:?}");
        assert!((v.similarity() - 1.0).abs() < f32::EPSILON);
    }

    // ------------------------------------------------------------------ FR-002

    #[test]
    fn fr_002_completely_different_input_pends_then_changes_then_settles() {
        let mut d = detector();
        d.commit(&screen(40));
        let other = "totally unrelated content about marine biology and tides".repeat(4);

        // stability_frames - 1 evaluations are Pending ...
        for i in 1..DEFAULT_STABILITY_FRAMES {
            let v = d.evaluate(&other, &plain());
            assert!(
                is_pending(v),
                "evaluation {i} of a new screen must be Pending, got {v:?}"
            );
        }
        // ... and the next one is Changed.
        let v = d.evaluate(&other, &plain());
        assert!(
            is_changed(v),
            "evaluation {DEFAULT_STABILITY_FRAMES} must be Changed, got {v:?}"
        );

        // After committing, the same text is the baseline and settles.
        d.commit(&other);
        let v = d.evaluate(&other, &plain());
        assert!(is_unchanged(v), "after commit the text must settle, got {v:?}");
    }

    #[test]
    fn fr_002_a_mid_scroll_frame_restarts_the_run_rather_than_advancing_it() {
        let mut d = detector();
        d.commit(&screen(40));

        // Two different unstable frames in a row: each differs from the base
        // AND from the other, so neither continues the other's run.
        let a = "aaaa ".repeat(60);
        let b = "zzzz ".repeat(60);
        let first = d.evaluate(&a, &plain());
        let second = d.evaluate(&b, &plain());

        assert!(is_pending(first), "got {first:?}");
        assert!(
            is_pending(second),
            "a frame unlike its predecessor must restart the stability run, got {second:?}"
        );
        assert!(
            matches!(second, Verdict::Pending { seen, .. } if seen == 1),
            "the run must restart at 1, got {second:?}"
        );
    }

    // ------------------------------------------------------------------ FR-003

    #[test]
    fn fr_003_slow_drift_accumulates_against_the_committed_base() {
        let mut d = detector();
        let base = screen(40);
        d.commit(&base);

        let mut current = base.clone();
        let mut verdicts = Vec::new();
        for i in 0..40 {
            current.push_str(&format!("\nappended line {i}: something new on screen"));
            verdicts.push(d.evaluate(&current, &plain()));
        }

        assert!(
            is_unchanged(verdicts[0]),
            "one appended line out of 40 must not trip the threshold, got {:?}",
            verdicts[0]
        );
        assert!(
            verdicts.iter().any(|v| is_changed(*v)),
            "cumulative drift must eventually reach Changed"
        );

        // Monotonic in the sense that matters: once it leaves Unchanged for
        // good, it does not drift back, because the base never moved.
        let first_non_unchanged = verdicts.iter().position(|v| !is_unchanged(*v)).unwrap();
        assert!(
            verdicts[first_non_unchanged..].iter().all(|v| !is_unchanged(*v)),
            "similarity to a fixed base must not recover as more text is appended"
        );
    }

    // ------------------------------------------------------------------ FR-004

    #[test]
    fn fr_004_the_overlays_own_answer_is_subtracted_before_comparing() {
        let mut d = detector();
        let base = screen(40);
        d.commit(&base);

        let answer = "The fox is quick.\nThe dog is lazy.";
        let contaminated = format!("{base}\n{answer}");

        let v = d.evaluate(
            &contaminated,
            &DetectorInput {
                exclude: Some(answer),
                force: false,
            },
        );
        assert!(
            is_unchanged(v),
            "base plus the overlay's own answer must read as Unchanged, got {v:?}"
        );

        // Control: without the exclusion the same frame is not Unchanged,
        // which is what makes the subtraction load-bearing rather than inert.
        let mut d2 = detector();
        d2.commit(&base);
        let long_answer = "The fox is quick.\n".repeat(40);
        let contaminated2 = format!("{base}\n{long_answer}");
        let without = d2.evaluate(&contaminated2, &plain());
        let mut d3 = detector();
        d3.commit(&base);
        let with = d3.evaluate(
            &contaminated2,
            &DetectorInput {
                exclude: Some(&long_answer),
                force: false,
            },
        );
        assert!(
            with.similarity() > without.similarity(),
            "excluding the answer must raise similarity: with={} without={}",
            with.similarity(),
            without.similarity()
        );
    }

    // ------------------------------------------------------------------ FR-005

    #[test]
    fn fr_005_force_changes_identical_text_without_touching_the_run() {
        let mut d = detector();
        let text = screen(40);
        d.commit(&text);

        // Arm a partial stability run.
        let other = "wholly different text about tides".repeat(8);
        let armed = d.evaluate(&other, &plain());
        assert!(is_pending(armed), "expected a partial run, got {armed:?}");

        // A forced evaluation answers regardless ...
        let forced = d.evaluate(
            &text,
            &DetectorInput {
                exclude: None,
                force: true,
            },
        );
        assert!(is_changed(forced), "force must be Changed, got {forced:?}");

        // ... and leaves the run exactly where it was: the next sighting of
        // `other` advances to Changed rather than restarting at 1.
        let after = d.evaluate(&other, &plain());
        assert!(
            is_changed(after),
            "force must not disturb the pending counter, got {after:?}"
        );
    }

    // ------------------------------------------------------------------ FR-006

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        })]

        /// FR-006: no pair of arbitrary Unicode strings may panic. The bound
        /// is well under the spec's 20 000 so the suite stays fast; the
        /// quadratic cost is capped by `max_compare_chars` regardless.
        #[test]
        fn fr_006_evaluate_never_panics_on_arbitrary_unicode(
            base in any::<String>(),
            current in any::<String>(),
            answer in any::<String>(),
            force in any::<bool>(),
        ) {
            let mut d = detector();
            d.commit(&base);
            let v = d.evaluate(&current, &DetectorInput { exclude: Some(&answer), force });
            let s = v.similarity();
            prop_assert!((0.0..=1.0).contains(&s), "similarity out of range: {}", s);
        }

        /// The prefix bound must hold on multi-byte input too: truncating by
        /// characters can never split a code point.
        #[test]
        fn fr_006_multibyte_prefixes_do_not_split_a_code_point(
            text in "\\PC{0,400}",
        ) {
            let mut d = LevenshteinDetector::new(DEFAULT_THRESHOLD, 2, 8);
            d.commit(&text);
            let v = d.evaluate(&text, &DetectorInput::default());
            prop_assert!(matches!(v, Verdict::Unchanged { .. }), "got {:?}", v);
        }
    }

    // ------------------------------------------------------- module contract

    #[test]
    fn reset_forgets_the_baseline_and_the_partial_run() {
        let mut d = detector();
        d.commit(&screen(40));
        let other = "different".repeat(50);
        let _ = d.evaluate(&other, &plain());

        d.reset();

        assert_eq!(d.base(), "", "reset must clear the baseline");
        // With an empty base, an empty frame is the identical case again.
        let v = d.evaluate("", &plain());
        assert!(is_unchanged(v), "after reset, empty vs empty is Unchanged, got {v:?}");
    }

    #[test]
    fn verdict_maps_down_to_the_reducers_float_free_mirror() {
        assert_eq!(
            machine::Verdict::from(Verdict::Unchanged { similarity: 1.0 }),
            machine::Verdict::Unchanged
        );
        assert_eq!(
            machine::Verdict::from(Verdict::Pending {
                similarity: 0.5,
                seen: 1
            }),
            machine::Verdict::Pending
        );
        assert_eq!(
            machine::Verdict::from(Verdict::Changed { similarity: 0.1 }),
            machine::Verdict::Changed
        );
    }

    #[test]
    fn a_nonsense_configuration_degrades_instead_of_panicking() {
        // Settings arrive from the user (spec 014); NaN and 0 must not panic.
        let mut d = LevenshteinDetector::new(f32::NAN, 0, 16);
        let v = d.evaluate("anything", &plain());
        assert!(
            !matches!(v, Verdict::Pending { .. }),
            "stability_frames must clamp to at least 1, so a first sighting is decisive"
        );
    }
}
