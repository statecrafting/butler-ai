// Spec: specs/010-assistant-inference/spec.md

//! The spend guard (spec 010 §3.5).
//!
//! Pure accounting fed by the runtime's tick, so a rate limit is deterministic
//! and testable rather than dependent on a wall clock. The machine already has
//! a monotonic tick (spec 009); using it here means the tests can drive a day
//! forward in microseconds and the product cannot be confused by a clock that
//! jumps.
//!
//! A denial is not an error the user has to act on: §3.5 turns it into the
//! same outcome as "the screen did not change", so the cycle returns to idle
//! and the overlay says so once per hour rather than on every attempt.

use butler_core::settings::BudgetSettings;

/// Ticks in one minute. Spec 009's tick is 100 ms.
pub const TICKS_PER_MINUTE: u64 = 600;
/// Ticks in one hour.
pub const TICKS_PER_HOUR: u64 = TICKS_PER_MINUTE * 60;
/// Ticks in one day.
pub const TICKS_PER_DAY: u64 = TICKS_PER_HOUR * 24;

/// The default request ceilings (§3.5).
///
/// Not in `BudgetSettings` because that carries what the *user* tunes (money
/// and input size); these are the rate limits that keep a runaway loop from
/// spending it all in a minute, and they are the same for everyone.
pub const MAX_REQUESTS_PER_MINUTE: u32 = 6;
/// As above, per hour.
pub const MAX_REQUESTS_PER_HOUR: u32 = 60;
/// As above, output tokens per day.
pub const MAX_OUTPUT_TOKENS_PER_DAY: u64 = 200_000;

/// Whether a request may proceed (§3.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admit {
    /// Go ahead.
    Yes,
    /// Refused, with the window that was full.
    No {
        /// Which limit, for the once-per-hour notice.
        reason: DenyReason,
    },
}

/// Which ceiling was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenyReason {
    /// Too many requests in the last minute.
    RequestsPerMinute,
    /// Too many requests in the last hour.
    RequestsPerHour,
    /// Too many output tokens today.
    OutputTokensPerDay,
    /// The next request's input alone would exceed the user's cap.
    InputTooLarge,
}

impl DenyReason {
    /// A stable name for the UI and the log.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestsPerMinute => "requests-per-minute",
            Self::RequestsPerHour => "requests-per-hour",
            Self::OutputTokensPerDay => "output-tokens-per-day",
            Self::InputTooLarge => "input-too-large",
        }
    }
}

/// Rolling request and token accounting (§3.5).
#[derive(Debug, Default)]
pub struct SpendGuard {
    /// The tick of each admitted request, oldest first.
    admitted: std::collections::VecDeque<u64>,
    /// Output tokens spent, with the tick they were spent at.
    spent: std::collections::VecDeque<(u64, u64)>,
}

impl SpendGuard {
    /// A guard with nothing spent.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a request may proceed now, and record it if so.
    ///
    /// Recording on admission rather than on completion is deliberate: a
    /// request that is in flight has already cost a slot, and a guard that
    /// only counted finished requests would admit an unbounded number of
    /// concurrent ones. The runtime allows one at a time (spec 009 FR-004),
    /// so this is belt and braces, but it is the kind of belt that matters
    /// when a later spec relaxes that.
    pub fn admit(&mut self, now: u64, settings: &BudgetSettings) -> Admit {
        self.evict(now);

        if self.count_since(now, TICKS_PER_MINUTE) >= MAX_REQUESTS_PER_MINUTE {
            return Admit::No {
                reason: DenyReason::RequestsPerMinute,
            };
        }
        if self.count_since(now, TICKS_PER_HOUR) >= MAX_REQUESTS_PER_HOUR {
            return Admit::No {
                reason: DenyReason::RequestsPerHour,
            };
        }
        if self.tokens_since(now, TICKS_PER_DAY) >= MAX_OUTPUT_TOKENS_PER_DAY {
            return Admit::No {
                reason: DenyReason::OutputTokensPerDay,
            };
        }
        let _ = settings;

        self.admitted.push_back(now);
        Admit::Yes
    }

    /// Record what a finished inference cost.
    pub fn record(&mut self, now: u64, usage: crate::assistant::Usage) {
        self.spent.push_back((now, usage.output_tokens));
        self.evict(now);
    }

    /// Whether an input of `tokens` fits the user's cap (§3.5, spec 014).
    #[must_use]
    pub fn admits_input(tokens: u64, settings: &BudgetSettings) -> Admit {
        if tokens > u64::from(settings.max_input_tokens) {
            Admit::No {
                reason: DenyReason::InputTooLarge,
            }
        } else {
            Admit::Yes
        }
    }

    /// Whether `tick` falls inside a `window`-tick window ending now.
    ///
    /// Compares the *distance* rather than against a cutoff computed with
    /// `saturating_sub`. That distinction is not cosmetic: near tick zero the
    /// saturating cutoff collapses to 0, and an entry at tick 0 then fails a
    /// `tick > cutoff` test even though no time has passed. Three tests
    /// caught it, and every one of them was a limit that would have failed to
    /// bind on a freshly started process (D-5).
    const fn within(now: u64, tick: u64, window: u64) -> bool {
        now.saturating_sub(tick) < window
    }

    /// Requests admitted in the last `window` ticks.
    fn count_since(&self, now: u64, window: u64) -> u32 {
        u32::try_from(
            self.admitted
                .iter()
                .filter(|tick| Self::within(now, **tick, window))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// Output tokens spent in the last `window` ticks.
    fn tokens_since(&self, now: u64, window: u64) -> u64 {
        self.spent
            .iter()
            .filter(|(tick, _)| Self::within(now, *tick, window))
            .map(|(_, tokens)| *tokens)
            .sum()
    }

    /// Drop what has fallen out of the longest window.
    ///
    /// Bounded memory matters: a guard that kept every request for the life of
    /// the process would grow without limit in a product designed to run all
    /// day.
    fn evict(&mut self, now: u64) {
        while self
            .admitted
            .front()
            .is_some_and(|tick| !Self::within(now, *tick, TICKS_PER_DAY))
        {
            self.admitted.pop_front();
        }
        while self
            .spent
            .front()
            .is_some_and(|(tick, _)| !Self::within(now, *tick, TICKS_PER_DAY))
        {
            self.spent.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Admit, DenyReason, MAX_OUTPUT_TOKENS_PER_DAY, MAX_REQUESTS_PER_HOUR,
        MAX_REQUESTS_PER_MINUTE, SpendGuard, TICKS_PER_DAY, TICKS_PER_MINUTE,
    };
    use crate::assistant::Usage;
    use butler_core::settings::BudgetSettings;

    fn settings() -> BudgetSettings {
        BudgetSettings::default()
    }

    #[test]
    fn the_defaults_are_the_ones_the_spec_names() {
        assert_eq!(MAX_REQUESTS_PER_MINUTE, 6);
        assert_eq!(MAX_REQUESTS_PER_HOUR, 60);
        assert_eq!(MAX_OUTPUT_TOKENS_PER_DAY, 200_000);
        // Spec 009's tick is 100 ms.
        assert_eq!(TICKS_PER_MINUTE, 600);
    }

    /// FR-005. The seventh request in a minute is denied, and the window
    /// reopens once the first has aged out.
    #[test]
    fn fr_005_the_seventh_request_in_a_minute_is_denied() {
        let mut guard = SpendGuard::new();
        let cfg = settings();

        for i in 0..MAX_REQUESTS_PER_MINUTE {
            assert_eq!(
                guard.admit(u64::from(i), &cfg),
                Admit::Yes,
                "request {i} should be admitted"
            );
        }
        assert_eq!(
            guard.admit(6, &cfg),
            Admit::No {
                reason: DenyReason::RequestsPerMinute
            }
        );

        // A minute after the first, there is room again.
        assert_eq!(guard.admit(TICKS_PER_MINUTE + 1, &cfg), Admit::Yes);
    }

    #[test]
    fn the_hourly_ceiling_holds_after_the_minute_one_reopens() {
        let mut guard = SpendGuard::new();
        let cfg = settings();

        // Spread requests a minute apart so the per-minute limit never binds.
        let mut admitted = 0;
        for i in 0..MAX_REQUESTS_PER_HOUR {
            if guard.admit(u64::from(i) * TICKS_PER_MINUTE, &cfg) == Admit::Yes {
                admitted += 1;
            }
        }
        assert_eq!(admitted, MAX_REQUESTS_PER_HOUR);

        // One tick after the last admission, so all sixty are still inside
        // the hour. At exactly `60 * TICKS_PER_MINUTE` the first has aged out
        // and a sixty-first is legitimately admitted, which is the window
        // working rather than a bug.
        let just_after = u64::from(MAX_REQUESTS_PER_HOUR - 1) * TICKS_PER_MINUTE + 1;
        assert_eq!(
            guard.admit(just_after, &cfg),
            Admit::No {
                reason: DenyReason::RequestsPerHour
            }
        );

        // And exactly an hour after the first, there is room again.
        assert_eq!(
            guard.admit(u64::from(MAX_REQUESTS_PER_HOUR) * TICKS_PER_MINUTE, &cfg),
            Admit::Yes,
            "the oldest request ages out of the hour"
        );
    }

    #[test]
    fn the_daily_token_ceiling_denies_and_then_reopens() {
        let mut guard = SpendGuard::new();
        let cfg = settings();

        assert_eq!(guard.admit(0, &cfg), Admit::Yes);
        guard.record(
            0,
            Usage {
                input_tokens: 10,
                output_tokens: MAX_OUTPUT_TOKENS_PER_DAY,
            },
        );

        assert_eq!(
            guard.admit(TICKS_PER_MINUTE * 2, &cfg),
            Admit::No {
                reason: DenyReason::OutputTokensPerDay
            }
        );

        // A day later the spend has aged out.
        assert_eq!(guard.admit(TICKS_PER_DAY + 1, &cfg), Admit::Yes);
    }

    #[test]
    fn an_oversized_input_is_refused_before_it_is_sent() {
        let cfg = settings();
        assert_eq!(
            SpendGuard::admits_input(u64::from(cfg.max_input_tokens), &cfg),
            Admit::Yes
        );
        assert_eq!(
            SpendGuard::admits_input(u64::from(cfg.max_input_tokens) + 1, &cfg),
            Admit::No {
                reason: DenyReason::InputTooLarge
            }
        );
    }

    /// A product that runs all day must not accumulate a record per request
    /// forever.
    #[test]
    fn accounting_does_not_grow_without_bound() {
        let mut guard = SpendGuard::new();
        let cfg = settings();

        for day in 0..3_u64 {
            for i in 0..50_u64 {
                let tick = day * TICKS_PER_DAY + i * TICKS_PER_MINUTE;
                let _ = guard.admit(tick, &cfg);
                guard.record(
                    tick,
                    Usage {
                        input_tokens: 1,
                        output_tokens: 1,
                    },
                );
            }
        }
        // Only the last day's records survive.
        assert!(
            guard.spent.len() <= 60,
            "spent grew to {}",
            guard.spent.len()
        );
    }

    #[test]
    fn every_denial_has_a_stable_name() {
        for reason in [
            DenyReason::RequestsPerMinute,
            DenyReason::RequestsPerHour,
            DenyReason::OutputTokensPerDay,
            DenyReason::InputTooLarge,
        ] {
            assert!(!reason.as_str().is_empty());
            assert!(
                reason
                    .as_str()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-')
            );
        }
    }
}
