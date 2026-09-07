// Spec: specs/015-privacy-boundary/spec.md

// AC-3: the redaction fixtures contain only synthetic values.
//
// Every credential-shaped string in the fixtures is built here, from a fixed
// alphabet and a fixed seed, so no real secret can enter the corpus by being
// pasted into a test. The values are shaped like the real thing (they satisfy
// the same prefixes, lengths and checksums the matchers look for) and are
// valueless: the AWS id spells `EXAMPLE`, the card number is the industry
// test number, and the PEM block's body is the word `SYNTHETIC` repeated.
//
// This file is a library of constructors rather than a binary. `tests/
// redaction.rs` uses it via `include!`, so the fixtures are compiled with the
// tests and cannot drift from them, and `cargo test` needs no build step and
// no committed generated output to review.
//
// To see what a fixture expands to, read the constant: they are all literals
// or trivial `concat!`s, deliberately, so review is by eye.

/// A deterministic filler of `n` lower-case letters, cycling `a..z`.
///
/// Not random: AC-3 wants values a reviewer can reproduce by reading.
#[allow(dead_code)]
pub fn filler(n: usize) -> String {
    (0..n)
        .map(|i| char::from(b'a' + u8::try_from(i % 26).expect("i % 26 < 26")))
        .collect()
}

/// The lorem the secrets are embedded in, so the matchers are exercised in
/// context rather than on a bare token.
#[allow(dead_code)]
pub const LOREM: &str =
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.";

// ------------------------------------------------------------ synthetic keys

/// `sk-ant-` plus 40 synthetic characters.
#[allow(dead_code)]
pub const ANTHROPIC_KEY: &str = "sk-ant-api03-SYNTHETICSYNTHETICSYNTHETICSYNTHETICxyz";
/// A generic `sk-` key comfortably over the 32-character floor.
#[allow(dead_code)]
pub const API_KEY: &str = "sk-SYNTHETICSYNTHETICSYNTHETICSYNTHETIC00";
/// A GitHub personal access token.
#[allow(dead_code)]
pub const GITHUB_TOKEN: &str = "ghp_SYNTHETICSYNTHETICSYNTHETIC000000";
/// A Slack bot token.
#[allow(dead_code)]
pub const SLACK_TOKEN: &str = "xoxb-000000000000-000000000000-SYNTHETICSYNTHETIC00";
/// An AWS access key id: `AKIA` plus exactly 16 upper-case alphanumerics.
#[allow(dead_code)]
pub const AWS_ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
/// A Google API key: `AIza` plus 35 characters.
#[allow(dead_code)]
pub const GOOGLE_API_KEY: &str = "AIzaSySYNTHETICSYNTHETICSYNTHETIC0000000";
/// A three-segment JWT whose payload decodes to `{"sub":"synthetic"}`.
#[allow(dead_code)]
pub const JWT: &str = concat!(
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.",
    "eyJzdWIiOiJzeW50aGV0aWMifQ.",
    "SYNTHETICSIGNATUREvalue00000000000000000000"
);
/// An HTTP `Bearer` credential.
#[allow(dead_code)]
pub const BEARER: &str = "Bearer SYNTHETICSYNTHETICSYNTHETIC00";
/// The industry-standard Visa *test* number, which passes Luhn and is not
/// issued to anyone.
#[allow(dead_code)]
pub const CARD_NUMBER: &str = "4111111111111111";
/// The same number in the grouped form a screen usually shows.
#[allow(dead_code)]
pub const CARD_NUMBER_GROUPED: &str = "4111 1111 1111 1111";

/// A PEM private key block whose body is the word SYNTHETIC, not a key.
#[allow(dead_code)]
pub const PRIVATE_KEY_BLOCK: &str = concat!(
    "-----BEGIN RSA PRIVATE KEY-----\n",
    "U1lOVEhFVElDU1lOVEhFVElDU1lOVEhFVElDU1lOVEhFVElD\n",
    "U1lOVEhFVElDU1lOVEhFVElDU1lOVEhFVElDU1lOVEhFVElD\n",
    "-----END RSA PRIVATE KEY-----"
);

// ------------------------------------------------------------- synthetic PII

/// `example.com` is reserved by RFC 2606 and cannot be registered.
#[allow(dead_code)]
pub const EMAIL: &str = "someone@example.com";
/// `+1 555 0100` style numbers are reserved for fiction.
#[allow(dead_code)]
pub const PHONE: &str = "+15550100999";
/// The IBAN registry's own documentation example.
#[allow(dead_code)]
pub const IBAN: &str = "GB82WEST12345698765432";

/// Embed `secret` in lorem so the matcher sees realistic surroundings.
#[allow(dead_code)]
pub fn in_context(secret: &str) -> String {
    format!("{LOREM}\nthe value is {secret} and the rest continues.\n{LOREM}")
}
