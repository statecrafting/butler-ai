---
id: "009-pipeline-state-machine"
title: "Pipeline state machine: a pure reducer that encloses capture → OCR → evaluate → infer → render"
status: draft
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: pending
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
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/runtime.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::runtime::Runtime" }, nature: additive }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "state diagram" }
summary: >
  The heart of butler-ai and the owner of the `butler-core` crate: the
  pipeline is a strict state machine expressed as a pure reducer
  `reduce(state, event) -> (state, effects)` with no I/O, no clock and no
  threads, plus a runtime in the desktop crate that executes the effects
  (capture, recognize, evaluate, infer, emit) against the real traits and
  feeds the results back as events. The outline's five states become an
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
- `apps/desktop/src-tauri/src/runtime.rs` (added to spec 004's crate): the
  effect executor, the only place the traits from 006/007/010 are called.

`butler-core` MUST depend on no platform crate, no `tokio`, no `tauri`. Its
only dependencies are `serde` (for the IPC DTOs), `strsim`, `thiserror`, and
`proptest` (dev).

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

### 3.5 The runtime (`runtime.rs`)

`Runtime` owns: the `State`, the traits (`Box<dyn ScreenSource>`, `Box<dyn
TextRecognizer>`, `Box<dyn Assistant>`, the detector, the pacing policy),
the frame and text slots, a `CancellationToken` per inference, and a tokio
task per blocking effect. It MUST:

- process events strictly in order on one task (an `mpsc` channel), applying
  `reduce` and then executing effects; effects that produce results send
  events back on the same channel;
- run `Capture` and `Recognize` on `spawn_blocking`, `StartInference` on an
  async task bound to the request's cancellation token;
- drop the frame slot on `ReleaseFrame` (which zeroes it, 006);
- emit `runtime.status` (011) on every state change with the state name,
  `seq`, `request`, exclusion status and the last error kind (never error
  text that could carry screen content);
- record every `(state, event) -> state` transition at `trace` level (016)
  with ids only.

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
- **FR-007.** The runtime executes a full cycle against mock traits in under
  50 ms of overhead beyond the mocks' own latency.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core machine::` passes on Linux, macOS and
  Windows with the transition table as a data-driven test.
- **AC-2.** `docs/architecture.md` §State machine shows the diagram and it
  matches the table (a test renders the table to the same Mermaid source and
  diffs it against the doc; the doc is regenerated, never hand-edited, for
  that section).
- **AC-3.** `cargo tree -p butler-core` contains no `tokio`, `tauri`,
  `windows`, `objc2`, or `xcap`.

## 6. Out of scope

- The algorithms behind each effect (006, 007, 008, 010, 013).
- Persisting state across launches: the machine always starts `Disarmed`.
