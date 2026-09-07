---
id: "015-privacy-boundary"
title: "Privacy boundary: what may leave the process, what may touch disk, and what may be logged"
status: approved
kind: "constraint"
domain: "platform"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: critical
platforms: "all"
phase: 1
depends_on:
  - "009-pipeline-state-machine"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/redaction.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/redaction.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::redaction" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::redaction::redact" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::redaction::RedactedText" }, nature: additive }
  # Host surface: `redaction` is only reachable once butler-core's `lib.rs`
  # declares it. The module is a pure pass (section 3), so it adds no
  # dependency and touches no manifest.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
constrains:
  - flavor: invariant-freeze
    unit: "crates/butler-capture/src/frame.rs"
    note: "Frame: no Serialize, no Clone, private pixels, zeroize on drop, monotonic timestamps only."
  - flavor: invariant-freeze
    unit: "crates/butler-ocr/src/recognized.rs"
    note: "Recognized: no Serialize; leaves the process only as RedactedText inside an InferenceRequest."
  - flavor: invariant-freeze
    unit: "crates/butler-llm/src/anthropic.rs"
    note: "Exactly one outbound host per provider; https only; the endpoint override is surfaced in the UI and logged as an event kind."
  - flavor: invariant-freeze
    unit: "crates/butler-llm/src/secrets.rs"
    note: "Secrets live in the OS keychain only; Secret has no Debug/Display/Clone."
  - flavor: invariant-freeze
    unit: "apps/desktop/src-tauri/src/logging.rs"
    note: "Logs carry ids, kinds and counts, never screen text, prompts, answers, or secrets."
  - flavor: invariant-freeze
    unit: "apps/desktop/src-tauri/tauri.conf.json"
    note: "CSP as in spec 004 §3.2; no remote content; no devtools in release."
  - flavor: invariant-freeze
    unit: { kind: directory, path: "apps/desktop/src-tauri/capabilities/" }
    note: "No fs, shell, http, or dialog grants without a spec that amends this one."
references:
  - { unit: { kind: file, path: "docs/threat-model.md" }, role: "threat model" }
summary: >
  The cross-cutting invariants that make constitution §V enforceable, asserted
  as `constrains` edges over the exact units that could violate them, plus the
  one piece of code the boundary needs: `redact`, a pure pass that removes
  secret-shaped content (API keys, tokens, card and account numbers, private
  key blocks, e-mail addresses and phone numbers when the user opts in) from
  recognized text before it can become an `InferenceRequest`. Frames and
  recognized text never touch disk; one endpoint per provider; secrets in the
  keychain; logs carry no content; the webview has no network or filesystem.
  An exception to any of these is an amendment to this spec, never a feature's
  side effect.
---

# 015: Privacy boundary

## 1. Purpose

butler-ai continuously reads whatever is on the user's screen: their mail,
their chats, their bank, their employer's documents. The single most important
property of the system is that this data goes exactly where the user
configured and nowhere else, and that nothing about it persists beyond the
moment it is useful. A property like that cannot live in one crate; it is a
set of invariants on the types and files that handle the data at each hop.
spec-spine's `constrains` edge exists for exactly this: this spec is an owner
of those units alongside their feature specs, so a change to any of them is a
change this spec reviews.

## 2. Territory

- The `redaction` module in `butler-core` (established here, added to 009's
  crate): the only constructor of `RedactedText`.
- Constraints over: `Frame` (006), `Recognized` (007), the provider client and
  the secret store (010), the logger (016), and the Tauri security surface
  (004). Each constraint's note is normative for that unit.

## 3. Behavior

### 3.1 Data classes and their rules

| Class | Type | Memory | Disk | Network | Logs |
|---|---|---|---|---|---|
| Pixels | `Frame`, `FrameView` | until `ReleaseFrame`, then zeroed | never | never | never (not even dimensions with a timestamp) |
| Recognized text | `Recognized` | until the next commit or disarm | never | never as-is | never |
| Redacted text | `RedactedText` | inside one `InferenceRequest` | never | to the configured provider only | never |
| Answer | `String` in the pacer and UI | until dismiss/disarm | never (v1) | never (except as `prior_answer` to the same provider) | never |
| Secret | `Secret` | during header construction | keychain only | as a header to the provider only | never |
| Settings | `Settings` | always | config dir, `0600` | never | field names only |
| Diagnostics | `DiagnosticsBundle` (016) | on demand | user-chosen path, user-initiated | never | n/a |

"Never" is enforced where possible by types (`!Serialize`, private fields,
newtypes, compile-fail tests) and elsewhere by tests named in the constrained
units' specs.

### 3.2 Redaction (`redact(text: &str, policy: &RedactionPolicy) -> RedactedText`)

Pure, deterministic, fixture-tested. With `policy.enabled` (default true):

- Always: strings matching high-confidence secret shapes are replaced with
  `[REDACTED:<kind>]`: `sk-ant-…`, `sk-…` (≥ 32 chars), `ghp_`/`gho_`/`ghs_`,
  `xox[abp]-`, `AKIA[0-9A-Z]{16}`, `AIza…`, JWTs (`eyJ…\.eyJ…`), PEM blocks
  (`-----BEGIN … PRIVATE KEY-----` through `END`), `Bearer <token>`, and
  16-digit sequences passing Luhn (card numbers).
- Opt-in (`policy.pii`): e-mail addresses, E.164-looking phone numbers, IBANs.
- The output keeps line structure so the assistant still sees layout.
- `RedactedText` exposes `as_str()` and `redaction_count()`; it has no `From
  <String>`.

With `policy.enabled = false` (an explicit user choice shown in settings with
a warning), `redact` is the identity but still the only constructor.

### 3.3 Network

- The process contacts one host per configured provider (010). Any other
  outbound connection (update checks, crash reporting, telemetry) requires a
  spec that amends this one and a settings toggle that defaults to off.
- The webview (012) has no network: the CSP's `connect-src` is the IPC
  scheme only, and the frontend build contains no URLs (012 FR-005).
- `endpoint_override` (014) is allowed (self-hosted gateways are legitimate)
  but MUST be `https://`, MUST be displayed in the status strip as
  "custom endpoint", and MUST be logged as an event kind at startup.

### 3.4 Disk

- Frames, recognized text, prompts and answers are never written. There is
  no history feature in v1; if one is added, it is an amendment here with
  encryption at rest and an explicit opt-in.
- The settings file contains no secrets (014, 010).
- Crash dumps: the app does not install a crash handler that writes memory;
  the OS's own crash reporting is the user's choice.

### 3.5 Logs and diagnostics

Logs (016) carry: timestamps, state and event names, `seq`/`request` ids,
error kinds, durations, counts (chars recognized, redactions applied, tokens
used). They never carry text from any data class above. The diagnostics
bundle is assembled from the same log and the settings (minus nothing, since
there is nothing secret in them) and is written only where the user chooses.

## 4. Functional requirements

- **FR-001.** Each fixture in `tests/fixtures/redaction/` (synthetic secrets
  of every kind in §3.2, embedded in lorem text) produces the expected output
  with the expected count.
- **FR-002.** `RedactedText` cannot be constructed outside
  `butler_core::redaction` (compile-fail test).
- **FR-003.** `InferenceRequest` cannot be constructed with a `String`
  screen text (compile-fail test in 010).
- **FR-004.** A whole-pipeline test with mock traits and a capturing log
  subscriber asserts that no log line contains any substring of the mock
  screen text, the mock answer, or the mock secret.
- **FR-005.** A test runs the app under a denying egress proxy with the
  provider mocked locally and asserts exactly one distinct destination host.

## 5. Acceptance criteria

- **AC-1.** `spec-spine index render` shows this spec as an owner of all
  seven constrained units once they exist.
- **AC-2.** `docs/threat-model.md` §Data handling is consistent with §3.1
  (reviewed together; the table is copied verbatim).
- **AC-3.** The redaction fixtures contain only synthetic values (generated
  by `tests/fixtures/redaction/gen.rs`, committed).

## 6. Out of scope

- Defending against a compromised machine (a keylogger or screen recorder
  already running with the user's privileges sees the same screen).
- The provider's own data handling; the user chooses the provider and the
  README links to its policy.
