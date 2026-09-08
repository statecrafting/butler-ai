---
id: "013-output-pacing"
title: "Output pacing: deliver the answer in chunks at a human reading rate"
status: approved
kind: "feature"
domain: "ui"
created: "2026-09-01"
implementation: complete
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
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/test/paced.test.tsx", nature: additive }
  # Host surfaces: `pacing` is only reachable once butler-core's `lib.rs`
  # declares it, and `PacedAnswer` only renders once spec 012's `App.tsx`
  # mounts it. The module is pure, so it adds no dependency.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/App.tsx", nature: additive }
  # §3.3's rendering surface inside spec 012's package: the panel that hosts
  # `PacedAnswer` (012 §3.3 assigns it that job), the store the chunk event
  # folds into (D-3), and the chunk fade and caret.
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/components/AnswerPanel.tsx", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/state/runtime.ts", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/styles/base.css", nature: additive }
  - { spec: "011-ipc-contract", unit: "apps/desktop/src/generated/bindings.ts", nature: additive }
  # D-1: §3.1 fixes the rate and its range, which spec 014 D-1 deferred to
  # this spec outright. Aligning `PacingSettings` with the policy touches the
  # settings model, its range table, the persisted golden, and the panel's
  # input bounds.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/settings.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/settings.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/settings_store.rs", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/components/SettingsPanel.tsx", nature: additive }
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

## 7. Resolved decisions

- **D-1 (2026-09-07, 220 wpm, and the settings default that disagreed).**
  §3.1 fixes the rate at 220 with a settings range of `120..=600`. Spec 014
  shipped `PacingSettings::default()` at **300**, validated over `60..=1200`.
  Spec 014 D-1 had already said the shape of this was "013's call, not this
  spec's", and 014's §3.1 names no number, so this is not an amendment to 014;
  it is this spec supplying the value 014 deferred.

  `PacingSettings::default()` now reads `pacing::DEFAULT_WORDS_PER_MINUTE` and
  the range check reads the two constants beside it, so there is one source
  and the two cannot drift again. A unit test asserts
  `PacingPolicy::from(PacingSettings::default()) == PacingPolicy::default()`,
  which is the property that was quietly false.

  Validation accepts `0` **outside** the range as well, because §3.1 gives `0`
  a meaning (no pacing) and a plain range check would have refused the
  documented setting.

- **D-2 (2026-09-07, `UiEvent` stopped deriving `Debug`).** `AnswerChunk` is
  the first variant to carry text the user was reading. Every tracing macro
  formats its arguments through `Debug`, so a single `?event` anywhere would
  put an answer into a log file, and spec 015 §3.3 and spec 016 §3.1 both say
  logs carry ids and kinds only.

  The derive comes off `UiEvent` and the hand-written impl prints the chunk's
  **character count** in place of its text. Everything else prints in full:
  state names, ids, error kinds, and a `Settings` that has no secrets in it by
  construction. The precedent is `UiCommand`, which has redacted
  `StoreSecret.secret` the same way since spec 011: spec 011 FR-003's claim is
  that the boundary is a property of the type rather than of anyone's
  diligence, and `Debug` is part of the type.

- **D-3 (2026-09-07, the out-of-order buffer is in the store).** §3.3 says
  `PacedAnswer` buffers out-of-order delivery. It is in `state/runtime.ts`
  instead, because spec 012 §3.4 gives the overlay exactly one event
  subscription and forbids a component from opening its own: a component
  cannot buffer an event it never sees. The behaviour §3.3 requires is
  unchanged (chunks render in `index` order, a chunk past a gap waits), and in
  the store it is testable without a DOM.

  The component keeps what is genuinely presentation: the per-chunk fade and
  the caret. §3.3's "it applies no delay of its own" is enforced by a check in
  §8 that greps the file for the browser timer functions.

- **D-4 (2026-09-07, §3.2's runtime wiring is owed to 019).** §3.2 says the
  runtime calls `push` on every `Chunk::Text`, `finish` on `Chunk::Done`, and
  `tick` on every `Tick`. There is no runtime to do it: `Ports::emit_chunk`
  exists as a seam and `Effect::EmitChunk` is dispatched to it, but the only
  implementation of `Ports` is a mock. The production adapter is spec 019's
  and the inference stream it would pace is spec 010's, neither wired.

  What is built instead is the half that can be, and it is the half an
  off-by-one would hide in: a test drives the pacer over the fixture corpus,
  hands `pacer.remaining()` to `Event::InferenceDone`, turns every release
  into `ChunkRendered`, and asserts the machine reaches `Idle` on exactly the
  last one. FR-005 asserts the count in isolation; this asserts it against the
  thing counting down. **Owed to 019**: the three calls above.

- **D-5 (2026-09-07, FR-004 needed the pacer to wait, not only to reach).**
  Read literally, §3.1's boundary rule is a reach: a release "extends or
  shortens by up to three words". Implemented as only that, over a six-word
  release, it covers seven end positions and landed on a boundary **61.7%** of
  the time against the fixture corpus. FR-004 asks for 80%.

  The missing half is that the pacer may also **hold**. If no boundary is
  within reach, waiting costs nothing (the budget is conserved, so FR-001's
  finish time is set by the word count and not by when each release goes out)
  and a boundary six words away comes into reach a few ticks later. It cannot
  wait forever: once the budget reaches `max_burst_words` a larger release is
  no longer possible, so the words go out unaligned. With that, the corpus
  measures **84.6%**, and a negative control with `prefer_boundaries: false`
  asserts the rate collapses, so the passing test cannot be passing because
  the corpus is punctuation soup.

  Two defects surfaced while measuring, both of which FR-004 was reporting as
  a low percentage rather than as themselves. `push(" ")` merged the words
  either side of it, because a whitespace-only chunk contributes no words and
  the next chunk does not begin with whitespace, so it looked like a
  continuation: a provider that emits `"all."`, `" "`, `"Capture"` produced
  `"all.Capture"`. And the lead clause was released without boundary
  alignment at all, so the first release of every answer missed by
  construction.

- **D-6 (2026-09-07, `remaining()` is a simulation, not a formula).** FR-005
  requires the count to equal the number of subsequent releases **exactly**,
  and there is no formula that does: a release is 3 to 9 words depending on
  where the next boundary falls, so `ceil(words / max_burst_words)` is wrong
  in both directions. `remaining()` therefore clones the pacer, marks the
  clone finished, and counts what it releases at the runtime's one-tick
  cadence.

  Marking the clone finished is what makes the loop terminate, and it is also
  what the question means: §3.2 reads the count at `InferenceDone`, so it is
  asking how many releases remain if nothing more arrives. The cost is a few
  hundred iterations of pure integer arithmetic, once per answer.

- **D-7 (2026-09-07, FR-003 does not cap an unpaced release).** FR-003 says no
  release exceeds `max_burst_words`; §3.1 says `words_per_minute = 0` releases
  "one release per provider chunk"; FR-006 repeats it. A provider chunk longer
  than fourteen words cannot satisfy both.

  §3.1 and FR-006 are specific and agree with each other, so they win: unpaced
  mode is raw streaming and hands back the chunk it was given, verbatim and
  whole. FR-003 is asserted over the paced modes, which is where a burst cap
  means anything. Unpaced mode buffers the provider's chunks rather than this
  module's words for the same reason, which is also what makes FR-006 exact
  regardless of tick cadence.

## 8. Verification

```verify:cli
# AC-1: FR-001 to FR-006 and the machine integration, driven by a
# deterministic tick. The tests live in a `pacing` module so this command
# selects exactly them.
cargo test -p butler-core --locked pacing::
# D-1: the policy defaults, and the settings default that has to equal them.
cargo test -p butler-core --locked --lib pacing::tests
cargo test -p butler-core --locked --test settings
# D-2: the one variant carrying the user's answer prints its length.
cargo test -p butler-core --locked --lib ipc::tests
# AC-2: chunks render in `index` order and the caret disappears on `is_last`.
pnpm --filter @butler-ai/desktop test
pnpm --filter @butler-ai/desktop typecheck
pnpm --filter @butler-ai/desktop lint
# D-1: the settings range is this spec's constants, not a second copy of the
# numbers. Grepping for `120` would match a dozen unrelated things.
grep -q "MIN_WORDS_PER_MINUTE" crates/butler-core/src/settings.rs
grep -q "DEFAULT_WORDS_PER_MINUTE" crates/butler-core/src/settings.rs
# §1 and §3.3: the overlay applies no timing of its own. The component's own
# comment deliberately names none of these, or this check would pass or fail
# on the documentation rather than on the code (spec 016 D-2).
sh -c '! grep -qE "setTimeout|setInterval|requestAnimationFrame" apps/desktop/src/components/PacedAnswer.tsx'
# §3.2: the machine's countdown and the pacer's count are the same number.
cargo test -p butler-core --locked pacing::the_machine_returns_to_idle
# §3.1: `0` means no pacing, so validation cannot be a bare range check.
grep -q "words_per_minute != 0" crates/butler-core/src/settings.rs
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "013-output-pacing" && exit 1 || exit 0'
```
