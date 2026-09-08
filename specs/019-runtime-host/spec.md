---
id: "019-runtime-host"
title: "Runtime host: the effect executor that drives the pure reducer from inside the desktop crate"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-02"
implementation: complete
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
  # Host surfaces in spec 004's crate: `runtime` is only reachable once
  # `lib.rs` declares it, and the executor's dependencies (tokio, plus the
  # 006/007/010 crates it calls) are pinned once in the root manifest
  # (001 FR-004) before the app manifest references them.
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/lib.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/Cargo.toml", nature: additive }
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
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

- **D-2 (2026-09-07, `Ports`, because 006, 007 and 010 do not exist yet).**
  §3.1 describes the runtime as owning `Box<dyn ScreenSource>`,
  `Box<dyn TextRecognizer>` and `Box<dyn Assistant>`. Those three traits are
  specs 006, 007 and 010, in phases 3 and 4. Defining them here would be this
  spec claiming another's territory, and inventing their shapes would force a
  retype when the real ones land.

  So the executor depends on **one** port it does own: `Ports`, with a method
  per effect that needs the outside world. `Runtime::spawn` is generic over
  it, which also keeps the trait free of boxed futures. When 006, 007 and 010
  land, one type implements `Ports` by delegating to them; the event loop,
  its ordering and its cancellation do not change, and every test here keeps
  its meaning because the mocks implement the same port a real adapter will.

  This is the hexagonal shape §3.1 is already reaching for, named. **Owed to
  phase 4**: the adapter that composes the three real traits behind it.

- **D-3 (2026-09-07, shutdown is a signal, not a dropped handle).** The first
  event loop ended when `rx.recv()` returned `None`, which never happened:
  the loop keeps a `Sender` of its own so spawned effects can post results
  back, so the channel could not close while the loop was alive. Every test
  hung.

  Shutdown is now an explicit `CancellationToken`. That also fixes a second
  problem the first design had: a caller that had cloned a handle could keep
  the process alive past shutdown without meaning to, because the channel's
  liveness was the exit condition.

  **Cancellation closes the receiver rather than breaking the loop.** §3.2
  says shutdown "closes the channel... and joins the event task", and what is
  already queued is still applied: a `Disarm` sent immediately before
  shutdown must take effect, or the machine's last recorded state is a lie.
  `Receiver::close` refuses new sends and lets the buffer drain, which is
  exactly that sentence. The first version broke out on cancellation instead
  and two tests caught it: the machine stayed `Armed` after a `Disarm` it had
  been handed.

- **D-4 (2026-09-07, FR-006's subscriber and the thread it lives on).**
  `tracing::subscriber::set_default` installs a subscriber for the **calling
  thread**. FR-006's test first ran the runtime on a multi-thread tokio
  runtime, where the event loop runs on a worker, so it captured nothing: the
  regex over "every line" passed because there were no lines.

  It now runs on a current-thread runtime, where the event loop runs on the
  test's own thread. The test asserts the output is non-empty before checking
  it, so the vacuous version cannot come back. Worth recording because a
  regex assertion over an empty capture is green, and green is what a test
  that has stopped testing looks like.

- **D-5 (2026-09-07, `Notice::AnswerDone` carries no stop reason).** Spec
  009's `Notice::AnswerDone` has only a request id, but spec 011's
  `UiEvent::AnswerDone` needs a `StopSummary`. The reducer does not have one:
  the pacer knows when an answer finished cleanly, and that is spec 013.

  `EndTurn` is emitted, which is the only honest default of the four: a
  refusal and a token cap both reach the overlay by other routes
  (`AnswerFailed`, or the provider's own reason once 010 supplies it), so
  this value cannot silently claim one of those happened. **Owed to spec 013**:
  the real stop reason, once the pacer reports it.

## 8. Verification

```verify:cli
# AC-1, AC-2 and FR-001 to FR-006: the mock-port harness, on both CI targets.
cargo test -p butler-desktop --locked runtime
# AC-2 names FR-005 and FR-006 as tests rather than review claims, so their
# absence must fail rather than pass quietly.
grep -q "fn fr_005_the_status_payload_carries_no_text" apps/desktop/src-tauri/src/runtime.rs
grep -q "fn fr_006_trace_records_name_ids_only" apps/desktop/src-tauri/src/runtime.rs
# AC-3: the decision log names both halves.
sh -c 'grep "^| D3 " docs/architecture.md | grep -q "009" && grep "^| D3 " docs/architecture.md | grep -q "019"'
# §2 and spec 009 AC-3: this spec owns no code in `butler-core`, and the
# executor's dependencies never reach it.
sh -c '! cargo tree -p butler-core --locked --edges normal | grep -Eq "tokio|tauri|windows|objc2|xcap"'
# §3.1: one task owns the state. A second `State::default()` outside the
# event loop would be a second owner.
sh -c 'test "$(grep -c "State::default()" apps/desktop/src-tauri/src/runtime.rs)" -le 3'
# D-3: shutdown drains what is queued rather than discarding it.
grep -q "rx.close()" apps/desktop/src-tauri/src/runtime.rs
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "019-runtime-host" && exit 1 || exit 0'
```
