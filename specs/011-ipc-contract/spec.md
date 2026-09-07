---
id: "011-ipc-contract"
title: "IPC contract: typed commands and events between the Rust runtime and the overlay, with generated bindings"
status: approved
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: medium
platforms: "all"
phase: 2
depends_on:
  - "009-pipeline-state-machine"
  - "004-desktop-shell"
establishes:
  - "apps/desktop/src/generated/bindings.ts"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/ipc.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::ipc" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::ipc::UiEvent" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::ipc::UiCommand" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::ipc::IPC_CONTRACT_VERSION" }, nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/commands.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/events.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/bin/export-bindings.rs", nature: additive }
  # Host surfaces: `ipc` and the Tauri glue are only reachable once their
  # crate roots declare them, and serde/specta/tauri-specta are pinned once
  # in the root manifest (001 FR-004) before either crate manifest
  # references them.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/Cargo.toml", nature: additive }
  # `ErrorKind` is spec 009's, and §3.1 re-exports it as the wire kind rather
  # than mirroring it, which spec 009's own doc comment anticipates. Crossing
  # the boundary is what it needs the serde and specta derives for, so this
  # spec adds them to that one type. See D-1.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/machine.rs", nature: additive }
  # Spec 009 AC-3's dependency-budget test reads the crate manifest and
  # refuses a name outside a fixed list. Adding `specta` and `serde_json` to
  # the manifest means adding them to that list in the same change.
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/machine.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/lib.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/Cargo.toml", nature: additive }
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  # `specta` builds with the `paste` proc-macro, which carries an
  # `unmaintained` advisory. Spec 004 D-8 established that such an advisory is
  # ignored individually, with a reason, in this file. See D-7.
  - { spec: "001-workspace-layout", unit: "deny.toml", nature: additive }
constrains:
  - flavor: invariant-freeze
    unit: "crates/butler-core/src/ipc.rs"
    note: "Within an IPC_CONTRACT_VERSION major, changes are additive: new variants and optional fields only. Removing or retyping a field is a major bump and a spec amendment."
summary: >
  The typed seam between the Rust process and the webview. `UiEvent` (Rust →
  UI: runtime status, answer lifecycle, paced chunks, exclusion status,
  prompts for credentials or permissions) and `UiCommand` (UI → Rust: arm,
  disarm, ask now, dismiss, set interaction, read and update settings, store a
  secret, run the self-test) are defined once in `butler-core::ipc` as serde
  types, exposed as Tauri commands and events in the desktop crate, and
  exported to TypeScript with `tauri-specta` into a committed, generated
  bindings file whose regeneration CI verifies is a no-op. The contract is
  versioned and additive within a major.
---

# 011: IPC contract

## 1. Purpose

Tauri's IPC is untyped JSON at the boundary. Left there, the overlay and the
runtime drift apart silently. This spec makes the boundary a single Rust
module whose types are the truth, generates the TypeScript from it, and treats
the generated file the way the corpus treats `.derived/`: committed,
deterministic, and checked for freshness in CI. The contract is deliberately
narrow: the UI is a renderer of runtime state, not a second brain.

## 2. Territory

`crates/butler-core/src/ipc.rs` (the types, added to spec 009's crate),
`apps/desktop/src-tauri/src/commands.rs` and `events.rs` (the Tauri glue,
added to spec 004's crate), the `export-bindings` binary that writes
`apps/desktop/src/generated/bindings.ts` (added to spec 012's package). The
invariant-freeze on `ipc.rs` binds every future spec that extends it (013
adds the paced chunk event; 014 the settings DTOs).

## 3. Behavior

### 3.1 Types (`ipc.rs`)

```rust
pub const IPC_CONTRACT_VERSION: (u16, u16) = (1, 0);

#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UiEvent {
    RuntimeStatus { state: StateName, seq: u64, request: Option<u64>,
                    exclusion: ExclusionSummary, last_error: Option<ErrorKind>, armed_for_ticks: u64 },
    AnswerStarted { request: u64 },
    AnswerChunk   { request: u64, index: u32, text: String, is_last: bool },   // spec 013 pacing
    AnswerDone    { request: u64, stop: StopSummary },
    AnswerFailed  { request: u64, kind: ErrorKind },
    NeedsCredential { provider: String },
    NeedsPermission { permission: PermissionKind },
    BudgetExhausted { window: BudgetWindow },
    SelfTestResult { verdict: ExclusionSummary },
    SettingsUpdated { settings: SettingsView },                                  // spec 014
}

#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UiCommand {
    Arm, Disarm, AskNow, Dismiss,
    SetInteractive { on: bool },
    GetSettings, UpdateSettings { patch: SettingsPatch },
    StoreSecret { provider: String, secret: String },   // handled by the command, zeroized after
    RunSelfTest,
    GetContractVersion,
}
```

- Every payload is `Serialize + Deserialize + specta::Type`. No payload
  carries screen text, frames, or prompts. `AnswerChunk.text` is model output
  only.
- `ErrorKind` is a closed enum of kinds (`Capture`, `Ocr`, `Network`,
  `Provider`, `Credential`, `Budget`, `Internal`); never a message string.

### 3.2 Tauri glue

- `commands.rs` registers one `#[tauri::command]` per `UiCommand` variant via
  `tauri-specta`, each delegating to `AppState`/`Runtime` and returning a
  typed `Result<T, ErrorKind>`.
- `events.rs` provides `emit(app, UiEvent)` that serializes to the single
  event channel `butler://event` with the tagged payload, so the UI
  subscribes once and switches on `type`.
- `StoreSecret` MUST move the string into a `Secret` (010) and zeroize the
  IPC buffer; the command MUST NOT log its argument even at `trace`.
- Commands are allowed to the `overlay` window only (004 capabilities).

### 3.3 Generated bindings

- `cargo run -p butler-desktop --bin export-bindings` writes
  `apps/desktop/src/generated/bindings.ts` with a header `// GENERATED FROM
  crates/butler-core/src/ipc.rs (IPC v1.0); do not edit`. The output is
  deterministic (sorted, LF, trailing newline).
- CI (`ci.yml` `jobs.rust`) runs the exporter and fails on a non-empty `git
  diff -- apps/desktop/src/generated`.
- The UI (012) MUST import from `generated/bindings.ts` only; hand-written
  types for IPC payloads are forbidden by the `overlay-frontend` rule and an
  ESLint restricted-import rule.

### 3.4 Versioning

`GetContractVersion` returns `IPC_CONTRACT_VERSION`; the UI compares its
generated major at startup and shows a fatal "rebuild" panel on mismatch
(only possible in development). Additive changes bump minor; a removal or
retype bumps major and requires amending this spec (the `constrains` edge).

## 4. Functional requirements

- **FR-001.** Round-trip: every `UiEvent`/`UiCommand` variant serializes and
  deserializes through `serde_json` identically (exhaustive test generated
  from the enum).
- **FR-002.** The exporter output is byte-identical across two runs and
  across platforms.
- **FR-003.** No `UiEvent` payload type transitively contains `Frame`,
  `Recognized`, `RedactedText`, or `Secret` (a compile-time assertion via a
  sealed marker trait `IpcSafe` that those types do not implement).
- **FR-004.** `StoreSecret`'s argument does not appear in trace logs (test
  with a capturing subscriber).

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core ipc::` and `-p butler-desktop
  commands::` pass; the generated file is fresh in CI.
- **AC-2.** `spec-spine index render` shows `ipc.rs` with two owners (009 by
  crate floor and this spec by file, plus the constraint).

## 6. Out of scope

- The pacing algorithm that decides when `AnswerChunk` fires (013).
- The settings schema (014); this spec carries only the DTO shape.

## 7. Resolved decisions

- **D-1 (2026-09-07, `specta` in `butler-core`, and a stale sentence in spec
  009).** §3.1 requires every payload to be `Serialize + Deserialize +
  specta::Type`, so `specta` has to reach the crate the types live in. This
  spec's `extends` edge onto `crates/butler-core/Cargo.toml` is the authority
  for that, and its own frontmatter comment names the crate outright
  ("serde/specta/tauri-specta are pinned once in the root manifest before
  either crate manifest references them").

  **Spec 009 §2 says something narrower**: "Its only runtime dependencies are
  `serde` (for the IPC DTOs), `strsim` and `thiserror`." That sentence predates
  this spec's edge and is now one crate short. It is *not* amended here: this
  spec has no authority over 009's prose, and the refusal rule is explicit that
  a spec is never edited mid-build to ratify what another spec's code did.
  **Left for a maintainer**: 009 §2 wants `specta` added to that list, and
  `serde_json` to the dev half.

  What is not in question is 009 **AC-3**, the requirement that sentence exists
  to serve. `specta` and `serde_json` are pure Rust with no platform crate
  underneath, so `cargo tree -p butler-core --edges normal` still contains no
  `tokio`, `tauri`, `windows`, `objc2` or `xcap`, on either target. §8 checks
  it here as well as in 009.

  `ErrorKind` is re-exported rather than mirrored, because spec 009's doc
  comment on it says this spec does exactly that. That is why the derives land
  on that one type in `machine.rs` and nowhere else in the module.

- **D-2 (2026-09-07, the specta versions the toolchain allows).** specta 2 and
  tauri-specta 2 exist only as release candidates; the 1.x lines resolve
  against Tauri v1 and cannot be used. The three crates also move together:
  `tauri-specta` requires `specta` with `=`, and so does `specta-typescript`,
  so the trio is chosen as a unit.

  The newest trio that compiles on the toolchain spec 001 pins (1.92.0) is
  **specta 2.0.0-rc.22, specta-typescript 0.0.9, tauri-specta 2.0.0-rc.21**.
  rc.24 and rc.25 call `fmt::from_fn`, still unstable on 1.92, and fail with
  E0658. Raising the toolchain is spec 001's decision and is not taken here.
  All three are pinned with `=` rather than a caret, so an rc bump is always a
  deliberate, reviewed change. Review trigger: the stable 2.0 line, or rc.25
  behind a toolchain bump, whichever arrives first.

- **D-3 (2026-09-07, the variants this spec does not land yet).** §3.1 lists
  the whole target contract, and §2 already assigns parts of it elsewhere
  ("013 adds the paced chunk event; 014 the settings DTOs"). Three groups need
  a payload type another spec owns and are therefore **not** implemented here:

  | Variant | Needs | From |
  |---|---|---|
  | `UiEvent::AnswerChunk` | the pacing policy that emits it | 013 |
  | `UiEvent::SettingsUpdated` | `SettingsView` | 014 |
  | `UiCommand::GetSettings`, `UiCommand::UpdateSettings` | `SettingsPatch` | 014 |
  | `UiEvent::BudgetExhausted` | `BudgetWindow` | 010 |

  Each arrives as a **new variant**, which the `constrains` edge on `ipc.rs`
  permits within a major without a version bump, so none of them is a breaking
  change to what ships today. Inventing placeholder shapes for them now would
  be the opposite: 014 would have to retype `SettingsView`, and retyping is a
  major bump by that same edge.

  Everything whose payload can be built from spec 009's types is here:
  `StateName`, `ExclusionSummary`, `StopSummary` and `PermissionKind` are the
  wire projections, and `ErrorKind` is the re-export.

- **D-4 (2026-09-07, `u64` on the wire is a TypeScript `number`).** §3.1 types
  `seq`, `request` and `armed_for_ticks` as `u64`, and specta refuses to export
  a 64-bit integer without being told how the wire carries it, because the
  three answers are not interchangeable.

  It is a **number**. `serde_json` writes a JSON number and `JSON.parse` reads
  a double, so that is what the TypeScript should say; `bigint` would describe
  a runtime type the webview never receives, and `string` would contradict the
  Rust serialization. The cost is exactness above 2^53, which these three
  counters cannot reach: `seq` increments once per capture, and at spec 008's
  polling rate 2^53 captures is longer than the age of the earth.

- **D-5 (2026-09-07, a generator that was not deterministic, caught by
  FR-002's test).** `tauri_specta::Builder` keeps commands in a `Vec`, events
  and types in `BTreeMap`s, and **constants in a `HashMap`**. Two renders in
  the same process emitted `EVENT_CHANNEL` and `IPC_CONTRACT_VERSION` in
  different orders, because each `HashMap` is seeded independently.

  Left alone this would not have failed a test. It would have made CI's
  `git diff --exit-code` in §3.3 fail *at random*, on unrelated pull requests,
  which is the kind of flake that gets a gate deleted rather than fixed.

  Both constants are therefore written by the exporter itself, after the
  render, in a fixed order, still read from the Rust constants so each has one
  source. Nothing else in the render needed handling: commands keep insertion
  order and types are sorted. FR-002's test compares two renders in one process
  and is what found this.

  The exporter also bypasses `Builder::export`, which runs whatever code
  formatter it finds on the machine and would make the bytes depend on what
  happens to be installed. It calls `export_str` and does its own writing: LF
  endings, one trailing newline, §3.3's header.

- **D-6 (2026-09-07, FR-004 has two halves and one of them waits for 016).**
  FR-004 asks that `StoreSecret`'s argument not appear in trace logs, "test
  with a capturing subscriber". There is no logging framework in the process
  yet; spec 016 brings it. So there are no trace logs for a subscriber to
  capture, and the end-to-end test cannot be written.

  The half that can be built is the stronger one, and it is built: `Debug` is
  implemented by hand for `UiCommand` so the `StoreSecret` arm renders its
  secret as `[redacted]`, in both the normal and the alternate (`{:#?}`) form.
  Every tracing and logging macro formats its arguments through `Debug`, so
  this closes the class rather than asking each future call site to remember.
  A test holds both forms.

  **Owed to spec 016**: the capturing-subscriber test, asserting the same
  property end to end once there is something to capture.

- **D-7 (2026-09-07, the supply chain `specta` brings).** Adding `specta` made
  `cargo deny check` fail with RUSTSEC-2024-0436: `paste`, `unmaintained`.

  `paste` is a **proc-macro**. It expands macros during compilation and is
  never linked into the shipped binary, so nothing about the product's runtime
  surface changes; and the advisory records no vulnerability, only that the
  crate is no longer maintained. Spec 001 §3.1 sets the deny bar at
  `vulnerability`, so refusing it would be the configuration being stricter
  than the spec rather than the dependency being worse than the spec allows.
  That is spec 004 D-8's reasoning, applied to the same class of finding.

  It is listed individually in `deny.toml`, with its reason and its path, never
  by disabling the `unmaintained` class. Spec 004 §8 asserts the count of
  listed advisories, so that assertion moves from five to six in the same
  commit; 004 D-13 records the reading, which is the behaviour D-8 designed
  the check to force. Review trigger: drop it when `specta` moves off `paste`.

  `cargo deny` also refused `butler-core` as a **wildcard** dependency: a path
  dependency with no `version` reads as `*`, which spec 001 FR-004 bans. The
  version is now carried alongside the path. Neither crate is published, so
  that number is read by nothing else.

## 8. Verification

```verify:cli
# FR-001, FR-003, FR-004's Debug half, and the wire shape.
cargo test -p butler-core --locked ipc::
# FR-003's compile-fail half: RedactedText cannot satisfy IpcSafe. The doc
# tests carry a positive control, so a passing compile_fail block cannot be
# passing because the import path is wrong.
cargo test -p butler-core --locked --doc
# AC-1's second half, plus FR-002's determinism test.
cargo test -p butler-desktop --locked commands::
cargo test -p butler-desktop --locked events::
# §3.3: regenerating the bindings is a no-op. This is the same check CI runs;
# a dirty tree here means the committed file and the Rust have drifted.
cargo run -p butler-desktop --bin export-bindings --locked
git diff --exit-code -- apps/desktop/src/generated
# §3.3: the header names the source and forbids editing.
grep -q "GENERATED FROM crates/butler-core/src/ipc.rs" apps/desktop/src/generated/bindings.ts
# §3.2: one channel, and the generated bindings agree with the Rust constant.
grep -q 'EVENT_CHANNEL = "butler://event"' apps/desktop/src/generated/bindings.ts
# D-1: spec 009 AC-3 still holds with specta and serde_json in the manifest.
sh -c '! cargo tree -p butler-core --locked --edges normal | grep -Eq "tokio|tauri|windows|objc2|xcap"'
# §3.1: no payload carries screen content. The marker is sealed, so this is a
# compile-time property, but the sealing itself is what makes it one.
grep -q "trait IpcSafe: sealed::Sealed" crates/butler-core/src/ipc.rs
# D-5: the constants are emitted by the exporter, not by the HashMap-backed
# Builder::constant, which rendered them in a different order each run.
sh -c '! grep -q "\.constant(" apps/desktop/src-tauri/src/commands.rs'
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "011-ipc-contract" && exit 1 || exit 0'
```
