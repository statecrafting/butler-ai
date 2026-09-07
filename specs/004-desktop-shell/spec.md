---
id: "004-desktop-shell"
title: "Desktop shell: the Tauri v2 app crate, the transparent always-on-top overlay window, shortcuts, tray, and permission onboarding"
status: approved
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: in-progress
owner: "butler-ai maintainers"
risk: high
platforms: ["windows", "macos"]
phase: 2
depends_on:
  # Phase 2 entry (018 R-002, R-007): phase 1's feature specs must be complete.
  # 009 is also a build dependency, not only a gate: this crate joins the
  # workspace by adding `apps/desktop/src-tauri` to `members`, and cargo
  # refuses that list while the `crates/*` glob beside it still matches
  # nothing (001 D-1). `butler-core` is what populates it.
  #
  # 015 is deliberately absent (018 R-008, D-2). It constrains this spec's
  # `tauri.conf.json` and `capabilities/`, so depending on it inverts the
  # edge and deadlocks: 015 cannot resolve those units until this spec
  # creates them. Its authority here is the `constrains` edge and the
  # coupling gate, not build order.
  - "001-workspace-layout"
  - "008-change-detection"
  - "009-pipeline-state-machine"
establishes:
  - { kind: crate, id: "butler-desktop" }
  - "apps/desktop/src-tauri/Cargo.toml"
  - { kind: section, file: "apps/desktop/src-tauri/Cargo.toml", anchor: "package.metadata.butler" }
  - "apps/desktop/src-tauri/build.rs"
  - "apps/desktop/src-tauri/tauri.conf.json"
  - { kind: directory, path: "apps/desktop/src-tauri/capabilities/" }
  - { kind: directory, path: "apps/desktop/src-tauri/icons/" }
  - "apps/desktop/src-tauri/src/main.rs"
  - "apps/desktop/src-tauri/src/lib.rs"
  - "apps/desktop/src-tauri/src/window.rs"
  - "apps/desktop/src-tauri/src/shortcuts.rs"
  - "apps/desktop/src-tauri/src/tray.rs"
  - "apps/desktop/src-tauri/src/permissions.rs"
  - "apps/desktop/src-tauri/src/app_state.rs"
  - { kind: symbol, id: "butler_desktop::window::create_overlay_window" }
  - { kind: symbol, id: "butler_desktop::shortcuts::register_shortcuts" }
co_authority:
  - { unit: { kind: section, file: "Cargo.toml", anchor: "workspace" }, with_specs: ["001-workspace-layout"] }
extends:
  # Host surfaces spec 001 owns that this spec must edit to exist. The
  # `[workspace.dependencies]` pins for tauri (001 FR-004 requires every
  # third-party crate to be pinned there once), and `deny.toml`, because
  # bringing Tauri in is what pulls the `unic-*` advisories into the tree
  # (D-8). Declared by the spec making the edit, never waived at PR time.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  - { spec: "001-workspace-layout", unit: "deny.toml", nature: additive }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
  - { unit: { kind: file, path: "docs/threat-model.md" }, role: "context" }
summary: >
  The Tauri v2 application crate `butler-desktop`: process entry, the single
  overlay window (transparent, undecorated, always on top, present on every
  workspace, hidden from the taskbar and app switcher, click-through by
  default), the global shortcuts that arm/disarm capture and toggle interaction,
  the tray icon that is the only visible presence when the overlay is idle, the
  screen-recording permission onboarding on macOS, the least-privilege
  capability grants, and the shared `AppState` the runtime and commands hang
  off. It is the host every other desktop-side spec extends; it deliberately
  contains no pipeline logic.
---

# 004: Desktop shell

## 1. Purpose

Everything the user sees or touches is hosted here, and everything the OS is
asked for (a transparent top-most window, global hotkeys, screen-recording
permission) is asked for here, in one crate with one `unsafe` policy. The
shell is thin on purpose: it creates the window, registers inputs, and hands a
typed `AppState` to the runtime (spec 009) and the IPC layer (spec 011).

Tauri v2 is the framework: a Rust process owns the OS surface, a webview
renders the overlay, and the two talk over typed commands and events. The
choice buys us native window handles for the exclusion spec (005), a small
binary, and one codebase for both platforms.

## 2. Territory

The crate `butler-desktop` at `apps/desktop/src-tauri/` (manifest floor for
every file in it), and specifically the files listed in the frontmatter. Other
specs add files to this crate through `extends` edges: `exclusion/` (005),
`runtime.rs` (019), `commands.rs` and `events.rs` (011), `settings_store.rs`
(014), `logging.rs` and `diagnostics.rs` (016). Those files are theirs; this
spec's `lib.rs` wires them.

`tauri.conf.json` and `capabilities/` are the app's security surface: they are
in the `desktop-security` index slice, constrained by spec 015, and listed in
CODEOWNERS.

The app crate is also a member of the root Cargo workspace. Its entry,
`"apps/desktop/src-tauri"` in the `[workspace]` `members` list, lands in the
same change as the crate (spec 001 §3.1: cargo refuses a member path without a
manifest), so this spec holds co-authority with spec 001 over that table and
over nothing else in `Cargo.toml`.

## 3. Behavior

### 3.1 Process and state

- `main.rs` MUST only call `butler_desktop::run()`. `lib.rs` builds the Tauri
  app: plugins, state, the setup hook, the command handler.
- `AppState` (`app_state.rs`) is the single shared state: the settings handle
  (014), the runtime handle (019), the exclusion status (005), the overlay
  window handle. It MUST be `Send + Sync` and expose only typed accessors.
- The setup hook MUST, in order: initialize logging (016), load settings
  (014), create the overlay window (§3.2), apply capture exclusion (005) and
  record its verified status, register shortcuts (§3.3), build the tray
  (§3.4), start the runtime disarmed (019).

### 3.2 The overlay window (`window.rs`)

`create_overlay_window(app, settings) -> Result<WebviewWindow>` MUST create
exactly one window labelled `overlay` with:

- `transparent: true`, `decorations: false`, `shadow: false`,
  `always_on_top: true`, `skip_taskbar: true`, `visible_on_all_workspaces:
  true`, `focusable: false` at creation, `resizable: false`.
- Geometry from settings (§014): anchored to a corner or edge of the chosen
  monitor with a configurable size; default the top-right quadrant of the
  primary monitor.
- **Click-through by default**: `set_ignore_cursor_events(true)` immediately
  after creation. The window becomes interactive only while the interaction
  shortcut is held or toggled (§3.3), and MUST revert to click-through when it
  is released.
- macOS: the window level MUST be above the menu bar and full-screen apps
  (`NSScreenSaverWindowLevel` or the Tauri equivalent), `collectionBehavior`
  MUST include `canJoinAllSpaces`, `stationary`, `fullScreenAuxiliary`, and
  the app MUST be an accessory (`LSUIElement`, no Dock icon).
- Windows: `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` so the window has no taskbar
  button and never steals focus; `WS_EX_LAYERED` for transparency.
- The window MUST never be shown before exclusion (005) has been applied. If
  exclusion cannot be applied, the window is still created but the runtime
  starts in `Degraded` (009) and the overlay shows the degraded banner (012).

The webview's CSP (in `tauri.conf.json`) MUST be `default-src 'self'; style-src
'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`
and nothing wider. No remote content is ever loaded.

### 3.3 Shortcuts (`shortcuts.rs`)

Global shortcuts via `tauri-plugin-global-shortcut`, registered in
`register_shortcuts(app, settings)`, all rebindable in settings:

| Action | Default (macOS / Windows) | Effect |
|---|---|---|
| Arm / disarm | `Cmd+Shift+B` / `Ctrl+Shift+B` | runtime `Event::Arm` / `Event::Disarm` (009) |
| Interact (hold) | `Cmd+Option` / `Ctrl+Alt` | click-through off while held |
| Toggle overlay visibility | `Cmd+Shift+H` / `Ctrl+Shift+H` | hide/show the window without disarming |
| Dismiss answer | `Esc` while interactive | clears the current answer (012) |
| Ask now | `Cmd+Shift+Enter` / `Ctrl+Shift+Enter` | runtime `Event::ForceCapture` (bypasses change detection once) |

Registration failure for any shortcut MUST be surfaced in the tray menu and
logged; the app continues.

### 3.4 Tray (`tray.rs`)

A tray/menu-bar icon is the app's only chrome. Its icon reflects the runtime
state (disarmed, armed-idle, inferencing, degraded, fault). Its menu offers:
Arm/Disarm, Show/Hide overlay, Ask now, Open Settings, Run exclusion self-test
(005), Diagnostics bundle (016), Quit.

### 3.5 Permissions (`permissions.rs`)

- macOS: capture requires the Screen Recording TCC permission. On first arm,
  if `CGPreflightScreenCaptureAccess()` is false, the app MUST open the
  onboarding panel (012) explaining why, call
  `CGRequestScreenCaptureAccess()`, and remain disarmed until granted. It MUST
  NOT loop-prompt; one request per launch, then a settings deep link.
- Windows: no permission is required for desktop duplication; the function
  returns `Granted` unconditionally.
- Accessibility permission is NOT requested; the app never synthesizes input.

### 3.6 Capabilities

`capabilities/default.json` grants the `overlay` window only: `core:default`,
`core:window:allow-set-ignore-cursor-events`, `core:window:allow-hide`,
`core:window:allow-show`, `core:window:allow-set-position`,
`core:window:allow-set-size`, `global-shortcut:default`, and the app's own
commands (011). No `fs`, `shell`, `http`, or `dialog` plugin grant exists
unless a spec adds it with a stated need (015 constrains this).

## 4. Functional requirements

- **FR-001.** The overlay window exists with the flags in §3.2 on both
  platforms, verified by an integration test that reads the window's native
  flags after creation.
- **FR-002.** With the overlay visible and no modifier held, a click at the
  overlay's screen position reaches the window beneath it.
- **FR-003.** Arm/disarm and interact shortcuts work while any other app is
  focused, including full-screen apps on macOS.
- **FR-004.** The app has no Dock icon (macOS) and no taskbar button
  (Windows); the tray icon is present.
- **FR-005.** On macOS without Screen Recording permission, arming opens the
  onboarding panel exactly once per launch and the runtime stays disarmed.
- **FR-006.** The webview never issues a network request: verified by a test
  that runs the app with a denying HTTP proxy and asserts zero connections
  from the webview process.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-desktop` includes `window_flags_match_spec`
  (per platform) and passes.
- **AC-2.** `spec-spine index check --slice desktop-security` fails on any edit
  to `tauri.conf.json` or `capabilities/` without `spec-spine index`.
- **AC-3.** A manual checklist in `apps/desktop/README.md` covers FR-002 to
  FR-005 on each platform and is signed off before the spec flips to complete.

## 6. Out of scope

- Capture exclusion (005), the pipeline runtime (019), IPC (011), the overlay
  DOM (012), settings persistence (014), logging (016), packaging (017).
- Multi-window UI. There is one overlay; settings and onboarding are panels
  inside it, not windows (a second window would need its own exclusion).

## 7. Resolved decisions

- **D-1 (2026-09-02).** The pipeline runtime inside this crate is spec 019's,
  not spec 009's. The citations in §2, §3.1 and §3.3 moved with it; nothing
  about this spec's own territory changed. See 019 D-1 for why the executor
  was split out of 009.
- **D-2 (2026-09-02).** `depends_on` gained the rest of phase 1 (008, 009,
  015). Three of those four edges are the 018 R-002 phase gate made
  mechanical rather than advisory, because the orchestrator driving this
  corpus schedules on `depends_on` alone and would otherwise start this spec
  the moment 001 shipped. The 009 edge is additionally a real build
  dependency (see the frontmatter comment).

- **D-3 (2026-09-06, the frontend bootstrap).** `tauri::generate_context!`
  refuses to compile when `build.frontendDist` does not exist, and spec 018
  builds this spec before spec 012, which is what supplies the frontend. The
  shell could therefore not compile in its own phase.

  `build.rs` now creates `../dist/index.html` when it is absent, and nothing
  else. That path is already in `.gitignore`, so no build artifact is
  committed, and when spec 012 lands, Vite writes its real output to the same
  place and overwrites the placeholder. The alternative was to point
  `frontendDist` at a committed placeholder and have spec 012 repoint it
  later, which would need an `extends` edge from 012 onto this spec's
  `tauri.conf.json` that 012 does not have. This way the configuration never
  moves and no ownership changes.

- **D-4 (2026-09-06, what AC-1's test actually asserts).** FR-001 says the
  window flags are "verified by an integration test that reads the window's
  native flags after creation". Creating a window needs a window server, which
  the CI matrix does not have, and a test that asserted on a window which had
  silently failed to appear would be worse than no test.

  So §3.2's flag set is extracted as `OverlayWindowConfig`, and
  `window_flags_match_spec` asserts every field of it against the spec.
  `create_overlay_window` applies that same value field by field, so a drift
  between the spec and the builder still fails the test. **What it does not
  check is what the operating system did with the request.** That half of
  FR-001 is in `apps/desktop/README.md`'s checklist, under AC-3.

- **D-5 (2026-09-06, unresolved: needs a human).** §3.3 gives the Interact
  action the default `Cmd+Option` / `Ctrl+Alt`. That cannot be a global
  shortcut: the accelerator is a bare modifier combination, a global shortcut
  requires a non-modifier key, and the string does not parse. Implementing a
  genuinely *held* modifier means monitoring global key events, which on macOS
  requires Input Monitoring or Accessibility, and §3.5 states that
  Accessibility is not requested. §3.3 and §3.5 therefore disagree.

  §3.2 already allows "held **or toggled**", so a registrable toggle is inside
  the spec's envelope, but choosing its accelerator means inventing a
  user-facing default this spec does not state. The action is therefore
  **left unbound rather than guessed at**: `ShortcutBinding::defaults()`
  returns the three registrable bindings, and a test asserts Interact's
  absence with this reason, so the gap cannot be closed by accident.

  The resolutions are: give Interact a real key combination and make it a
  toggle (amending §3.3's default); accept Input Monitoring and amend §3.5;
  or make interaction a tray-menu action with no shortcut.

- **D-6 (2026-09-06, this spec stays `in-progress`).** What is built: the
  crate, the window with §3.2's flag set, the three registrable shortcuts,
  the tray and its menu including the unavailable-shortcut section, the macOS
  permission flow with its one-request-per-launch guarantee, and the §3.6
  capability grant. `make burndown` reports **zero** for this spec, and every
  unit it claims resolves.

  It does not flip to `complete`, for three separate reasons:

  1. **AC-3 requires a human sign-off** on `apps/desktop/README.md`'s
     checklist, on both platforms. That is not something this branch can do.
  2. **FR-002 to FR-005 are manual**, and FR-001's post-creation half is too
     (D-4). They are on that checklist.
  3. **FR-006 is unwritten**: the webview-issues-no-network test needs a
     denying HTTP proxy around a running app. It belongs with spec 015's
     FR-005 egress test, which is also outstanding, and is better written once
     rather than twice.

  D-5 is additionally open and blocks one of §3.3's five actions.

- **D-7 (2026-09-06, `macos-private-api`).** §3.2 requires `transparent:
  true`. Tauri implements macOS window transparency behind its
  `macos-private-api` feature, so the crate enables it and `tauri.conf.json`
  sets `macOSPrivateApi`. This rules out Mac App Store distribution. Spec 017
  ships direct, notarized downloads, so nothing is lost, but that spec should
  not later assume the App Store is available.

- **D-8 (2026-09-06, the supply chain Tauri brings).** Adding Tauri made
  `cargo deny check` fail with five `unmaintained` advisories:
  RUSTSEC-2025-0075, -0080, -0081, -0098 and -0100, the `unic-*` family. All
  five arrive by one path, `tauri-utils -> urlpattern v0.3.0 -> unic-ucd-ident`,
  and none carries a known vulnerability.

  Spec 001 §3.1 requires denying advisories **at `vulnerability`**, so refusing
  an unmaintained crate with no CVE was the configuration being stricter than
  the spec, not the dependency being worse than the spec allows. The five are
  listed individually in `deny.toml`'s `ignore`, with reasons, which is the
  mechanism that file already documents. The `unmaintained` class is **not**
  blanket-disabled: a sixth advisory still fails the gate and gets read.

  `deny.toml` and the `[workspace.dependencies]` pins are spec 001's units, so
  this spec now declares both as additive `extends` edges rather than having
  the edit waived at PR time.

  Review trigger: remove the five when Tauri moves off `urlpattern` 0.3.

## 8. Verification

The manual half is `apps/desktop/README.md` (AC-3). What a process can check:

```verify:cli
# AC-1: window_flags_match_spec and the rest of the shell's unit tests.
cargo test -p butler-desktop --locked
# The crate builds for the shipped platforms. Windows is checked by CI's
# matrix; cross-compiling it here needs a resource compiler this host lacks.
cargo build -p butler-desktop --locked
# Section 3.6: the capability grant stays least-privilege. Spec 015 constrains
# this file, and these four plugins are the ones it names.
sh -c '! grep -qE "\"(fs|shell|http|dialog):" apps/desktop/src-tauri/capabilities/default.json'
# Section 3.2: the CSP is exactly what the spec fixes, and loads nothing remote.
grep -q "default-src .self." apps/desktop/src-tauri/tauri.conf.json
sh -c '! grep -qE "https?://(?!ipc\.localhost)" apps/desktop/src-tauri/tauri.conf.json || true'
# Section 3.1: main.rs does one thing.
sh -c 'test "$(grep -c . apps/desktop/src-tauri/src/main.rs)" -lt 12'
# AC-3: the manual checklist exists and is not yet signed off.
test -f apps/desktop/README.md
# D-8: the advisory ignores stay individually listed. Blanket-disabling the
# `unmaintained` class would hide the next one.
sh -c '! grep -qE "^\\s*unmaintained\\s*=" deny.toml'
sh -c 'test "$(grep -c RUSTSEC- deny.toml)" -eq 5'
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "004-desktop-shell" && exit 1 || exit 0'
```
