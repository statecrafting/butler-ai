---
id: "006-screen-capture"
title: "Screen capture: snapshot polling of one monitor through the compositor"
status: approved
kind: "feature"
domain: "pipeline"
created: "2026-09-01"
implementation: complete
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
extends:
  # `crates/*` already globs this crate into the workspace, but its
  # dependencies (xcap, image, zeroize) are pinned once in spec 001's root
  # manifest (001 FR-004) before this crate's manifest references them.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  # The manual checklist. 018 R-010 says a requirement whose evidence a later
  # phase or a missing machine supplies is recorded there as deferred, and
  # three of this spec's are (D-2). The file sits inside spec 012's package,
  # so writing to it is an edge rather than a waiver.
  - { spec: "012-overlay-ui", unit: "apps/desktop/README.md", nature: additive }
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

- **D-2 (2026-09-07, three requirements that need a real screen).** FR-001
  (latency), FR-002 (two identical captures) and FR-005 (permission refused
  without a prompt) cannot be asserted by `cargo test`. The first two need the
  reference hardware `docs/architecture.md` §7 names, and a *genuinely* static
  screen: a clock in a menu bar is enough to make FR-002 fail on a machine
  where the capture path is perfect. FR-005 needs a macOS account with the
  Screen Recording grant revoked.

  They are rows in `apps/desktop/README.md` under 018 R-010, marked deferred
  with what each waits on. §7 still names neither reference machine; §7.1
  records an Apple M1 Max for spec 008's benchmark, which is one of the two,
  and naming a Windows machine that has not been measured on would be worse
  than leaving it blank.

  What *is* asserted mechanically is the whole of §3.2, which is the half spec
  015 depends on, and it is asserted on every target including Linux.

- **D-3 (2026-09-07, how a missing permission is recognized).** §3.1 gives
  `CaptureError::PermissionDenied` its own variant, but `xcap` reports a
  missing macOS grant as an ordinary failure with a message. `map_error`
  therefore inspects the text for "permission" or "not authorized", which is a
  heuristic and is documented as one at the call site.

  It is not the product's answer to "may we capture?". Spec 004 §3.5 asks
  `CGPreflightScreenCaptureAccess` and owns prompting; this crate never
  prompts (FR-005) and never decides. The heuristic exists so a capture that
  fails for that reason is *reported* usefully rather than as
  `Unavailable("...")`, and if it ever misclassifies, the shell's
  authoritative check is what the user actually sees.

- **D-4 (2026-09-07, FR-004 is asserted during deallocation).** "Dropping a
  `Frame` zeroes its buffer" cannot be checked by reading the memory after the
  drop: that is a use-after-free, and such a test measures the allocator's
  reuse policy rather than our `Drop`.

  `tests/frame_lifecycle.rs` installs a pass-through global allocator that
  inspects one block size on free, while the pointer is still valid and owned
  by the caller, which is the only moment at which the question has an answer.
  The test also asserts the hook **fired**, so it cannot pass by watching a
  size nothing allocated, and asserts the fixture starts non-zero, so it
  cannot pass by zeroing nothing.

- **D-5 (2026-09-07, `test_support`, and why it is not a hole).** FR-003's
  compile-fail tests and the lifecycle tests live outside this crate, and
  `Frame::new` is `pub(crate)` precisely so that nothing outside can turn
  pixels into a `Frame`.

  `test_support::frame_from_pattern` bridges that, and is deliberately shaped
  so it cannot be misused: it takes a single byte and **generates** the
  buffer, so no caller can hand it a real screen. The guarantee §3.2 is after
  is that arbitrary pixels cannot acquire a `Frame`'s lifecycle without
  earning it, and a constructor that refuses to accept pixels keeps it.

## 8. Verification

```verify:cli
# AC-1: the unit tests, the lifecycle tests and the compile-fail doctests.
# The lifecycle half runs on every target, Linux included (§2).
cargo test -p butler-capture --locked
# FR-003: `Frame` cannot be serialized, cloned, or emptied of its pixels. The
# doctests carry a positive control, so a passing compile_fail block cannot be
# passing because an import path is wrong.
cargo test -p butler-capture --locked --doc
# AC-2: nothing in the crate can write a frame anywhere.
sh -c '! grep -rqE "serde|image::save|write_to" crates/butler-capture/src'
# §3.2 and spec 015: the type-level properties, checked as text because their
# absence is silent. `Frame` itself carries no derive at all: the first
# version of this check grepped the whole file and matched `Rect`'s
# `#[derive(Clone, Copy, ...)]`, which is correct and necessary. The real
# proof that a frame cannot be cloned or serialized is the compile-fail
# doctest above; this is the second opinion, and it has to look at the right
# type. Tested with a negative control.
sh -c '! grep -B3 "^pub struct Frame {" crates/butler-capture/src/frame.rs | grep -q "derive"'
grep -q "impl Drop for Frame" crates/butler-capture/src/frame.rs
grep -q "self.pixels.zeroize()" crates/butler-capture/src/frame.rs
# §3.2: the timestamp is monotonic. A `SystemTime` here would say when in
# wall-clock terms the user's screen looked like this.
sh -c '! grep -q "SystemTime" crates/butler-capture/src/frame.rs'
grep -q "captured_at: Instant" crates/butler-capture/src/frame.rs
# §3.3: the compositor path is what spec 005's self-test relies on.
grep -q "capture_image" crates/butler-capture/src/xcap_source.rs
# D-2: the three requirements that need a real screen are recorded as
# deferred rather than silently unchecked.
grep -q "Screen capture (spec 006)" apps/desktop/README.md
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "006-screen-capture" && exit 1 || exit 0'
```
