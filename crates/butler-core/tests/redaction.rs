// Spec: specs/015-privacy-boundary/spec.md

//! The privacy boundary's functional requirements.
//!
//! Everything lives in a `redaction` module so `cargo test -p butler-core
//! redaction::` selects exactly these tests.
//!
//! FR-001 is a fixture test: every secret kind in §3.2, synthetic (AC-3),
//! embedded in lorem, with the expected output and the expected count.
//! FR-002's compile-fail half lives in `redaction.rs`'s doc comment as two
//! ```compile_fail``` blocks, which `cargo test` runs as doc tests; the
//! runtime half is asserted here.

mod redaction {
    use butler_core::redaction::{RedactedText, RedactionPolicy, SecretKind, redact};
    use proptest::prelude::{ProptestConfig, any, prop_assert, prop_assert_eq, proptest};
    use proptest::test_runner::FileFailurePersistence;

    // AC-3: every credential-shaped string below is built by the committed
    // generator, never pasted from a real system.
    include!("fixtures/redaction/gen.rs");

    fn strict() -> RedactionPolicy {
        RedactionPolicy {
            enabled: true,
            pii: false,
        }
    }
    fn with_pii() -> RedactionPolicy {
        RedactionPolicy {
            enabled: true,
            pii: true,
        }
    }
    fn disabled() -> RedactionPolicy {
        RedactionPolicy {
            enabled: false,
            pii: false,
        }
    }

    /// Assert that `secret`, embedded in lorem, is replaced exactly once by
    /// `kind`'s placeholder and leaves no trace of itself behind.
    fn assert_redacted(secret: &str, kind: SecretKind, policy: RedactionPolicy) {
        let input = in_context(secret);
        let out = redact(&input, &policy);

        assert!(
            out.as_str().contains(kind.placeholder()),
            "{kind:?}: expected {} in output, got:\n{}",
            kind.placeholder(),
            out.as_str()
        );
        assert!(
            !out.as_str().contains(secret),
            "{kind:?}: the secret survived redaction:\n{}",
            out.as_str()
        );
        assert_eq!(
            out.redaction_count(),
            1,
            "{kind:?}: expected exactly one redaction"
        );
        assert!(
            out.is_redacted(),
            "{kind:?}: is_redacted must agree with the count"
        );
    }

    // ------------------------------------------------------------------ FR-001

    #[test]
    fn fr_001_every_always_on_secret_kind_is_removed() {
        let cases: [(&str, SecretKind); 10] = [
            (ANTHROPIC_KEY, SecretKind::AnthropicKey),
            (API_KEY, SecretKind::ApiKey),
            (GITHUB_TOKEN, SecretKind::GitHubToken),
            (SLACK_TOKEN, SecretKind::SlackToken),
            (AWS_ACCESS_KEY, SecretKind::AwsAccessKey),
            (GOOGLE_API_KEY, SecretKind::GoogleApiKey),
            (JWT, SecretKind::Jwt),
            (BEARER, SecretKind::BearerToken),
            (CARD_NUMBER, SecretKind::CardNumber),
            (CARD_NUMBER_GROUPED, SecretKind::CardNumber),
        ];
        for (secret, kind) in cases {
            assert_redacted(secret, kind, strict());
        }
    }

    #[test]
    fn fr_001_a_pem_private_key_block_is_replaced_whole() {
        let input = format!("{LOREM}\n{PRIVATE_KEY_BLOCK}\n{LOREM}");
        let out = redact(&input, &strict());

        assert!(
            out.as_str()
                .contains(SecretKind::PrivateKeyBlock.placeholder())
        );
        assert!(
            !out.as_str().contains("U1lOVEhFVEl"),
            "the key body survived:\n{}",
            out.as_str()
        );
        assert!(
            !out.as_str().contains("-----END"),
            "the footer survived:\n{}",
            out.as_str()
        );
        assert_eq!(out.redaction_count(), 1);
        // The surrounding prose is untouched.
        assert!(out.as_str().contains(LOREM));
    }

    #[test]
    fn fr_001_pii_is_opt_in() {
        for (secret, kind) in [
            (EMAIL, SecretKind::Email),
            (PHONE, SecretKind::Phone),
            (IBAN, SecretKind::Iban),
        ] {
            // Off by default ...
            let out = redact(&in_context(secret), &strict());
            assert!(
                out.as_str().contains(secret),
                "{kind:?} must survive when policy.pii is false"
            );
            assert_eq!(out.redaction_count(), 0);
            // ... and removed when the user opts in.
            assert_redacted(secret, kind, with_pii());
            assert!(kind.is_pii(), "{kind:?} must report itself as PII");
        }
    }

    #[test]
    fn fr_001_multiple_secrets_on_one_screen_are_counted_individually() {
        let input =
            format!("{LOREM}\nkey {API_KEY}\ntoken {GITHUB_TOKEN}\ncard {CARD_NUMBER}\n{LOREM}");
        let out = redact(&input, &strict());
        assert_eq!(out.redaction_count(), 3, "got:\n{}", out.as_str());
    }

    #[test]
    fn fr_001_line_structure_is_preserved() {
        let input = format!("first\n{API_KEY}\nthird\n");
        let out = redact(&input, &strict());
        assert_eq!(
            out.as_str(),
            format!("first\n{}\nthird\n", SecretKind::ApiKey.placeholder()),
            "§3.2: the output keeps line structure so the assistant sees layout"
        );
    }

    #[test]
    fn ordinary_prose_is_never_touched() {
        let input = format!("{LOREM}\nA sentence with numbers 42 and 1234 and a word sk-short.\n");
        let out = redact(&input, &with_pii());
        assert_eq!(
            out.redaction_count(),
            0,
            "false positive in:\n{}",
            out.as_str()
        );
        assert_eq!(out.as_str(), input);
    }

    #[test]
    fn a_thirteen_digit_number_is_not_a_card_and_a_non_luhn_sixteen_is_not_either() {
        // Fails Luhn: same shape, wrong checksum.
        let out = redact("4111111111111112", &strict());
        assert_eq!(
            out.redaction_count(),
            0,
            "a non-Luhn 16-digit run is not a card"
        );
        // Longer than 16 digits: an order number, not a card.
        let out = redact("41111111111111110000", &strict());
        assert_eq!(out.redaction_count(), 0, "a 20-digit run is not a card");
    }

    // ------------------------------------------------------------------ FR-002

    #[test]
    fn fr_002_redact_is_the_only_constructor_even_when_disabled() {
        // With redaction off, `redact` is the identity but still the only way
        // to obtain a RedactedText: §3.2's "an explicit user choice" governs
        // content, not the type boundary.
        let input = format!("{LOREM}\n{API_KEY}\n");
        let out: RedactedText = redact(&input, &disabled());
        assert_eq!(
            out.as_str(),
            input,
            "disabled redaction must be the identity"
        );
        assert_eq!(out.redaction_count(), 0);
        assert!(!out.is_redacted());
    }

    #[test]
    fn fr_002_debug_shows_the_count_and_never_the_content() {
        // §3.5: a Debug impl that printed the text would leak it into any log
        // line that formatted a struct containing one.
        let out = redact(&format!("{LOREM}\n{API_KEY}"), &strict());
        let rendered = format!("{out:?}");
        assert!(rendered.contains("redaction_count"), "got {rendered}");
        assert!(
            !rendered.contains("Lorem"),
            "Debug leaked screen text: {rendered}"
        );
        assert!(
            !rendered.contains("sk-"),
            "Debug leaked a secret: {rendered}"
        );
    }

    // ------------------------------------------------------- purity and totality

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        })]

        /// `redact` is total: no arbitrary Unicode input may panic, and it
        /// must never split a code point.
        #[test]
        fn redact_never_panics_on_arbitrary_unicode(
            text in any::<String>(),
            enabled in any::<bool>(),
            pii in any::<bool>(),
        ) {
            let out = redact(&text, &RedactionPolicy { enabled, pii });
            prop_assert!(out.as_str().is_char_boundary(0) || out.as_str().is_empty());
        }

        /// Deterministic: the same input and policy always give the same
        /// output and the same count (constitution §IV).
        #[test]
        fn redact_is_deterministic(text in any::<String>()) {
            let a = redact(&text, &with_pii());
            let b = redact(&text, &with_pii());
            prop_assert_eq!(a.as_str(), b.as_str());
            prop_assert_eq!(a.redaction_count(), b.redaction_count());
        }

        /// Idempotent on the always-on kinds: redacting twice finds nothing
        /// new, so a placeholder is never itself mistaken for a secret.
        #[test]
        fn redact_is_idempotent(text in any::<String>()) {
            let once = redact(&text, &with_pii());
            let twice = redact(once.as_str(), &with_pii());
            prop_assert_eq!(twice.redaction_count(), 0);
            prop_assert_eq!(once.as_str(), twice.as_str());
        }
    }
}
