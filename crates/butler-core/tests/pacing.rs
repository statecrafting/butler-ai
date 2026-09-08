// Spec: specs/013-output-pacing/spec.md

//! Spec 013's functional requirements, driven by a deterministic tick.
//!
//! Every test here is a simulation: no clock, no sleep, no thread. Thirty
//! seconds of pacing costs microseconds, which is the whole reason §1 puts
//! the policy in `butler-core` rather than in a webview timer.
//!
//! Everything lives in a `pacing` module so that AC-1's command,
//! `cargo test -p butler-core pacing::`, selects exactly these tests.

mod pacing {
    use butler_core::machine::Tick;
    use butler_core::pacing::{BOUNDARY_REACH, Pacer, PacingPolicy, Release, TICKS_PER_MINUTE};

    /// Prose the pacer is measured against (FR-004).
    ///
    /// Ordinary explanatory writing with ordinary punctuation, taken from no
    /// fixture generator and tuned to no threshold: the point of a corpus is that
    /// it was not chosen to make the number come out. Its shape is what the
    /// assistant actually produces, which is what FR-004 is asking about.
    const CORPUS: &[&str] = &[
        "The overlay polls the display every few seconds, runs recognition on \
         device, and asks the model about the text only when it has meaningfully \
         changed. That last condition is doing most of the work, because a screen \
         that has not changed is a screen worth no tokens at all.",
        "Capture exclusion is a compositor feature, not an encryption boundary. \
         It stops the window appearing in a recording, a screen share, or a \
         screenshot taken through the usual interfaces; it does not stop a phone \
         pointed at the monitor, and it does not stop anything running with the \
         privileges to read the framebuffer directly.",
        "When the model streams an answer, the tokens do not arrive at a readable \
         rate. They arrive in bursts, sometimes ten words in a hundred \
         milliseconds and then nothing for half a second. Reading that is like \
         reading a page that someone keeps shaking, so the pacer buffers the \
         stream and lets it out at the rate a person reads.",
        "The state machine is a pure function: it takes the state and an event, \
         and it returns a new state and a list of effects. Nothing in it opens a \
         socket, reads a file, or looks at a clock. That constraint is what makes \
         the whole pipeline testable as a table, and it is why the pacing policy \
         lives beside it rather than in the window.",
        "Settings are a single validated document. A change either applies whole \
         or not at all; there is no state in which half the new configuration is \
         live. If the file on disk fails to parse, the defaults are used, the bad \
         file is kept beside it, and the failure is reported rather than silently \
         swallowed, because a settings file that quietly resets is worse than one \
         that complains.",
    ];

    /// Drive a pacer for `ticks` ticks, returning every release with the tick it
    /// happened on.
    fn run(pacer: &mut Pacer, ticks: u64) -> Vec<(u64, Release)> {
        let mut out = Vec::new();
        for t in 1..=ticks {
            if let Some(release) = pacer.tick(Tick(t)) {
                out.push((t, release));
            }
        }
        out
    }

    /// Words, the way the pacer counts them.
    fn word_count(text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// A synthetic answer of exactly `n` words, with no boundary characters, so
    /// boundary preference cannot perturb a timing measurement.
    fn plain_words(n: usize) -> String {
        (0..n)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// FR-001. At 220 wpm a 110-word answer spans 29 s to 31 s.
    ///
    /// 110 words at 220 words per minute is half a minute exactly. The window is
    /// two seconds wide because §3.1 lets a release stretch or shrink by up to
    /// three words to reach a boundary, and three words at this rate is a little
    /// under a second either way.
    #[test]
    fn fr_001_a_110_word_answer_at_220_wpm_takes_about_thirty_seconds() {
        let policy = PacingPolicy::default();
        assert_eq!(policy.words_per_minute, 220);

        let mut pacer = Pacer::new(policy);
        pacer.push(&plain_words(110));
        pacer.finish();

        // Forty seconds of ticks: more than the answer can possibly need, so a
        // pacer that stalls fails on the drained assertion rather than on the
        // window.
        let releases = run(&mut pacer, 400);
        assert!(pacer.is_drained(), "the answer never finished");

        let total: usize = releases.iter().map(|(_, r)| word_count(&r.text)).sum();
        assert_eq!(total, 110, "words were lost or duplicated");

        let last_tick = releases.last().expect("no releases at all").0;
        assert!(
            (290..=310).contains(&last_tick),
            "110 words at 220 wpm finished on tick {last_tick} ({}.{} s), \
             outside the 29 s to 31 s window",
            last_tick / 10,
            last_tick % 10,
        );
    }

    /// FR-001, the other rates. The window scales with the rate, which is what
    /// makes it a pacing policy rather than one tuned constant.
    #[test]
    fn fr_001_holds_across_the_settings_range() {
        for wpm in [120_u16, 180, 220, 300, 400, 600] {
            let words = 110_u64;
            let mut pacer = Pacer::new(PacingPolicy {
                words_per_minute: wpm,
                ..PacingPolicy::default()
            });
            pacer.push(&plain_words(usize::try_from(words).unwrap()));
            pacer.finish();

            // Integer arithmetic throughout: the assertion is about a tick count,
            // and a float here would make it depend on rounding.
            let rate = u64::from(wpm);
            let expected = words * u64::from(TICKS_PER_MINUTE) / rate;
            let releases = run(&mut pacer, expected * 3 + 100);
            assert!(pacer.is_drained(), "{wpm} wpm never drained");

            let last = releases.last().expect("no releases").0;
            // The same three-word reach, expressed in ticks at this rate.
            let slack =
                u64::try_from(BOUNDARY_REACH).unwrap() * u64::from(TICKS_PER_MINUTE) / rate + 1;
            assert!(
                last.abs_diff(expected) <= slack,
                "{wpm} wpm: finished on tick {last}, expected {expected} +/- {slack}",
            );
        }
    }

    /// FR-002. The lead clause goes out on the first tick after six words are
    /// buffered, even though a single tick's budget is 0.37 words.
    #[test]
    fn fr_002_the_lead_clause_ignores_the_budget() {
        let policy = PacingPolicy::default();
        let mut pacer = Pacer::new(policy);

        // Five words is not enough: the lead clause is six.
        pacer.push("one two three four five");
        assert!(pacer.tick(Tick(1)).is_none(), "released before six words");
        assert!(pacer.tick(Tick(2)).is_none());

        pacer.push(" six seven eight");
        let release = pacer.tick(Tick(3)).expect("no lead release");
        assert_eq!(release.index, 0);
        assert_eq!(word_count(&release.text), 6);
        assert_eq!(release.text, "one two three four five six");
        assert!(!release.is_last, "the stream has not finished");

        // Three ticks of budget is 1.1 words, far less than the six released.
        let budget_words = 3.0 * f64::from(policy.words_per_minute) / f64::from(TICKS_PER_MINUTE);
        assert!(budget_words < 6.0, "the premise of this test is gone");
    }

    /// FR-002, the short-answer case: a stream that ends before the lead clause
    /// is full still releases, rather than waiting for words that will never come.
    #[test]
    fn fr_002_a_short_answer_still_releases() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        pacer.push("yes.");
        assert!(pacer.tick(Tick(1)).is_none(), "released before finish");

        pacer.finish();
        let release = pacer
            .tick(Tick(2))
            .expect("a finished stream did not flush");
        assert_eq!(release.text, "yes.");
        assert!(release.is_last);
        assert!(pacer.is_drained());
    }

    /// FR-003. No release exceeds `max_burst_words`, including the ones boundary
    /// preference stretched.
    #[test]
    fn fr_003_no_release_exceeds_the_burst_cap() {
        for policy in [
            PacingPolicy::default(),
            // A rate high enough that budget outruns the cap every tick, which is
            // the only way the cap can be tested at all.
            PacingPolicy {
                words_per_minute: 600,
                max_burst_words: 4,
                first_chunk_words: 3,
                prefer_boundaries: true,
            },
            PacingPolicy {
                words_per_minute: 600,
                max_burst_words: 1,
                first_chunk_words: 6,
                prefer_boundaries: true,
            },
        ] {
            let mut pacer = Pacer::new(policy);
            for paragraph in CORPUS {
                pacer.push(paragraph);
                pacer.push(" ");
            }
            pacer.finish();

            let releases = run(&mut pacer, 20_000);
            assert!(pacer.is_drained(), "{policy:?} never drained");
            assert!(!releases.is_empty());
            for (tick, release) in &releases {
                assert!(
                    word_count(&release.text) <= usize::from(policy.max_burst_words),
                    "{policy:?}: tick {tick} released {} words",
                    word_count(&release.text),
                );
            }
        }
    }

    /// FR-004. At least 80% of releases over the corpus end on a boundary
    /// character.
    #[test]
    fn fr_004_most_releases_end_on_a_boundary() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        for paragraph in CORPUS {
            pacer.push(paragraph);
            pacer.push(" ");
        }
        pacer.finish();

        let releases = run(&mut pacer, 50_000);
        assert!(pacer.is_drained());
        assert!(releases.len() > 30, "too few releases to measure a rate");

        let on_boundary = releases
            .iter()
            .filter(|(_, r)| {
                r.text
                    .trim_end()
                    .chars()
                    .last()
                    .is_some_and(|c| matches!(c, '.' | '!' | '?' | ';' | ',' | ':' | '\n'))
            })
            .count();
        assert!(
            on_boundary * 100 >= releases.len() * 80,
            "only {on_boundary}/{} releases ({}%) ended on a boundary",
            releases.len(),
            on_boundary * 100 / releases.len(),
        );
    }

    /// FR-004's control: with `prefer_boundaries` off the rate collapses, so the
    /// test above cannot be passing because the corpus is punctuation soup.
    #[test]
    fn fr_004_has_a_negative_control() {
        let mut pacer = Pacer::new(PacingPolicy {
            prefer_boundaries: false,
            ..PacingPolicy::default()
        });
        for paragraph in CORPUS {
            pacer.push(paragraph);
            pacer.push(" ");
        }
        pacer.finish();

        let releases = run(&mut pacer, 50_000);
        let on_boundary = releases
            .iter()
            .filter(|(_, r)| r.text.trim_end().ends_with([',', '.', ';', ':', '!', '?']))
            .count();
        assert!(
            on_boundary * 100 < releases.len() * 80,
            "boundary preference makes no difference: {}% without it",
            on_boundary * 100 / releases.len(),
        );
    }

    /// FR-005. `remaining()` after `finish()` equals the number of subsequent
    /// releases exactly, for every prefix of the corpus.
    ///
    /// Exactly, not approximately: the machine returns to `Idle` when the count
    /// reaches zero (009 §3.4.7). One too many leaves the pipeline rendering
    /// forever; one too few cuts the answer off mid-sentence.
    #[test]
    fn fr_005_remaining_is_exact() {
        for wpm in [120_u16, 220, 600] {
            for paragraphs in 1..=CORPUS.len() {
                let mut pacer = Pacer::new(PacingPolicy {
                    words_per_minute: wpm,
                    ..PacingPolicy::default()
                });
                for paragraph in &CORPUS[..paragraphs] {
                    pacer.push(paragraph);
                    pacer.push(" ");
                }
                pacer.finish();

                let predicted = pacer.remaining();
                let actual = u32::try_from(run(&mut pacer, 100_000).len()).unwrap();
                assert!(pacer.is_drained());
                assert_eq!(
                    predicted, actual,
                    "{wpm} wpm, {paragraphs} paragraphs: predicted {predicted}, got {actual}",
                );
            }
        }
    }

    /// FR-005, mid-stream: the count stays exact as releases go out, which is
    /// what `Rendering.remaining_chunks` counts down.
    #[test]
    fn fr_005_remaining_stays_exact_while_draining() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        pacer.push(CORPUS[2]);
        pacer.finish();

        let mut tick = 0_u64;
        loop {
            let predicted = pacer.remaining();
            if predicted == 0 {
                break;
            }
            let mut seen = 0_u32;
            // Advance by one release and re-ask.
            loop {
                tick += 1;
                assert!(tick < 100_000, "never drained");
                if pacer.tick(Tick(tick)).is_some() {
                    seen += 1;
                    break;
                }
            }
            assert_eq!(pacer.remaining(), predicted - seen);
        }
        assert!(pacer.is_drained());
    }

    /// FR-005: an empty pacer owes nothing.
    #[test]
    fn fr_005_an_empty_pacer_has_nothing_remaining() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        assert_eq!(pacer.remaining(), 0);
        pacer.finish();
        assert_eq!(pacer.remaining(), 0);
        assert!(pacer.tick(Tick(1)).is_none());
        assert!(pacer.is_drained());
    }

    /// FR-006. `words_per_minute = 0` yields one release per `push`, verbatim,
    /// whatever the tick cadence.
    #[test]
    fn fr_006_unpaced_is_one_release_per_push() {
        let mut pacer = Pacer::new(PacingPolicy {
            words_per_minute: 0,
            ..PacingPolicy::default()
        });

        let chunks = [
            "The overlay ",
            "is a passive ",
            "renderer of what core decides.",
        ];
        for chunk in chunks {
            pacer.push(chunk);
        }
        // Every chunk was pushed before a single tick: the count cannot come from
        // the tick cadence.
        assert_eq!(pacer.remaining(), 3);
        pacer.finish();

        let releases = run(&mut pacer, 10);
        assert_eq!(releases.len(), chunks.len());
        for (i, (_, release)) in releases.iter().enumerate() {
            assert_eq!(release.index, u32::try_from(i).unwrap());
            assert_eq!(release.text, chunks[i], "unpaced mode altered the chunk");
        }
        assert!(releases.last().expect("no releases").1.is_last);
        assert!(pacer.is_drained());

        // An empty push is not a release.
        let mut pacer = Pacer::new(PacingPolicy {
            words_per_minute: 0,
            ..PacingPolicy::default()
        });
        pacer.push("");
        assert_eq!(pacer.remaining(), 0);
        assert!(pacer.tick(Tick(1)).is_none());
    }

    /// §3.1: a release contains whole words only, and a chunk that ends mid-word
    /// joins the next rather than splitting. Providers split on tokens.
    #[test]
    fn provider_chunks_that_split_a_word_are_rejoined() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        pacer.push("compos");
        pacer.push("itor");
        pacer.push("-level exclusion");
        pacer.push(" is not encryption");
        pacer.finish();

        let releases = run(&mut pacer, 10_000);
        let text: Vec<String> = releases.into_iter().map(|(_, r)| r.text).collect();
        assert_eq!(
            text.join(" "),
            "compositor-level exclusion is not encryption",
        );
    }

    /// §3.1: `is_last` is true only after `finish()` and an empty buffer, never
    /// on a release that merely emptied the buffer mid-stream.
    #[test]
    fn is_last_is_only_true_at_the_real_end() {
        let mut pacer = Pacer::new(PacingPolicy::default());
        pacer.push("the buffer is empty after this one");

        // Five hundred ticks is eighty seconds: far more than seven words need,
        // so anything still buffered is held on purpose.
        let before = run(&mut pacer, 500);
        assert!(!before.is_empty(), "nothing was released at all");
        assert!(
            before.iter().all(|r| !r.1.is_last),
            "is_last was set before finish()",
        );
        assert!(
            !pacer.is_drained(),
            "a stream that never finished cannot be drained",
        );

        pacer.finish();
        let after = run(&mut pacer, 1000);
        assert!(pacer.is_drained(), "finish did not flush the tail");
        assert_eq!(
            after.iter().filter(|r| r.1.is_last).count(),
            1,
            "exactly one release ends the answer",
        );
        assert!(
            after.last().expect("finish released nothing").1.is_last,
            "is_last belongs on the final release",
        );

        let text: Vec<String> = before
            .into_iter()
            .chain(after)
            .map(|(_, r)| r.text)
            .collect();
        assert_eq!(text.join(" "), "the buffer is empty after this one");
    }

    /// §3.2: `Dismiss` and `Disarm` drain the pacer and release nothing.
    #[test]
    fn drain_releases_nothing_and_clears() {
        for wpm in [0_u16, 220] {
            let mut pacer = Pacer::new(PacingPolicy {
                words_per_minute: wpm,
                ..PacingPolicy::default()
            });
            for paragraph in CORPUS {
                pacer.push(paragraph);
            }
            pacer.finish();

            assert!(pacer.remaining() > 0);
            assert!(pacer.drain().is_empty(), "{wpm} wpm: drain released text");
            assert_eq!(pacer.remaining(), 0);
            assert!(
                !pacer.is_drained(),
                "drain resets the stream, it does not end it"
            );
            assert!(pacer.tick(Tick(9999)).is_none());
        }
    }

    /// A pacer ticked irregularly (the runtime is not a metronome) still delivers
    /// the same words in the same order, and still finishes on budget.
    #[test]
    fn a_skipped_tick_accrues_the_budget_it_missed() {
        let words = plain_words(110);

        let mut steady = Pacer::new(PacingPolicy::default());
        steady.push(&words);
        steady.finish();
        let steady_text: String = run(&mut steady, 400)
            .into_iter()
            .map(|(_, r)| r.text)
            .collect::<Vec<_>>()
            .join(" ");

        let mut skipping = Pacer::new(PacingPolicy::default());
        skipping.push(&words);
        skipping.finish();
        let mut skipping_text = Vec::new();
        let mut finished_at = 0_u64;
        // Every fifth tick, arriving late but carrying five ticks of budget.
        for step in 1..=80_u64 {
            let now = step * 5;
            if let Some(release) = skipping.tick(Tick(now)) {
                skipping_text.push(release.text);
                finished_at = now;
            }
        }
        assert!(skipping.is_drained(), "a lumpy tick stream stalled");
        assert_eq!(skipping_text.join(" "), steady_text);
        assert!(
            (290..=315).contains(&finished_at),
            "skipped ticks did not accrue their budget: finished at {finished_at}",
        );
    }

    /// §3.2, and the seam an off-by-one would hide in: the machine's
    /// `Rendering.remaining_chunks` is `pacer.remaining()` at `InferenceDone`,
    /// and the machine returns to `Idle` exactly when the pacer drains.
    ///
    /// FR-005 asserts the count in isolation. This asserts the two halves
    /// together, because "exactly" only means anything against the thing counting
    /// down: one too many leaves the pipeline rendering forever, one too few
    /// returns to `Idle` with words still to show.
    ///
    /// What is not here is the runtime that would drive both. It is spec 019's,
    /// and its `Ports` has no production implementation yet (spec 013 D-4). The
    /// property this test pins is the one that would break silently if it did.
    #[test]
    fn the_machine_returns_to_idle_exactly_when_the_pacer_drains() {
        use butler_core::machine::{
            Cycle, Event, ExclusionState, MachineConfig, MonitorTarget, RequestId, Seq, Session,
            State, StopReason, Tick as MachineTick, Ticks, reduce,
        };

        const CFG: MachineConfig = MachineConfig {
            capture_interval_ticks: Ticks(25),
            inference_timeout_ticks: Ticks(600),
            post_answer_delay_ticks: Ticks(30),
            retry_backoff_base_ticks: Ticks(10),
            retry_backoff_max_ticks: Ticks(600),
            allow_degraded: false,
            monitor: MonitorTarget::Primary,
        };

        let request = RequestId(4);
        let mut pacer = Pacer::new(PacingPolicy::default());
        for paragraph in CORPUS {
            pacer.push(paragraph);
            pacer.push(" ");
        }
        pacer.finish();

        // §3.2: the count the runtime hands the reducer at `InferenceDone`.
        let remaining_chunks = pacer.remaining();
        assert!(remaining_chunks > 1, "a one-chunk answer proves nothing");

        let session = Session {
            tick: MachineTick(20),
            next_seq: Seq(8),
            next_request: RequestId(5),
            exclusion: ExclusionState::Verified,
            retry_attempt: 0,
        };
        let inferencing = State::Armed {
            session,
            cycle: Cycle::Inferencing {
                seq: Seq(7),
                request,
                started_tick: MachineTick(12),
            },
        };
        let (mut state, _) = reduce(
            inferencing,
            Event::InferenceDone {
                request,
                stop: StopReason::EndTurn,
                remaining_chunks,
            },
            &CFG,
        );
        assert!(
            matches!(
                &state,
                State::Armed {
                    cycle: Cycle::Rendering { .. },
                    ..
                }
            ),
            "the machine did not enter Rendering",
        );

        // Every release the pacer hands back is one `ChunkRendered`.
        let mut rendered = 0_u32;
        for t in 1..=100_000_u64 {
            let Some(release) = pacer.tick(Tick(t)) else {
                continue;
            };
            assert!(
                matches!(
                    &state,
                    State::Armed {
                        cycle: Cycle::Rendering { .. },
                        ..
                    }
                ),
                "release {} arrived after the machine left Rendering",
                release.index,
            );
            let (next, _) = reduce(state, Event::ChunkRendered { request }, &CFG);
            state = next;
            rendered += 1;
            if pacer.is_drained() {
                break;
            }
        }

        assert!(pacer.is_drained(), "the pacer never drained");
        assert_eq!(rendered, remaining_chunks, "the count was not the truth");
        assert!(
            matches!(
                &state,
                State::Armed {
                    cycle: Cycle::Idle { .. },
                    ..
                }
            ),
            "the machine is still rendering an answer that is fully on screen: {state:?}",
        );
    }
}
