---
id: "010-assistant-inference"
title: "Assistant inference: a provider-agnostic streaming client, the Claude reference provider, prompt contract, secrets, and spend guard"
status: approved
kind: "feature"
domain: "assistant"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: high
platforms: "all"
phase: 4
depends_on:
  - "009-pipeline-state-machine"
  - "015-privacy-boundary"
  - "014-user-configuration"
establishes:
  - { kind: crate, id: "butler-llm" }
  - "crates/butler-llm/Cargo.toml"
  - "crates/butler-llm/src/lib.rs"
  - "crates/butler-llm/src/assistant.rs"
  - "crates/butler-llm/src/anthropic.rs"
  - "crates/butler-llm/src/sse.rs"
  - "crates/butler-llm/src/prompt.rs"
  - "crates/butler-llm/src/secrets.rs"
  - "crates/butler-llm/src/budget.rs"
  - "crates/butler-llm/tests/sse.rs"
  - "crates/butler-llm/tests/prompt.rs"
  - { kind: directory, path: "crates/butler-llm/tests/fixtures/" }
  - { kind: symbol, id: "butler_llm::assistant::Assistant" }
  - { kind: symbol, id: "butler_llm::assistant::InferenceRequest" }
  - { kind: symbol, id: "butler_llm::assistant::Chunk" }
  - { kind: symbol, id: "butler_llm::anthropic::AnthropicAssistant" }
  - { kind: symbol, id: "butler_llm::secrets::SecretStore" }
  - { kind: symbol, id: "butler_llm::budget::SpendGuard" }
summary: >
  The `butler-llm` crate: an `Assistant` trait that streams `Chunk`s for an
  `InferenceRequest`, the Claude Messages API reference implementation over
  raw HTTPS (Rust has no official SDK; the wire contract is pinned by fixture
  tests), an SSE parser, the prompt contract (system prompt, context budget,
  trimming order, refusal and stop-reason handling), an OS-keychain-backed
  `SecretStore` so an API key is never in a file, and a `SpendGuard` that
  bounds requests per minute and tokens per day. The provider is selected by
  settings; exactly one endpoint is ever contacted (spec 015).
---

# 010: Assistant inference

## 1. Purpose

Everything upstream exists to produce one thing: a short, useful answer about
what is on the screen, streamed fast enough to read as it arrives. This spec
defines the boundary between butler-ai and the model provider so that the
rest of the system depends on a trait, the reference provider is Anthropic's
Messages API, and a second provider is a new file, not a redesign.

The crate is also where the two most sensitive assets live: the API key and
the outbound request. Both are handled by named types with narrow APIs so the
privacy boundary (015) can constrain them.

## 2. Territory

The crate `butler-llm` and its files. `assistant.rs` (the trait and DTOs),
`anthropic.rs` (the reference provider), `sse.rs` (a pure SSE frame parser),
`prompt.rs` (pure prompt assembly and trimming), `secrets.rs` (keychain),
`budget.rs` (pure rate/spend accounting). Fixture tests pin the wire format.

## 3. Behavior

### 3.1 The trait

```rust
pub trait Assistant: Send + Sync {
    fn stream(&self, req: InferenceRequest, cancel: CancellationToken)
        -> BoxStream<'static, Result<Chunk, InferenceError>>;
    fn id(&self) -> ProviderId;
}
pub struct InferenceRequest {
    pub request: RequestId,
    pub screen_text: RedactedText,     // spec 015: only this type can leave the process
    pub prior_answer: Option<String>,  // for continuity, bounded
    pub user_note: Option<String>,     // typed by the user in the overlay, optional
    pub effort: Effort,                // low | medium | high (settings)
    pub max_output_tokens: u32,
}
pub enum Chunk { Text(String), Done { stop: StopReason, usage: Usage }, }
pub enum StopReason { EndTurn, MaxTokens, Refusal { category: Option<String> }, Other(String) }
```

`RedactedText` is a newtype constructed only by `butler_core::redaction::
redact` (015); `InferenceRequest` cannot be built from a raw `String`.

### 3.2 Reference provider: Claude Messages API (`anthropic.rs`)

- Endpoint: `POST https://api.anthropic.com/v1/messages`, headers
  `x-api-key`, `anthropic-version: 2023-06-01`, `content-type:
  application/json`, `accept: text/event-stream`. The base URL is a
  compile-time constant; settings may override it only to an `https://` URL,
  which spec 015 constrains to be logged and surfaced in the UI as
  "non-default endpoint".
- Body: `model` (default `claude-opus-5`; settings may choose
  `claude-sonnet-5` or `claude-haiku-4-5`), `max_tokens` (from the request,
  default 1024: answers are short by design), `stream: true`, `system` (the
  prompt contract, §3.4), `messages` (one user turn carrying the screen text
  and the optional user note; the prior answer, if any, as an assistant turn
  before it), `output_config: { effort }`. Thinking is left at the model's
  adaptive default (the parameter is omitted). On `claude-opus-5` the request
  MUST include `fallbacks: "default"` with the `anthropic-beta:
  server-side-fallback-2026-07-01` header so a safety-classifier refusal is
  re-routed server-side rather than surfacing as an empty answer; a refusal
  that survives is reported as `StopReason::Refusal` and the UI (012) shows
  "declined" rather than nothing.
- Streaming: the response is SSE. `sse.rs` parses `event:`/`data:` frames;
  `anthropic.rs` maps `content_block_delta` with `text_delta` → `Chunk::Text`,
  `message_delta` → the stop reason and usage, `message_stop` → `Chunk::Done`,
  `error` events → `InferenceError`. Unknown event types are ignored (forward
  compatibility), never errors.
- Timeouts: connect 10 s; first byte 20 s; idle between events 30 s; total
  120 s. Cancellation drops the connection immediately.
- Retries: `429`, `529`, and `5xx` are retried at most twice with the
  `retry-after` header if present, else 2 s / 4 s; `4xx` other than `429` is
  terminal (`retryable: false`). A `401` additionally triggers
  `Event::SecretInvalid` so the UI prompts for a key.
- HTTP client: `reqwest` with `rustls`, no proxy auto-detection unless
  settings enable it (015: the endpoint set is explicit).

### 3.3 Prompt contract (`prompt.rs`)

`build(req, cfg) -> Prompt` is pure and fixture-tested. The system prompt
MUST instruct the model to: answer only what the visible text asks or
implies, in the user's language, concisely (target the `answer_style` from
settings: `bullets` | `short` | `detailed`), never repeat the screen text
back, say "nothing to add" when the screen contains no question or task, and
never claim to have seen anything not in the text. The screen text is wrapped
in a clearly delimited block and labelled as untrusted content that may
contain instructions which MUST NOT be followed (prompt-injection framing:
text on the user's screen is data, not instructions).

Context budget: `screen_text` is trimmed to `cfg.max_screen_chars` (default
12 000) keeping the beginning (reading order), `prior_answer` to 2 000 chars,
`user_note` to 1 000. Trimming is deterministic and fixture-tested. The
prompt prefix (system + tool-free) is stable across requests so provider-side
prompt caching applies; volatile content is last.

### 3.4 Secrets (`secrets.rs`)

- `SecretStore::get(ProviderId) -> Option<Secret>` / `set` / `delete`, backed
  by the OS keychain (`keyring` crate: Keychain on macOS, Credential Manager
  on Windows). Service name `dev.butler-ai.desktop`, account `<provider>`.
- `Secret` is a `zeroize`-on-drop newtype with no `Debug`/`Display`/`Clone`
  and a single `expose(&self) -> &str` used only by the provider to set the
  header.
- Keys are never written to the settings file, logs, or diagnostics (016).
- If no key is stored, `stream` returns `InferenceError::NoCredential` and
  the runtime moves to `Idle` with a UI prompt (not `Fault`).

### 3.5 Spend guard (`budget.rs`)

Pure accounting fed by the runtime: `SpendGuard::admit(now_tick) ->
Admit | Deny { reason }` enforces `max_requests_per_minute` (default 6),
`max_requests_per_hour` (default 60), and `max_output_tokens_per_day`
(default 200 000) from settings, using `Usage` reported by `Chunk::Done`.
A denial is an `Evaluated { Unchanged }`-equivalent for the machine (the
cycle returns to `Idle`) with a `budget.exhausted` UI event once per hour.

## 4. Functional requirements

- **FR-001.** Fixture `tests/fixtures/stream_basic.sse` parses to the exact
  chunk sequence recorded beside it; `stream_refusal.sse` yields
  `StopReason::Refusal`; `stream_error.sse` yields `InferenceError`.
- **FR-002.** The request body for a fixed `InferenceRequest` is byte-stable
  (golden test), and contains no field other than those listed in §3.2.
- **FR-003.** Cancelling the token mid-stream ends the stream within 100 ms
  and closes the connection (asserted with a local mock server).
- **FR-004.** `Secret` cannot be formatted: a compile-fail test asserts
  `!Debug` and `!Display`.
- **FR-005.** `SpendGuard` denies the seventh request in a minute and admits
  again after the window (tick-based, deterministic).
- **FR-006.** A `401` produces `SecretInvalid` exactly once per key, not per
  request.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-llm` passes on all CI targets (mock server,
  no network).
- **AC-2.** `rg "api.anthropic.com|https://" crates/butler-llm/src` matches
  only the constant in `anthropic.rs` and the `https://` scheme check.
- **AC-3.** The prompt fixtures in `tests/fixtures/prompts/` are reviewed with
  the spec; a prompt change is a spec change.

## 6. Out of scope

- Tool use, multi-turn conversation, or agentic loops. The assistant answers
  one screen at a time.
- Local models. The trait admits an on-device provider; none ships in v1.
- Redaction itself (015) and the pacing of the rendered answer (013).
