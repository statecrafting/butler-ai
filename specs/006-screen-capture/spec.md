---
id: "006-screen-capture"
title: "Screen capture: snapshot polling of one monitor through the compositor"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: medium
platforms: ["windows", "macos"]
phase: 3
depends_on:
  # Phase 3 entry (018 R-002, R-007). 014, 016 and 019 are the leaves of
  # phase 2: between them they transitively require 004, 011, 012 and 015,
  # so these five edges gate the whole of phases 1 and 2.
  - "001-workspace-layout"
  - "009-pipeline-state-machine"
  - "014-user-configuration"
  - "016-diagnostics-and-logging"
  - "019-runtime-host"
establishes:
  - { kind: crate, id: "butler-capture" }
  - "crates/butler-capture/Cargo.toml"
  - "crates/butler-capture/src/lib.rs"
  - "crates/butler-capture/src/source.rs"
  - "crates/butler-capture/src/xcap_source.rs"
  - "crates/butler-capture/src/frame.rs"
  - "crates/butler-capture/src/monitor.rs"
  - "crates/butler-capture/tests/frame_lifecycle.rs"
  - { kind: symbol, id: "butler_capture::source::ScreenSource" }
  - { kind: symbol, id: "butler_capture::frame::Frame" }
  - { kind: symbol, id: "butler_capture::monitor::MonitorId" }
summary: >
  The `butler-capture` crate: a `ScreenSource` trait that yields one `Frame`
  of one monitor on demand, an `xcap`-backed implementation for Windows and
  macOS, monitor enumeration and selection, and the `Frame` type whose
  lifecycle rules (in-memory only, no `Serialize`, dropped after recognition)
  are the first link in the privacy chain. Snapshot polling at a configurable
  interval replaces continuous video capture: the pipeline asks for a frame
  when the state machine says so, never on a free-running loop. Cadence,
  scheduling and backoff are the state machine's (009); this crate only knows
  how to take one picture.
---

# 006: Screen capture

## 1. Purpose

A continuous video stream of the display costs CPU, GPU and battery, and it
produces far more data than the pipeline can use: text on a screen changes on
the order of seconds, not frames. The outline's design is snapshot polling at
a two-to-three second interval. This spec provides the primitive that makes
that possible, and nothing more: "give me the pixels of monitor M, now".

Keeping the crate this small has two benefits. The state machine (009) owns
*when* to capture and can be tested without a screen; and the privacy
boundary (015) has a single type, `Frame`, to constrain.

## 2. Territory

The crate `butler-capture` and its files. `source.rs` defines the trait;
`xcap_source.rs` is the production implementation; `frame.rs` the frame type;
`monitor.rs` enumeration. `tests/frame_lifecycle.rs` holds the lifecycle
property tests. The crate is `cfg`-gated for `windows` and `macos`; on other
targets it compiles to the trait and a `NullSource` that always returns
`Unavailable`, so `butler-core` tests can run on Linux CI.

## 3. Behavior

### 3.1 The trait

```rust
pub trait ScreenSource: Send + Sync {
    fn monitors(&self) -> Result<Vec<MonitorInfo>, CaptureError>;
    fn capture(&self, monitor: MonitorId) -> Result<Frame, CaptureError>;
}
```

- `capture` is synchronous and MUST return within 250 ms or `CaptureError::
  Timeout`. The runtime calls it from a blocking task.
- `CaptureError` variants: `PermissionDenied` (macOS TCC), `MonitorGone`,
  `Timeout`, `Unavailable(String)`. No variant carries pixel data.
- `MonitorInfo { id: MonitorId, name: String, bounds: Rect, scale: f32,
  is_primary: bool }`. `MonitorId` is stable across the process lifetime and
  derived from the platform's monitor handle, not from enumeration order.

### 3.2 `Frame`

```rust
pub struct Frame {
    pub monitor: MonitorId,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub captured_at: Instant,   // monotonic, never wall-clock
    pixels: Box<[u8]>,          // RGBA8, row-major, private
}
```

- `Frame` MUST NOT implement `Serialize`, `Debug` with pixel content, `Clone`,
  or any conversion to an image file format. Access to pixels is through
  `as_rgba(&self) -> &[u8]` and `crop(&self, rect) -> FrameView<'_>` only.
- `Frame` MUST implement `Drop` that zeroes the pixel buffer (`zeroize`), so a
  dropped frame does not linger in freed memory.
- `captured_at` is `Instant`, so a frame can never carry a timestamp that
  identifies *when* in wall-clock terms the user's screen looked like this.
- A `FrameView` borrows a rectangle for the recognizer (007) and the exclusion
  self-test (005); it cannot outlive the frame.

### 3.3 The `xcap` implementation

- `XcapSource` uses the `xcap` crate's monitor capture, pinned exact in
  `Cargo.lock`. On Windows this goes through DXGI desktop duplication (which
  honours `WDA_EXCLUDEFROMCAPTURE`); on macOS through ScreenCaptureKit /
  `CGWindowListCreateImage` (which honours `NSWindowSharingNone`). This is the
  property spec 005's self-test relies on: capture MUST use the compositor
  path, never a window-list composition that could re-include excluded
  windows.
- The captured image is converted to RGBA8 once and never retained by the
  source between calls.
- HiDPI: `Frame.scale` carries the monitor scale; the OCR crate decides
  whether to downscale.

### 3.4 Monitor selection

- Default: the primary monitor. Settings (014) may pin a monitor by its
  persistent name; if that monitor is absent at capture time the source
  returns `MonitorGone` and the runtime falls back to primary and notifies the
  UI once.
- The overlay's own monitor is the default *target*: the assumption is the
  user reads and shares the same display. Settings may decouple them.

## 4. Functional requirements

- **FR-001.** `capture` on a 4K monitor completes in under 100 ms p50 and
  250 ms p99 on the reference hardware named in `docs/architecture.md`.
- **FR-002.** Two consecutive captures of a static screen yield
  byte-identical pixel buffers (determinism of the source; the change detector
  depends on OCR stability rather than pixel stability, but this catches
  capture-path noise early).
- **FR-003.** The crate's public API exposes no way to write a `Frame` to disk
  or to serialize it: enforced by a compile-fail test
  (`tests/frame_lifecycle.rs` uses `trybuild`) that asserts `Frame: !Serialize`
  and that `pixels` is private.
- **FR-004.** Dropping a `Frame` zeroes its buffer (asserted with a test
  allocator hook).
- **FR-005.** On macOS without Screen Recording permission, `capture` returns
  `PermissionDenied` within 50 ms and does not trigger the system prompt (the
  shell (004) owns prompting).

## 5. Acceptance criteria

- **AC-1.** `cargo test -p butler-capture` passes on both platform runners;
  the lifecycle tests also pass on Linux against `NullSource`.
- **AC-2.** `rg "serde|image::save|write_to" crates/butler-capture/src` returns
  nothing outside `Cargo.toml` dev-dependencies.
- **AC-3.** Spec 015's `constrains` on `frame.rs` is in place before this spec
  flips to complete.

## 6. Out of scope

- Capture cadence, scheduling, pausing when the screen is locked or the
  screensaver is active (009).
- Region-of-interest selection and window-scoped capture (a later feature;
  the outline captures the whole display).
- Video or audio capture of any kind.

## 7. Resolved decisions

- **D-1 (2026-09-02).** `depends_on` gained the phase 2 leaves (014, 016,
  019) as the 018 R-002 gate. Nothing in this crate calls settings, logging
  or the runtime; the edges exist so the orchestrator, which schedules on
  `depends_on` alone, cannot start phase 3 while phase 2 is unfinished.
