---
id: "009-pipeline-state-machine"
title: "Pipeline state machine: a pure reducer that encloses capture → OCR → evaluate → infer → render"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: high
platforms: "all"
phase: 1
depends_on:
  - "001-workspace-layout"
establishes:
  - { kind: crate, id: "butler-core" }
  - "crates/butler-core/Cargo.toml"
  - "crates/butler-core/src/lib.rs"
  - "crates/butler-core/src/machine.rs"
  - "crates/butler-core/tests/machine.rs"
  - { kind: module, id: "butler_core::machine" }
  - { kind: symbol, id: "butler_core::machine::State" }
  - { kind: symbol, id: "butler_core::machine::Event" }
  - { kind: symbol, id: "butler_core::machine::Effect" }
  - { kind: symbol, id: "butler_core::machine::reduce" }
extends:
  # Spec 001 section 3.1 requires every third-party crate to be pinned in the
  # root [workspace.dependencies], and that table's own comment says it fills
  # as the crate-owning specs land their dependencies. This is that landing
  # for spec 009's proptest pin (FR-001, FR-002): an additive edit to a unit
  # 001 owns, declared by the spec making it rather than waived at PR time.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "state diagram" }
summary: >
  The heart of butler-ai and the owner of the `butler-core` crate: the
  pipeline is a strict state machine expressed as a pure reducer
  `reduce(state, event) -> (state, effects)` with no I/O, no clock and no
  threads. The effects it returns are executed by the runtime host in the
  desktop crate (019), which feeds every result back as an event; this spec
  owns the decision, not the execution. The outline's five states become an
  armed/disarmed session flag over a capture cycle with single in-flight
  inference, explicit cancellation, a fault state with backoff, and a degraded
  state driven by the exclusion status. Duplicate or out-of-order frames
  cannot reach the assistant because the reducer refuses them by construction.
---

# 009: Pipeline state machine

## 1. Purpose

The outline asks for a state machine "to prevent race conditions during
OCR/LLM streaming" and "to ensure deterministic execution and prevent the LLM
from processing duplicate or out-of-order screen frames". The strongest form
of that guarantee is a machine whose transitions are a pure function that can
be exhaustively tested, and whose side effects are data the runtime executes.
That is what this spec defines. Constitution §IV: the core of the product is
deterministic for the same reason the corpus is.

## 2. Territory

- The crate `butler-core` (manifest floor), `lib.rs`, and `machine.rs`: the
  `State`, `Event`, `Effect` types and `reduce`. `tests/machine.rs`: the
  transition table tests and property tests. Other core modules are added by
  their specs: `delta` (008), `pacing` (013), `settings` (014), `redaction`
  (015), `ipc` (011).

The effect executor that runs these effects against the real traits
(`apps/desktop/src-tauri/src/runtime.rs` and `butler_desktop::runtime::Runtime`)
is spec 019's territory, not this spec's: it lives inside the crate spec 004
establishes, and 018 puts that crate a phase later (019 D-1).

`butler-core` MUST depend on no platform crate, no `tokio`, no `tauri`. Its
only runtime dependencies are `serde` (for the IPC DTOs), `strsim` and
`thiserror`; its only dev dependencies are `proptest` and `criterion` (the
latter for spec 008 AC-2's benchmark target alone).

The budget is stated in two halves because they defend different things. The
*runtime* half is the product guarantee: what the shipped library links is what
runs on the user's machine, and nothing platform-shaped may be in it. The *dev*
half is a hygiene budget: a benchmark harness never ships, so a transitive
platform crate underneath one is not a breach of the guarantee, but the direct
dependency list stays closed so the crate cannot accumulate a test-time
dependency tree nobody chose. See D-2.

## 3. Behavior

### 3.1 States

```rust
pub enum State {
    Disarmed,
    Armed(Cycle),
    Degraded { reason: DegradedReason, cycle: Option<Cycle> },   // exclusion not verified
    Fault { error: FaultKind, retry_in: Ticks, cycle_state: Box<State> },
}
pub enum Cycle {
    Idle       { next_capture_in: Ticks },
    Capturing  { seq: Seq },
    Recognizing{ seq: Seq },
    Evaluating { seq: Seq },
    Inferencing{ seq: Seq, request: RequestId, started_tick: Tick },
    Rendering  { seq: Seq, request: RequestId, remaining_chunks: u32 },
}
```

`Seq` is a monotonically increasing frame sequence number; `RequestId` an
inference id; `Ticks` a count of runtime ticks (the runtime ticks every 100 ms,
so the machine has time without a clock).

### 3.2 Events

`Arm`, `Disarm`, `Tick`, `ForceCapture`, `Captured { seq }`, `CaptureFailed {
seq, error }`, `Recognized { seq, text_len, mean_conf }`, `RecognizeFailed {
seq, error }`, `Evaluated { seq, verdict }`, `InferenceChunk { request, chunk_
index }`, `InferenceDone { request, stop }`, `InferenceFailed { request, error,
retryable }`, `ChunkRendered { request }`, `ExclusionChanged { status }`,
`SettingsChanged { capture_interval, .. }`, `ScreenLocked`, `ScreenUnlocked`,
`MonitorChanged`.

Events carry ids and small metadata only; the text, frames and chunks
themselves stay in the runtime's typed slots keyed by `seq`/`request`, so the
reducer is cheap to log and replay.

### 3.3 Effects

`Capture { seq, monitor }`, `Recognize { seq }`, `Evaluate { seq }`,
`StartInference { request, seq }`, `CancelInference { request }`,
`EmitChunk { request }`, `Emit(UiEvent)` (011), `RunSelfTest`, `ScheduleTick`,
`ReleaseFrame { seq }`, `Log(Level, &'static str)`.

### 3.4 The reducer

`pub fn reduce(state: State, event: Event, cfg: &MachineConfig) -> (State,
Vec<Effect>)`. Normative transitions (the full table lives in
`tests/machine.rs` as data and in `docs/architecture.md` as a diagram):

1. `Disarmed + Arm` → `Armed(Idle { next_capture_in: 0 })` + `[RunSelfTest,
   Emit(status)]`, unless the last known exclusion status is not `Verified`
   and `cfg.allow_degraded` is false, in which case → `Degraded` + `[Emit]`.
2. `Idle + Tick` decrements; at zero → `Capturing { seq: next }` +
   `[Capture]`. `ForceCapture` in any `Armed` sub-state except `Inferencing`
   → `Capturing` with `force` remembered for the evaluation.
3. `Capturing + Captured { seq == current }` → `Recognizing` +
   `[Recognize]`. A `Captured` whose `seq` does not match the current one is
   **dropped** with `[ReleaseFrame, Log]` (the out-of-order guard).
4. `Recognizing + Recognized` → `Evaluating` + `[Evaluate]`.
5. `Evaluating + Evaluated { Unchanged | Pending }` → `Idle {
   next_capture_in: cfg.capture_interval_ticks }` + `[ReleaseFrame]`.
   `Evaluated { Changed }` → `Inferencing { request: next }` +
   `[StartInference, ReleaseFrame, Emit(answer.started)]`.
6. `Inferencing`: `Tick` is a no-op except for a timeout at
   `cfg.inference_timeout_ticks` → `Fault { retryable }`. `Captured`,
   `Recognized`, `Evaluated` for any `seq` are dropped: **single in-flight
   inference, no queue** (a frame during inference is stale by definition;
   the next cycle after rendering evaluates against the committed text, so a
   real change is not lost). `InferenceChunk` → same state + `[EmitChunk]`
   only when pacing (013) says so. `InferenceDone` → `Rendering` with the
   pacing policy's remaining count. `InferenceFailed { retryable: true }` →
   `Fault` with backoff; `{ false }` → `Idle` + `[Emit(answer.failed)]`.
7. `Rendering + ChunkRendered` decrements; at zero → `Idle {
   next_capture_in: cfg.post_answer_delay_ticks }` + `[Emit(answer.done)]`.
8. `Disarm` in any state → `Disarmed` + `[CancelInference?, ReleaseFrame?,
   Emit(status)]`. Every effect that could hold user data is released.
9. `ScreenLocked` → `Disarmed` (same effects) and `ScreenUnlocked` does not
   re-arm (explicit user action required).
10. `ExclusionChanged { !Verified }` in any `Armed` state → `Degraded` with
    the current cycle preserved if `cfg.allow_degraded`, else `Disarmed` +
    cancel. `ExclusionChanged { Verified }` from `Degraded` → `Armed`.
11. `Fault + Tick` counts down `retry_in` (exponential, 1 s → 60 s, jitter
    provided by the runtime as an event field, never generated in the
    reducer); at zero → the boxed prior state's `Idle`.
12. `MonitorChanged` → `Idle` with an immediate capture and a detector reset
    effect; `SettingsChanged` updates `cfg`-derived values on the next `Idle`.

Every transition MUST be total: an `(state, event)` pair not in the table
returns the same state with `[Log(Warn, "ignored")]`, never a panic.

## 4. Functional requirements

- **FR-001.** `reduce` is a pure function: same `(state, event, cfg)` → same
  `(state, effects)`; verified by a proptest that calls it twice.
- **FR-002.** No `(state, event)` pair panics (proptest over arbitrary
  sequences of up to 1000 events).
- **FR-003.** Out-of-order guard: after `Capturing { seq: 5 }`, `Captured {
  seq: 4 }` is dropped and `Captured { seq: 5 }` advances.
- **FR-004.** Single in-flight: in `Inferencing`, no sequence of `Captured`/
  `Recognized`/`Evaluated` events produces a second `StartInference`.
- **FR-005.** `Disarm` from every state yields `Disarmed` and, if an
  inference was in flight, exactly one `CancelInference` for its id.
- **FR-006.** `Degraded` is unreachable with `allow_degraded = false`: any
  non-`Verified` exclusion status leads to `Disarmed`.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core machine::` passes on every host in the
  CI matrix (003 §3.2) with the transition table as a data-driven test. The
  crate's independence from any host is AC-3's job, not a third runner's
  (018 D-3).
- **AC-2.** `docs/architecture.md` §State machine shows the diagram and it
  matches the table (a test renders the table to the same Mermaid source and
  diffs it against the doc; the doc is regenerated, never hand-edited, for
  that section).
- **AC-3.** `cargo tree -p butler-core --edges normal` (the runtime graph, on
  every target in the matrix) contains no `tokio`, `tauri`, `windows`, `objc2`,
  or `xcap`, and no *direct* dependency of the crate, runtime or dev, is a
  platform crate. Dev-only transitive edges are out of scope: see D-2.

## 6. Out of scope

- The algorithms behind each effect (006, 007, 008, 010, 013).
- The effect executor and everything it needs to be impure: tokio, blocking
  pools, cancellation tokens, the frame and text slots (019).
- Persisting state across launches: the machine always starts `Disarmed`.

## 7. Resolved decisions

- **D-1 (2026-09-02).** This spec originally claimed `runtime.rs` and
  `butler_desktop::runtime::Runtime` as `extends` edges into spec 004's crate,
  and carried the executor's behavior as §3.5 and FR-007. Both units are now
  spec 019's, for the reason recorded in 019 D-1: a phase 1 spec cannot reach
  zero unresolved units while two of them sit inside a crate that phase 2
  builds. What remains here is exactly what the phase 1 entry condition
  claims, a crate with no OS, no `tokio` and no `tauri` in its tree (AC-3),
  buildable and testable on Linux CI before any desktop code exists.

## 8. Verification

```verify:cli
# AC-1: the reducer's transition table passes on this host.
cargo test -p butler-core --locked machine::
# AC-3: the shipped library pulls in no OS, async or UI dependency, on the
# Windows target too (D-2 narrowed this to the runtime graph; a dev-only
# benchmark harness does not ship).
sh -c '! cargo tree -p butler-core --locked --edges normal | grep -Eq "tokio|tauri|windows|objc2|xcap"'
sh -c '! cargo tree -p butler-core --locked --edges normal --target x86_64-pc-windows-msvc | grep -Eq "tokio|tauri|windows|objc2|xcap"'
# §Territory: butler-core is claimed end to end, and the spec is at zero.
spec-spine index coverage --fail-on-untraced
sh -c 'spec-spine index render | grep "W-001" | grep -q "009-pipeline-state-machine" && exit 1 || exit 0'
```

- **D-2 (2026-09-06, amendment, approved by the maintainer in session).**
  Spec 008 AC-2 requires a criterion benchmark inside this crate. Adding it
  broke two of this spec's claims at once: §2's closed dependency list, and
  AC-3, because on `x86_64-pc-windows-msvc` the chain `criterion -> walkdir ->
  same-file -> winapi-util -> windows-sys v0.61.2` puts a `windows` crate in
  `cargo tree`. This spec's own test `ac_003_no_platform_dependencies` caught
  it, which is the governance working rather than failing.

  Two approved specs genuinely contradicted each other, so the resolution was a
  human decision, not an implementation choice. It was taken as an amendment
  here rather than by weakening spec 008, and rather than by editing the test
  to pass: AC-3's purpose is that *the shipped library* has no operating system
  in it, and a benchmark harness does not ship. AC-3 therefore now reads the
  runtime graph (`--edges normal`) and additionally forbids any direct
  dependency, runtime or dev, from being a platform crate. The guarantee the
  criterion was written to defend is unchanged; what changed is that it no
  longer also catches dev-only transitive edges it was never aimed at.

  `criterion` joins §2's dev half and `ALLOWED_DEPENDENCIES` in
  `tests/machine.rs`. Nothing else about the crate moved.