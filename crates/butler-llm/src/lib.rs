// Spec: specs/010-assistant-inference/spec.md

//! Streaming assistant inference (spec 010).
//!
//! Everything upstream exists to produce one thing: a short, useful answer
//! about what is on the screen, streamed fast enough to read as it arrives.
//! This crate is the boundary between butler-ai and the model provider, so
//! the rest of the system depends on [`assistant::Assistant`] and a second
//! provider is a new file rather than a redesign.
//!
//! It is also where the two most sensitive things live: the API key and the
//! outbound request. Both are named types with narrow APIs, so spec 015 has
//! something to constrain rather than a convention to hope for.
//!
//! Rust has no official Anthropic SDK, so [`anthropic`] is raw HTTPS and the
//! wire contract is pinned by the fixtures under `tests/fixtures/`.

pub mod anthropic;
pub mod assistant;
pub mod budget;
pub mod prompt;
pub mod secrets;
pub mod sse;

pub use anthropic::{AnthropicAssistant, DEFAULT_BASE_URL, DEFAULT_MODEL};
pub use assistant::{
    Assistant, Chunk, InferenceError, InferenceRequest, ProviderId, StopReason, Usage,
};
pub use budget::{Admit, DenyReason, SpendGuard};
pub use prompt::{Prompt, PromptConfig, SYSTEM_PROMPT, build};
pub use secrets::{CredentialAlarm, Secret, SecretError, SecretStore};
pub use sse::{Frame, SseParser};
