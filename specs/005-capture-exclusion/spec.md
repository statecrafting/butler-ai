---
id: "005-capture-exclusion"
title: "Capture exclusion: keep the overlay out of the OS frame buffer, and verify it"
status: draft
kind: "feature"
domain: "platform"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: critical
platforms: ["windows", "macos"]
phase: 3
depends_on:
  - "004-desktop-shell"
  - "006-screen-capture"
extends:
  - { spec: "004-desktop-shell", unit: { kind: module, id: "butler_desktop::exclusion" }, nature: additive }
  - spec: "004-desktop-shell"
    nature: additive
    paths:
      - "apps/desktop/src-tauri/src/exclusion/mod.rs"
      - "apps/desktop/src-tauri/src/exclusion/windows.rs"
      - "apps/desktop/src-tauri/src/exclusion/macos.rs"
      - "apps/desktop/src-tauri/src/exclusion/selftest.rs"
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::exclusion::apply_exclusion" }, nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::exclusion::verify_exclusion" }, nature: additive }
  - { spec: "004-desktop-shell", unit: { kind: symbol, id: "butler_desktop::exclusion::ExclusionStatus" }, nature: additive }
refines:
  - { aspect: "capture-exclusion", unit: "apps/desktop/src-tauri/src/window.rs" }
references:
  - { unit: { kind: file, path: "docs/threat-model.md" }, role: "threat model" }
summary: >
  The stealth mechanism and its verification. On Windows the overlay window's
  handle receives `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`; on macOS
  the `NSWindow.sharingType` is set to `NSWindowSharingNone`. Both are
  requests to the compositor, not guarantees, so this spec also defines a
  self-test that captures the screen through the same platform path recording
  tools use and asserts the overlay's pixels are absent, and a typed
  `ExclusionStatus` the runtime and UI must honour: the app never claims to be
  hidden when it has not verified that it is. The same property protects the
  pipeline from OCR-ing its own answers.
---

# 005: Capture exclusion

## 1. Purpose

The product promise is that the overlay is invisible to screen sharing and
recording (Zoom, Teams, Meet, OBS, the OS screenshot tools) while fully
visible to the person at the keyboard. Both platforms expose a compositor-level
switch for exactly this: Windows' display affinity and macOS's window sharing
type. Those switches exist for password managers and DRM surfaces and are
honoured by every capture API that goes through the compositor.

Two things make this spec more than two API calls:

1. **The switch is a request.** Its effect depends on OS version (Windows 10
   2004+ for `WDA_EXCLUDEFROMCAPTURE`; earlier builds only offer
   `WDA_MONITOR`, which renders a black box), on the capture API the other
   party uses, and on Apple's evolving ScreenCaptureKit behaviour. Constitution
   §VI: the product verifies rather than assumes.
2. **The pipeline captures the screen too** (006). If the overlay's own pixels
   were in the frame, the OCR text would contain the previous answer, the
   change detector would fire on it, and the assistant would be asked about
   its own output. Exclusion is therefore also a correctness property of the
   pipeline, and the self-test doubles as the feedback-loop guard.

## 2. Territory

The `exclusion` module inside `butler-desktop` (added to spec 004's crate):
`mod.rs` (the platform-neutral API and `ExclusionStatus`), `windows.rs`,
`macos.rs` (the `cfg`-gated implementations), `selftest.rs`. This spec refines
`window.rs` on the aspect `capture-exclusion`: the window creation sequence
MUST call `apply_exclusion` before the window is shown, and the window MUST
not be shown if the call returns `Unsupported` unless the user has explicitly
accepted degraded mode.

## 3. Behavior

### 3.1 API

```rust
pub enum ExclusionStatus {
    /// Applied and verified by the self-test.
    Verified { method: ExclusionMethod, verified_at: Instant },
    /// Applied, self-test not yet run (transient during startup).
    Applied { method: ExclusionMethod },
    /// The OS accepted the call but the self-test saw overlay pixels.
    Compromised { method: ExclusionMethod, evidence: SelfTestEvidence },
    /// The OS cannot exclude this window (old Windows build, unknown platform).
    Unsupported { reason: String },
}
pub enum ExclusionMethod { WindowsDisplayAffinity, MacOsSharingNone }

pub fn apply_exclusion(window: &WebviewWindow) -> Result<ExclusionStatus, ExclusionError>;
pub fn verify_exclusion(window: &WebviewWindow, source: &dyn ScreenSource) -> ExclusionStatus;
```

`apply_exclusion` is idempotent and MUST be re-applied whenever the window is
recreated or re-shown after being hidden (some window managers reset affinity
on `SW_HIDE`/`SW_SHOW`).

### 3.2 Windows

- Obtain the `HWND` from the Tauri window handle and call
  `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`.
- If the call fails with `ERROR_INVALID_PARAMETER` on a build older than
  19041, return `Unsupported` (do NOT fall back to `WDA_MONITOR`: a black
  rectangle where the overlay is reveals more than showing it).
- The window MUST be layered (`WS_EX_LAYERED`) for the affinity to apply to a
  transparent window; the shell (004) guarantees this.
- The `unsafe` FFI block MUST be the only `unsafe` in the module and carry a
  `// SAFETY:` comment (spec 001 §3.1).

### 3.3 macOS

- Obtain the `NSWindow` from the Tauri window handle and set
  `sharingType = NSWindowSharingNone` via `objc2`/`objc2-app-kit`.
- This excludes the window from `CGWindowListCreateImage`, `SCStream`, and
  legacy `CGDisplayStream` paths that honour sharing type. The self-test is
  what tells us whether the running OS still honours it.
- The Screen Recording permission (004 §3.5) is required for the self-test's
  capture, not for exclusion itself.

### 3.4 The self-test (`selftest.rs`)

`verify_exclusion` MUST:

1. Ask the overlay (012) to render a sentinel: a full-window high-contrast
   test pattern with a known pixel signature (a checkerboard of two saturated
   colours not otherwise used by the UI) for the duration of the test.
2. Capture the monitor the overlay is on through `ScreenSource` (006), which
   uses the same compositor path capture tools use.
3. Sample the overlay's screen rectangle in the captured frame. If more than
   `0.5%` of sampled pixels match the sentinel signature, return
   `Compromised` with the match ratio as evidence; otherwise `Verified`.
4. Restore the overlay's normal content.

The self-test MUST run: once at startup after the window is created; on every
transition from `Disarmed` to `Armed` (009) if the last verification is older
than ten minutes or the display configuration changed; and on demand from the
tray. It MUST complete in under one second and MUST NOT be observable by the
user for longer than one frame (the sentinel is rendered and captured within a
single capture interval; the overlay is hidden from the human eye for at most
one frame by rendering the sentinel at the lowest alpha the capture path
still resolves, chosen per platform in implementation and asserted by test).

### 3.5 Honesty contract (consumed by 009 and 012)

- The runtime MUST NOT enter `Armed` while status is `Unsupported` or
  `Compromised` unless the user has enabled `allow_degraded_mode` in settings
  (014), in which case the overlay MUST show a persistent, unmistakable
  "VISIBLE TO SCREEN SHARING" banner and the tray icon MUST show the degraded
  glyph.
- The status is part of the `runtime.status` IPC event (011) and is shown in
  the overlay's status strip (012).
- The status is never persisted; it is recomputed every launch.

### 3.6 Feedback-loop guard

Independently of the banner, when the status is anything but `Verified`, the
change detector (008) MUST be given the overlay's last rendered text as an
exclusion set, so the pipeline never treats its own answer as new screen
content. When `Verified`, no such filtering is needed and none is applied (the
guard must not mask a broken exclusion in tests).

## 4. Functional requirements

- **FR-001.** On Windows 10 2004+ and Windows 11, `apply_exclusion` returns
  `Applied { WindowsDisplayAffinity }` and `GetWindowDisplayAffinity` reads
  back `WDA_EXCLUDEFROMCAPTURE`.
- **FR-002.** On macOS 13+, `apply_exclusion` returns `Applied {
  MacOsSharingNone }` and `window.sharingType` reads back `.none`.
- **FR-003.** With exclusion applied, a capture via `CGWindowListCreateImage`
  (macOS) or the DXGI desktop duplication path (Windows) does not contain the
  sentinel; with exclusion deliberately not applied (test hook), it does. Both
  directions are asserted so the test cannot pass vacuously.
- **FR-004.** The runtime refuses `Arm` under `Unsupported`/`Compromised`
  without `allow_degraded_mode`, and the overlay shows the banner with it.
- **FR-005.** Hiding and re-showing the overlay re-applies exclusion and the
  read-back in FR-001/002 still holds.

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-desktop exclusion::` passes on both
  platform runners with FR-003's two-direction assertion.
- **AC-2.** `docs/threat-model.md` §Capture exclusion lists what the mechanism
  does and does not defend against (hardware capture devices, a camera pointed
  at the screen, remote-desktop protocols that read the frame buffer below the
  compositor, accessibility APIs reading the DOM) and the spec's behavior under
  each; the list is reviewed with the spec.
- **AC-3.** The self-test's sentinel alpha per platform is recorded in the
  test as a constant with the empirical justification in a comment.

## 6. Out of scope

- Defending against out-of-band capture (see the threat model): this spec
  defends against compositor-based capture only and says so.
- Linux: no equivalent mechanism under X11; Wayland's screencast portal can
  exclude windows in some compositors, but not uniformly. Deferred (spec 001 §6).
- Hiding the process from process lists, task managers, or endpoint agents.
  butler-ai is a normal, signed desktop application (spec 017) and does not
  attempt to conceal its own existence on the machine.
