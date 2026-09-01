---
id: "013-output-pacing"
title: "Output pacing: deliver the answer in chunks at a human reading rate"
status: draft
kind: "feature"
domain: "ui"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: low
platforms: "all"
phase: 4
depends_on:
  - "009-pipeline-state-machine"
  - "010-assistant-inference"
  - "011-ipc-contract"
  - "012-overlay-ui"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/pacing.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/pacing.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::pacing" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::pacing::PacingPolicy" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::pacing::Pacer" }, nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/components/PacedAnswer.tsx", nature: additive }
refines:
  - { aspect: "rendering-state", unit: "crates/butler-core/src/machine.rs" }
  - { aspect: "answer-chunk-event", unit: "crates/butler-core/src/ipc.rs" }
summary: >
  Model output arrives in bursts of tokens; the outline requires that the
  overlay present it at a natural reading pace in chunks rather than as a wall
  of text that appears at once. The pacing policy is a pure function in
  `butler-core` (`Pacer`), driven by the runtime's tick: it buffers provider
  chunks, releases them at a configurable words-per-minute rate with sentence
  and clause boundaries preferred, front-loads the first clause so the answer
  starts quickly, and never exceeds a burst cap. The machine's `Rendering`
  state (009) ends when the pacer drains. The UI (012) is a passive consumer
  of `AnswerChunk` events; it applies no timing of its own.
---

# 013: Output pacing

## 1. Purpose

A streamed answer is readable only if it arrives at roughly the rate a person
reads. Faster, and the reader loses their place as the panel grows; slower,
and it feels broken. Pacing also smooths the token stream's burstiness into a
steady repaint cadence, which keeps the overlay visually calm. The policy is
specified in core, not in the UI, so the machine knows when rendering is
complete and so the behavior is testable without a browser.

## 2. Territory

`crates/butler-core/src/pacing.rs` and `tests/pacing.rs` (added to 009's
crate), the `PacedAnswer` component in the overlay (added to 012's package),
and refinements of two existing units: the `Rendering` sub-state in
`machine.rs` (its `remaining_chunks` is fed by the pacer) and the
`AnswerChunk` event in `ipc.rs` (its `index`/`is_last` semantics).

## 3. Behavior

### 3.1 Policy

```rust
pub struct PacingPolicy {
    pub words_per_minute: u16,     // default 220; settings range 120..=600; 0 = no pacing
    pub first_chunk_words: u8,     // default 6: the lead clause is released immediately
    pub max_burst_words: u8,       // default 14
    pub prefer_boundaries: bool,   // default true: end chunks at . ! ? ; , or newline
}
pub struct Pacer { /* buffer, released count, tick accounting */ }
impl Pacer {
    pub fn push(&mut self, text: &str);                 // provider chunk arrives
    pub fn finish(&mut self);                           // provider stream ended
    pub fn tick(&mut self, tick: Tick) -> Option<Release>;   // maybe release a chunk
    pub fn remaining(&self) -> u32;                     // chunks still to release (for Rendering)
    pub fn drain(&mut self) -> Vec<Release>;            // on Dismiss / Disarm: release nothing, clear
}
pub struct Release { pub index: u32, pub text: String, pub is_last: bool }
```

- Word budget per tick is `words_per_minute / 600` (ticks are 100 ms); the
  pacer accumulates fractional budget so low rates still progress.
- A release contains whole words only; with `prefer_boundaries` it extends or
  shortens by up to three words to end on a boundary character.
- The first release happens on the first tick after at least
  `first_chunk_words` (or the stream's end) is buffered, regardless of budget.
- `is_last` is true only after `finish()` and the buffer is empty.
- `words_per_minute = 0` releases everything as it arrives (one release per
  provider chunk, for users who prefer raw streaming).

### 3.2 Machine and IPC integration

- The runtime calls `push` on every `Chunk::Text`, `finish` on `Chunk::Done`,
  and `tick` on every `Tick` while in `Inferencing` or `Rendering`; each
  `Some(Release)` becomes `Effect::EmitChunk` → `UiEvent::AnswerChunk`.
- `Rendering.remaining_chunks` is `pacer.remaining()` at the moment of
  `InferenceDone`; the machine returns to `Idle` when it reaches zero (009
  §3.4.7). If the provider stream ends before the first release, the whole
  answer is still paced, not dumped.
- `Dismiss` and `Disarm` drain the pacer and the UI clears the panel.

### 3.3 UI (`PacedAnswer.tsx`)

Appends `AnswerChunk.text` to the panel in `index` order (buffering any
out-of-order delivery, which Tauri does not produce but the component
tolerates), renders a caret while `!is_last`, and applies a 120 ms opacity
fade per chunk. It applies no delay of its own.

## 4. Functional requirements

- **FR-001.** At 220 wpm, a 110-word answer pushed at once is released across
  ticks spanning 29 s to 31 s (deterministic tick simulation).
- **FR-002.** The first release occurs on the first tick after six words are
  buffered, even if that tick's budget is below six.
- **FR-003.** No release exceeds `max_burst_words`.
- **FR-004.** With `prefer_boundaries`, at least 80% of releases in the
  fixture corpus end on a boundary character.
- **FR-005.** `remaining()` after `finish()` equals the number of subsequent
  `Some(Release)` results exactly.
- **FR-006.** `words_per_minute = 0` yields one release per `push`.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core pacing::` passes on all CI targets.
- **AC-2.** `PacedAnswer` Vitest: chunks render in `index` order and the
  caret disappears on `is_last`.

## 6. Out of scope

- Typewriter (per-character) effects; the unit of release is the word.
- Adjusting pace to the reader (eye tracking, scroll position). Not planned.
