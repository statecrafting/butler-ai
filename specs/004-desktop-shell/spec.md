---
id: "004-desktop-shell"
title: "Desktop shell: the Tauri v2 app crate, the transparent always-on-top overlay window, shortcuts, tray, and permission onboarding"
status: approved
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: complete
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
  shortcut's toggle is on (§3.3), and MUST revert to click-through when it is
  toggled off.
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
| Interact (toggle) | `Cmd+Shift+Space` / `Ctrl+Shift+Space` | click-through off until pressed again |
| Toggle overlay visibility | `Cmd+Shift+H` / `Ctrl+Shift+H` | hide/show the window without disarming |
| Dismiss answer | `Esc` while interactive | clears the current answer (012) |
| Ask now | `Cmd+Shift+Enter` / `Ctrl+Shift+Enter` | runtime `Event::ForceCapture` (bypasses change detection once) |

The Interact toggle MUST act on key **press** only. A global shortcut reports
both press and release, so acting on each would flip interactivity twice per
keystroke and leave it where it started.

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

- **D-5 (2026-09-06, resolved 2026-09-07 by D-9).** §3.3 gave the Interact
  action the default `Cmd+Option` / `Ctrl+Alt`. That cannot be a global
  shortcut: the accelerator is a bare modifier combination, a global shortcut
  requires a non-modifier key, and the string does not parse. Implementing a
  genuinely *held* modifier means monitoring global key events, which on macOS
  requires Input Monitoring or Accessibility, and §3.5 states that
  Accessibility is not requested. §3.3 and §3.5 therefore disagree.

  §3.2 already allowed "held **or toggled**", so a registrable toggle was
  inside the spec's envelope, but choosing its accelerator meant inventing a
  user-facing default this spec did not state. The action was therefore left
  unbound rather than guessed at, with a test asserting its absence, until a
  human chose among three resolutions: give Interact a real key combination
  and make it a toggle (amending §3.3's default); accept Input Monitoring and
  amend §3.5; or make interaction a tray-menu action with no shortcut.

  **Resolved 2026-09-07: the first.** See D-9.

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

  D-5 was additionally open at the time, and blocked one of §3.3's five
  actions. It is resolved (D-9).

  **Reasons 1 and 2 are resolved (2026-09-07) by 018 R-010 and D-10.** Most of
  that checklist could never have been signed in phase 2 by anyone, on either
  platform, which is what R-010 now says out loud; D-10 disposes of the
  Windows column. **Reason 3 stands**: FR-006 is still unwritten, and D-11
  records what completing this spec found instead.

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

- **D-9 (2026-09-07, Interact becomes a registrable toggle).** Resolves D-5,
  by the maintainer's decision: of the three resolutions D-5 listed, take the
  first. §3.3's Interact row is amended from the unregistrable
  `Cmd+Option` / `Ctrl+Alt` hold to the toggle
  `Cmd+Shift+Space` / `Ctrl+Shift+Space`, and §3.2's "held or toggled"
  narrows to the toggle alone.

  **§3.5 is untouched.** That is the point of the choice: a toggle is an
  ordinary global shortcut, so the app still requests no Input Monitoring and
  no Accessibility, and §3.3 and §3.5 no longer disagree.

  **Why `Space` and not the mnemonic letter.** `Cmd+Shift+I` reads better, but
  its Windows twin `Ctrl+Shift+I` is the browser developer-tools key, as are
  `Ctrl+Shift+J` and `Ctrl+Shift+C`. A global hotkey on Windows goes through
  `RegisterHotKey`, which intercepts the combination before the focused
  application sees it, so shipping that default would take developer tools
  away from every browser on the machine. `Space` keeps the
  `Mod+Shift+<key>` shape the other three defaults already use, is not a
  stock macOS binding, and is not a Windows system shortcut.

  **Press only.** A global shortcut fires on both press and release. A toggle
  that acted on each would flip twice per keystroke and appear dead, so §3.3
  now states the press-only rule and a test holds it.

  This also fits what spec 011 had already specified independently: its
  `UiCommand::SetInteractive { on: bool }` is a two-state command, which is a
  toggle's shape and not a hold's.

- **D-10 (2026-09-07, the Windows sign-off is deferred to 017; maintainer's
  decision).** AC-3 requires the checklist signed on both platforms. No
  Windows host is available to this project. The maintainer's decision is to
  defer the Windows column rather than hold the entire build order behind
  acquiring one: five of its eight rows are deferred to 005 and 019 in any
  case (018 R-010), and the other three are observable today with nowhere to
  observe them.

  The deferral names spec **017** as the point at which the Windows column
  comes due. 017 is what builds, signs and notarizes a Windows artifact, so it
  is the first spec that cannot honestly ship without someone having run the
  app on Windows. `apps/desktop/README.md` carries it in the row's `When`
  column so it cannot be lost between here and there.

  This weakens no Windows requirement. `window_flags_match_spec` asserts §3.2's
  flag set on both platforms, CI's `rust (windows-latest)` job compiles and
  tests the crate on Windows on every PR, and what stays unverified is exactly
  what a manual checklist verifies anywhere: what the operating system did with
  the request, as opposed to what was requested.

- **D-11 (2026-09-07, two requirements that were specified and never built).**
  Completing this spec found two gaps between §3 and the crate. Neither was a
  design question; both were simply absent.

  **FR-004's macOS half did not exist.** §3.2 requires the app to be an
  accessory (`LSUIElement`, no Dock icon), and nothing in the crate set an
  activation policy, nor does `tauri.conf.json` carry `LSUIElement`.
  `OverlayWindowConfig::skip_taskbar` was documented as covering it. It does
  not: `skip_taskbar` is a *window* flag Tauri documents as unsupported on
  macOS, the Dock tile is an *application* property, and Tauri's default is
  `NSApplicationActivationPolicyRegular`. So the app had a Dock tile, and
  `window_flags_match_spec` passed the whole time, because it asserts the
  requested config and this requirement was never in the config.

  `MACOS_ACTIVATION_POLICY` now carries it as a value, on the D-4 pattern;
  `run()`'s setup hook applies it before the window is created, so no tile
  appears even for the moment creating one would take; `macos_app_is_an_accessory`
  holds it. Verified on this host: `lsappinfo` reports
  `ApplicationType="UIElement"`.

  **§3.4's tray menu was inert.** `build_menu` created all seven items and
  `build_tray` attached no handler, so every one of them, `Quit` included, did
  nothing. With no Dock tile and no taskbar button that menu is the only chrome
  the app has, so the effect was a running process a user could not end through
  any interface the product offers. `on_menu_event` now routes `Quit` to
  `app.exit(0)`; the other six belong to specs 005, 014, 016 and 019 and are
  matched by id and left visibly unhandled, the way `on_shortcut` already
  handles the same situation.

  Both are the same failure: a requirement whose only evidence was a manual
  checklist row nobody had run. That is the argument for 018 R-010 marking a
  deferred row *as deferred*, rather than leaving it an empty box among other
  empty boxes where a genuine miss looks identical to a scheduled one.

- **D-12 (2026-09-07, a verification command that could not fail).** §8
  contained

  ```sh
  sh -c '! grep -qE "https?://(?!ipc\.localhost)" .../tauri.conf.json || true'
  ```

  `grep -E` has no negative lookahead, so the pattern is a syntax error, `grep`
  exits 2, `!` turns that into success, and the trailing `|| true` would have
  masked it regardless. The command passed unconditionally and could not have
  caught the remote URL it was written to catch. This is the defect spec 003
  fixed for its own gates: a gate that passes by never executing.

  The replacement drops the `$schema` line, extracts every remaining URL, and
  fails if any is not `http://ipc.localhost`. **`$schema` is excluded
  deliberately**, and that judgement is what this decision records: the file's
  first line is `"$schema": "https://schema.tauri.app/config/2"`, a JSON Schema
  reference read by editors and by `tauri` at build time, never fetched by the
  app and not reachable from the webview, which is what §3.2's "No remote
  content is ever loaded" and 015 §3.3 are about. Excluding it keeps the check
  meaningful; failing on it would make the check something a future reader
  deletes.

  Both controls were run before it landed: it exits 0 on the file as it stands,
  and exits 1 when a remote URL is injected into it.

- **D-13 (2026-09-07, the sixth advisory D-8 was waiting for).** D-8 pinned §8
  at `grep -c RUSTSEC- deny.toml -eq 5` and said what the pin was for: "The
  `unmaintained` class is **not** blanket-disabled: a sixth advisory still
  fails the gate and gets read." Spec 011 added `specta`, `specta` builds with
  the `paste` proc-macro, and `paste` carries RUSTSEC-2024-0436,
  `unmaintained`. The gate failed, as designed.

  Read: `paste` is a **proc-macro**, so it runs at build time and is never
  linked into the shipped binary, and the advisory records no vulnerability.
  Spec 001 §3.1 sets the deny bar at `vulnerability`, so refusing it would be
  the configuration being stricter than the spec, which is the same reasoning
  D-8 applied to the `unic-*` five. It is listed individually in `deny.toml`
  with its reason and its path, and the count here moves to six.

  **Nothing about the mechanism changes.** `unmaintained` is still not
  blanket-disabled (§8's other check still forbids the key outright), the
  ignores are still individually listed, and a seventh advisory still fails
  the gate and gets read. This edit was made on spec 011's branch rather than
  its own, because the count and the dependency that moves it have to land in
  the same commit: split across two pull requests, `main` is red in between.
  That is a deliberate, stated exception to 018 R-003.

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
# D-15: asserted by keypath through the parser, not by matching URL text. The
# three build-time keys are removed by name and everything remaining must be
# `http://ipc.localhost`, so a remote URL added to the CSP, to a capability, or
# to an updater endpoint fails here even though `bundle.homepage` carries one
# legitimately. `chr(36)` is a literal `$`, written this way so the shell
# running the line does not expand `$schema`.
python3 -c "import json,re; c=json.load(open('apps/desktop/src-tauri/tauri.conf.json')); c.pop(chr(36)+'schema',None); b=c.get('bundle',{}); b.pop('homepage',None); b.get('windows',{}).pop('timestampUrl',None); u=[x for x in re.findall('https?://[^ ;\"]+', json.dumps(c)) if x!='http://ipc.localhost']; assert not u, u"
# Section 3.1: main.rs does one thing.
sh -c 'test "$(grep -c . apps/desktop/src-tauri/src/main.rs)" -lt 12'
# D-14: the crate says which of its two binaries is the product. Without this
# the bundler makes spec 011's exporter the application inside Butler.app and
# `cargo run` refuses to choose. Asserted through the parser, so the key in
# D-14's prose cannot satisfy it.
python3 -c "import tomllib; m=tomllib.load(open('apps/desktop/src-tauri/Cargo.toml','rb')); assert m['package']['default-run'] == 'butler-desktop', m['package'].get('default-run')"
# AC-3, read through 018 R-010: the checklist exists and marks its deferred
# rows as deferred, naming the spec each waits on, rather than leaving them as
# empty boxes indistinguishable from unknowns.
test -f apps/desktop/README.md
grep -q 'deferred to 005' apps/desktop/README.md
grep -q 'deferred to 019' apps/desktop/README.md
# D-11: §3.2's macOS accessory requirement and §3.4's Quit are asked for in
# code, not only in prose. Both were specified and unbuilt until 2026-09-07.
grep -q 'ActivationPolicy::Accessory' apps/desktop/src-tauri/src/window.rs
grep -q 'ids::QUIT => app.exit(0)' apps/desktop/src-tauri/src/tray.rs
# D-9: §3.3's Interact default is registrable, and the code binds the exact
# accelerator the table states. A drift between the two fails here.
grep -q 'Cmd+Shift+Space' apps/desktop/src-tauri/src/shortcuts.rs
grep -q 'Ctrl+Shift+Space' apps/desktop/src-tauri/src/shortcuts.rs
# D-8: the advisory ignores stay individually listed. Blanket-disabling the
# `unmaintained` class would hide the next one.
sh -c '! grep -qE "^\\s*unmaintained\\s*=" deny.toml'
sh -c 'test "$(grep -c RUSTSEC- deny.toml)" -eq 6'
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "004-desktop-shell" && exit 1 || exit 0'
```

- **D-14 (2026-09-09, the bundle's application was not the application).**
  This crate has two binary targets: the app (`src/main.rs`) and spec 011's
  bindings exporter (`src/bin/export-bindings.rs`, added here by 011's
  `extends` edge). Nothing told the toolchain which one is the product, and
  both consumers guessed wrong in different ways.

  `cargo tauri build` runs `cargo build --bins`, builds both, and makes
  **`export-bindings`** the executable inside `Butler.app`: a 3 MB program that
  writes a TypeScript file and exits, bundled with a valid `Info.plist` and a
  signature over the wrong thing. `cargo run` refuses to choose at all
  ("could not determine which binary to run"), which is how spec 020 found it.

  **Neither obvious lever corrects the bundler.** Passing `-- --bin
  butler-desktop` yields `cargo build --bin butler-desktop --bins ...`, because
  the CLI appends `--bins` after the caller's arguments, so both are built and
  the selection is untouched. Setting `mainBinaryName` renames the copy without
  changing which file is copied, which is the more dangerous of the two: the
  bundle then contains `Contents/MacOS/butler-desktop` holding the exporter's
  bytes, and every name-based check passes on it. It was caught by a content
  marker, not a name.

  `default-run` fixes both consumers, and it is the key cargo's own error
  message points at. §8 asserts the manifest carries it, read through a TOML
  parser so the key appearing in this paragraph cannot satisfy the check.

  What §8 does **not** assert is the bundle's own `CFBundleExecutable`, because
  producing a bundle takes a release build and the Tauri CLI, neither of which
  a verification run may assume. That evidence is spec 020's `local_app.sh`,
  which compares the installed executable against `target/release/butler-desktop`
  byte for byte on every `make app`, and it was run for this change.

  **Why this mattered beyond a local build.** `.github/workflows/release.yml`
  calls `cargo tauri build` with no binary named, so a `v0.1.0` tag would have
  signed, notarized, stapled, hashed and attested an installer whose
  application is the bindings exporter, with every check in spec 017 passing on
  it. That is 017 D-10's failure mode ("nothing else looks inside the bundle")
  one level deeper, and 017 needed no change to be fixed by this one.

  **Still owed.** `default-run` names the main binary; it does not stop the
  exporter being *built* and copied, so `Contents/MacOS/` now holds both. Three
  megabytes of dead code inside a signed bundle is untidy rather than unsafe,
  and removing it means `required-features` on a `[[bin]]` table, which changes
  how spec 011's bindings are generated (`.github/workflows/ci.yml` and 011 §8
  both run `cargo run --bin export-bindings`). That is three other specs'
  territory and is left to them. Spec 020's `local_app.sh` already strips the
  second binary from the bundle it installs, so the local path ships one.

- **D-15 (2026-09-09, a URL check that could not tell where a URL was).** §8's
  remote-URL assertion, rewritten by D-12 after it was found passing
  vacuously, extracted every URL in `tauri.conf.json` and failed on anything
  that was not `http://ipc.localhost`. Spec 017 then added `bundle.homepage`
  and `bundle.windows.timestampUrl`, and the check began failing on `main`.

  Both additions are legitimate and neither is a destination the app can
  reach: `homepage` is installer metadata, and `timestampUrl` is read by
  `signtool` on the signing machine at build time. The check could not say so,
  because it matched URL *text* and knew nothing about where in the file the
  text sat. That is the same defect D-12 fixed one layer up: an assertion that
  is not reading what the requirement is about.

  It now parses the file, removes those two keys and `$schema` **by name**, and
  requires every remaining URL to be `http://ipc.localhost`. This is narrower
  than the old check by exactly three named build-time keypaths and unchanged
  everywhere else, which is what §3.2's "No remote content is ever loaded" and
  015 §3.3 are actually about.

  Four controls were run before it landed: it exits 0 on the file as it stands;
  it exits non-zero when a remote URL is appended to the CSP; it exits non-zero
  when a `plugins.updater.endpoints` entry is added, which is the destination
  017 D-4 says needs an amendment to 015 before it may exist; and it still
  exits 0 when `bundle.homepage` is pointed somewhere else, since that key is
  metadata whatever its value.

  The alternative was to keep the blanket check and have 017 stop adding bundle
  metadata, which would be this spec dictating another spec's content on the
  strength of an assertion that was imprecise. The refusal rule forbids editing
  a spec to ratify code that contradicts it; it does not require keeping a
  check that tests the wrong thing. The requirement is unchanged, and the
  maintainer approved the reading in session.
