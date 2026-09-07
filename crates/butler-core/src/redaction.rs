// Spec: specs/015-privacy-boundary/spec.md

//! The privacy boundary's one piece of code: [`redact`].
//!
//! Constitution §V says the user's screen is the user's data. Everything
//! derived from it stays in memory, and exactly one thing ever leaves the
//! machine: a [`RedactedText`] inside an `InferenceRequest`, addressed to the
//! one provider the user configured.
//!
//! This module is the gate in front of that. It removes secret-shaped content
//! from recognized text, and it is the *only* constructor of [`RedactedText`],
//! so "text that has been through redaction" is a type rather than a promise:
//! there is no `From<String>`, no public field, and no way to obtain one
//! except by calling [`redact`] (FR-002).
//!
//! The patterns in §3.2 are deliberately high-confidence shapes rather than
//! general-purpose entropy heuristics. A false positive costs the assistant a
//! little context; a false negative sends a live credential to a third party.
//! When the two trade off, this module prefers the former.
//!
//! Pure and deterministic: no clock, no filesystem, no network, no
//! randomness, no allocation the caller cannot see. The same input and policy
//! always give the same output and the same count, which is what makes the
//! fixtures in `tests/fixtures/redaction/` a meaningful contract.

use core::fmt;

/// What [`redact`] removed, for the log line that records only the count.
///
/// Spec 015 §3.5: logs carry counts and kinds, never content. These names are
/// the whole vocabulary a log line may use about a redaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKind {
    /// An Anthropic API key (`sk-ant-...`).
    AnthropicKey,
    /// A generic `sk-`-prefixed key of at least 32 characters.
    ApiKey,
    /// A GitHub token (`ghp_`, `gho_`, `ghs_`).
    GitHubToken,
    /// A Slack token (`xoxb-`, `xoxa-`, `xoxp-`).
    SlackToken,
    /// An AWS access key id (`AKIA` plus 16 upper-case alphanumerics).
    AwsAccessKey,
    /// A Google API key (`AIza...`).
    GoogleApiKey,
    /// A JSON Web Token (`eyJ....eyJ...`).
    Jwt,
    /// A PEM private key block, header through footer.
    PrivateKeyBlock,
    /// An HTTP `Bearer` credential.
    BearerToken,
    /// A 16-digit sequence that passes the Luhn check.
    CardNumber,
    /// An e-mail address (opt-in).
    Email,
    /// An E.164-shaped phone number (opt-in).
    Phone,
    /// An IBAN (opt-in).
    Iban,
}

impl SecretKind {
    /// The token that replaces the secret, as it appears in the output.
    #[must_use]
    pub fn placeholder(self) -> &'static str {
        match self {
            SecretKind::AnthropicKey => "[REDACTED:AnthropicKey]",
            SecretKind::ApiKey => "[REDACTED:ApiKey]",
            SecretKind::GitHubToken => "[REDACTED:GitHubToken]",
            SecretKind::SlackToken => "[REDACTED:SlackToken]",
            SecretKind::AwsAccessKey => "[REDACTED:AwsAccessKey]",
            SecretKind::GoogleApiKey => "[REDACTED:GoogleApiKey]",
            SecretKind::Jwt => "[REDACTED:Jwt]",
            SecretKind::PrivateKeyBlock => "[REDACTED:PrivateKeyBlock]",
            SecretKind::BearerToken => "[REDACTED:BearerToken]",
            SecretKind::CardNumber => "[REDACTED:CardNumber]",
            SecretKind::Email => "[REDACTED:Email]",
            SecretKind::Phone => "[REDACTED:Phone]",
            SecretKind::Iban => "[REDACTED:Iban]",
        }
    }

    /// Whether this kind is only removed when the user opts in to PII
    /// redaction (§3.2: the "Opt-in" list).
    #[must_use]
    pub fn is_pii(self) -> bool {
        matches!(
            self,
            SecretKind::Email | SecretKind::Phone | SecretKind::Iban
        )
    }
}

/// What the user asked to be removed (spec 014 supplies the values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedactionPolicy {
    /// Master switch. `false` is an explicit, warned-about user choice; it
    /// makes [`redact`] the identity but *not* optional (§3.2).
    pub enabled: bool,
    /// Also remove e-mail addresses, phone numbers and IBANs.
    pub pii: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            pii: false,
        }
    }
}

/// Text that has been through [`redact`], and the only thing that may leave
/// the process (spec 015 §3.1).
///
/// There is deliberately no `From<String>`, no `new`, no public field and no
/// `DerefMut`: the only way to obtain one is [`redact`]. FR-002 pins that
/// shut with a compile-fail test:
///
/// ```compile_fail
/// use butler_core::redaction::RedactedText;
/// // No public constructor exists, so this cannot compile.
/// let t = RedactedText::from("sk-ant-secret".to_string());
/// ```
///
/// ```compile_fail
/// use butler_core::redaction::RedactedText;
/// // The field is private, so neither can this.
/// let t = RedactedText { text: String::new(), redaction_count: 0 };
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedText {
    text: String,
    redaction_count: usize,
}

impl RedactedText {
    /// The redacted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// How many secrets were replaced. The only number a log may record.
    #[must_use]
    pub fn redaction_count(&self) -> usize {
        self.redaction_count
    }

    /// Whether anything was removed.
    #[must_use]
    pub fn is_redacted(&self) -> bool {
        self.redaction_count > 0
    }
}

/// Shows the count, never the content.
///
/// `Debug` on a type that wraps screen text is a leak waiting for a `dbg!` or
/// a `#[derive(Debug)]` on a struct that holds one, so this prints the shape
/// and nothing else (§3.5: logs carry counts, never content).
impl fmt::Debug for RedactedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedactedText")
            .field("chars", &self.text.chars().count())
            .field("redaction_count", &self.redaction_count)
            .finish()
    }
}

/// Remove secret-shaped content from `text`.
///
/// The single constructor of [`RedactedText`]. With `policy.enabled == false`
/// this is the identity, but it is still the only way through: disabling
/// redaction is a user choice about *content*, not a way to bypass the type.
///
/// This is the positive control for the two `compile_fail` blocks on
/// [`RedactedText`]: the same import path, used the one legal way, compiles.
///
/// ```
/// use butler_core::redaction::{redact, RedactionPolicy};
///
/// let out = redact("token ghp_AAAAAAAAAAAAAAAA end", &RedactionPolicy::default());
/// assert_eq!(out.redaction_count(), 1);
/// assert!(out.as_str().contains("[REDACTED:GitHubToken]"));
/// ```
#[must_use]
pub fn redact(text: &str, policy: &RedactionPolicy) -> RedactedText {
    if !policy.enabled {
        return RedactedText {
            text: text.to_owned(),
            redaction_count: 0,
        };
    }

    // PEM blocks span lines, so they are handled before the line-wise pass;
    // everything else is contained within one line.
    let (mut out, mut count) = redact_private_key_blocks(text);

    let mut redacted_lines = Vec::with_capacity(out.lines().count());
    for line in out.lines() {
        let (new_line, n) = redact_line(line, *policy);
        count += n;
        redacted_lines.push(new_line);
    }
    // §3.2: the output keeps line structure so the assistant still sees layout.
    out = redacted_lines.join("\n");
    if text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }

    RedactedText {
        text: out,
        redaction_count: count,
    }
}

/// Replace each `-----BEGIN ... PRIVATE KEY-----` ... `-----END ...-----`
/// block, inclusive, with a single placeholder line.
fn redact_private_key_blocks(text: &str) -> (String, usize) {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut in_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if !in_block && trimmed.starts_with(BEGIN) && trimmed.contains("PRIVATE KEY") {
            in_block = true;
            out.push_str(SecretKind::PrivateKeyBlock.placeholder());
            out.push('\n');
            count += 1;
            continue;
        }
        if in_block {
            if trimmed.starts_with(END) {
                in_block = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    // `lines()` dropped the trailing newline distinction; restore the input's.
    if !text.ends_with('\n') {
        out.pop();
    }
    (out, count)
}

/// Every single-line rule, applied left to right so that offsets stay valid.
fn redact_line(line: &str, policy: RedactionPolicy) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let mut count = 0;
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < line.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &line[i..];

        if let Some((kind, len)) = match_secret(rest, bytes, i, policy) {
            out.push_str(kind.placeholder());
            count += 1;
            i += len;
            continue;
        }

        // Advance one whole character; never split a code point (FR-006-style
        // totality: this must not panic on any Unicode input).
        let ch_len = rest.chars().next().map_or(1, char::len_utf8);
        out.push_str(&rest[..ch_len]);
        i += ch_len;
    }

    (out, count)
}

/// Try every pattern at `rest`'s start; return the kind and byte length.
fn match_secret(
    rest: &str,
    bytes: &[u8],
    at: usize,
    policy: RedactionPolicy,
) -> Option<(SecretKind, usize)> {
    // Only start a match at a token boundary, so `task-sk-...` inside a word
    // is not silently rewritten mid-identifier.
    if at > 0 && !is_boundary(bytes[at - 1]) {
        return None;
    }

    if let Some(len) = match_prefixed(rest, "sk-ant-", 8) {
        return Some((SecretKind::AnthropicKey, len));
    }
    if let Some(len) = match_prefixed(rest, "sk-", 32) {
        return Some((SecretKind::ApiKey, len));
    }
    for p in ["ghp_", "gho_", "ghs_"] {
        if let Some(len) = match_prefixed(rest, p, 8) {
            return Some((SecretKind::GitHubToken, len));
        }
    }
    for p in ["xoxb-", "xoxa-", "xoxp-"] {
        if let Some(len) = match_prefixed(rest, p, 8) {
            return Some((SecretKind::SlackToken, len));
        }
    }
    if let Some(len) = match_aws(rest) {
        return Some((SecretKind::AwsAccessKey, len));
    }
    if let Some(len) = match_prefixed(rest, "AIza", 35) {
        return Some((SecretKind::GoogleApiKey, len));
    }
    if let Some(len) = match_jwt(rest) {
        return Some((SecretKind::Jwt, len));
    }
    if let Some(len) = match_bearer(rest) {
        return Some((SecretKind::BearerToken, len));
    }
    if let Some(len) = match_card(rest) {
        return Some((SecretKind::CardNumber, len));
    }
    if policy.pii {
        if let Some(len) = match_email(rest) {
            return Some((SecretKind::Email, len));
        }
        if let Some(len) = match_iban(rest) {
            return Some((SecretKind::Iban, len));
        }
        if let Some(len) = match_phone(rest) {
            return Some((SecretKind::Phone, len));
        }
    }
    None
}

/// A byte that may precede a secret: anything that is not part of a token.
fn is_boundary(b: u8) -> bool {
    !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b'@')
}

/// The run of token characters at the start of `s`.
fn token_len(s: &str) -> usize {
    s.bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        .count()
}

/// A token starting with `prefix` and at least `min_total` characters long.
fn match_prefixed(s: &str, prefix: &str, min_total: usize) -> Option<usize> {
    if !s.starts_with(prefix) {
        return None;
    }
    let len = token_len(s);
    (len >= min_total).then_some(len)
}

/// `AKIA` followed by exactly 16 upper-case alphanumerics.
fn match_aws(s: &str) -> Option<usize> {
    if !s.starts_with("AKIA") {
        return None;
    }
    let len = token_len(s);
    if len != 20 {
        return None;
    }
    s[4..20]
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        .then_some(20)
}

/// `eyJ<base64url>.eyJ<base64url>` with an optional third segment.
fn match_jwt(s: &str) -> Option<usize> {
    if !s.starts_with("eyJ") {
        return None;
    }
    let len = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_' || *b == b'.')
        .count();
    let candidate = &s[..len];
    let mut parts = candidate.split('.');
    let (Some(h), Some(p)) = (parts.next(), parts.next()) else {
        return None;
    };
    (h.starts_with("eyJ") && p.starts_with("eyJ") && p.len() > 3).then_some(len)
}

/// `Bearer <token>`, case-insensitive on the scheme.
///
/// `get` rather than a slice: the scheme is 7 bytes, and a 7-byte index into
/// arbitrary screen text can land inside a multi-byte character. The token
/// length is summed over whole characters for the same reason, so the byte
/// offset this returns is always a char boundary.
fn match_bearer(s: &str) -> Option<usize> {
    let scheme = "bearer ";
    let head = s.get(..scheme.len())?;
    if !head.eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = &s[scheme.len()..];
    let len: usize = rest
        .chars()
        .take_while(|c| !c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    (len >= 8).then_some(scheme.len() + len)
}

/// Exactly 16 digits (optionally in groups of four) passing the Luhn check.
fn match_card(s: &str) -> Option<usize> {
    let mut digits = Vec::with_capacity(16);
    let mut consumed = 0;
    for (idx, b) in s.bytes().enumerate() {
        if b.is_ascii_digit() {
            digits.push(b - b'0');
            consumed = idx + 1;
            if digits.len() == 16 {
                break;
            }
        } else if !((b == b' ' || b == b'-')
            && !digits.is_empty()
            && digits.len().is_multiple_of(4))
        {
            break;
        }
    }
    if digits.len() != 16 {
        return None;
    }
    // A 17th adjacent digit means this is a longer number, not a card.
    if s.as_bytes().get(consumed).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    luhn(&digits).then_some(consumed)
}

/// The Luhn checksum, which every real card number satisfies.
fn luhn(digits: &[u8]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            let mut v = u32::from(d);
            if i % 2 == 1 {
                v *= 2;
                if v > 9 {
                    v -= 9;
                }
            }
            v
        })
        .sum();
    sum.is_multiple_of(10)
}

/// `local@domain.tld`, conservative on both sides.
fn match_email(s: &str) -> Option<usize> {
    let local = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || b"._%+-".contains(b))
        .count();
    if local == 0 || s.as_bytes().get(local) != Some(&b'@') {
        return None;
    }
    let after = &s[local + 1..];
    let domain = after
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'.' || *b == b'-')
        .count();
    let host = &after[..domain];
    let tld_ok = host
        .rsplit_once('.')
        .is_some_and(|(_, tld)| tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic()));
    tld_ok.then_some(local + 1 + domain)
}

/// Two country letters, two check digits, then 11 to 26 alphanumerics.
fn match_iban(s: &str) -> Option<usize> {
    let len = token_len(s);
    if !(15..=34).contains(&len) {
        return None;
    }
    let b = s.as_bytes();
    let shaped = b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && s[4..len].bytes().all(|c| c.is_ascii_alphanumeric());
    // An IBAN's body is not all letters; that shape is an ordinary word.
    let has_digit = s[4..len].bytes().any(|c| c.is_ascii_digit());
    (shaped && has_digit).then_some(len)
}

/// `+` followed by 8 to 15 digits, E.164 shaped.
fn match_phone(s: &str) -> Option<usize> {
    if !s.starts_with('+') {
        return None;
    }
    let digits = s[1..].bytes().take_while(u8::is_ascii_digit).count();
    (8..=15).contains(&digits).then_some(1 + digits)
}
