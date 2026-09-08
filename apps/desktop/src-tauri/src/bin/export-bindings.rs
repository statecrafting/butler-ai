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
use butler_core::settings::Settings;
use specta_typescript::{BigIntExportBehavior, Typescript};

/// Where the bindings live, relative to the workspace root.
const OUTPUT: &str = "apps/desktop/src/generated/bindings.ts";

/// A path that only exists at the workspace root, used to recognize it.
const ROOT_MARKER: &str = "apps/desktop/src-tauri/Cargo.toml";

/// Find the workspace root by walking up from the current directory.
///
/// Deliberately not the compile-time manifest-directory macro. Spec 014
/// FR-005 requires that no environment read appears under `crates/` or
/// `apps/desktop/src-tauri` outside `build.rs`, so the product has no
/// environment-variable surface at all. A compile-time path would not be a
/// configuration surface, but the requirement is checked by grep, and a rule
/// with an exception is a rule nobody can check. Walking up is also the more
/// robust of the two: it works from any directory in the tree, where a
/// hardcoded relative path works from exactly one (spec 014 D-5).
fn workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(ROOT_MARKER).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// §3.3 fixes this line. It is the first thing anyone opening the file sees,
/// and it names the file to edit instead.
fn header() -> String {
    let (major, minor) = IPC_CONTRACT_VERSION;
    format!(
        "// GENERATED FROM crates/butler-core/src/ipc.rs (IPC v{major}.{minor}); do not edit\n\
         // Regenerate: cargo run -p butler-desktop --bin export-bindings\n"
    )
}

/// The contract's constants, in a fixed order.
///
/// Each is read from Rust, so there is exactly one source: the channel name
/// from spec 011 §3.2's `EVENT_CHANNEL`, the version from §3.4's
/// `IPC_CONTRACT_VERSION`, and the settings defaults from spec 014's
/// `Settings::default()`. What this function adds over `Builder::constant` is
/// only the ordering (spec 011 D-5).
///
/// `DEFAULT_SETTINGS` exists so spec 014 §3.4's "Reset to defaults" has a
/// source. Hand-writing the defaults in TypeScript would be a second copy of
/// values that live in `settings.rs`, and the first divergence would silently
/// reset a user's configuration to something nobody chose.
fn constants() -> Result<String, serde_json::Error> {
    let (major, minor) = IPC_CONTRACT_VERSION;
    let defaults = serde_json::to_string_pretty(&Settings::default())?;
    Ok(format!(
        "\n/** contract constants (spec 011 §3.2, §3.4; spec 014 §3.4) **/\n\n\
         export const EVENT_CHANNEL = \"{channel}\" as const;\n\
         export const IPC_CONTRACT_VERSION = [{major}, {minor}] as const;\n\
         export const DEFAULT_SETTINGS: Settings = {defaults};\n",
        channel = butler_desktop::events::EVENT_CHANNEL,
    ))
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
    let constants = match constants() {
        Ok(constants) => constants,
        Err(e) => {
            eprintln!("export-bindings: could not serialize the defaults: {e}");
            return ExitCode::FAILURE;
        }
    };
    let contents = format!("{}{}\n{}", header(), body.trim_end(), constants);

    let Some(root) = workspace_root() else {
        eprintln!(
            "export-bindings: no workspace root above the current directory \
             (looked for {ROOT_MARKER})"
        );
        return ExitCode::FAILURE;
    };
    let path = root.join(OUTPUT);
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
