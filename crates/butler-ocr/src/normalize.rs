// Spec: specs/007-text-recognition/spec.md

//! The pure normalization pass (spec 007 §3.4).
//!
//! This is the part of the spec that makes the rest of the pipeline
//! deterministic. Engine output is noisy in ways that would make change
//! detection useless: line order varies with layout heuristics, whitespace
//! differs frame to frame, and confidence flickers on anti-aliased text. Two
//! captures of a screen nobody touched must produce the *same string*, or
//! spec 008's detector fires on nothing and the product asks the model a
//! question every few seconds.
//!
//! # What it may not do
//!
//! §3.4 rule 4: **letters and digits are never altered**. Normalization is
//! layout and whitespace only. The assistant must see what the user sees, and
//! a pass that "corrected" a word would be putting text on the user's screen
//! that was never there.
//!
//! No platform code lives here, which is what lets these rules be tested on
//! every CI target including Linux (§2, AC-1).

use unicode_normalization::UnicodeNormalization as _;
use unicode_segmentation::UnicodeSegmentation as _;

use crate::recognized::{Line, Size};

/// The default confidence floor (§3.4 rule 1).
pub const DEFAULT_MIN_CONFIDENCE: f32 = 0.5;

/// The shortest line worth keeping, in graphemes (§3.4 rule 1).
///
/// One grapheme is almost always engine noise on an icon or a border; two is
/// a real word ("no", "OK") often enough to keep.
pub const MIN_GRAPHEMES: usize = 2;

/// Turn engine lines into the pipeline's string (§3.4).
///
/// Deterministic: the same lines and the same frame always give the same
/// string, with no dependence on iteration order, hashing or time.
#[must_use]
pub fn normalize(lines: &[Line], frame: Size, min_confidence: f32) -> String {
    let kept = filter(lines, min_confidence);
    if kept.is_empty() {
        return String::new();
    }

    let ordered = reading_order(kept, frame);

    let mut out = String::new();
    for (index, line) in ordered.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&clean(&line.text));
    }
    out
}

/// §3.4 rule 1: drop the lines that are noise.
fn filter(lines: &[Line], min_confidence: f32) -> Vec<&Line> {
    lines
        .iter()
        .filter(|line| line.confidence >= min_confidence)
        .filter(|line| {
            // Graphemes, not chars: an accented letter is one thing to a
            // reader, and a two-character line of combining marks is not two
            // words.
            clean(&line.text).graphemes(true).count() >= MIN_GRAPHEMES
        })
        .collect()
}

/// §3.4 rule 2: top-to-bottom, then left-to-right, in rows.
///
/// The row tolerance is **half the median line height**. Fixed-pixel
/// tolerances break on the two things this product does constantly: a `HiDPI`
/// display, where every number doubles, and a font-size change. Deriving it
/// from the content makes the rule scale-free.
///
/// The median rather than the mean because a single tall heading should not
/// widen the tolerance for a page of body text.
fn reading_order(mut lines: Vec<&Line>, frame: Size) -> Vec<&Line> {
    let tolerance = row_tolerance(&lines, frame);

    // Sort by vertical centre first, so the row grouping below sees lines in
    // top-to-bottom order regardless of what the engine returned.
    lines.sort_by(|a, b| {
        centre_y(a)
            .partial_cmp(&centre_y(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.bbox.x.cmp(&b.bbox.x))
    });

    // Group into rows, then order each row left to right. A row is a run of
    // lines whose centres stay within `tolerance` of the row's first line:
    // comparing against the first rather than the previous stops a gentle
    // gradient of centres from merging a whole page into one row.
    let mut out: Vec<&Line> = Vec::with_capacity(lines.len());
    let mut row: Vec<&Line> = Vec::new();
    let mut anchor = 0.0_f64;

    for line in lines {
        if row.is_empty() {
            anchor = centre_y(line);
            row.push(line);
            continue;
        }
        if (centre_y(line) - anchor).abs() <= tolerance {
            row.push(line);
        } else {
            flush_row(&mut row, &mut out);
            anchor = centre_y(line);
            row.push(line);
        }
    }
    flush_row(&mut row, &mut out);
    out
}

fn flush_row<'a>(row: &mut Vec<&'a Line>, out: &mut Vec<&'a Line>) {
    row.sort_by_key(|line| line.bbox.x);
    out.append(row);
}

fn centre_y(line: &Line) -> f64 {
    // `f64` rather than `f32`: every `u32` converts exactly, so there is no
    // precision question to reason about at all.
    f64::from(line.bbox.y) + f64::from(line.bbox.height) / 2.0
}

/// Half the median line height, with a floor so degenerate input cannot make
/// the tolerance zero and put every line in its own row.
fn row_tolerance(lines: &[&Line], frame: Size) -> f64 {
    let mut heights: Vec<u32> = lines.iter().map(|line| line.bbox.height).collect();
    heights.sort_unstable();
    let median = heights.get(heights.len() / 2).copied().unwrap_or(0);

    if median == 0 {
        // Nothing to derive from. One part in two hundred of the frame is
        // about a line of body text on any display this runs on.
        (f64::from(frame.height) / 200.0).max(1.0)
    } else {
        (f64::from(median) / 2.0).max(1.0)
    }
}

/// §3.4 rule 3: whitespace, NFC, typographic punctuation, zero-width.
///
/// Rule 4 is what this function does *not* do: no letter or digit is changed.
/// The punctuation map is deliberately tiny and covers only characters an OCR
/// engine substitutes for their ASCII forms depending on the font.
fn clean(text: &str) -> String {
    let mapped: String = text
        .chars()
        .filter(|c| !is_zero_width(*c))
        .map(map_punctuation)
        .collect();

    // Collapse internal runs of whitespace to one space, and trim. This is
    // what makes two captures of the same screen agree: engines vary the
    // spacing they report between words across frames.
    let collapsed: String = mapped.split_whitespace().collect::<Vec<_>>().join(" ");

    // NFC last, so a decomposed accent produced by the mapping above is
    // composed too.
    collapsed.nfc().collect()
}

/// Characters that are invisible and would otherwise make two identical
/// screens compare unequal.
const fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'    // zero width space
            | '\u{200C}'  // zero width non-joiner
            | '\u{200D}'  // zero width joiner
            | '\u{2060}'  // word joiner
            | '\u{FEFF}' // zero width no-break space
    )
}

/// Typographic punctuation to its ASCII form.
///
/// Letters and digits are absent from this table on purpose (§3.4 rule 4).
const fn map_punctuation(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        '\u{00A0}' | '\u{2007}' | '\u{202F}' => ' ',
        other => other,
    }
}
