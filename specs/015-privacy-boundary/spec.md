---
id: "015-privacy-boundary"
title: "Privacy boundary: what may leave the process, what may touch disk, and what may be logged"
status: approved
kind: "constraint"
domain: "platform"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: critical
platforms: "all"
phase: 1
depends_on:
  - "009-pipeline-state-machine"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/redaction.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/redaction.rs", nature: additive }
  # AC-3's synthetic fixture generator. Declared here because AC-3 requires the
  # file to exist and no spec claimed it; `require_ownership` refuses an
  # unclaimed source file inside a discovered package.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/fixtures/redaction/gen.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::redaction" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::redaction::redact" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::redaction::RedactedText" }, nature: additive }
  # Host surface: `redaction` is only reachable once butler-core's `lib.rs`
  # declares it. The module is a pure pass (section 3), so it adds no
  # dependency and touches no manifest.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
  # D-4: the three requirements that waited for later phases. FR-003 is a
  # compile-fail test on the request type (spec 010's), FR-005 is an egress
  # test beside it, and FR-004 is a whole-pipeline test that needs the runtime
  # (spec 019's) to drive mock ports.
  - { spec: "010-assistant-inference", unit: "crates/butler-llm/src/assistant.rs", nature: additive }
  - { spec: "010-assistant-inference", unit: "crates/butler-llm/tests/egress.rs", nature: additive }
  - { spec: "019-runtime-host", unit: "apps/desktop/src-tauri/src/runtime.rs", nature: additive }
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

## 7. Resolved decisions

- **D-1 (2026-09-06, phase 1 partial; this spec stays `in-progress`).** The
  redaction half of this spec is built: `redaction.rs`, `tests/redaction.rs`,
  the fixture generator, and the `redact` / `RedactedText` / `SecretKind` /
  `RedactionPolicy` surface. FR-001, FR-002, AC-2 and AC-3 hold and are
  checked by §8.

  **This spec cannot reach zero `W-001` in phase 1, so it does not flip to
  `complete`.** Seven of its twelve owned units are the `constrains`
  invariant-freeze edges, and each names a file a later phase creates:
  `tauri.conf.json` and `capabilities/` (004, phase 2), `logging.rs` (016,
  phase 2), `frame.rs` (006, phase 3), `recognized.rs` (007, phase 3),
  `anthropic.rs` and `secrets.rs` (010, phase 4). `constrains` is an owning
  edge, so the indexer counts them; AC-1 says so outright ("once they exist").

  This makes spec 018's phase 1 exit criterion ("`make burndown` shows zero for
  these four") unsatisfiable for this spec as written. That is a real
  contradiction in the plan, not a defect here, and it is the same shape as the
  one 009 D-1 records, where spec 019 was split out because a phase 1 spec
  cannot own units a phase 2 crate contains. It is left for a human: the
  options are to amend 018's phase 1 exit criterion to except a constraint
  spec's forward edges, to split the forward `constrains` edges into a spec
  that phases with them, or to accept that this spec is legitimately
  `in-progress` until phase 4 completes. FR-003 (010), FR-004 (whole pipeline)
  and FR-005 (egress proxy) likewise cannot be discharged before those phases.

  **What remains, precisely:** the seven `constrains` units and FR-003 to
  FR-005. Nothing in the redaction module is outstanding.

- **D-2 (2026-09-06, a real bug the property test caught).** `match_bearer`
  sliced `s[..7]` to compare the scheme, which panics when byte 7 falls inside
  a multi-byte character, and counted the token's length in bytes, which can
  return an offset that is not a character boundary. Arbitrary screen text is
  exactly where that input comes from. Both are fixed by `str::get` and by
  summing `char::len_utf8`. The other matchers were already safe because they
  bound their scans to ASCII bytes, which are whole characters. Recorded
  because it is the argument for the property tests being in §8 rather than a
  reviewer's judgement.

- **D-3 (2026-09-06, D-1 resolved).** The maintainer chose to amend spec 018.
  It gained **R-009**, which says a constraint spec whose `constrains` edges
  name units later phases create is `in-progress` for the duration by design
  and gates nothing, and reads R-004's "zero unresolved units" against its own
  `establishes`/`extends` units rather than its forward edges. Phase 1's exit
  criterion now matches.

  Amending the prose alone would have changed nothing, because `registry plan`
  schedules on `depends_on`, and four specs named this one as a phase gate:
  004, 010, 016 and 017. That is exactly the set that owns a unit this spec
  constrains, so each pair was a cycle. 018 **R-008** now forbids the shape and
  those four edges are gone; 004 became schedulable immediately
  (`ready: 1, blocked: 12` to `ready: 2, blocked: 11`).

  This spec therefore stays `in-progress` until phase 4, correctly and by rule
  rather than as an unresolved question. Its authority over the seven units is
  undiminished: it is carried by the `constrains` edges, which point the right
  way and are what the coupling gate reads. What remains here is unchanged
  from D-1: the seven forward units and FR-003 to FR-005.

- **D-4 (2026-09-07, the three requirements that waited, and this spec
  closes).** D-1 and D-3 left exactly two things outstanding: the seven
  forward `constrains` units, and FR-003 to FR-005. Phases 2 to 4 have landed,
  so all seven units exist and `make burndown` shows zero for this spec. The
  three requirements are now built, each with a control that would catch it
  going quiet.

  **FR-003** is a `compile_fail` doctest on `InferenceRequest`, showing that a
  `String` screen does not typecheck where a `RedactedText` is required, with
  a positive control beside it. The control earned its place immediately: the
  first version of the `compile_fail` block passed because `Effort` and
  `AnswerStyle` were imported from the wrong module, so it was failing to
  compile for a reason that had nothing to do with the property. A
  `compile_fail` block with no positive control is a test that passes when the
  code around it rots.

  **FR-004** is a whole-pipeline test: mock `Ports` that hold the screen text,
  an answer and a credential for the length of the run, driven through the
  real `Runtime` and the real reducer, with the process-wide capturing
  subscriber. The mock logs the way a careless call site would (`?event` on
  the chunk it releases), because a mock that logged carefully would prove
  only that the mock is careful. Two negative controls were run while writing
  it: leaking the screen text from `recognize` fails on `vorplex`, and
  un-redacting `UiEvent::AnswerChunk`'s `Debug` fails on `thrandible`. The
  second is the interesting one, because it shows what is actually holding
  that path: spec 013 D-2's hand-written `Debug`, not any discipline at the
  call site.

  This is the half spec 016 D-3 owed to phase 4. What that decision could
  build then covered `logging.rs`'s own calls; what it could not check was
  whether some *other* module logs something it should not. Now it does.

  **FR-005** is measured on two loopback listeners rather than against a
  network: one stands in for the provider, the other is the "denying egress
  proxy", and the test asserts which one the client opened a socket to. The
  TLS handshake fails, which does not matter: the question is which host was
  contacted, and that is answered before the handshake starts. Nothing reaches
  the internet, so the result is the same on a laptop and on a runner with no
  egress. Removing `.no_proxy()` from the client makes it fail.

  The proxy environment is set on a **child** invocation of the test binary.
  `std::env::set_var` is `unsafe` in edition 2024 and spec 001 forbids
  `unsafe` in `butler-llm` outright, and a process-wide mutation would race
  every other test in the binary in any case.

  **One defect the work surfaced**, in this repository rather than in the
  spec: the process-wide trace capture is shared, `cargo test` runs tests in
  parallel, and FR-004's mock logging landed inside spec 019 FR-006's window,
  breaking a test that had nothing to do with this change. The readers are
  serialised on a mutex now. Making the capture thread-local is not the fix:
  spec 019 D-4 records why it has to be global, since the event loop runs on
  a tokio worker.

  **What remains owed elsewhere, unchanged by this spec:** spec 004 FR-006
  asks for a denying proxy around a *running app*, asserting zero connections
  from the **webview** process. That is a different measurement from this one,
  004 D-11 already records it as unwritten, and it is 004's to close.

## 8. Verification

```verify:cli
# FR-001 and FR-002, plus determinism, idempotence and totality.
cargo test -p butler-core --locked redaction::
# FR-002's compile-fail half: RedactedText has no constructor but `redact`.
# The doc tests include a positive control, so a passing compile_fail block
# cannot be passing because the import path is wrong.
cargo test -p butler-core --locked --doc
# AC-3: the fixtures are synthetic and committed.
test -f crates/butler-core/tests/fixtures/redaction/gen.rs
sh -c '! grep -rnE "sk-ant-api03-[A-Za-z0-9]{90,}" crates/butler-core/tests/'
# AC-2: docs reproduce spec 015 section 3.1 in full, including Diagnostics.
sh -c 'for c in Pixels Answer Secret Settings Diagnostics; do grep -q "^| $c |" docs/architecture.md || exit 1; done'
# Section 3.2: redaction is pure. No clock, filesystem or network.
sh -c '! grep -nE "std::(time|fs|net)" crates/butler-core/src/redaction.rs'
# FR-003: an `InferenceRequest` cannot be built from a `String` screen. The
# doctest carries a positive control, and it earned it: the compile_fail block
# first passed on a private-import error rather than on the type (D-4).
cargo test -p butler-llm --locked --doc
# FR-004: a whole cycle with the screen, an answer and a credential in scope,
# through the real runtime and reducer, produces no log line carrying any of
# them. The positive control in the same module asserts the capture reports a
# leak when one is made on purpose.
cargo test -p butler-desktop --locked --lib runtime::tests::fr_004
# FR-005: exactly one destination host, measured on two loopback listeners,
# and a proxy in the environment is not it.
cargo test -p butler-llm --locked --test egress
# FR-005 and section 3.3: the client refuses a proxy from the environment. If
# this call goes, the egress test above fails on its non-vacuity assertion
# rather than silently measuring nothing.
grep -q "no_proxy()" crates/butler-llm/src/anthropic.rs
# AC-1: this spec owns all seven constrained units, and they exist.
sh -c 'for u in crates/butler-capture/src/frame.rs crates/butler-ocr/src/recognized.rs crates/butler-llm/src/anthropic.rs crates/butler-llm/src/secrets.rs apps/desktop/src-tauri/src/logging.rs apps/desktop/src-tauri/tauri.conf.json; do test -e "$u" || exit 1; done'
test -d apps/desktop/src-tauri/capabilities
sh -c 'spec-spine index render | grep "W-001" | grep -q "015-privacy-boundary" && exit 1 || exit 0'
```