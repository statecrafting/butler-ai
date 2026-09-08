// Spec: specs/010-assistant-inference/spec.md

//! The Claude Messages API reference provider (spec 010 §3.2).
//!
//! Spec 015 constrains this file: **exactly one outbound host**, `https://`
//! only, and the override surfaced in the UI and logged as an event kind.
//!
//! Rust has no official Anthropic SDK, so this is raw HTTPS and the wire
//! contract is pinned by the fixtures under `tests/fixtures/` rather than by a
//! dependency. That is a feature of the arrangement, not a workaround: a
//! fixture is a contract someone can read, and it fails loudly when the wire
//! changes rather than silently when a dependency updates.
//!
//! # The request shape, and three things that are 400s
//!
//! Current Claude models reject parameters that older ones accepted, and each
//! of these would be a hard failure rather than a degraded answer:
//!
//! - **`temperature` / `top_p` / `top_k`**: removed. None is sent.
//! - **`thinking.budget_tokens`**: removed. Thinking is left at the model's
//!   adaptive default by omitting the parameter entirely (§3.2).
//! - **Assistant prefill**: rejected. The prior answer is a complete assistant
//!   turn, never a partial one to continue.
//!
//! Depth is controlled by `output_config.effort` instead, which is where the
//! current API puts it.

use std::time::Duration;

use butler_core::settings::Effort;
use futures_util::StreamExt as _;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::assistant::{
    Assistant, Chunk, InferenceError, InferenceRequest, ProviderId, StopReason, Usage,
};
use crate::prompt::{Prompt, PromptConfig, build};
use crate::secrets::Secret;
use crate::sse::SseParser;

/// The one host this provider contacts (spec 015 §3.3).
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// The Messages API path.
const MESSAGES_PATH: &str = "/v1/messages";

/// The API version header every request carries (§3.2).
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The beta that enables server-side refusal fallbacks (§3.2).
///
/// Paired with the scalar `fallbacks: "default"` form. The array form uses a
/// different date, and mixing the two is a 400, so the constant and the body
/// field below must move together.
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// The default model (§3.2, spec 014).
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// §3.2's timeouts.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The whole call, including streaming.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// The provider (§3.2).
pub struct AnthropicAssistant {
    client: reqwest::Client,
    base_url: String,
    model: String,
    key: std::sync::Arc<Secret>,
}

/// Written by hand, because `Secret` has no `Debug` and must not gain one
/// (FR-004). A derived `Debug` here would not compile, which is the guarantee
/// working: there is no route by which the key reaches a formatter.
impl std::fmt::Debug for AnthropicAssistant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicAssistant")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("key", &"[redacted]")
            // The HTTP client is omitted, which is what
            // `finish_non_exhaustive` announces: it prints its own
            // configuration, and none of that is useful here.
            .finish_non_exhaustive()
    }
}

impl AnthropicAssistant {
    /// Build a provider.
    ///
    /// # Errors
    ///
    /// [`InferenceError::Malformed`] if the HTTP client cannot be built, or
    /// [`InferenceError::Provider`] if `base_url` is not `https://`, which
    /// spec 015 §3.3 requires of any override.
    pub fn new(
        key: Secret,
        model: String,
        base_url: Option<String>,
    ) -> Result<Self, InferenceError> {
        let base_url = base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        if !base_url.starts_with("https://") {
            return Err(InferenceError::Provider {
                status: 0,
                kind: "endpoint-not-https".to_owned(),
                retryable: false,
            });
        }

        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            // Spec 015 §3.3: the endpoint set is explicit. A proxy read from
            // the environment would be an endpoint nobody chose and nothing
            // surfaced in the UI.
            .no_proxy()
            .build()
            .map_err(|e| InferenceError::Malformed(e.to_string()))?;

        Ok(Self {
            client,
            base_url,
            model,
            key: std::sync::Arc::new(key),
        })
    }

    /// Whether this provider is talking to somewhere other than the default
    /// host (spec 015 §3.3: surfaced in the UI, logged as an event kind).
    #[must_use]
    pub fn is_custom_endpoint(&self) -> bool {
        self.base_url != DEFAULT_BASE_URL
    }
}

/// The request body (§3.2).
///
/// FR-002 pins this byte for byte, and `deny_unknown_fields` is not enough:
/// the golden test asserts the serialized JSON, so a field added here without
/// a spec change fails the test rather than reaching the provider.
#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    stream: bool,
    system: &'a str,
    messages: Vec<Message<'a>>,
    output_config: OutputConfig,
    /// §3.2: on the default model, a safety-classifier refusal is re-routed
    /// server-side rather than surfacing as an empty answer. The scalar
    /// `"default"` form routes by category, so no model list is maintained
    /// here.
    #[serde(skip_serializing_if = "Option::is_none")]
    fallbacks: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct OutputConfig {
    effort: &'static str,
}

/// The effort string the API takes (§3.2).
const fn effort_str(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
    }
}

/// Assemble the JSON body for a request (§3.2).
///
/// Split out so FR-002's golden test can assert the bytes without a network.
#[must_use]
pub fn request_body(model: &str, request: &InferenceRequest, prompt: &Prompt) -> serde_json::Value {
    let mut messages = Vec::new();
    if let Some(prior) = &prompt.prior_answer {
        // A complete assistant turn, never a prefill: prefill is a 400 on
        // current models.
        messages.push(Message {
            role: "assistant",
            content: prior,
        });
    }
    messages.push(Message {
        role: "user",
        content: &prompt.user,
    });

    let body = MessagesRequest {
        model,
        max_tokens: request.max_output_tokens,
        stream: true,
        system: &prompt.system,
        messages,
        output_config: OutputConfig {
            effort: effort_str(request.effort),
        },
        fallbacks: (model == DEFAULT_MODEL).then_some("default"),
    };

    serde_json::to_value(&body).unwrap_or(serde_json::Value::Null)
}

/// Map one SSE frame onto a chunk, if it carries one (§3.2).
///
/// **Unknown event types are ignored, never errors.** The API adds event types
/// over time, and a client that failed on one it had not seen would break on
/// an ordinary provider release.
#[must_use]
pub fn map_frame(event: &str, data: &str) -> Option<Result<Chunk, InferenceError>> {
    let json: serde_json::Value = match serde_json::from_str(data) {
        Ok(json) => json,
        Err(e) => return Some(Err(InferenceError::Malformed(e.to_string()))),
    };

    match event {
        "content_block_delta" => {
            let delta = json.get("delta")?;
            if delta.get("type")?.as_str()? != "text_delta" {
                // A thinking delta, or a tool-input delta from some future
                // feature: not answer text, so not a chunk.
                return None;
            }
            Some(Ok(Chunk::Text(delta.get("text")?.as_str()?.to_owned())))
        }
        "message_delta" => {
            let stop = stop_reason(&json);
            let usage = Usage {
                input_tokens: 0,
                output_tokens: json
                    .get("usage")
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            };
            Some(Ok(Chunk::Done { stop, usage }))
        }
        "error" => {
            let kind = json
                .get("error")
                .and_then(|e| e.get("type"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            Some(Err(InferenceError::Provider {
                status: 200,
                kind,
                retryable: false,
            }))
        }
        // `message_start`, `content_block_start`, `content_block_stop`,
        // `message_stop`, `ping`, and anything added later.
        _ => None,
    }
}

/// Read the response body, mapping frames to chunks until it ends (§3.2).
///
/// Split out of `stream` so neither half is long enough to hide a branch.
async fn pump(
    response: reqwest::Response,
    tx: &tokio::sync::mpsc::Sender<Result<Chunk, InferenceError>>,
    cancel: &CancellationToken,
) {
    let mut parser = SseParser::new();
    let mut bytes = response.bytes_stream();

    loop {
        let next = tokio::select! {
            // FR-003: cancelling ends the stream and drops the connection.
            // Returning drops `bytes`, which is what closes it.
            () = cancel.cancelled() => return,
            next = bytes.next() => next,
        };

        let Some(part) = next else { break };
        let part = match part {
            Ok(part) => part,
            Err(e) => {
                let _ = tx
                    .send(Err(if e.is_timeout() {
                        InferenceError::Timeout
                    } else {
                        InferenceError::Network(e.to_string())
                    }))
                    .await;
                return;
            }
        };

        let Ok(text) = std::str::from_utf8(&part) else {
            let _ = tx
                .send(Err(InferenceError::Malformed(
                    "the stream carried invalid UTF-8".to_owned(),
                )))
                .await;
            return;
        };

        for frame in parser.push(text) {
            if let Some(chunk) = map_frame(&frame.event, &frame.data)
                && tx.send(chunk).await.is_err()
            {
                // The consumer dropped the stream: stop reading and let the
                // connection close.
                return;
            }
        }
    }

    if let Some(frame) = parser.finish()
        && let Some(chunk) = map_frame(&frame.event, &frame.data)
    {
        let _ = tx.send(chunk).await;
    }
}

/// Read the stop reason out of a `message_delta` (§3.2).
///
/// `stop_details` is populated **only** for a refusal and is null otherwise,
/// so it is read behind that check rather than unconditionally.
fn stop_reason(json: &serde_json::Value) -> StopReason {
    let reason = json
        .get("delta")
        .and_then(|d| d.get("stop_reason"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    match reason {
        "end_turn" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal {
            category: json
                .get("stop_details")
                .and_then(|d| d.get("category"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
        },
        other => StopReason::Other(other.to_owned()),
    }
}

/// Whether an HTTP status is worth retrying (§3.2).
#[must_use]
pub const fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 529) || status >= 500
}

impl Assistant for AnthropicAssistant {
    fn stream(
        &self,
        request: InferenceRequest,
        cancel: CancellationToken,
    ) -> futures_core::stream::BoxStream<'static, Result<Chunk, InferenceError>> {
        let prompt = build(&request, PromptConfig::default());
        let body = request_body(&self.model, &request, &prompt);
        let url = format!("{}{MESSAGES_PATH}", self.base_url);
        let client = self.client.clone();
        let key = std::sync::Arc::clone(&self.key);
        let with_fallbacks = self.model == DEFAULT_MODEL;

        // A channel plus a task, rather than a generator macro: the crate
        // needs one streaming function and a macro dependency for it would be
        // a dependency spec 015 has to think about.
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Chunk, InferenceError>>(16);

        tokio::spawn(async move {
            let mut builder = client
                .post(&url)
                .header("x-api-key", key.expose())
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .header("accept", "text/event-stream");
            if with_fallbacks {
                builder = builder.header("anthropic-beta", FALLBACK_BETA);
            }

            let response = tokio::select! {
                () = cancel.cancelled() => return,
                result = builder.json(&body).send() => match result {
                    Ok(response) => response,
                    Err(e) => {
                        let _ = tx
                            .send(Err(if e.is_timeout() {
                                InferenceError::Timeout
                            } else {
                                InferenceError::Network(e.to_string())
                            }))
                            .await;
                        return;
                    }
                },
            };

            let status = response.status().as_u16();
            if status == 401 {
                // §3.4 and FR-006: the runtime raises the prompt once per key,
                // not once per request; this reports the fact, and the caller
                // owns the "once" (D-4).
                let _ = tx.send(Err(InferenceError::SecretInvalid)).await;
                return;
            }
            if status >= 400 {
                let kind = response
                    .text()
                    .await
                    .ok()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
                    .and_then(|json| {
                        json.get("error")
                            .and_then(|e| e.get("type"))
                            .and_then(serde_json::Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or_else(|| "unknown".to_owned());
                let _ = tx
                    .send(Err(InferenceError::Provider {
                        status,
                        kind,
                        retryable: is_retryable_status(status),
                    }))
                    .await;
                return;
            }

            pump(response, &tx, &cancel).await;
        });

        let mut rx = rx;
        Box::pin(futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx)))
    }

    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANTHROPIC_VERSION, DEFAULT_BASE_URL, DEFAULT_MODEL, FALLBACK_BETA, is_retryable_status,
        map_frame,
    };
    use crate::assistant::{Chunk, InferenceError, StopReason};

    #[test]
    fn the_constants_are_the_ones_the_spec_names() {
        assert_eq!(DEFAULT_BASE_URL, "https://api.anthropic.com");
        assert_eq!(ANTHROPIC_VERSION, "2023-06-01");
        assert_eq!(DEFAULT_MODEL, "claude-opus-5");
        // §3.2: the scalar `fallbacks: "default"` form pairs with this date.
        // The array form uses a different one and mixing them is a 400.
        assert_eq!(FALLBACK_BETA, "server-side-fallback-2026-07-01");
    }

    /// §3.2: 429, 529 and 5xx are retried; every other 4xx is terminal.
    #[test]
    fn only_the_documented_statuses_are_retryable() {
        for status in [429, 529, 500, 502, 503] {
            assert!(is_retryable_status(status), "{status} should be retryable");
        }
        for status in [400, 401, 403, 404, 413, 422] {
            assert!(!is_retryable_status(status), "{status} is terminal");
        }
    }

    #[test]
    fn a_text_delta_becomes_a_chunk() {
        let chunk = map_frame(
            "content_block_delta",
            r#"{"delta":{"type":"text_delta","text":"Hello"}}"#,
        );
        assert_eq!(chunk, Some(Ok(Chunk::Text("Hello".to_owned()))));
    }

    /// A thinking delta is not answer text, so it is not a chunk.
    #[test]
    fn a_non_text_delta_is_not_a_chunk() {
        assert_eq!(
            map_frame(
                "content_block_delta",
                r#"{"delta":{"type":"thinking_delta","thinking":"..."}}"#
            ),
            None
        );
    }

    /// §3.2: unknown event types are ignored, never errors. A client that
    /// failed on one it had not seen would break on a provider release.
    #[test]
    fn an_unknown_event_is_ignored() {
        assert_eq!(
            map_frame("message_start", r#"{"type":"message_start"}"#),
            None
        );
        assert_eq!(map_frame("ping", "{}"), None);
        assert_eq!(
            map_frame("some_event_from_2027", r#"{"type":"whatever"}"#),
            None
        );
    }

    #[test]
    fn a_message_delta_carries_the_stop_reason_and_usage() {
        let chunk = map_frame(
            "message_delta",
            r#"{"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":12}}"#,
        );
        assert_eq!(
            chunk,
            Some(Ok(Chunk::Done {
                stop: StopReason::EndTurn,
                usage: crate::assistant::Usage {
                    input_tokens: 0,
                    output_tokens: 12
                }
            }))
        );
    }

    /// A refusal carries its category, and `stop_details` is read only here
    /// because the API populates it only for a refusal.
    #[test]
    fn a_refusal_carries_its_category() {
        let chunk = map_frame(
            "message_delta",
            r#"{"delta":{"stop_reason":"refusal"},"stop_details":{"type":"refusal","category":"cyber"},"usage":{"output_tokens":3}}"#,
        );
        assert_eq!(
            chunk,
            Some(Ok(Chunk::Done {
                stop: StopReason::Refusal {
                    category: Some("cyber".to_owned())
                },
                usage: crate::assistant::Usage {
                    input_tokens: 0,
                    output_tokens: 3
                }
            }))
        );
    }

    #[test]
    fn a_refusal_without_a_category_still_parses() {
        let chunk = map_frame(
            "message_delta",
            r#"{"delta":{"stop_reason":"refusal"},"usage":{"output_tokens":0}}"#,
        );
        assert_eq!(
            chunk,
            Some(Ok(Chunk::Done {
                stop: StopReason::Refusal { category: None },
                usage: crate::assistant::Usage::default()
            }))
        );
    }

    #[test]
    fn an_error_event_becomes_a_terminal_provider_error() {
        let chunk = map_frame("error", r#"{"error":{"type":"overloaded_error"}}"#);
        assert_eq!(
            chunk,
            Some(Err(InferenceError::Provider {
                status: 200,
                kind: "overloaded_error".to_owned(),
                retryable: false
            }))
        );
    }

    #[test]
    fn malformed_json_is_reported_rather_than_panicking() {
        let chunk = map_frame("message_delta", "not json");
        assert!(matches!(chunk, Some(Err(InferenceError::Malformed(_)))));
    }
}
