// Spec: specs/009-pipeline-state-machine/spec.md

//! Pure domain logic for butler-ai.
//!
//! `butler-core` is the half of the product that has no operating system in
//! it: no clock, no filesystem, no network, no threads, no `tokio`, no
//! `tauri`. Everything here is a function of typed inputs, which is what
//! makes the pipeline's ordering guarantees testable rather than hoped for
//! (constitution §IV).
//!
//! Today the crate holds one module:
//!
//! - [`machine`]: the pipeline state machine, a pure
//!   `reduce(state, event, cfg) -> (state, effects)`. The effects are data;
//!   the runtime host in the desktop crate (spec 019) executes them and feeds
//!   every result back as an event.
//!
//! The remaining modules arrive with their own specs and extend this crate:
//! `delta` (spec 008), `ipc` (spec 011), `pacing` (spec 013), `settings`
//! (spec 014), `redaction` (spec 015).

pub mod machine;
