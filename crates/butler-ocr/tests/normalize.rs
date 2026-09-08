// Spec: specs/007-text-recognition/spec.md

//! Spec 007 §3.4's rules, and FR-002 and FR-003.
//!
//! These run on **every** target, Linux included (§2, AC-1): `normalize` has
//! no platform code, and the fixtures are `Line` lists rather than
//! screenshots, so no OCR engine sits in the middle of a test about layout
//! and whitespace (D-4).

// The file is `fixtures/gen.rs`, which spec 007 §2 names. The module
// identifier is not: `gen` is a reserved keyword in edition 2024.
#[path = "fixtures/gen.rs"]
mod fixtures;

use butler_ocr::{DEFAULT_MIN_CONFIDENCE, Line, Rect, Size, normalize};

const FRAME: Size = Size {
    width: 1920,
    height: 1080,
};

fn line(text: &str, x: u32, y: u32, width: u32, height: u32, confidence: f32) -> Line {
    Line {
        text: text.to_owned(),
        bbox: Rect {
            x,
            y,
            width,
            height,
        },
        confidence,
    }
}

/// A paragraph of `rows` lines at a fixed left margin.
fn paragraph(rows: u32, x: u32, top: u32, height: u32) -> Vec<Line> {
    (0..rows)
        .map(|i| {
            line(
                &fixtures::phrase(i as usize, 6),
                x,
                top + i * (height + 6),
                600,
                height,
                0.95,
            )
        })
        .collect()
}

// ------------------------------------------------------------ §3.4 rule 1

#[test]
fn low_confidence_lines_are_dropped() {
    let lines = vec![
        line("lorem ipsum", 10, 10, 200, 20, 0.9),
        line("dolor sit", 10, 40, 200, 20, 0.2),
    ];
    let out = normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE);
    assert_eq!(out, "lorem ipsum");
}

#[test]
fn lines_shorter_than_two_graphemes_are_dropped() {
    let lines = vec![
        line("x", 10, 10, 10, 20, 0.99),
        line("ok", 10, 40, 20, 20, 0.99),
        // A combining sequence is one grapheme, not two chars.
        line("e\u{0301}", 10, 70, 10, 20, 0.99),
    ];
    assert_eq!(normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE), "ok");
}

// ------------------------------------------------------------ §3.4 rule 2

/// FR-003. Two columns read down the left, then down the right.
#[test]
fn fr_003_columns_read_in_reading_order() {
    let mut lines = Vec::new();
    // Right column first, so a pass that trusted engine order would fail.
    for i in 0..3 {
        lines.push(line(
            &format!("right {i}"),
            1000,
            100 + i * 40,
            300,
            24,
            0.95,
        ));
    }
    for i in 0..3 {
        lines.push(line(&format!("left {i}"), 100, 100 + i * 40, 300, 24, 0.95));
    }

    let out = normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE);
    assert_eq!(
        out, "left 0\nright 0\nleft 1\nright 1\nleft 2\nright 2",
        "lines at the same height are one row, left to right"
    );
}

/// The row tolerance is half the median line height, so a few pixels of
/// jitter does not reorder a row.
#[test]
fn small_vertical_jitter_does_not_reorder_a_row() {
    let steady = vec![
        line("alpha", 100, 100, 200, 24, 0.95),
        line("beta", 400, 100, 200, 24, 0.95),
        line("gamma", 700, 100, 200, 24, 0.95),
    ];
    let jittered = vec![
        line("alpha", 100, 102, 200, 24, 0.95),
        line("beta", 400, 98, 200, 24, 0.95),
        line("gamma", 700, 101, 200, 24, 0.95),
    ];

    assert_eq!(
        normalize(&steady, FRAME, DEFAULT_MIN_CONFIDENCE),
        normalize(&jittered, FRAME, DEFAULT_MIN_CONFIDENCE)
    );
    assert_eq!(
        normalize(&jittered, FRAME, DEFAULT_MIN_CONFIDENCE),
        "alpha\nbeta\ngamma"
    );
}

/// The tolerance scales with the text, not with the display: a `HiDPI` capture
/// doubles every number and must order identically.
#[test]
fn the_row_tolerance_scales_with_the_content() {
    let normal = paragraph(4, 100, 100, 24);
    let hidpi: Vec<Line> = normal
        .iter()
        .map(|l| {
            line(
                &l.text,
                l.bbox.x * 2,
                l.bbox.y * 2,
                l.bbox.width * 2,
                l.bbox.height * 2,
                l.confidence,
            )
        })
        .collect();

    assert_eq!(
        normalize(&normal, FRAME, DEFAULT_MIN_CONFIDENCE),
        normalize(
            &hidpi,
            Size {
                width: 3840,
                height: 2160
            },
            DEFAULT_MIN_CONFIDENCE
        )
    );
}

// ------------------------------------------------------------ §3.4 rule 3

#[test]
fn whitespace_is_collapsed_and_trimmed() {
    let lines = vec![line("  lorem   ipsum \t dolor  ", 10, 10, 400, 20, 0.95)];
    assert_eq!(
        normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE),
        "lorem ipsum dolor"
    );
}

#[test]
fn typographic_punctuation_becomes_ascii() {
    let lines = vec![
        line(
            "\u{201C}lorem\u{201D} \u{2014} ipsum",
            10,
            10,
            400,
            20,
            0.95,
        ),
        line("don\u{2019}t \u{2013} sit", 10, 40, 400, 20, 0.95),
    ];
    assert_eq!(
        normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE),
        "\"lorem\" - ipsum\ndon't - sit"
    );
}

#[test]
fn zero_width_characters_are_dropped() {
    let with = vec![line("lo\u{200B}rem ip\u{FEFF}sum", 10, 10, 400, 20, 0.95)];
    let without = vec![line("lorem ipsum", 10, 10, 400, 20, 0.95)];
    assert_eq!(
        normalize(&with, FRAME, DEFAULT_MIN_CONFIDENCE),
        normalize(&without, FRAME, DEFAULT_MIN_CONFIDENCE)
    );
}

#[test]
fn output_is_nfc_normalized() {
    let decomposed = vec![line("cafe\u{0301} lorem", 10, 10, 400, 20, 0.95)];
    let composed = vec![line("caf\u{00E9} lorem", 10, 10, 400, 20, 0.95)];
    assert_eq!(
        normalize(&decomposed, FRAME, DEFAULT_MIN_CONFIDENCE),
        normalize(&composed, FRAME, DEFAULT_MIN_CONFIDENCE)
    );
}

// ------------------------------------------------------------ §3.4 rule 4

/// The rule that matters most: normalization is layout and whitespace only.
/// A pass that "corrected" a word would put text on the user's screen that
/// was never there, and the assistant would answer about it.
#[test]
fn letters_and_digits_are_never_altered() {
    let tricky = "rn m 0O l1I 5S 8B teh recieve 3.14159 42";
    let lines = vec![line(tricky, 10, 10, 800, 20, 0.95)];
    assert_eq!(normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE), tricky);
}

// ------------------------------------------------------------------ FR-002

/// FR-002. Two captures of an unchanged screen normalize identically.
///
/// The fixture pairs model what actually differs between two frames of a
/// static screen: whitespace the engine reports differently, a pixel or two
/// of box jitter, confidence flicker, and engine line order.
#[test]
fn fr_002_an_unchanged_screen_normalizes_identically() {
    let first = vec![
        line("lorem ipsum dolor", 100, 100, 400, 24, 0.97),
        line("sit amet consectetur", 100, 130, 460, 24, 0.93),
        line("adipiscing elit sed", 100, 160, 430, 24, 0.91),
    ];
    let second = vec![
        // Reordered by the engine, jittered by a pixel, spaced differently,
        // and with confidence flicker. Same screen.
        line("adipiscing  elit   sed", 101, 161, 430, 24, 0.88),
        line("lorem ipsum  dolor", 100, 99, 400, 24, 0.95),
        line("sit amet consectetur ", 99, 130, 461, 25, 0.96),
    ];

    assert_eq!(
        normalize(&first, FRAME, DEFAULT_MIN_CONFIDENCE),
        normalize(&second, FRAME, DEFAULT_MIN_CONFIDENCE),
        "two captures of one screen must give one string, or spec 008 fires on noise"
    );
}

/// Determinism proper: the same input always gives the same output, with no
/// dependence on iteration order or hashing.
#[test]
fn normalize_is_deterministic() {
    let lines = paragraph(20, 100, 100, 24);
    let once = normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE);
    for _ in 0..8 {
        assert_eq!(normalize(&lines, FRAME, DEFAULT_MIN_CONFIDENCE), once);
    }
}

#[test]
fn an_empty_screen_normalizes_to_an_empty_string() {
    assert_eq!(normalize(&[], FRAME, DEFAULT_MIN_CONFIDENCE), "");
    let all_noise = vec![line("x", 10, 10, 10, 10, 0.1)];
    assert_eq!(normalize(&all_noise, FRAME, DEFAULT_MIN_CONFIDENCE), "");
}

/// AC-2: the generator is committed and produces nothing but lorem.
#[test]
fn ac_002_fixtures_are_synthetic() {
    for offset in 0..12 {
        let text = fixtures::phrase(offset, 6);
        for word in text.split(' ') {
            assert!(
                fixtures::WORDS.contains(&word),
                "the generator emitted something outside its word list: {word}"
            );
        }
    }
}
