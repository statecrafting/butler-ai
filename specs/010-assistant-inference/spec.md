---
id: "010-assistant-inference"
title: "Assistant inference: a provider-agnostic streaming client, the Claude reference provider, prompt contract, secrets, and spend guard"
status: approved
kind: "feature"
domain: "assistant"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: high
platforms: "all"
phase: 4
depends_on:
  # Phase 4 entry (018 R-002, R-007). 005 and 007 are the leaves of phase 3
  # and transitively require 004 and 006.
  #
  # 015 is deliberately absent (018 R-008, D-2): it constrains this spec's
  # `anthropic.rs` and `secrets.rs`. The compile-time dependency on
  # `RedactedText` is real but is enforced by the compiler, not the plan;
  # 015's redaction module landed in phase 1 and is not what this waits on.
  - "005-capture-exclusion"
  - "007-text-recognition"
  - "009-pipeline-state-machine"
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
extends:
  # `crates/*` already globs this crate into the workspace, but the provider
  # client's dependencies (reqwest, keyring, zeroize) are pinned once in spec
  # 001's root manifest (001 FR-004) before this crate's manifest references
  # them.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
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

## 7. Resolved decisions

- **D-1 (2026-09-02).** `depends_on` gained the phase 3 leaves (005, 007) as
  the 018 R-002 gate. This crate names no capture or OCR type; the edges make
  the phase order mechanical for an orchestrator that schedules on
  `depends_on` alone.

- **D-2 (2026-09-07, the wire contract was checked, not recalled).** §3.2
  describes the Messages API precisely, and it was verified against the
  current API reference rather than written from memory. It is accurate: the
  scalar `fallbacks: "default"` form does pair with
  `anthropic-beta: server-side-fallback-2026-07-01` (the array form uses a
  different date, and mixing them is a 400), depth is `output_config.effort`,
  and thinking is left adaptive by omitting the parameter.

  Three things are **hard failures** on the models §3.2 names, and none is
  sent: `temperature` / `top_p` / `top_k`, `thinking.budget_tokens`, and an
  assistant prefill. FR-002's golden asserts the body's whole key set, so any
  of them appearing later fails a test rather than a request.

- **D-3 (2026-09-07, fixtures are the contract).** Rust has no official
  Anthropic SDK, so nothing pins the shape of the stream except the fixtures
  under `tests/fixtures/`. That is the arrangement working rather than a
  workaround: a fixture is a contract a reviewer can read, and it fails loudly
  when the wire changes instead of silently when a dependency updates.

  `stream_unknown_events.sse` is the one worth keeping deliberately. It carries
  an event type from the future and a thinking delta, and asserts the text
  around them still arrives: §3.2 requires unknown events to be **ignored,
  never errors**, and without that fixture the first ordinary provider release
  would break the product.

- **D-4 (2026-09-07, "once per key" without keeping anything derived from the
  key).** FR-006 wants a rejected key reported once per key, not per request:
  a user whose key expired should be asked once, not on every capture cycle.

  The identity of "the key" is a **generation counter** bumped when the stored
  key changes, not a hash of the key. A hash of a secret is still a function of
  a secret, and there is no reason to keep one when a counter answers the same
  question. `CredentialAlarm` is where that lives; the provider reports
  `SecretInvalid` every time, and the caller owns the "once".

- **D-5 (2026-09-07, three bugs the tests found, all in windows).** The spend
  guard's rolling windows were written against a cutoff computed with
  `saturating_sub`, and near tick zero that cutoff collapses to 0, so an entry
  at tick 0 failed a `tick > cutoff` test even though no time had passed.
  Every rate limit therefore failed to bind on a freshly started process,
  which is exactly when a runaway loop is most likely. Three tests caught it;
  the comparison is now on the distance rather than a cutoff.

  A second bug in the same file: an `admitted.len() >= MAX_REQUESTS_PER_MINUTE`
  fast path, which is only equivalent to the real check while the deque holds
  a minute's worth. It holds a day's, so the seventh request of the *hour* was
  refused as if it were the seventh of the minute.

  A third in the SSE parser: `finish` drained the buffer, discarded the frame
  that draining dispatched, and then dispatched again on cleared state,
  returning nothing. A server that closes without a final blank line would
  have lost the last chunk of every answer.

- **D-6 (2026-09-07, the request rates are constants, not settings).** §3.5
  says `max_requests_per_minute`, `max_requests_per_hour` and
  `max_output_tokens_per_day` come "from settings". Spec 014's
  `BudgetSettings` carries what a user tunes, which is money and input size:
  `daily_usd`, `monthly_usd`, `max_input_tokens`. It has no request-rate
  fields, and adding three would be an edit to 014's model rather than this
  spec's to make.

  They are constants here, at §3.5's documented defaults, with the reason
  stated in the module: these are the limits that stop a runaway loop from
  spending a user's cap in a minute, and they are the same for everyone. The
  money caps *are* read from settings. **Owed to spec 014** if the rates should
  become user-tunable, which is a settings-model change and a UI addition.

- **D-7 (2026-09-07, this crate cannot be cross-checked for Windows).**
  Specs 005 and 007 validated their Windows code from macOS, either directly
  (`cargo check --target x86_64-pc-windows-msvc`) or in a scratch crate. Not
  here: `rustls` pulls `aws-lc-sys`, whose build script needs a C toolchain
  this host does not have for that target.

  Almost none of the crate is platform-specific: the SSE parser, the prompt
  contract, the spend guard and the frame mapper are pure and are tested on
  every target. The platform surface is `keyring`, which selects the OS
  credential store. CI's `windows-latest` job builds and clippy-checks it on
  every pull request, which is where that is caught.

- **D-8 (2026-09-07, what the runtime still has to wire).** This crate
  implements the trait, the provider, the prompt, the keychain and the guard.
  It is not yet *called*: §3.5 says the runtime feeds the guard and turns a
  denial into an idle cycle with a once-per-hour notice, and that runs through
  spec 019's `Ports` adapter, which 019 D-2 leaves owed.

  **Owed with that adapter**: feeding `SpendGuard` from the runtime's tick,
  raising `NeedsCredential` on `NoCredential`, and the once-per-hour
  `budget.exhausted` event. The pieces each have their own tests; what is
  missing is the wiring, and spec 013 is the next spec to touch it.

## 8. Verification

```verify:cli
# AC-1: the whole crate, with no network. The mock server FR-003 uses is a
# local socket the test starts and aborts.
cargo test -p butler-llm --locked
# FR-004: `Secret` cannot be formatted or cloned. The doctests carry a
# positive control, so a passing compile_fail block cannot be passing because
# the import path is wrong.
cargo test -p butler-llm --locked --doc
# AC-2: exactly one host, and it is a constant. Matching code lines only: the
# module's own prose explains the rule and would otherwise fail this check,
# which is the trap spec 016 D-2 names.
sh -c 'test "$(grep -rn "api\.anthropic\.com" crates/butler-llm/src | grep -v "^[^:]*:[0-9]*://" | grep -v "//!" | wc -l | tr -d " ")" -eq 2'
# §3.2 and spec 015 §3.3: an endpoint override must be https, and the client
# cannot read a proxy from the environment. `system-proxy` is not merely
# unused; without the feature reqwest has no such code at all.
grep -q 'starts_with("https://")' crates/butler-llm/src/anthropic.rs
sh -c 'grep "^reqwest = " Cargo.toml | grep -qv "system-proxy"'
# §3.2: the three parameters that are 400s on the models this spec names are
# not sent, and FR-002's golden pins the whole key set.
sh -c '! grep -qE "\"(temperature|top_p|top_k|budget_tokens)\"" crates/butler-llm/src/anthropic.rs'
test -f crates/butler-llm/tests/fixtures/prompts/request_body.json
sh -c '! grep -qE "\"(temperature|top_p|top_k|thinking)\"" crates/butler-llm/tests/fixtures/prompts/request_body.json'
# §3.2: the scalar fallbacks form and its beta header move together. Pairing
# either with the other form is a 400.
grep -q 'server-side-fallback-2026-07-01' crates/butler-llm/src/anthropic.rs
# AC-3: the prompt is a reviewed fixture, and a prompt change is a spec change.
test -f crates/butler-llm/tests/fixtures/prompts/system.txt
sh -c 'grep -q "UNTRUSTED DATA" crates/butler-llm/tests/fixtures/prompts/system.txt'
# §3.4 and spec 015: the key is never written anywhere but the keychain.
sh -c '! grep -rq "impl std::fmt::Debug for Secret\|impl Display for Secret" crates/butler-llm/src'
grep -q "fn drop" crates/butler-llm/src/secrets.rs
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "010-assistant-inference" && exit 1 || exit 0'
```
