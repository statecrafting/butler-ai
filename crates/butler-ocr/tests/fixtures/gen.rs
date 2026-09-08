// Spec: specs/007-text-recognition/spec.md

//! The fixture generator (spec 007 AC-2).
//!
//! Committed beside the fixtures it produces, so anyone can see that they
//! contain **no personal data**: every string here is lorem ipsum or a
//! layout label, and every coordinate is arithmetic.
//!
//! Run with:
//!
//! ```sh
//! cargo test -p butler-ocr --test normalize -- --ignored regenerate
//! ```
//!
//! The fixtures are `Line` lists rather than screenshots. §3.4's rules are
//! about geometry, confidence and text, and a PNG would add an OCR engine to
//! the middle of a test whose whole point is to be pure and to run on every
//! target including Linux (spec 007 D-4).

/// Lorem words, so no fixture can carry anything real.
pub const WORDS: [&str; 12] = [
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "eiusmod",
    "tempor",
    "incididunt",
];

/// A deterministic phrase of `n` words, starting at `offset`.
#[must_use]
pub fn phrase(offset: usize, n: usize) -> String {
    (0..n)
        .map(|i| WORDS[(offset + i) % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}
