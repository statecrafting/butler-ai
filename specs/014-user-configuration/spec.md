---
id: "014-user-configuration"
title: "User configuration: a typed settings model, an atomic on-disk store, and the settings panel"
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
  - "011-ipc-contract"
  - "012-overlay-ui"
extends:
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/settings.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/tests/settings.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: module, id: "butler_core::settings" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::settings::Settings" }, nature: additive }
  - { spec: "009-pipeline-state-machine", unit: { kind: symbol, id: "butler_core::settings::SettingsPatch" }, nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/settings_store.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::settings_store::SettingsStore" }, nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/components/SettingsPanel.tsx", nature: additive }
  # Host surfaces: `settings` and `settings_store` are only reachable once
  # their crate roots declare them, `SettingsPanel` only renders once spec
  # 012's `App.tsx` mounts it, and serde is pinned once in the root manifest
  # (001 FR-004).
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/src/lib.rs", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/Cargo.toml", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/lib.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/Cargo.toml", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/App.tsx", nature: additive }
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  # The settings handle spec 004 §3.1 already names as belonging in `AppState`.
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/app_state.rs", nature: additive }
  # §3.3's two commands. `commands.rs` is spec 011's file; the DTOs they carry
  # are this spec's, on the `refines` edge over `ipc.rs`.
  - { spec: "011-ipc-contract", unit: "apps/desktop/src-tauri/src/commands.rs", nature: additive }
  # §3.4's "Reset to defaults" needs the defaults. The exporter emits them
  # from `Settings::default()` rather than letting the UI keep a second copy
  # (D-6).
  - { spec: "011-ipc-contract", unit: "apps/desktop/src-tauri/src/bin/export-bindings.rs", nature: additive }
  # The generated bindings, which this spec's DTOs and D-6's constant change.
  # The file is spec 011's output, so every spec that adds to the contract
  # declares the edge rather than the regeneration being waived at PR time.
  - { spec: "011-ipc-contract", unit: "apps/desktop/src/generated/bindings.ts", nature: additive }
  # The overlay's store gains the `SettingsUpdated` case and the client
  # re-exports the settings types; the store test covers the new case.
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/state/runtime.ts", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/ipc/client.ts", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/src/test/store.test.ts", nature: additive }
refines:
  - { aspect: "settings-dtos", unit: "crates/butler-core/src/ipc.rs" }
summary: >
  One typed `Settings` struct in `butler-core` is the only configuration
  surface: capture (monitor, interval, region), detection (threshold,
  stability), assistant (provider, model, effort, answer style, budget caps,
  endpoint override), pacing (words per minute), shortcuts, privacy toggles
  (redaction on, allow degraded mode, diagnostics level), and window geometry.
  Defaults are in code, validation is pure, the file is TOML in the platform
  config directory written atomically, secrets are never in it (010), and
  every change flows through one `UpdateSettings` command that validates,
  persists, applies, and broadcasts. No environment variables configure the
  product.
---

# 014: User configuration

## 1. Purpose

Every tunable named by another spec (capture interval, similarity threshold,
stability frames, provider and model, pacing rate, shortcuts, degraded-mode
consent, redaction) needs one home with one validation path. Scattering them
across crates would mean scattered defaults and invisible coupling; an
environment-variable surface would mean configuration that no UI can show.
This spec centralizes the model in core (pure, testable) and the storage in
the shell.

## 2. Territory

`crates/butler-core/src/settings.rs` and `tests/settings.rs` (added to 009's
crate); `apps/desktop/src-tauri/src/settings_store.rs` (added to 004's
crate); `apps/desktop/src/components/SettingsPanel.tsx` (added to 012's
package); the `SettingsView`/`SettingsPatch` DTOs in `ipc.rs` (refining 011's
contract on the `settings-dtos` aspect).

## 3. Behavior

### 3.1 The model (`settings.rs`)

```rust
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    pub schema: u16,                     // 1
    pub capture:   CaptureSettings,      // monitor: MonitorSelector, interval_ms: 2500 (1000..=10000), region: Option<RectPct>
    pub detection: DetectionSettings,    // threshold: 0.85 (0.5..=0.99), stability_frames: 2 (1..=5), max_compare_chars: 6000
    pub assistant: AssistantSettings,    // provider: "anthropic", model: "claude-opus-5", effort: Medium, answer_style: Short, max_output_tokens: 1024, endpoint_override: Option<HttpsUrl>, budget: BudgetSettings
    pub pacing:    PacingPolicy,         // spec 013
    pub shortcuts: ShortcutSettings,     // spec 004 §3.3, as accelerator strings
    pub privacy:   PrivacySettings,      // redaction_enabled: true, allow_degraded_mode: false, diagnostics_level: Minimal, region_only: false
    pub window:    WindowSettings,       // anchor: TopRight, width_px: 420, max_height_px: 600, opacity: 0.92
    pub ui:        UiSettings,           // theme: System, font_scale: 1.0
}
```

- `Settings::default()` is the documented default; `validate(&self) ->
  Result<(), Vec<SettingsError>>` checks every range above and that
  `endpoint_override`, if set, is `https://`.
- `SettingsPatch` is a partial (`Option` per field, recursively) applied by
  `Settings::apply(patch) -> Settings`, then validated; an invalid patch is
  rejected whole (no partial application).
- `SettingsView` (IPC) is `Settings` minus nothing: there are no secrets in
  it by construction.
- Migration: `schema` is read first; a lower schema is migrated by a pure
  `migrate(from: Value) -> Settings` chain; a higher schema is refused with a
  clear error and the app starts with defaults in memory, never overwriting
  the file.

### 3.2 The store (`settings_store.rs`)

- Location: `dirs::config_dir()/butler-ai/settings.toml`
  (`%APPDATA%\butler-ai\settings.toml`, `~/Library/Application Support/
  butler-ai/settings.toml`).
- Reads at startup; on parse error, backs the file up to `settings.toml.bad`
  and starts with defaults, surfacing a one-time UI notice.
- Writes atomically: serialize to `settings.toml.tmp`, `fsync`, rename.
  Serialization is canonical (sorted keys, LF) so the file diffs cleanly.
- File permissions `0600` on macOS; the default user ACL on Windows.
- Watches nothing: the app is the only writer; external edits take effect on
  next launch.

### 3.3 Flow

`UiCommand::UpdateSettings { patch }` → `apply` → `validate` → store `save`
→ `Runtime` receives `Event::SettingsChanged` → shortcuts re-registered if
changed (004) → `UiEvent::SettingsUpdated { settings }` to the UI. A failed
validation returns `ErrorKind::Internal` with the field list in the typed
error, and nothing is persisted.

### 3.4 Panel (`SettingsPanel.tsx`)

Sections mirroring the struct; every control bound to a patch; `Save` sends
one `UpdateSettings`; `Reset to defaults` sends the default view as a patch.
The credential entry lives in `CredentialPanel` (012), not here.

## 4. Functional requirements

- **FR-001.** `Settings::default().validate()` is `Ok`.
- **FR-002.** Every documented range is enforced: a table-driven test sets
  each field to below-min, min, max, above-max and asserts the verdicts.
- **FR-003.** `toml::to_string(&Settings::default())` is byte-stable (golden).
- **FR-004.** A `settings.toml` with an unknown key is rejected (`deny_unknown_
  fields`), backed up, and defaults are used.
- **FR-005.** `rg "std::env::var|env!" crates apps/desktop/src-tauri` returns
  nothing outside `build.rs`.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-core settings::` and `-p butler-desktop
  settings_store::` pass on all CI targets.
- **AC-2.** The defaults table in `docs/architecture.md` §Settings is
  generated from `Settings::default()` by a test and diffed (as for the state
  diagram in 009).

## 6. Out of scope

- Secrets (010). Profiles or per-app settings. Cloud sync.

## 7. Resolved decisions

- **D-1 (2026-09-07, `PacingSettings` stands where §3.1 names
  `PacingPolicy`).** §3.1 types the pacing field as spec 013's
  `PacingPolicy`, which is phase 4 and does not exist. The summary says what
  the *user-facing* setting is, "pacing (words per minute)", and that is what
  `PacingSettings` carries.

  The two are different concerns and the split is worth keeping: this struct
  is the tunable the panel shows and the file stores, and 013's policy is the
  algorithm that reads it. When 013 lands it either constructs its policy from
  this or absorbs it, and that is 013's call, not this spec's.

- **D-2 (2026-09-07, FR-002's table found a flaw in itself).** The range table
  drives every field through `f64` so one row shape covers all of them. For
  `detection.stability_frames`, range 1..=5, the computed step was 0.04, so
  "above max" was 5.04, which is **5** once cast to `u8`. The row then asserted
  that a valid value must be rejected, and failed.

  A row per integer type would have hidden it. Instead each row now declares
  whether its field is integral, and an integral field steps by one. Recorded
  because the failure looked like a validator bug and was a test bug, and the
  next person to widen this table needs to know which it was.

- **D-3 (2026-09-07, spec 012's exhaustiveness claim was not true).** Spec 012
  D-5 says the overlay's store has "no `default` arm and returns `void`, so
  013's `AnswerChunk` and 014's `SettingsUpdated` will fail to typecheck until
  each is handled". **That is wrong.** TypeScript does not error on an
  unhandled case in a `void`-returning `switch`; it falls through.

  Adding `SettingsUpdated` typechecked cleanly against a store that ignored
  it, which is exactly the silence the generated union exists to prevent. The
  store now has a `default` arm that assigns the event to `never`, which is
  what actually makes the compiler check every case. Verified by deleting a
  handled case and watching `tsc` report
  `Type '{ type: "self-test-result"; ... }' is not assignable to type 'never'`.

  Spec 012's D-5 is left as written: it records what that branch believed, and
  this entry records what turned out to be true. Amending it retroactively
  would hide that the guard was absent for one commit.

- **D-4 (2026-09-07, the typed field list is not on the wire yet).** §3.3 says
  a failed validation "returns `ErrorKind::Internal` with the field list in
  the typed error". `ErrorKind` is a **closed** enum by spec 011 §3.1 and
  carries no payload, so there is nowhere in the current contract to put the
  list.

  `update_settings` therefore returns `ErrorKind::Internal` and the panel says
  "Rejected; nothing was changed", which is true and actionable but less
  precise than §3.3 wants. Widening the error type is spec 011's call: it owns
  `ErrorKind` and its `constrains` edge makes a retype a major bump. `Vec<SettingsError>`
  already exists, derives the wire traits, and is `IpcSafe`, so the change is
  a small one when 011 makes it. **Owed to spec 011.**

- **D-5 (2026-09-07, FR-005 and the exporter).** FR-005 is checked by grep:
  `std::env::var` and `env!` must appear nowhere under `crates/` or
  `apps/desktop/src-tauri` except `build.rs`. Spec 011's `export-bindings`
  binary used the compile-time manifest-directory macro to find its output,
  which the grep catches.

  A compile-time path is not an environment-variable *surface*, so an
  exemption would have been defensible. It was not taken: a rule with an
  exception is a rule nobody can check, and the exemption would have had to
  live in FR-005, which is this spec's requirement to satisfy rather than to
  soften. The exporter now walks up from the working directory to find the
  workspace root, which uses no environment read at all and is the more robust
  of the two: it works from any directory in the tree, where the previous
  relative path worked from exactly one.

- **D-6 (2026-09-07, `DEFAULT_SETTINGS` is generated, not written).** §3.4's
  "Reset to defaults" needs the defaults, and the UI had no source for them:
  `GetSettings` returns the *current* configuration. Hand-writing them in
  TypeScript would have been a second copy of values that live in
  `settings.rs`, and the first divergence would have quietly reset a user's
  configuration to something nobody chose.

  The exporter emits `DEFAULT_SETTINGS` from `Settings::default()` into
  `bindings.ts`, beside the two constants spec 011 D-5 already writes there.
  It is a constant, not a command, so the IPC contract is unchanged and no
  version bump is involved.

- **D-7 (2026-09-07, boxing the settings payloads, and the `Eq` that went with
  them).** `Settings` is an order of magnitude larger than any other IPC
  payload, and an enum is as large as its largest variant, so every `UiEvent`
  value in the process would have carried that size. `clippy::large_enum_variant`
  said so. Both settings payloads are boxed; serde and specta see through a
  `Box`, so the wire format and the generated TypeScript are byte-identical
  (checked: `updateSettings(patch: SettingsPatch)` and
  `{ type: "settings-updated"; settings: Settings }` are unchanged).

  `UiEvent` and `UiCommand` also lost their `Eq` derives, because `Settings`
  carries `f64` fields (opacity, font scale, the similarity threshold) and
  floats have no total equality. Nothing needed `Eq`: the tests compare with
  `assert_eq!`, which does not.

## 8. Verification

```verify:cli
# AC-1 and FR-001, FR-002, FR-004's model half, plus the patch semantics.
cargo test -p butler-core --locked --test settings
# AC-1's second half and FR-003's golden: the store, its atomic write, its
# recovery from an unreadable file, and 0600.
cargo test -p butler-desktop --locked settings_store
# AC-2: docs/architecture.md §8 is generated from `Settings::default()` and
# diffed. The test above includes it; this names it so a failure is legible.
cargo test -p butler-core --locked --test settings ac_002
# FR-005: no environment surface anywhere in the product, `build.rs` aside.
sh -c '! grep -rnE "std::env::var|env!\(" crates apps/desktop/src-tauri --include="*.rs" | grep -v "src-tauri/build.rs"'
# §3.3: the two commands reach the overlay, and D-6's defaults with them.
grep -q "async getSettings" apps/desktop/src/generated/bindings.ts
grep -q "async updateSettings" apps/desktop/src/generated/bindings.ts
grep -q "DEFAULT_SETTINGS" apps/desktop/src/generated/bindings.ts
# D-3: the overlay's store checks exhaustiveness with an assignment to
# `never`. Without it a new contract variant is silently ignored.
grep -q "const unhandled: never = event" apps/desktop/src/state/runtime.ts
# §3.4 and AC-1: the panel typechecks, lints and its store case is tested.
pnpm --filter @butler-ai/desktop typecheck
pnpm --filter @butler-ai/desktop lint
pnpm --filter @butler-ai/desktop test
# §3.2: the write is atomic. A plain write would leave a truncated file that
# the next launch cannot parse, which is how a configuration is lost.
grep -q "fs::rename" apps/desktop/src-tauri/src/settings_store.rs
grep -q "sync_all" apps/desktop/src-tauri/src/settings_store.rs
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "014-user-configuration" && exit 1 || exit 0'
```
