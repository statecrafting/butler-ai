---
id: "004-desktop-shell"
title: "Desktop shell: the Tauri v2 app crate, the transparent always-on-top overlay window, shortcuts, tray, and permission onboarding"
status: approved
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: high
platforms: ["windows", "macos"]
phase: 2
depends_on:
  # Phase 2 entry (018 R-002, R-007): the whole of phase 1 must be complete.
  # 009 is also a build dependency, not only a gate: this crate joins the
  # workspace by adding `apps/desktop/src-tauri` to `members`, and cargo
  # refuses that list while the `crates/*` glob beside it still matches
  # nothing (001 D-1). `butler-core` is what populates it.
  - "001-workspace-layout"
  - "008-change-detection"
  - "009-pipeline-state-machine"
  - "015-privacy-boundary"
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
