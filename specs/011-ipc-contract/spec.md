---
id: "011-ipc-contract"
title: "IPC contract: typed commands and events between the Rust runtime and the overlay, with generated bindings"
status: approved
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: pending
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
