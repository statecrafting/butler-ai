---
id: "016-diagnostics-and-logging"
title: "Diagnostics and logging: structured, content-free tracing and a user-initiated diagnostics bundle"
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
  # 015 is deliberately absent (018 R-008, D-2): it constrains this spec's
  # `logging.rs`, so the edge would invert and deadlock. The "logs carry no
  # content" invariant reaches this spec as a `constrains` edge.
  - "004-desktop-shell"
extends:
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/logging.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/diagnostics.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::logging::init" }, nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::diagnostics::DiagnosticsBundle" }, nature: additive }
  # Host surfaces in spec 004's crate: `logging` and `diagnostics` are only
  # reachable once `lib.rs` declares them, and tracing/tracing-subscriber are
  # pinned once in the root manifest (001 FR-004) before the app manifest
  # references them.
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/lib.rs", nature: additive }
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/Cargo.toml", nature: additive }
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  # §3.2's tray action. The menu item is spec 004's and already exists; this
  # spec is what makes it do something.
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/src/tray.rs", nature: additive }
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

## 7. Resolved decisions

- **D-1 (2026-09-07, the compile-fail harness FR-001 names).** FR-001 says
  `field!("screen_text", …)` must fail to compile, "(trybuild)". The
  requirement is the compile failure; `trybuild` is a tool named in passing.

  It is a `compile_fail` doctest instead, with a passing positive control
  beside it so the block cannot be green because an import path is wrong.
  `trybuild` asserts the *exact stderr* against a committed golden, which
  drifts with every rustc release and would have to be regenerated on a
  matrix of two platforms; that is a maintenance cost and a flake source for
  a property a doctest holds exactly as well. The corpus already uses
  `compile_fail` doctests for the same job in spec 015's `RedactedText` and
  spec 011's `IpcSafe`, so this is the established shape rather than a new one.

- **D-2 (2026-09-07, a check that matched its own explanation).** The test
  asserting `env-filter` is not enabled, so `RUST_LOG` cannot configure the
  product, first grepped the whole workspace manifest. It failed, because the
  manifest **comment** explaining why the feature is off contains the word.

  This is the third time in this corpus that a grep-shaped check has matched
  the prose written to explain it (spec 011 D-5's constants, spec 012's two
  verification commands). The test now reads the `tracing-subscriber`
  dependency line specifically and asserts both that the feature is absent
  and that `default-features = false`, so a default cannot add it back. The
  general lesson is worth stating once: a check written as a substring search
  over a file that also documents the check will pass or fail on the
  documentation.

- **D-3 (2026-09-07, AC-2's whole-pipeline half is owed to phase 4).** AC-2
  requires spec 015 FR-004's assertion to pass "with this subscriber": a
  whole-pipeline test with mock traits and a capturing subscriber, asserting
  no log line contains the mock screen text, answer or secret. Those mocks are
  specs 006, 007 and 010, and the runtime that drives them is 019. None
  exists.

  What is built is the half that can be: a capturing-subscriber test that
  exercises every logging call this module offers, with screen text, an
  answer and a secret in scope, and asserts none of them reaches a line while
  the ids do (so it cannot pass vacuously). That is the property the
  whole-pipeline test would be checking; what it cannot yet check is that no
  *other* module logs something it should not, because the other modules do
  not exist.

  **Owed to phase 4**, with spec 015 FR-004, which is outstanding for the same
  reason.

- **D-4 (2026-09-07, no save dialog, and why the bundle still lands
  somewhere).** §3.2 says the tray action "opens a save dialog", and adds
  that this is "the one place a `dialog` plugin grant is needed... an
  amendment to 015 §Constraints records it when implemented".

  Spec 015's `constrains` edge on `capabilities/` reads: "No fs, shell, http,
  or dialog grants without a spec that amends this one." Amending 015 is a
  human act with a privacy-critical spec's authority behind it, and not
  something this branch takes on its own. So **no grant was added**.

  The bundle is written to `butler-diagnostics.zip` in the log directory and
  its path is reported. The user gets the artefact; what they do not get is
  choosing where it lands, which is a smaller loss than a capability grant
  nobody reviewed. **Owed**: the 015 amendment, then the `dialog:allow-save`
  grant and the dialog itself.

- **D-5 (2026-09-07, logging starts before settings are read).** §3.1 takes
  the level from `settings.privacy.diagnostics_level`, and spec 004 §3.1
  orders the setup hook logging-first, settings-second. Both cannot hold at
  once for the very first read.

  Logging starts at the shipped default, `Minimal`. The alternative was to
  read settings first, which would make the settings read itself the one
  event in the process that nothing could observe, and that read is exactly
  where a corrupt file is discovered. A level change therefore takes effect
  on the next launch, which is also true of the file's other consumers (§3.2:
  external edits take effect on next launch).

## 8. Verification

```verify:cli
# AC-1: the allowlist, the level table, rotation, and FR-002's capturing
# subscriber, plus D-3's "no content can reach a line".
cargo test -p butler-desktop --locked logging
# FR-003 and §3.2: exactly four members, the newest transitions, and a bundle
# that still writes when the log is missing.
cargo test -p butler-desktop --locked diagnostics
# FR-001: `field!` refuses a key outside the allowlist at compile time. The
# doctests carry a positive control, so a passing compile_fail block cannot be
# passing because the import path is wrong (D-1).
cargo test -p butler-desktop --locked --doc
# §3.1 and spec 014 FR-005: the level comes from settings, never from the
# environment. `env-filter` is the feature that would provide `RUST_LOG`.
sh -c 'grep "^tracing-subscriber = " Cargo.toml | grep -qv "env-filter"'
sh -c 'grep "^tracing-subscriber = " Cargo.toml | grep -q "default-features = false"'
# §3.1: the field allowlist is closed, and the macro is where it is enforced.
grep -q "pub const ALLOWED_FIELDS" apps/desktop/src-tauri/src/logging.rs
# D-4: no dialog grant was added. Spec 015 constrains this file and only a
# spec that amends it may widen the grant.
sh -c '! grep -q "dialog:" apps/desktop/src-tauri/capabilities/default.json'
# §3.2: the bundle's member list is a constant, so the writer and the test
# cannot disagree about what "and nothing else" means.
grep -q "pub const BUNDLE_MEMBERS" apps/desktop/src-tauri/src/diagnostics.rs
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "016-diagnostics-and-logging" && exit 1 || exit 0'
```
