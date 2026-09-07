// Spec: specs/004-desktop-shell/spec.md

//! Process entry. Spec 004 §3.1: this file does one thing.

// The overlay is the only surface; a console window on Windows would be a
// second one, and an empty one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    butler_desktop::run();
}
