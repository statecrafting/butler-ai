---
id: "016-diagnostics-and-logging"
title: "Diagnostics and logging: structured, content-free tracing and a user-initiated diagnostics bundle"
status: draft
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: medium
platforms: "all"
phase: 2
depends_on:
  - "004-desktop-shell"
  - "015-privacy-boundary"
extends:
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/logging.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/diagnostics.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::logging::init" }, nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::diagnostics::DiagnosticsBundle" }, nature: additive }
summary: >
  Operational visibility without content: `tracing` with a rolling file
  appender in the platform log directory, level from settings (default
  `info`; `trace` records every state transition by id), structured fields
  restricted to a closed set of key names, and a diagnostics bundle (recent
  log, settings, versions, exclusion status, platform info) that the user
  builds from the tray and saves where they choose. The privacy boundary (015)
  constrains `logging.rs`; this spec makes the constraint operational with a
  field allowlist enforced at compile time.
---

# 016: Diagnostics and logging

## 1. Purpose

A stealthy overlay that misbehaves is hard to debug by design: there is little
UI and the interesting state is inside a state machine. Structured tracing of
transitions and effect outcomes, by id, gives enough to reconstruct a failure
without ever recording what the user was looking at.

## 2. Territory

`apps/desktop/src-tauri/src/logging.rs` and `diagnostics.rs` (added to 004's
crate). `logging.rs` is constrained by 015.

## 3. Behavior

### 3.1 Logging (`logging::init(settings) -> Result<Guard>`)

- `tracing-subscriber` with a JSON-lines rolling file appender (daily, keep
  7) in `dirs::data_local_dir()/butler-ai/logs/`, plus stderr in debug builds.
- Level from `settings.privacy.diagnostics_level`: `Minimal` = `warn`,
  `Normal` = `info`, `Verbose` = `trace`. The runtime's transition log is at
  `trace`.
- **Field allowlist.** A `butler_desktop::logging::field!` macro accepts only
  keys from a closed list (`state`, `event`, `seq`, `request`, `kind`,
  `duration_ms`, `count`, `provider`, `model`, `monitor`, `status`, `code`);
  any other key is a compile error. Free-form messages are `&'static str`.
  This is how 015 FR-004 is made structural rather than hopeful.
- Panics are logged (kind and location) via a panic hook; no backtrace with
  values.

### 3.2 Diagnostics bundle

`DiagnosticsBundle::collect(state) -> Bundle` gathers: app version and git
sha, OS version, monitor list (names, bounds, scales), exclusion status, the
last 500 log lines, `Settings` (canonical TOML), the last 50 machine
transitions from an in-memory ring buffer, and `spec` of the running build
(the commit's spec ids, for support triage). `Bundle::write(path)` writes a
single `.zip`. The tray action opens a save dialog: this is the one place a
`dialog` plugin grant is needed, and it is scoped to `save` only (an
amendment to 015 §Constraints records it when implemented).

## 4. Functional requirements

- **FR-001.** `field!("screen_text", …)` fails to compile (trybuild).
- **FR-002.** With `Verbose`, a full mock cycle produces one `trace` line per
  transition with `state`, `event`, `seq`.
- **FR-003.** The bundle contains no file other than `log.jsonl`,
  `settings.toml`, `transitions.json`, `system.json`.
- **FR-004.** Log rotation keeps at most seven files.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-desktop logging::` passes on all CI targets.
- **AC-2.** 015 FR-004's whole-pipeline log assertion passes with this
  subscriber.

## 6. Out of scope

- Remote telemetry, crash upload, analytics: none (015 §3.3).
