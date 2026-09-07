---
id: "008-change-detection"
title: "Change detection: fire inference only when the screen's text has meaningfully changed"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: in-progress
owner: "butler-ai maintainers"
risk: medium
platforms: "all"
phase: 1
depends_on:
  # Phase 1 (018): this module is pure code over a normalized `String`. It
  # never names a `butler-ocr` type, so it does not wait for 007.
  - "009-pipeline-state-machine"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/delta.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/delta.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::delta" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::delta::ChangeDetector" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::delta::LevenshteinDetector" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::delta::Verdict" }, nature: additive }
  # Host surfaces in spec 009's crate: `delta` is only reachable once
  # `lib.rs` declares it, and `strsim` (section 2) is pinned once in the root
  # manifest (001 FR-004) before butler-core's manifest references it.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/Cargo.toml", nature: additive }
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
summary: >
  The `delta` module of `butler-core`: a `ChangeDetector` trait and the v1
  `LevenshteinDetector`, which compares the normalized text of the current
  frame with the text last sent to inference and returns `Unchanged` or
  `Changed` with a similarity ratio. A normalized edit-distance ratio, a
  configurable threshold, a stability window (a change must persist across
  consecutive frames so mid-scroll and mid-render frames are not sent), and an
  exclusion set (the overlay's own last answer, used only when exclusion is
  not verified) make the outline's "only call the LLM when the Levenshtein
  distance exceeds a threshold" precise, cheap and testable. Pure code: no OS,
  no clock, no I/O.
---

# 008: Change detection

## 1. Purpose

Inference is the expensive, slow, and privacy-relevant step. Everything before
it is cheap and local. The change detector is the valve between them: it turns
a stream of frames into a much sparser stream of "the user is now looking at
something new". The outline specifies a Levenshtein threshold; this spec
keeps that as the v1 algorithm and adds the two refinements that make it
work in practice: comparing against the *last inferred* text rather than the
last frame (so slow drift still accumulates into a change), and requiring
stability (so a frame captured mid-scroll does not trigger).

## 2. Territory

`crates/butler-core/src/delta.rs` (the module) and `tests/delta.rs`, added
to spec 009's crate. Pure Rust, no dependencies beyond `strsim` (pinned) for
the distance; `no_std`-compatible is not required but no `std::time`,
`std::fs`, or `std::net` may be used.

## 3. Behavior

### 3.1 The trait

```rust
pub trait ChangeDetector {
    fn evaluate(&mut self, current: &str, input: &DetectorInput) -> Verdict;
    fn commit(&mut self, inferred: &str);   // called when inference starts on `inferred`
    fn reset(&mut self);                    // on Disarm or monitor change
}
pub struct DetectorInput<'a> { pub exclude: Option<&'a str>, pub force: bool }
pub enum Verdict {
    Unchanged { similarity: f32 },
    Pending  { similarity: f32, seen: u8 },   // changed, but not yet stable
    Changed  { similarity: f32 },
}
```

### 3.2 `LevenshteinDetector`

Configured (014) by `threshold: f32` (default `0.85`), `stability_frames: u8`
(default `2`), `max_compare_chars: usize` (default `6000`).

1. If `input.force`, return `Changed` regardless (the "ask now" shortcut).
2. Let `base` be the last committed text (empty at start). If `input.exclude`
   is `Some(answer)`, remove from `current` every line that appears in
   `answer` (exact line match after normalization) before comparing.
3. Similarity is `1 - levenshtein(base, current) / max(len(base),
   len(current))`, computed over the first `max_compare_chars` chars of each
   (long screens are compared by prefix; the prefix is in reading order, so
   the top of the screen dominates, which matches attention).
4. If `similarity >= threshold`, return `Unchanged` and clear the pending
   counter.
5. Otherwise increment the pending counter for this candidate. A candidate is
   the same if its similarity to the previous candidate is `>= threshold`.
   Return `Pending` until the counter reaches `stability_frames`, then
   `Changed`.
6. `commit(inferred)` replaces `base` and clears the counter.

Bounded cost: `6000 × 6000` cells is ~36 M byte operations, single-digit
milliseconds; the runtime calls this off the UI thread regardless.

### 3.3 Determinism

The detector is a pure function of its inputs and its own state. No clock: the
stability window is counted in frames, and the capture interval (009) makes
that a time bound.

## 4. Functional requirements

- **FR-001.** Identical inputs → `Unchanged { similarity: 1.0 }`.
- **FR-002.** A completely different input → `Pending` on the first
  `stability_frames - 1` calls and `Changed` on the next, then `Unchanged`
  after `commit`.
- **FR-003.** Slow drift: appending one line per call to a 40-line base
  yields `Unchanged` until the cumulative similarity crosses the threshold,
  then `Changed` (the base is the last *committed* text, not the previous
  frame).
- **FR-004.** With `exclude = Some(answer)`, a `current` that is `base` plus
  the answer's lines is `Unchanged`.
- **FR-005.** `force` returns `Changed` even for identical inputs and does
  not alter the pending counter.
- **FR-006.** Property test: `evaluate` never panics for any pair of
  arbitrary Unicode strings up to 20 000 chars (proptest).

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core delta::` covers FR-001 to FR-006 and
  passes on Linux, macOS and Windows.
- **AC-2.** A benchmark (`benches/delta.rs`, criterion, not gated) records
  the p50 for two 6000-char inputs; the number is quoted in
  `docs/architecture.md`.

## 6. Out of scope

- Semantic change detection (embeddings, structural diffing). The trait
  admits it; v1 ships Levenshtein.
- Deciding *what* to send after a change (010) and *when* to evaluate (009).

## 7. Resolved decisions

- **D-1 (2026-09-02).** `depends_on` no longer lists 007. The edge described
  the pipeline's data flow (OCR produces the text this module compares), not
  an implementation dependency: `delta.rs` takes `&str`, names no
  `butler_ocr` type, and this spec's own summary calls it pure code with no
  OS. Keeping the edge put a phase 1 spec behind a phase 3 spec and made 018's
  phase 1 unreachable in the graph the orchestrator actually schedules on.
