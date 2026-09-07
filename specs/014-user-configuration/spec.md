---
id: "014-user-configuration"
title: "User configuration: a typed settings model, an atomic on-disk store, and the settings panel"
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
