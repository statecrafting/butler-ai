// Spec: specs/009-pipeline-state-machine/spec.md

//! Pure domain logic for butler-ai.
//!
//! `butler-core` is the half of the product that has no operating system in
//! it: no clock, no filesystem, no network, no threads, no `tokio`, no
//! `tauri`. Everything here is a function of typed inputs, which is what
//! makes the pipeline's ordering guarantees testable rather than hoped for
//! (constitution §IV).
//!
//! Today the crate holds three modules:
//!
//! - [`delta`]: change detection, the valve in front of inference. A
//!   [`delta::ChangeDetector`] turns a stream of frames into the much sparser
//!   stream of "the user is now looking at something new" (spec 008).
//! - [`redaction`]: the privacy boundary's gate. `redact` is the only
//!   constructor of `RedactedText`, and `RedactedText` is the only thing that
//!   may leave the process (spec 015, constitution §V).
//! - [`machine`]: the pipeline state machine, a pure
//!   `reduce(state, event, cfg) -> (state, effects)`. The effects are data;
//!   the runtime host in the desktop crate (spec 019) executes them and feeds
//!   every result back as an event.
//!
//! - [`ipc`]: the typed seam to the overlay. `UiEvent` and `UiCommand` are
//!   defined here once and generated into the TypeScript the webview imports,
//!   so the two halves of the boundary cannot drift (spec 011).
//!
//! The remaining modules arrive with their own specs and extend this crate:
//! `pacing` (spec 013), `settings` (spec 014).

pub mod delta;
pub mod ipc;
pub mod machine;
pub mod redaction;
