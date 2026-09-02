---
id: "019-runtime-host"
title: "Runtime host: the effect executor that drives the pure reducer from inside the desktop crate"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-02"
implementation: pending
owner: "butler-ai maintainers"
risk: high
platforms: "all"
phase: 2
depends_on:
  - "009-pipeline-state-machine"
  - "004-desktop-shell"
extends:
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/runtime.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::runtime::Runtime" }, nature: additive }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "state diagram" }
summary: >
  The impure half of the pipeline: `Runtime`, the effect executor that lives in
  spec 004's Tauri crate, owns the machine's `State`, holds the trait objects
  from 006, 007 and 010, and turns the `Effect` values that `reduce` returns
  into real capture, recognition, inference and IPC emission, feeding every
  result back as an ordered event. Spec 009 keeps the pure reducer and the
  `butler-core` crate; this spec keeps everything the reducer refuses to know
  about: tokio tasks, cancellation tokens, blocking pools, and the frame and
  text slots. The split is what lets 009 be phase 1 (no OS, no tauri, Linux CI)
  while its executor waits for the phase 2 crate that hosts it.
---

# 019: Runtime host

## 1. Purpose

Spec 009 defines `reduce(state, event) -> (state, effects)` as a pure
function whose side effects are *data*. Something has to execute that data
against the real world. That executor cannot live in `butler-core`: 009 AC-3
forbids `tokio`, `tauri`, `windows`, `objc2` and `xcap` anywhere in that
crate's dependency tree, and the executor needs most of them. It lives in the
Tauri crate that spec 004 establishes.

It also cannot be authored by 009. The unit sits inside 004's crate, and 018
puts 009 in phase 1 and 004 in phase 2. A phase 1 spec cannot reach zero
unresolved units while two of them are inside a crate phase 2 has not built
yet. Splitting the executor into its own phase 2 spec is what makes both
specs completable in the order the plan already declares (D-1).

## 2. Territory

- `apps/desktop/src-tauri/src/runtime.rs` (added to spec 004's crate): the
  effect executor, and the only place the traits from 006, 007 and 010 are
  called.
- `butler_desktop::runtime::Runtime`: the type that owns the machine state and
  the executor's resources.

This spec owns no code in `butler-core`. The `State`, `Event`, `Effect` types
and `reduce` are 009's, consumed here and never redefined.

## 3. Behavior

### 3.1 The runtime (`runtime.rs`)

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

### 3.2 Construction and shutdown

`Runtime::spawn` takes the trait objects and returns a handle that spec 004's
`AppState` stores (004 §3.3). The handle is the only way the shell, the
shortcuts and the IPC commands (011) reach the machine: nothing outside this
module touches `State`. On shutdown the handle closes the channel, cancels any
in-flight inference, and joins the event task before the process exits, so a
capture in flight cannot outlive the window it belongs to.

The runtime starts `Disarmed` (009 §3.1). Where 004 §3.4 says the shell
"starts the runtime disarmed", the initial state is the reducer's, and this
spec is what makes the call.

## 4. Functional requirements

- **FR-001.** The runtime executes a full cycle against mock traits in under
  50 ms of overhead beyond the mocks' own latency.
- **FR-002.** Events are applied in channel order on a single task: a test
  that injects `Captured { seq: 5 }` and `Captured { seq: 4 }` concurrently
  from two producers observes exactly the reducer's out-of-order verdict
  (009 FR-003), never an interleaved state.
- **FR-003.** No `Capture` or `Recognize` call blocks the event task: with a
  mock source that sleeps 200 ms, a `Disarm` sent 10 ms later is applied
  before the capture returns.
- **FR-004.** `ReleaseFrame` zeroes and drops the frame slot before the next
  `Capture` is issued.
- **FR-005.** No `runtime.status` payload field carries recognized text or
  error text: the emitted struct's fields are state name, `seq`, `request`,
  exclusion status and an error *kind*, all `Copy` or enum-typed (015).
- **FR-006.** Trace records name ids only: a test asserts the tracing output
  for a full cycle matches `^[a-z_.:= 0-9-]*$` with no captured text.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-desktop runtime::` passes on macOS and
  Windows with a mock-trait harness that drives arm, capture, recognize,
  evaluate, infer, render and disarm.
- **AC-2.** FR-005 and FR-006 are each a named test, not a review claim.
- **AC-3.** `docs/architecture.md` decision log row D3 cites this spec for the
  executor half and 009 for the reducer half.

## 6. Out of scope

- The reducer, its types and its transition table (009).
- The algorithms behind each effect (006, 007, 008, 010, 013).
- The IPC wire types and generated bindings (011); this spec emits them, it
  does not define them.
- Persisting state across launches: the machine always starts `Disarmed`.

## 7. Resolved decisions

- **D-1 (2026-09-02).** This spec exists because spec 009 claimed two units
  inside spec 004's crate (`runtime.rs` and `butler_desktop::runtime::Runtime`)
  while 018 places 009 in phase 1 and 004 in phase 2. The orchestrator driving
  this corpus schedules on `depends_on` and cannot satisfy both: 009 could
  never reach zero unresolved units before the crate existed, and 004 could
  not be built first because adding `apps/desktop/src-tauri` to a `members`
  list whose `crates/*` glob still matches nothing is the exact cargo failure
  recorded in 001 D-1. Three resolutions were considered: hand the two units
  to 004 (rejected: 004 §6 puts the pipeline runtime out of its scope, and the
  shell spec would absorb the most intricate loop in the app), fold them into
  011 (rejected: 011 is an invariant-freeze contract spec and hosting a
  stateful executor dilutes that), or split the executor into its own phase 2
  spec. The third is this spec. Spec 009 keeps the pure reducer and its
  Linux-CI guarantee; the executor is scheduled after the crate that hosts it.
