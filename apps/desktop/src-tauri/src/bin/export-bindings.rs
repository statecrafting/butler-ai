// Spec: specs/011-ipc-contract/spec.md

//! Writes `apps/desktop/src/generated/bindings.ts` from the Rust contract.
//!
//! Spec 011 §3.3. The generated file is committed and CI fails on a non-empty
//! `git diff` after re-running this, so the TypeScript the overlay imports is
//! always the TypeScript this binary would produce from the current Rust. It
//! is the same discipline the corpus applies to `.derived/`.
//!
//! # Determinism (FR-002)
//!
//! `tauri_specta::Builder::export` runs whatever code formatter it can find
//! on the way out, which would make the output depend on what happens to be
//! installed on the machine. This binary calls `export_str` instead and does
//! the writing itself: normalize to LF, exactly one trailing newline, and a
//! fixed header. Two runs on two platforms then produce the same bytes.
//!
//! The two contract constants are written here rather than through
//! `Builder::constant`, because that stores them in a `HashMap` while every
//! other part of the render is a `BTreeMap` or a `Vec`. Two renders in one
//! process emitted them in different orders, which would have made CI's
//! `git diff --exit-code` fail at random. See spec 011 D-5.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use butler_core::ipc::IPC_CONTRACT_VERSION;
use specta_typescript::{BigIntExportBehavior, Typescript};

/// Where the bindings live, relative to this crate's manifest.
const OUTPUT: &str = "../src/generated/bindings.ts";

/// §3.3 fixes this line. It is the first thing anyone opening the file sees,
/// and it names the file to edit instead.
fn header() -> String {
    let (major, minor) = IPC_CONTRACT_VERSION;
    format!(
        "// GENERATED FROM crates/butler-core/src/ipc.rs (IPC v{major}.{minor}); do not edit\n\
         // Regenerate: cargo run -p butler-desktop --bin export-bindings\n"
    )
}

/// The contract's two constants, in a fixed order.
///
/// Both are read from Rust, so there is still exactly one source for each:
/// the channel name from spec 011 §3.2's `EVENT_CHANNEL`, the version from
/// §3.4's `IPC_CONTRACT_VERSION`. What this function adds over
/// `Builder::constant` is only the ordering (D-5).
fn constants() -> String {
    let (major, minor) = IPC_CONTRACT_VERSION;
    format!(
        "\n/** contract constants (spec 011 §3.2, §3.4) **/\n\n\
         export const EVENT_CHANNEL = \"{channel}\" as const;\n\
         export const IPC_CONTRACT_VERSION = [{major}, {minor}] as const;\n",
        channel = butler_desktop::events::EVENT_CHANNEL,
    )
}

fn main() -> ExitCode {
    // Spec 011 §3.1 types the counters as `u64`, and specta refuses to export
    // a 64-bit integer unless told how the wire carries it. Here it is a JSON
    // number: serde_json writes one, `JSON.parse` reads a double, so `number`
    // is what the TypeScript should say. `bigint` would describe a runtime
    // type the webview never receives, and `string` would contradict the Rust
    // serialization. See spec 011 D-4 on the range this is safe over.
    let typescript = Typescript::default().bigint(BigIntExportBehavior::Number);

    let rendered = match butler_desktop::commands::builder().export_str(typescript) {
        Ok(rendered) => rendered,
        Err(e) => {
            eprintln!("export-bindings: could not render TypeScript: {e}");
            return ExitCode::FAILURE;
        }
    };

    // LF everywhere, exactly one trailing newline, and no trailing spaces:
    // three things a Windows checkout or a formatter would otherwise vary.
    let body: String = rendered.replace("\r\n", "\n");
    let contents = format!("{}{}\n{}", header(), body.trim_end(), constants());

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OUTPUT);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        eprintln!(
            "export-bindings: could not create {}: {e}",
            parent.display()
        );
        return ExitCode::FAILURE;
    }
    if let Err(e) = fs::write(&path, contents) {
        eprintln!("export-bindings: could not write {}: {e}", path.display());
        return ExitCode::FAILURE;
    }

    println!("export-bindings: wrote {}", path.display());
    ExitCode::SUCCESS
}
