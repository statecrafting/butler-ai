// Spec: specs/004-desktop-shell/spec.md

//! Build script for the desktop shell.
//!
//! `tauri_build::build()` reads `tauri.conf.json` and, via
//! `tauri::generate_context!`, **hard-fails when `frontendDist` does not
//! exist**. Spec 018 builds this spec before spec 012, which is what supplies
//! the frontend, so without help the shell could not compile in its own phase.
//!
//! So this script guarantees the directory exists, and nothing more. When
//! spec 012 lands, Vite writes its real output to the same path and the
//! placeholder is simply overwritten; no configuration moves, and no spec
//! needs an edge onto `tauri.conf.json` to repoint it. See spec 004 D-3.
//!
//! The directory is a build artifact (`.gitignore` already excludes
//! `/apps/desktop/dist/`), so nothing generated here is ever committed.

use std::fs;
use std::path::Path;

/// The placeholder shell. Deliberately inert: no script, no network, no
/// styling beyond a transparent background, so that if it ever reaches a user
/// it looks like nothing rather than like a broken overlay.
const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Butler</title>
    <style>
      html, body { margin: 0; height: 100%; background: transparent; }
    </style>
  </head>
  <body><!-- Placeholder. The real overlay is spec 012. --></body>
</html>
"#;

fn main() {
    let dist = Path::new("../dist");
    let index = dist.join("index.html");
    if !index.exists()
        && let Err(e) = fs::create_dir_all(dist).and_then(|()| fs::write(&index, PLACEHOLDER))
    {
        // Not fatal here: let `tauri_build` produce its own, clearer error
        // about the missing frontend rather than masking it with an I/O one.
        println!("cargo:warning=could not create the placeholder frontend: {e}");
    }
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=capabilities");

    tauri_build::build();
}
