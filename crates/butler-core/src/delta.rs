// Spec: specs/008-change-detection/spec.md

//! Change detection: the valve in front of inference.
//!
//! Everything before inference is cheap and local; inference is slow,
//! expensive and the only step that leaves the machine (spec 015). This
//! module turns a stream of frames into the much sparser stream of "the user
//! is now looking at something new".
//!
//! Two refinements make the outline's "call the LLM when the Levenshtein
//! distance exceeds a threshold" work in practice:
//!
//! - **Compare against the last *inferred* text, not the last frame.** A
//!   screen that drifts by one line per frame is never different enough from
//!   its predecessor to trip a threshold, but it does drift away from what was
//!   actually asked about. [`ChangeDetector::commit`] moves the baseline, and
//!   nothing else does.
//! - **Require stability.** A frame captured mid-scroll or mid-render differs
//!   from everything, including the frame after it. A change must persist
//!   across `stability_frames` consecutive evaluations before it counts, so
//!   the transient never reaches the model.
//!
//! Purity, as spec 008 §3.3 requires: no clock, no filesystem, no network, no
//! interior mutability beyond the detector's own `&mut self`. The stability
//! window is counted in frames rather than seconds, and the capture interval
//! (spec 009) is what makes that a time bound.
//!
//! [`Verdict`] here is the rich one, carrying the similarity ratio. The
//! reducer's [`crate::machine::Verdict`] is its float-free mirror; the runtime
//! host (spec 019) maps one to the other, and the [`From`] impl at the bottom
//! of this file is that mapping.

use crate::machine;

/// The strategy interface, so a later spec can add a semantic detector
/// (embeddings, structural diffing) without touching the pipeline.
///
/// v1 ships exactly one implementation, [`LevenshteinDetector`].
pub trait ChangeDetector {
    /// Judge `current` against the committed baseline.
    fn evaluate(&mut self, current: &str, input: &DetectorInput) -> Verdict;

    /// Move the baseline to `inferred`, the text inference is starting on.
    ///
    /// Called when inference *starts*, not when it finishes: the question was
    /// asked about this text, so this is what the next frame must differ from.
    fn commit(&mut self, inferred: &str);

    /// Forget the baseline and any partial stability run.
    ///
    /// Called on `Disarm` and on a monitor change, where the previous
    /// baseline describes a screen that is no longer being watched.
    fn reset(&mut self);
}

/// The per-evaluation inputs that are not the screen text itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetectorInput<'a> {
    /// The overlay's own last answer, when the overlay might be in the frame.
    ///
    /// `Some` only while capture exclusion is unverified (spec 005): a
    /// verified overlay is not in the capture at all, and subtracting text
    /// that was never there can only lose real screen content.
    pub exclude: Option<&'a str>,
    /// The user pressed "ask now": answer regardless of similarity.
    pub force: bool,
}

/// What the detector concluded, with the ratio it concluded it from.
///
/// `similarity` is `1.0` for identical text and `0.0` for text sharing
/// nothing, so a *higher* number means *less* reason to ask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// The screen still says what it said when inference last ran.
    Unchanged {
        /// Similarity to the committed baseline, in `0.0..=1.0`.
        similarity: f32,
    },
    /// Changed, but not yet stable across enough frames.
    Pending {
        /// Similarity to the committed baseline, in `0.0..=1.0`.
        similarity: f32,
        /// How many consecutive frames this candidate has been seen for.
        seen: u8,
    },
    /// Changed and stable: worth asking about.
    Changed {
        /// Similarity to the committed baseline, in `0.0..=1.0`.
        similarity: f32,
    },
}

impl Verdict {
    /// The similarity ratio, whatever the outcome.
    #[must_use]
    pub fn similarity(&self) -> f32 {
        match *self {
            Verdict::Unchanged { similarity }
            | Verdict::Pending { similarity, .. }
            | Verdict::Changed { similarity } => similarity,
        }
    }
}

/// The reducer sees the outcome without the ratio: spec 009 keeps floating
/// point out of the machine's state so its equality stays exact.
impl From<Verdict> for machine::Verdict {
    fn from(v: Verdict) -> Self {
        match v {
            Verdict::Unchanged { .. } => machine::Verdict::Unchanged,
            Verdict::Pending { .. } => machine::Verdict::Pending,
            Verdict::Changed { .. } => machine::Verdict::Changed,
        }
    }
}

/// Spec 008 §3.2 defaults, which spec 014 makes configurable.
pub const DEFAULT_THRESHOLD: f32 = 0.85;
/// Consecutive frames a change must survive before it counts.
pub const DEFAULT_STABILITY_FRAMES: u8 = 2;
/// Prefix length compared, in characters.
pub const DEFAULT_MAX_COMPARE_CHARS: usize = 6000;

/// The v1 detector: a normalized Levenshtein ratio with a stability window.
///
/// Cost is bounded by `max_compare_chars` squared. At the 6000-character
/// default that is ~36M cell updates, single-digit milliseconds, and the
/// runtime calls it off the UI thread regardless.
#[derive(Clone, Debug)]
pub struct LevenshteinDetector {
    threshold: f32,
    stability_frames: u8,
    max_compare_chars: usize,
    /// The text inference last started on. Empty until the first `commit`.
    base: String,
    /// The candidate currently accumulating stability, if any.
    candidate: Option<String>,
    /// Consecutive sightings of `candidate`, saturating at `u8::MAX`.
    seen: u8,
}

impl Default for LevenshteinDetector {
    fn default() -> Self {
        Self::new(
            DEFAULT_THRESHOLD,
            DEFAULT_STABILITY_FRAMES,
            DEFAULT_MAX_COMPARE_CHARS,
        )
    }
}

impl LevenshteinDetector {
    /// Build a detector.
    ///
    /// `threshold` is clamped to `0.0..=1.0` and `stability_frames` to at
    /// least `1`, so a nonsense configuration degrades rather than panicking:
    /// these values arrive from user settings (spec 014).
    #[must_use]
    pub fn new(threshold: f32, stability_frames: u8, max_compare_chars: usize) -> Self {
        Self {
            threshold: if threshold.is_nan() {
                DEFAULT_THRESHOLD
            } else {
                threshold.clamp(0.0, 1.0)
            },
            stability_frames: stability_frames.max(1),
            max_compare_chars,
            base: String::new(),
            candidate: None,
            seen: 0,
        }
    }

    /// The committed baseline: the text inference last started on.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Drop every line of `text` that also appears in `answer`.
    ///
    /// Matching is on the trimmed line, so indentation drift between the
    /// overlay's rendering and the OCR of it does not defeat the subtraction.
    /// This runs only when exclusion is unverified, where the alternative is
    /// asking the model about its own previous answer.
    fn subtract_answer(text: &str, answer: &str) -> String {
        let drop: Vec<&str> = answer.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let kept: Vec<&str> = text
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.is_empty() || !drop.contains(&t)
            })
            .collect();
        kept.join("\n")
    }

    /// The first `max` characters, counted in `char`s so a multi-byte
    /// boundary can never be split (FR-006 forbids a panic on any input).
    fn prefix(text: &str, max: usize) -> String {
        text.chars().take(max).collect()
    }

    /// `1 - levenshtein / max(len)`, in characters, with empty-vs-empty as
    /// perfectly similar rather than a division by zero.
    #[allow(
        clippy::cast_precision_loss,
        reason = "distances and lengths are bounded by max_compare_chars (6000 by default), \
                  far inside f32's exactly-representable integer range"
    )]
    fn similarity(a: &str, b: &str) -> f32 {
        let (a_len, b_len) = (a.chars().count(), b.chars().count());
        let longest = a_len.max(b_len);
        if longest == 0 {
            return 1.0;
        }
        let distance = strsim::levenshtein(a, b);
        1.0 - (distance as f32 / longest as f32)
    }
}

impl ChangeDetector for LevenshteinDetector {
    fn evaluate(&mut self, current: &str, input: &DetectorInput) -> Verdict {
        let compared = match input.exclude {
            Some(answer) => Self::subtract_answer(current, answer),
            None => current.to_owned(),
        };
        let compared = Self::prefix(&compared, self.max_compare_chars);
        let base = Self::prefix(&self.base, self.max_compare_chars);
        let similarity = Self::similarity(&base, &compared);

        // §3.2.1: the "ask now" shortcut answers regardless, and deliberately
        // leaves the stability run alone (FR-005) so a forced question does
        // not also arm or disarm the next organic one.
        if input.force {
            return Verdict::Changed { similarity };
        }

        // §3.2.4: close enough to what was already asked about.
        if similarity >= self.threshold {
            self.candidate = None;
            self.seen = 0;
            return Verdict::Unchanged { similarity };
        }

        // §3.2.5: a candidate continues the run when it is itself close to the
        // previous candidate; a mid-scroll frame differs from its neighbour
        // too, so it restarts the count instead of advancing it.
        let continues = self
            .candidate
            .as_ref()
            .is_some_and(|prev| Self::similarity(prev, &compared) >= self.threshold);
        self.seen = if continues { self.seen.saturating_add(1) } else { 1 };
        self.candidate = Some(compared);

        if self.seen >= self.stability_frames {
            Verdict::Changed { similarity }
        } else {
            Verdict::Pending {
                similarity,
                seen: self.seen,
            }
        }
    }

    fn commit(&mut self, inferred: &str) {
        inferred.clone_into(&mut self.base);
        self.candidate = None;
        self.seen = 0;
    }

    fn reset(&mut self) {
        self.base.clear();
        self.candidate = None;
        self.seen = 0;
    }
}
