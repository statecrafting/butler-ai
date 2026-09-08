// Spec: specs/013-output-pacing/spec.md

//! Output pacing (spec 013).
//!
//! Model output arrives in bursts of tokens. A streamed answer is readable
//! only if it arrives at roughly the rate a person reads: faster, and the
//! reader loses their place as the panel grows; slower, and it feels broken.
//!
//! §1 gives two reasons the policy lives here rather than in the overlay. The
//! machine needs to know when rendering is complete, which it cannot if the
//! timing is inside a webview; and the behaviour has to be testable without a
//! browser, which a `setTimeout` in a component is not.
//!
//! # Pure, and driven by the runtime's tick
//!
//! Nothing here reads a clock. [`Pacer::tick`] is called by the runtime once
//! per 100 ms tick, so a test can drive thirty seconds of pacing in
//! microseconds and assert the result exactly, which is what FR-001 does.
//!
//! # The budget is a signed debt, not a float
//!
//! Budget accrues in thousandths of a word per tick and is *debited* by every
//! word released, including the lead clause FR-002 front-loads. Going
//! negative is the point: the lead is borrowed against the reader's future
//! budget and repaid, so a 110-word answer at 220 words per minute still
//! finishes in the half-minute it should rather than 6 words early.
//!
//! Integers rather than `f32` because FR-001 asserts a tick count, and a
//! float accumulator would make that assertion depend on rounding.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::machine::Tick;
use crate::settings::PacingSettings;

/// Ticks per minute. Spec 009's tick is 100 ms.
pub const TICKS_PER_MINUTE: u32 = 600;

/// The lowest paced rate the settings allow (§3.1).
pub const MIN_WORDS_PER_MINUTE: u16 = 120;
/// The highest.
pub const MAX_WORDS_PER_MINUTE: u16 = 600;
/// The shipped rate (§3.1).
pub const DEFAULT_WORDS_PER_MINUTE: u16 = 220;

/// How far a release may stretch or shrink to reach a boundary (§3.1).
pub const BOUNDARY_REACH: usize = 3;

/// Thousandths of a word, the unit the budget is carried in.
const MILLI: u64 = 1000;

/// How the answer is released (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(deny_unknown_fields, default)]
pub struct PacingPolicy {
    /// Reading rate. `0` disables pacing: one release per provider chunk,
    /// for users who prefer raw streaming.
    pub words_per_minute: u16,
    /// The lead clause, released immediately so the answer starts quickly.
    ///
    /// Doubles as the release quantum: the pacer waits until it can afford
    /// this many words rather than dribbling out one word at a time. §1 asks
    /// for a "steady repaint cadence", and a release per word is neither
    /// steady nor calm; it also leaves boundary preference nothing to work
    /// with, since a one-word release ends wherever the word ends.
    pub first_chunk_words: u8,
    /// The most words one release may carry.
    pub max_burst_words: u8,
    /// Whether releases end on a sentence or clause boundary.
    pub prefer_boundaries: bool,
}

impl Default for PacingPolicy {
    fn default() -> Self {
        Self {
            words_per_minute: DEFAULT_WORDS_PER_MINUTE,
            first_chunk_words: 6,
            max_burst_words: 14,
            prefer_boundaries: true,
        }
    }
}

impl From<PacingSettings> for PacingPolicy {
    /// The user tunes the rate; the rest of the policy is this spec's.
    ///
    /// Spec 014 D-1 left the shape of this to spec 013, and this is the
    /// answer: `PacingSettings` carries the one value a settings panel shows,
    /// and the three that shape a release are not things a user has an
    /// opinion about.
    fn from(settings: PacingSettings) -> Self {
        Self {
            words_per_minute: settings.words_per_minute,
            ..Self::default()
        }
    }
}

impl PacingPolicy {
    /// Words per release the pacer waits for, at least one.
    fn quantum(self) -> usize {
        usize::from(self.first_chunk_words.min(self.max_burst_words)).max(1)
    }

    /// The burst cap, at least one.
    fn burst(self) -> usize {
        usize::from(self.max_burst_words).max(1)
    }
}

/// One released piece of the answer (§3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Its position in the sequence, from zero.
    pub index: u32,
    /// The words.
    pub text: String,
    /// Whether the answer is complete after this one.
    pub is_last: bool,
}

/// Buffers provider chunks and releases them at a reading pace (§3.1).
#[derive(Clone, Debug, Default)]
pub struct Pacer {
    policy: PacingPolicy,
    /// Words waiting to be released, in order. Paced mode only.
    words: VecDeque<String>,
    /// Provider chunks waiting verbatim. Unpaced mode only (§3.1).
    ///
    /// Unpaced mode is "raw streaming", so it re-emits what the provider
    /// sent rather than what this module's word splitting made of it. That is
    /// also what makes FR-006 exact: one release per `push`, whatever the
    /// tick cadence happens to be.
    chunks: VecDeque<String>,
    /// Whether the provider stream has ended.
    finished: bool,
    /// How many releases have been emitted.
    released: u32,
    /// Word budget in thousandths, signed: the lead clause borrows.
    budget: i64,
    /// The tick `tick` was last called with.
    last_tick: Option<Tick>,
    /// Whether the lead clause has gone out.
    lead_released: bool,
    /// Whether the last buffered word may still be continued.
    ///
    /// A provider chunk that ends in whitespace closes the word it completed.
    /// Without this, a stream of `"all."`, `" "`, `"Capture"` produces
    /// `"all.Capture"`: the space chunk contributes no words, and the next
    /// chunk does not begin with whitespace, so it looks like a continuation.
    open_word: bool,
}

impl Pacer {
    /// A pacer with the given policy.
    #[must_use]
    pub fn new(policy: PacingPolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    /// The policy in force.
    #[must_use]
    pub const fn policy(&self) -> PacingPolicy {
        self.policy
    }

    /// Whether pacing is off (§3.1: `words_per_minute = 0`).
    #[must_use]
    const fn unpaced(&self) -> bool {
        self.policy.words_per_minute == 0
    }

    /// A provider chunk arrived (§3.1).
    ///
    /// Paced mode splits into words here rather than at release time, so a
    /// chunk that ends mid-word joins correctly with the next: providers
    /// split on tokens, not on spaces.
    pub fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.unpaced() {
            self.chunks.push_back(text.to_owned());
            return;
        }

        // A chunk that does not begin with whitespace continues the previous
        // word rather than starting a new one, but only if that word is still
        // open: a chunk that ended in whitespace closed it.
        let continues = self.open_word && !text.starts_with(char::is_whitespace);
        self.open_word = !text.ends_with(char::is_whitespace);
        let mut parts = text.split_whitespace();

        if continues && let Some(first) = parts.next() {
            if let Some(last) = self.words.back_mut() {
                last.push_str(first);
            } else {
                self.words.push_back(first.to_owned());
            }
        }
        for word in parts {
            self.words.push_back(word.to_owned());
        }
    }

    /// The provider stream ended (§3.1).
    pub fn finish(&mut self) {
        self.finished = true;
    }

    /// Whether everything buffered has been released and the stream is over.
    #[must_use]
    pub fn is_drained(&self) -> bool {
        self.finished && self.buffered() == 0
    }

    /// Words (paced) or chunks (unpaced) still buffered.
    #[must_use]
    fn buffered(&self) -> usize {
        if self.unpaced() {
            self.chunks.len()
        } else {
            self.words.len()
        }
    }

    /// How many releases are still to come (§3.2).
    ///
    /// Fed to the machine's `Rendering.remaining_chunks`, so FR-005 requires
    /// it to equal the number of subsequent releases **exactly**: the machine
    /// returns to `Idle` when the count reaches zero, and an over-estimate
    /// would leave the pipeline rendering forever while an under-estimate
    /// would cut the answer off.
    ///
    /// Exactness rules out a formula. `take` is 3 to 9 words depending on
    /// where the next boundary falls, so `ceil(words / burst)` is not the
    /// answer; the honest count is the one the pacer itself would produce.
    /// This module is pure and deterministic, so it simulates a clone at the
    /// runtime's one-tick cadence and counts. The clone is marked finished,
    /// which is both what makes the loop terminate and what the question
    /// means: how many releases if nothing more arrives (§3.2 reads it at
    /// `InferenceDone`).
    #[must_use]
    pub fn remaining(&self) -> u32 {
        if self.buffered() == 0 {
            return 0;
        }

        let mut probe = self.clone();
        probe.finished = true;
        let mut next = probe.last_tick.map_or(0, |t| t.0);
        let mut count: u32 = 0;

        // At the slowest paced rate a word costs five ticks, so this bound is
        // two orders of magnitude of slack rather than a real limit.
        let ceiling = self.buffered().saturating_mul(600).saturating_add(1000);
        for _ in 0..ceiling {
            if probe.buffered() == 0 {
                break;
            }
            next = next.saturating_add(1);
            if probe.tick(Tick(next)).is_some() {
                count = count.saturating_add(1);
            }
        }
        debug_assert_eq!(probe.buffered(), 0, "pacing simulation did not drain");
        count
    }

    /// Release what this tick allows, if anything (§3.1).
    pub fn tick(&mut self, tick: Tick) -> Option<Release> {
        let elapsed = self.advance(tick);

        // §3.1: unpaced mode hands back exactly what arrived, one push at a
        // time, and never accrues budget.
        if self.unpaced() {
            let text = self.chunks.pop_front()?;
            return Some(self.emit(text));
        }

        self.budget = self.budget.saturating_add(self.accrual(elapsed));

        if self.words.is_empty() {
            return None;
        }

        let quantum = self.policy.quantum();

        // FR-002: the lead clause goes out on the first tick after enough is
        // buffered, whatever this tick's budget is. It is still debited, so
        // the rest of the answer repays it and FR-001's total holds.
        if !self.lead_released && (self.words.len() >= quantum || self.finished) {
            self.lead_released = true;
            let take = self.extend_to_boundary(quantum.min(self.words.len()));
            return Some(self.take(take));
        }

        let affordable = usize::try_from(self.budget.max(0) / i64::try_from(MILLI).unwrap_or(1))
            .unwrap_or(usize::MAX);

        // Wait for a whole quantum, except at the tail: once the stream has
        // ended, whatever is left is all there will ever be.
        let tail = self.finished && self.words.len() <= affordable;
        if !tail && (affordable < quantum || self.words.len() < quantum) {
            return None;
        }

        let want = affordable.min(self.policy.burst()).min(self.words.len());
        if want == 0 {
            return None;
        }
        let take = self.extend_to_boundary(want);

        // Holding for a boundary is the other half of the preference, and the
        // half FR-004's 80% actually depends on. A three-word reach around a
        // six-word release covers seven end positions, which lands on a
        // clause boundary barely three times in five; but the budget keeps
        // accruing while the pacer waits, and a boundary six words out comes
        // into reach a few ticks later at no cost to the total. Waiting is
        // free because the budget is conserved: FR-001's finish time is set
        // by the word count, not by when each release goes out.
        //
        // It cannot wait forever. Once the budget reaches the burst cap a
        // larger release is no longer possible, so the boundary is out of
        // reach for good and the words go out unaligned.
        let stalled = self.policy.prefer_boundaries
            && !self.ends_on_boundary(take)
            && affordable < self.policy.burst();
        if stalled && !tail {
            return None;
        }
        Some(self.take(take))
    }

    /// Clear without releasing (§3.2: `Dismiss` and `Disarm`).
    ///
    /// Returns an empty vector by construction: §3.2 says these "drain the
    /// pacer and the UI clears the panel", so releasing the buffer on the way
    /// out would paint text the user just dismissed. The signature returns
    /// `Vec<Release>` because §3.1 specifies it; what it must never return is
    /// a non-empty one.
    pub fn drain(&mut self) -> Vec<Release> {
        self.words.clear();
        self.chunks.clear();
        self.budget = 0;
        self.lead_released = false;
        self.finished = false;
        self.open_word = false;
        Vec::new()
    }

    /// Budget accrued over `elapsed` ticks, in thousandths of a word.
    fn accrual(&self, elapsed: u32) -> i64 {
        let milliwords = u64::from(self.policy.words_per_minute) * MILLI * u64::from(elapsed)
            / u64::from(TICKS_PER_MINUTE);
        i64::try_from(milliwords).unwrap_or(i64::MAX)
    }

    /// Ticks since the last call, and record this one.
    fn advance(&mut self, tick: Tick) -> u32 {
        let elapsed = match self.last_tick {
            // The first tick counts as one, so budget accrues from the start.
            None => 1,
            Some(previous) => u32::try_from(tick.0.saturating_sub(previous.0)).unwrap_or(u32::MAX),
        };
        self.last_tick = Some(tick);
        elapsed
    }

    /// Stretch or shrink a release by up to [`BOUNDARY_REACH`] words so it
    /// ends on a boundary character (§3.1).
    ///
    /// Shrinking is tried first: ending early on a full stop reads better
    /// than running three words past one.
    fn extend_to_boundary(&self, take: usize) -> usize {
        if !self.policy.prefer_boundaries || take == 0 || self.ends_on_boundary(take) {
            return take;
        }

        for back in 1..=BOUNDARY_REACH.min(take.saturating_sub(1)) {
            if self.ends_on_boundary(take - back) {
                return take - back;
            }
        }
        for forward in 1..=BOUNDARY_REACH {
            let candidate = take + forward;
            // FR-003 is a hard cap: the boundary is a preference, the burst
            // limit is not.
            if candidate > self.words.len() || candidate > self.policy.burst() {
                break;
            }
            if self.ends_on_boundary(candidate) {
                return candidate;
            }
        }
        take
    }

    /// Whether the `take`-th word ends on a boundary character.
    fn ends_on_boundary(&self, take: usize) -> bool {
        take > 0
            && self
                .words
                .get(take - 1)
                .and_then(|word| word.chars().last())
                .is_some_and(is_boundary)
    }

    /// Take `count` words as one release, debiting the budget.
    fn take(&mut self, count: usize) -> Release {
        let mut text = String::new();
        for i in 0..count {
            if i > 0 {
                text.push(' ');
            }
            if let Some(word) = self.words.pop_front() {
                text.push_str(&word);
            }
        }
        let cost = i64::try_from(count)
            .unwrap_or(0)
            .saturating_mul(i64::try_from(MILLI).unwrap_or(1));
        self.budget = self.budget.saturating_sub(cost);
        self.emit(text)
    }

    /// Wrap released text with its index and end-of-answer flag.
    fn emit(&mut self, text: String) -> Release {
        let index = self.released;
        self.released += 1;
        Release {
            index,
            text,
            // §3.1: true only after `finish()` with nothing left.
            is_last: self.finished && self.buffered() == 0,
        }
    }
}

/// The characters §3.1 calls boundaries.
const fn is_boundary(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | ';' | ',' | ':' | '\n')
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_WORDS_PER_MINUTE, MAX_WORDS_PER_MINUTE, MIN_WORDS_PER_MINUTE, PacingPolicy,
    };
    use crate::settings::PacingSettings;

    #[test]
    fn the_defaults_are_the_ones_the_spec_names() {
        let policy = PacingPolicy::default();
        assert_eq!(policy.words_per_minute, 220);
        assert_eq!(policy.first_chunk_words, 6);
        assert_eq!(policy.max_burst_words, 14);
        assert!(policy.prefer_boundaries);

        assert_eq!(DEFAULT_WORDS_PER_MINUTE, 220);
        assert_eq!(MIN_WORDS_PER_MINUTE, 120);
        assert_eq!(MAX_WORDS_PER_MINUTE, 600);
    }

    /// Spec 014 D-1 left the shape of this to this spec, and the settings
    /// default must be the policy's or a fresh install paces differently from
    /// the documented rate.
    #[test]
    fn the_settings_default_is_the_policy_default() {
        assert_eq!(
            PacingSettings::default().words_per_minute,
            DEFAULT_WORDS_PER_MINUTE
        );
        assert_eq!(
            PacingPolicy::from(PacingSettings::default()),
            PacingPolicy::default()
        );
    }
}
