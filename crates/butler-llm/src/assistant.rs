// Spec: specs/010-assistant-inference/spec.md

//! The provider boundary (spec 010 §3.1).
//!
//! Everything upstream exists to produce one thing: a short, useful answer
//! about what is on the screen, streamed fast enough to read as it arrives.
//! This module is the seam, so a second provider is a new file rather than a
//! redesign.
//!
//! # What may leave the process
//!
//! [`InferenceRequest::screen_text`] is a [`RedactedText`], and spec 015 makes
//! `butler_core::redaction::redact` its only constructor. There is no
//! `From<String>`, so a caller cannot assemble a request out of raw screen
//! text even by accident: the redaction pass is not a step someone can forget,
//! it is the only door.

use butler_core::machine::RequestId;
use butler_core::redaction::RedactedText;
use butler_core::settings::{AnswerStyle, Effort};

/// Which provider a client speaks to (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderId {
    /// Anthropic's Messages API, the reference implementation.
    Anthropic,
}

impl ProviderId {
    /// The name settings and the keychain use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
        }
    }
}

/// One question about one screen (§3.1).
///
/// Deliberately not `Clone`: a request carries the user's screen, and one
/// request should have one owner for the same reason a `Frame` does.
#[derive(Debug)]
pub struct InferenceRequest {
    /// Which inference this is, for correlating events.
    pub request: RequestId,
    /// The screen, redacted. Spec 015: the only type that may leave.
    pub screen_text: RedactedText,
    /// The previous answer, for continuity. Bounded by the prompt budget.
    pub prior_answer: Option<String>,
    /// What the user typed in the overlay, if anything.
    pub user_note: Option<String>,
    /// How hard to think (settings).
    pub effort: Effort,
    /// How much to say (settings).
    pub answer_style: AnswerStyle,
    /// The answer's token cap.
    pub max_output_tokens: u32,
}

/// Why the model stopped (§3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// It finished.
    EndTurn,
    /// It hit `max_tokens`.
    MaxTokens,
    /// A safety classifier declined.
    ///
    /// The category is an **open set** on the wire, so it is a `String` rather
    /// than an enum: a category this build has never heard of must still reach
    /// the UI intact rather than being flattened into "other".
    Refusal {
        /// The category the API reported, if it reported one.
        category: Option<String>,
    },
    /// Anything else the provider said.
    Other(String),
}

/// What one inference cost (§3.5 feeds this to the spend guard).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Tokens the request consumed.
    pub input_tokens: u64,
    /// Tokens the answer consumed.
    pub output_tokens: u64,
}

/// A piece of the answer, or its end (§3.1).
#[derive(Clone, Debug, PartialEq)]
pub enum Chunk {
    /// More answer text.
    Text(String),
    /// The stream ended.
    Done {
        /// Why.
        stop: StopReason,
        /// What it cost.
        usage: Usage,
    },
}

/// Why an inference failed (§3.2).
///
/// Every variant carries a kind and, at most, the provider's own words about
/// its own state. None carries screen text: spec 015 §3.5 keeps the user's
/// content out of errors as firmly as out of logs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InferenceError {
    /// No API key is stored for this provider.
    ///
    /// §3.4: the runtime moves to `Idle` with a UI prompt, **not** `Fault`.
    /// A missing key is a setup step, not a failure.
    #[error("no credential stored for {0}")]
    NoCredential(&'static str),
    /// The key was rejected (HTTP 401).
    #[error("the stored credential was rejected")]
    SecretInvalid,
    /// The request never reached the provider.
    #[error("network: {0}")]
    Network(String),
    /// The provider answered with an error.
    #[error("provider {status}: {kind}")]
    Provider {
        /// The HTTP status.
        status: u16,
        /// The provider's `error.type`, or `unknown`.
        kind: String,
        /// Whether retrying could help (§3.2).
        retryable: bool,
    },
    /// The stream stalled or the whole call ran too long.
    #[error("timed out")]
    Timeout,
    /// The response did not parse as the contract says it should.
    #[error("malformed response: {0}")]
    Malformed(String),
    /// The spend guard refused before anything was sent (§3.5).
    #[error("budget exhausted: {0}")]
    BudgetExhausted(String),
}

impl InferenceError {
    /// Whether the runtime should retry.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::Timeout => true,
            Self::Provider { retryable, .. } => *retryable,
            Self::NoCredential(_)
            | Self::SecretInvalid
            | Self::Malformed(_)
            | Self::BudgetExhausted(_) => false,
        }
    }

    /// The wire kind the machine and the overlay see (spec 011 §3.1).
    #[must_use]
    pub const fn kind(&self) -> butler_core::machine::ErrorKind {
        use butler_core::machine::ErrorKind;
        match self {
            Self::NoCredential(_) | Self::SecretInvalid => ErrorKind::Credential,
            Self::Network(_) | Self::Timeout => ErrorKind::Network,
            Self::Provider { .. } => ErrorKind::Provider,
            Self::BudgetExhausted(_) => ErrorKind::Budget,
            Self::Malformed(_) => ErrorKind::Internal,
        }
    }
}

/// A streaming assistant (§3.1).
///
/// The stream is boxed rather than an associated type so the runtime can hold
/// `Box<dyn Assistant>` and swap providers from settings without the whole
/// pipeline becoming generic over the provider.
pub trait Assistant: Send + Sync {
    /// Stream the answer to `request`.
    ///
    /// Cancelling `cancel` MUST end the stream and drop the connection
    /// (FR-003). The runtime cancels when the user disarms or a newer capture
    /// supersedes this one.
    fn stream(
        &self,
        request: InferenceRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> futures_core::stream::BoxStream<'static, Result<Chunk, InferenceError>>;

    /// Which provider this is.
    fn id(&self) -> ProviderId;
}

#[cfg(test)]
mod tests {
    use super::{InferenceError, ProviderId, StopReason};
    use butler_core::machine::ErrorKind;

    #[test]
    fn the_provider_name_is_the_one_settings_and_the_keychain_use() {
        assert_eq!(ProviderId::Anthropic.as_str(), "anthropic");
        // Spec 014's default provider, so a fresh install finds its key.
        assert_eq!(
            butler_core::settings::Settings::default()
                .assistant
                .provider,
            ProviderId::Anthropic.as_str()
        );
    }

    /// §3.2 and §3.4. A missing key is a setup step, a rejected key is not
    /// retryable, and only the transport failures are.
    #[test]
    fn only_transport_failures_are_retryable() {
        assert!(InferenceError::Network("reset".into()).retryable());
        assert!(InferenceError::Timeout.retryable());
        assert!(
            InferenceError::Provider {
                status: 429,
                kind: "rate_limit_error".into(),
                retryable: true
            }
            .retryable()
        );

        assert!(!InferenceError::NoCredential("anthropic").retryable());
        assert!(!InferenceError::SecretInvalid.retryable());
        assert!(!InferenceError::Malformed("bad frame".into()).retryable());
        assert!(!InferenceError::BudgetExhausted("daily".into()).retryable());
        assert!(
            !InferenceError::Provider {
                status: 400,
                kind: "invalid_request_error".into(),
                retryable: false
            }
            .retryable()
        );
    }

    #[test]
    fn every_error_maps_to_a_wire_kind() {
        assert_eq!(
            InferenceError::NoCredential("anthropic").kind(),
            ErrorKind::Credential
        );
        assert_eq!(InferenceError::SecretInvalid.kind(), ErrorKind::Credential);
        assert_eq!(InferenceError::Timeout.kind(), ErrorKind::Network);
        assert_eq!(
            InferenceError::BudgetExhausted("x".into()).kind(),
            ErrorKind::Budget
        );
        assert_eq!(
            InferenceError::Malformed("x".into()).kind(),
            ErrorKind::Internal
        );
    }

    /// §3.1: the refusal category is an open set on the wire, so a category
    /// this build has never heard of still reaches the UI intact.
    #[test]
    fn an_unknown_refusal_category_survives() {
        let stop = StopReason::Refusal {
            category: Some("a-category-from-2027".to_owned()),
        };
        let StopReason::Refusal { category } = stop else {
            panic!("constructed a refusal");
        };
        assert_eq!(category.as_deref(), Some("a-category-from-2027"));
    }
}
