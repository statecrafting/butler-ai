// Spec: specs/009-pipeline-state-machine/spec.md

//! The transition table and the properties of the pipeline reducer.
//!
//! Everything lives in a `machine` module so that spec 009 AC-1's command,
//! `cargo test -p butler-core machine::`, selects exactly these tests.
//!
//! The table in [`machine::transitions`] is the normative one: spec 009 §3.4
//! numbers the transitions and every row cites its clause. The same table
//! renders the Mermaid diagram in `docs/architecture.md` §3, and a test diffs
//! the two, so the picture cannot drift from the behaviour (AC-2).

mod machine {
    use butler_core::machine::{
        Cycle, DegradedReason, Effect, ErrorKind, Event, ExclusionState, FaultKind, Level,
        MachineConfig, MonitorTarget, Notice, RequestId, Seq, Session, State, StopReason, Tick,
        Ticks, Verdict, reduce,
    };
    use proptest::prelude::{ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, proptest};
    use proptest::test_runner::FileFailurePersistence;
    use std::fmt::Write as _;

    // ----------------------------------------------------------------- setup

    /// The reference configuration: the defaults spec 014 §3.1 documents,
    /// expressed in ticks, with degraded mode refused.
    const CFG: MachineConfig = cfg(false);
    /// The same, for a user who consented to running without verified
    /// exclusion.
    const CFG_DEGRADED: MachineConfig = cfg(true);

    const fn cfg(allow_degraded: bool) -> MachineConfig {
        MachineConfig {
            capture_interval_ticks: Ticks(25),
            inference_timeout_ticks: Ticks(600),
            post_answer_delay_ticks: Ticks(30),
            retry_backoff_base_ticks: Ticks(10),
            retry_backoff_max_ticks: Ticks(600),
            allow_degraded,
            monitor: MonitorTarget::Primary,
        }
    }

    const fn sess(
        tick: u64,
        next_seq: u64,
        next_request: u64,
        exclusion: ExclusionState,
        retry_attempt: u32,
    ) -> Session {
        Session {
            tick: Tick(tick),
            next_seq: Seq(next_seq),
            next_request: RequestId(next_request),
            exclusion,
            retry_attempt,
        }
    }

    const VERIFIED: ExclusionState = ExclusionState::Verified;

    const fn armed(session: Session, cycle: Cycle) -> State {
        State::Armed { session, cycle }
    }

    const fn idle(ticks: u32) -> Cycle {
        Cycle::Idle {
            next_capture_in: Ticks(ticks),
        }
    }

    fn fault(error: FaultKind, retry_in: u32, resume: State) -> State {
        State::Fault {
            error,
            retry_in: Ticks(retry_in),
            cycle_state: Box::new(resume),
        }
    }

    /// One normative transition.
    struct Transition {
        /// The clause of spec 009 §3.4 (or the FR) this row is evidence for.
        clause: &'static str,
        /// Which configuration the row runs under.
        cfg: MachineConfig,
        from: State,
        event: Event,
        to: State,
        effects: Vec<Effect>,
        /// The label this row contributes to the rendered diagram, if it is
        /// a transition worth drawing. Rows that drop, refuse or merely
        /// count down are tested but not drawn.
        edge: Option<&'static str>,
    }

    // ------------------------------------------------------------- the table

    #[allow(clippy::too_many_lines)]
    fn transitions() -> Vec<Transition> {
        vec![
            Transition {
                clause: "§3.4.1 arm with exclusion verified",
                cfg: CFG,
                from: State::Disarmed {
                    session: sess(0, 0, 0, VERIFIED, 0),
                },
                event: Event::Arm,
                to: armed(sess(0, 0, 0, VERIFIED, 0), idle(0)),
                effects: vec![
                    Effect::RunSelfTest,
                    Effect::Emit(Notice::Status),
                    Effect::ScheduleTick,
                ],
                edge: Some("Arm (exclusion Verified)"),
            },
            Transition {
                clause: "§3.4.1 arm into degraded mode with the user's consent",
                cfg: CFG_DEGRADED,
                from: State::Disarmed {
                    session: sess(0, 0, 0, ExclusionState::Compromised, 0),
                },
                event: Event::Arm,
                to: State::Degraded {
                    session: sess(0, 0, 0, ExclusionState::Compromised, 0),
                    reason: DegradedReason::Exclusion(ExclusionState::Compromised),
                    cycle: Some(idle(0)),
                },
                effects: vec![
                    Effect::RunSelfTest,
                    Effect::Emit(Notice::Status),
                    Effect::ScheduleTick,
                ],
                edge: Some("Arm (exclusion not Verified, allow_degraded)"),
            },
            Transition {
                clause: "§3.4.1 / FR-006 arm refused without consent",
                cfg: CFG,
                from: State::Disarmed {
                    session: sess(0, 0, 0, ExclusionState::Unsupported, 0),
                },
                event: Event::Arm,
                to: State::Disarmed {
                    session: sess(0, 0, 0, ExclusionState::Unsupported, 0),
                },
                effects: vec![
                    Effect::RunSelfTest,
                    Effect::Emit(Notice::Status),
                    Effect::Log(Level::Warn, "arm-refused-exclusion-not-verified"),
                ],
                edge: None,
            },
            Transition {
                clause: "§3.4.2 the idle countdown",
                cfg: CFG,
                from: armed(sess(3, 0, 0, VERIFIED, 0), idle(2)),
                event: Event::Tick,
                to: armed(sess(4, 0, 0, VERIFIED, 0), idle(1)),
                effects: vec![Effect::ScheduleTick],
                edge: None,
            },
            Transition {
                clause: "§3.4.2 the countdown reaches zero and a frame is claimed",
                cfg: CFG,
                from: armed(sess(9, 7, 0, VERIFIED, 0), idle(0)),
                event: Event::Tick,
                to: armed(
                    sess(10, 8, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                effects: vec![
                    Effect::Capture {
                        seq: Seq(7),
                        monitor: MonitorTarget::Primary,
                    },
                    Effect::ScheduleTick,
                ],
                edge: Some("Tick reaches 0"),
            },
            Transition {
                clause: "§3.4.2 ask now jumps the countdown and is remembered",
                cfg: CFG,
                from: armed(sess(5, 7, 0, VERIFIED, 0), idle(20)),
                event: Event::ForceCapture,
                to: armed(
                    sess(5, 8, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(7),
                        force: true,
                    },
                ),
                effects: vec![Effect::Capture {
                    seq: Seq(7),
                    monitor: MonitorTarget::Primary,
                }],
                edge: Some("ForceCapture"),
            },
            Transition {
                clause: "§3.4.2 / FR-004 ask now is refused during an inference",
                cfg: CFG,
                from: armed(
                    sess(5, 7, 3, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(6),
                        request: RequestId(2),
                        started_tick: Tick(4),
                    },
                ),
                event: Event::ForceCapture,
                to: armed(
                    sess(5, 7, 3, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(6),
                        request: RequestId(2),
                        started_tick: Tick(4),
                    },
                ),
                effects: vec![Effect::Log(Level::Warn, "force-capture-during-inference")],
                edge: None,
            },
            Transition {
                clause: "§3.4.3 the frame the cycle asked for",
                cfg: CFG,
                from: armed(
                    sess(10, 8, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Captured { seq: Seq(7) },
                to: armed(
                    sess(10, 8, 0, VERIFIED, 0),
                    Cycle::Recognizing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                effects: vec![Effect::Recognize { seq: Seq(7) }],
                edge: Some("Captured (seq matches)"),
            },
            Transition {
                clause: "§3.4.3 / FR-003 an out-of-order frame is released, not recognized",
                cfg: CFG,
                from: armed(
                    sess(10, 8, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Captured { seq: Seq(6) },
                to: armed(
                    sess(10, 8, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(6) },
                    Effect::Log(Level::Warn, "frame-dropped"),
                ],
                edge: None,
            },
            Transition {
                clause: "§3.4.4 recognized text goes to the detector, carrying the force flag",
                cfg: CFG,
                from: armed(
                    sess(11, 8, 0, VERIFIED, 0),
                    Cycle::Recognizing {
                        seq: Seq(7),
                        force: true,
                    },
                ),
                event: Event::Recognized {
                    seq: Seq(7),
                    text_len: 412,
                    mean_conf: 0.94,
                },
                to: armed(
                    sess(11, 8, 0, VERIFIED, 0),
                    Cycle::Evaluating {
                        seq: Seq(7),
                        force: true,
                    },
                ),
                effects: vec![Effect::Evaluate {
                    seq: Seq(7),
                    force: true,
                }],
                edge: Some("Recognized"),
            },
            Transition {
                clause: "§3.4.5 nothing changed: back to idle, frame released",
                cfg: CFG,
                from: armed(
                    sess(12, 8, 0, VERIFIED, 0),
                    Cycle::Evaluating {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Evaluated {
                    seq: Seq(7),
                    verdict: Verdict::Unchanged,
                },
                to: armed(sess(12, 8, 0, VERIFIED, 0), idle(25)),
                effects: vec![Effect::ReleaseFrame { seq: Seq(7) }],
                edge: Some("Evaluated (Unchanged / Pending)"),
            },
            Transition {
                clause: "§3.4.5 a change that is not yet stable is treated the same",
                cfg: CFG,
                from: armed(
                    sess(12, 8, 0, VERIFIED, 0),
                    Cycle::Evaluating {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Evaluated {
                    seq: Seq(7),
                    verdict: Verdict::Pending,
                },
                to: armed(sess(12, 8, 0, VERIFIED, 0), idle(25)),
                effects: vec![Effect::ReleaseFrame { seq: Seq(7) }],
                edge: None,
            },
            Transition {
                clause: "§3.4.5 a real change starts the one inference",
                cfg: CFG,
                from: armed(
                    sess(12, 8, 4, VERIFIED, 0),
                    Cycle::Evaluating {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Evaluated {
                    seq: Seq(7),
                    verdict: Verdict::Changed,
                },
                to: armed(
                    sess(12, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                effects: vec![
                    Effect::StartInference {
                        request: RequestId(4),
                        seq: Seq(7),
                    },
                    Effect::ReleaseFrame { seq: Seq(7) },
                    Effect::Emit(Notice::AnswerStarted {
                        request: RequestId(4),
                    }),
                ],
                edge: Some("Evaluated (Changed)"),
            },
            Transition {
                clause: "§3.4.6 a provider chunk asks the pacer for a release",
                cfg: CFG,
                from: armed(
                    sess(13, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::InferenceChunk {
                    request: RequestId(4),
                    chunk_index: 0,
                },
                to: armed(
                    sess(13, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                effects: vec![Effect::EmitChunk {
                    request: RequestId(4),
                }],
                edge: None,
            },
            Transition {
                clause: "§3.4.6 / FR-004 a frame taken during an inference is released, never queued",
                cfg: CFG,
                from: armed(
                    sess(13, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::Captured { seq: Seq(9) },
                to: armed(
                    sess(13, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(9) },
                    Effect::Log(Level::Warn, "frame-dropped"),
                ],
                edge: None,
            },
            Transition {
                clause: "§3.4.6 a tick before the timeout only advances the clock",
                cfg: CFG,
                from: armed(
                    sess(13, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::Tick,
                to: armed(
                    sess(14, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                effects: vec![Effect::ScheduleTick],
                edge: None,
            },
            Transition {
                clause: "§3.4.6 an inference that never lands is a fault",
                cfg: CFG,
                from: armed(
                    sess(611, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(11),
                    },
                ),
                event: Event::Tick,
                to: fault(
                    FaultKind::InferenceTimeout,
                    10,
                    armed(sess(612, 8, 5, VERIFIED, 1), idle(0)),
                ),
                effects: vec![
                    Effect::CancelInference {
                        request: RequestId(4),
                    },
                    Effect::Emit(Notice::AnswerFailed {
                        request: RequestId(4),
                        error: ErrorKind::Network,
                    }),
                    Effect::ScheduleTick,
                ],
                edge: Some("inference timeout"),
            },
            Transition {
                clause: "§3.4.6 the stream ends and the pacer still has chunks",
                cfg: CFG,
                from: armed(
                    sess(20, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::InferenceDone {
                    request: RequestId(4),
                    stop: StopReason::EndTurn,
                    remaining_chunks: 3,
                },
                to: armed(
                    sess(20, 8, 5, VERIFIED, 0),
                    Cycle::Rendering {
                        seq: Seq(7),
                        request: RequestId(4),
                        remaining_chunks: 3,
                    },
                ),
                effects: vec![],
                edge: Some("InferenceDone"),
            },
            Transition {
                clause: "§3.4.6 a retryable provider failure backs off",
                cfg: CFG,
                from: armed(
                    sess(20, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::InferenceFailed {
                    request: RequestId(4),
                    error: ErrorKind::Network,
                    retryable: true,
                    jitter_pct: 20,
                },
                to: fault(
                    FaultKind::Inference(ErrorKind::Network),
                    12,
                    armed(sess(20, 8, 5, VERIFIED, 1), idle(0)),
                ),
                effects: vec![Effect::Emit(Notice::AnswerFailed {
                    request: RequestId(4),
                    error: ErrorKind::Network,
                })],
                edge: Some("InferenceFailed (retryable)"),
            },
            Transition {
                clause: "§3.4.6 a permanent provider failure just ends the cycle",
                cfg: CFG,
                from: armed(
                    sess(20, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::InferenceFailed {
                    request: RequestId(4),
                    error: ErrorKind::Provider,
                    retryable: false,
                    jitter_pct: 0,
                },
                to: armed(sess(20, 8, 5, VERIFIED, 0), idle(25)),
                effects: vec![Effect::Emit(Notice::AnswerFailed {
                    request: RequestId(4),
                    error: ErrorKind::Provider,
                })],
                edge: Some("InferenceFailed (not retryable)"),
            },
            Transition {
                clause: "§3.4.7 rendering counts down",
                cfg: CFG,
                from: armed(
                    sess(21, 8, 5, VERIFIED, 0),
                    Cycle::Rendering {
                        seq: Seq(7),
                        request: RequestId(4),
                        remaining_chunks: 3,
                    },
                ),
                event: Event::ChunkRendered {
                    request: RequestId(4),
                },
                to: armed(
                    sess(21, 8, 5, VERIFIED, 0),
                    Cycle::Rendering {
                        seq: Seq(7),
                        request: RequestId(4),
                        remaining_chunks: 2,
                    },
                ),
                effects: vec![],
                edge: None,
            },
            Transition {
                clause: "§3.4.7 the last chunk gives the reader time before the next capture",
                cfg: CFG,
                from: armed(
                    sess(25, 8, 5, VERIFIED, 0),
                    Cycle::Rendering {
                        seq: Seq(7),
                        request: RequestId(4),
                        remaining_chunks: 1,
                    },
                ),
                event: Event::ChunkRendered {
                    request: RequestId(4),
                },
                to: armed(sess(25, 8, 5, VERIFIED, 0), idle(30)),
                effects: vec![Effect::Emit(Notice::AnswerDone {
                    request: RequestId(4),
                })],
                edge: Some("ChunkRendered (last)"),
            },
            Transition {
                clause: "§3.4.8 / FR-005 disarming cancels the inference in flight",
                cfg: CFG,
                from: armed(
                    sess(20, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::Disarm,
                to: State::Disarmed {
                    session: sess(20, 8, 5, VERIFIED, 0),
                },
                effects: vec![
                    Effect::CancelInference {
                        request: RequestId(4),
                    },
                    Effect::ResetDetector,
                    Effect::Emit(Notice::Status),
                ],
                edge: Some("Disarm"),
            },
            Transition {
                clause: "§3.4.8 / FR-005 disarming releases the frame in the slot",
                cfg: CFG,
                from: armed(
                    sess(11, 8, 0, VERIFIED, 0),
                    Cycle::Recognizing {
                        seq: Seq(7),
                        force: false,
                    },
                ),
                event: Event::Disarm,
                to: State::Disarmed {
                    session: sess(11, 8, 0, VERIFIED, 0),
                },
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(7) },
                    Effect::ResetDetector,
                    Effect::Emit(Notice::Status),
                ],
                edge: None,
            },
            Transition {
                clause: "§3.4.9 a locked screen disarms",
                cfg: CFG,
                from: armed(sess(30, 8, 0, VERIFIED, 0), idle(5)),
                event: Event::ScreenLocked,
                to: State::Disarmed {
                    session: sess(30, 8, 0, VERIFIED, 0),
                },
                effects: vec![Effect::ResetDetector, Effect::Emit(Notice::Status)],
                edge: Some("ScreenLocked"),
            },
            Transition {
                clause: "§3.4.9 unlocking does not re-arm: that is the user's call",
                cfg: CFG,
                from: State::Disarmed {
                    session: sess(30, 8, 0, VERIFIED, 0),
                },
                event: Event::ScreenUnlocked,
                to: State::Disarmed {
                    session: sess(30, 8, 0, VERIFIED, 0),
                },
                effects: vec![Effect::Log(Level::Info, "screen-unlocked-not-rearmed")],
                edge: None,
            },
            Transition {
                clause: "§3.4.10 exclusion lost, with consent: keep the cycle, tell the user",
                cfg: CFG_DEGRADED,
                from: armed(sess(30, 8, 0, VERIFIED, 0), idle(10)),
                event: Event::ExclusionChanged {
                    status: ExclusionState::Compromised,
                },
                to: State::Degraded {
                    session: sess(30, 8, 0, ExclusionState::Compromised, 0),
                    reason: DegradedReason::Exclusion(ExclusionState::Compromised),
                    cycle: Some(idle(10)),
                },
                effects: vec![Effect::Emit(Notice::Status)],
                edge: Some("ExclusionChanged (not Verified)"),
            },
            Transition {
                clause: "§3.4.10 / FR-006 exclusion lost, without consent: stop everything",
                cfg: CFG,
                from: armed(
                    sess(30, 8, 5, VERIFIED, 0),
                    Cycle::Inferencing {
                        seq: Seq(7),
                        request: RequestId(4),
                        started_tick: Tick(12),
                    },
                ),
                event: Event::ExclusionChanged {
                    status: ExclusionState::Unsupported,
                },
                to: State::Disarmed {
                    session: sess(30, 8, 5, ExclusionState::Unsupported, 0),
                },
                effects: vec![
                    Effect::CancelInference {
                        request: RequestId(4),
                    },
                    Effect::ResetDetector,
                    Effect::Emit(Notice::Status),
                ],
                edge: None,
            },
            Transition {
                clause: "§3.4.10 verification restores the armed session where it left off",
                cfg: CFG_DEGRADED,
                from: State::Degraded {
                    session: sess(31, 8, 0, ExclusionState::Compromised, 0),
                    reason: DegradedReason::Exclusion(ExclusionState::Compromised),
                    cycle: Some(idle(10)),
                },
                event: Event::ExclusionChanged { status: VERIFIED },
                to: armed(sess(31, 8, 0, VERIFIED, 0), idle(10)),
                effects: vec![Effect::Emit(Notice::Status)],
                edge: Some("ExclusionChanged (Verified)"),
            },
            Transition {
                clause: "§3.4.3 a failed capture faults with the first backoff",
                cfg: CFG,
                from: armed(
                    sess(40, 9, 0, VERIFIED, 0),
                    Cycle::Capturing {
                        seq: Seq(8),
                        force: false,
                    },
                ),
                event: Event::CaptureFailed {
                    seq: Seq(8),
                    error: ErrorKind::Capture,
                    jitter_pct: 0,
                },
                to: fault(
                    FaultKind::Capture(ErrorKind::Capture),
                    10,
                    armed(sess(40, 9, 0, VERIFIED, 1), idle(0)),
                ),
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(8) },
                    Effect::Emit(Notice::Status),
                ],
                edge: Some("CaptureFailed"),
            },
            Transition {
                clause: "§3.4.11 a second consecutive fault doubles the delay, jitter included",
                cfg: CFG,
                from: armed(
                    sess(41, 9, 0, VERIFIED, 1),
                    Cycle::Recognizing {
                        seq: Seq(8),
                        force: false,
                    },
                ),
                event: Event::RecognizeFailed {
                    seq: Seq(8),
                    error: ErrorKind::Ocr,
                    jitter_pct: -10,
                },
                to: fault(
                    FaultKind::Recognize(ErrorKind::Ocr),
                    18,
                    armed(sess(41, 9, 0, VERIFIED, 2), idle(0)),
                ),
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(8) },
                    Effect::Emit(Notice::Status),
                ],
                edge: Some("RecognizeFailed"),
            },
            Transition {
                clause: "§3.4.11 the fault counts down",
                cfg: CFG,
                from: fault(
                    FaultKind::Capture(ErrorKind::Capture),
                    3,
                    armed(sess(40, 9, 0, VERIFIED, 1), idle(0)),
                ),
                event: Event::Tick,
                to: fault(
                    FaultKind::Capture(ErrorKind::Capture),
                    2,
                    armed(sess(41, 9, 0, VERIFIED, 1), idle(0)),
                ),
                effects: vec![Effect::ScheduleTick],
                edge: None,
            },
            Transition {
                clause: "§3.4.11 at zero the interrupted state resumes, idle and ready",
                cfg: CFG,
                from: fault(
                    FaultKind::Capture(ErrorKind::Capture),
                    1,
                    armed(sess(45, 9, 0, VERIFIED, 1), idle(0)),
                ),
                event: Event::Tick,
                to: armed(sess(46, 9, 0, VERIFIED, 1), idle(0)),
                effects: vec![
                    Effect::Log(Level::Info, "fault-retry"),
                    Effect::ScheduleTick,
                ],
                edge: Some("retry_in reaches 0"),
            },
            Transition {
                clause: "§3.4.8 disarming abandons a fault and clears the backoff",
                cfg: CFG,
                from: fault(
                    FaultKind::Capture(ErrorKind::Capture),
                    5,
                    armed(sess(42, 9, 0, VERIFIED, 1), idle(0)),
                ),
                event: Event::Disarm,
                to: State::Disarmed {
                    session: sess(42, 9, 0, VERIFIED, 0),
                },
                effects: vec![Effect::ResetDetector, Effect::Emit(Notice::Status)],
                edge: Some("Disarm"),
            },
            Transition {
                clause: "§3.4.12 a new display resets the detector and restarts the cycle",
                cfg: CFG,
                from: armed(
                    sess(50, 9, 0, VERIFIED, 0),
                    Cycle::Recognizing {
                        seq: Seq(8),
                        force: false,
                    },
                ),
                event: Event::MonitorChanged,
                to: armed(sess(50, 9, 0, VERIFIED, 0), idle(0)),
                effects: vec![Effect::ReleaseFrame { seq: Seq(8) }, Effect::ResetDetector],
                edge: None,
            },
            Transition {
                clause: "§3.4.12 a shortened capture interval does not wait out the old one",
                cfg: CFG,
                from: armed(sess(50, 9, 0, VERIFIED, 0), idle(24)),
                event: Event::SettingsChanged {
                    capture_interval: Ticks(10),
                },
                to: armed(sess(50, 9, 0, VERIFIED, 0), idle(10)),
                effects: vec![Effect::Log(Level::Info, "settings-changed")],
                edge: None,
            },
            Transition {
                clause: "§3.4 totality: a pair with no transition changes nothing",
                cfg: CFG,
                from: State::Disarmed {
                    session: sess(0, 0, 0, VERIFIED, 0),
                },
                event: Event::InferenceChunk {
                    request: RequestId(1),
                    chunk_index: 0,
                },
                to: State::Disarmed {
                    session: sess(0, 0, 0, VERIFIED, 0),
                },
                effects: vec![Effect::Log(Level::Warn, "ignored")],
                edge: None,
            },
            Transition {
                clause: "§3.4 totality: a frame that outlived its session is still released",
                cfg: CFG,
                from: State::Disarmed {
                    session: sess(0, 9, 0, VERIFIED, 0),
                },
                event: Event::Captured { seq: Seq(8) },
                to: State::Disarmed {
                    session: sess(0, 9, 0, VERIFIED, 0),
                },
                effects: vec![
                    Effect::ReleaseFrame { seq: Seq(8) },
                    Effect::Log(Level::Warn, "stray-frame-released"),
                ],
                edge: None,
            },
        ]
    }

    // ------------------------------------------------------- the table tests

    #[test]
    fn transition_table() {
        for row in transitions() {
            let (next, effects) = reduce(row.from.clone(), row.event.clone(), &row.cfg);
            assert_eq!(
                next, row.to,
                "{}: {:?} + {:?} produced the wrong state",
                row.clause, row.from, row.event
            );
            assert_eq!(
                effects, row.effects,
                "{}: {:?} + {:?} produced the wrong effects",
                row.clause, row.from, row.event
            );
        }
    }

    #[test]
    fn the_machine_starts_disarmed() {
        assert_eq!(
            State::default(),
            State::Disarmed {
                session: Session::default()
            }
        );
        assert!(!State::default().wants_ticks());
    }

    #[test]
    fn the_reference_configuration_is_the_default() {
        assert_eq!(CFG, MachineConfig::default());
    }

    // ------------------------------------------------------- named scenarios

    /// FR-003. The guard is on the sequence number, not on arrival order, so
    /// a late frame from the previous cycle cannot displace the current one.
    #[test]
    fn fr_003_out_of_order_frames_are_dropped() {
        let waiting = armed(
            sess(10, 6, 0, VERIFIED, 0),
            Cycle::Capturing {
                seq: Seq(5),
                force: false,
            },
        );

        let (after_stale, effects) = reduce(waiting.clone(), Event::Captured { seq: Seq(4) }, &CFG);
        assert_eq!(
            after_stale, waiting,
            "a frame the cycle is not waiting for changes nothing"
        );
        assert_eq!(
            effects,
            vec![
                Effect::ReleaseFrame { seq: Seq(4) },
                Effect::Log(Level::Warn, "frame-dropped"),
            ]
        );

        let (after_current, effects) = reduce(after_stale, Event::Captured { seq: Seq(5) }, &CFG);
        assert_eq!(
            after_current,
            armed(
                sess(10, 6, 0, VERIFIED, 0),
                Cycle::Recognizing {
                    seq: Seq(5),
                    force: false
                }
            )
        );
        assert_eq!(effects, vec![Effect::Recognize { seq: Seq(5) }]);
    }

    /// A full cycle, arm to answer, as the runtime host will drive it.
    #[test]
    fn one_whole_cycle() {
        let mut state = State::Disarmed {
            session: sess(0, 0, 0, VERIFIED, 0),
        };
        let mut log = Vec::new();
        for event in [
            Event::Arm,
            Event::Tick,
            Event::Captured { seq: Seq(0) },
            Event::Recognized {
                seq: Seq(0),
                text_len: 900,
                mean_conf: 0.97,
            },
            Event::Evaluated {
                seq: Seq(0),
                verdict: Verdict::Changed,
            },
            Event::InferenceChunk {
                request: RequestId(0),
                chunk_index: 0,
            },
            Event::InferenceDone {
                request: RequestId(0),
                stop: StopReason::EndTurn,
                remaining_chunks: 1,
            },
            Event::ChunkRendered {
                request: RequestId(0),
            },
        ] {
            let (next, effects) = reduce(state, event, &CFG);
            state = next;
            log.extend(effects);
        }
        assert_eq!(state, armed(sess(1, 1, 1, VERIFIED, 0), idle(30)));
        assert_eq!(
            log.iter()
                .filter(|e| matches!(e, Effect::StartInference { .. }))
                .count(),
            1
        );
        assert!(log.contains(&Effect::ReleaseFrame { seq: Seq(0) }));
        assert!(log.contains(&Effect::Emit(Notice::AnswerDone {
            request: RequestId(0)
        })));
    }

    /// §3.4.11. Consecutive faults double the delay up to the ceiling; a
    /// cycle that gets through resets the count.
    #[test]
    fn the_backoff_grows_and_then_resets() {
        let delays: Vec<u32> = (0..8)
            .map(|attempts| {
                let capturing = armed(
                    sess(0, 1, 0, VERIFIED, attempts),
                    Cycle::Capturing {
                        seq: Seq(0),
                        force: false,
                    },
                );
                let (next, _) = reduce(
                    capturing,
                    Event::CaptureFailed {
                        seq: Seq(0),
                        error: ErrorKind::Capture,
                        jitter_pct: 0,
                    },
                    &CFG,
                );
                match next {
                    State::Fault { retry_in, .. } => retry_in.0,
                    other => panic!("a failed capture must fault, got {other}"),
                }
            })
            .collect();
        assert_eq!(delays, vec![10, 20, 40, 80, 160, 320, 600, 600]);

        // A cycle that reaches a verdict clears the counter, so the next
        // fault starts from the base delay again.
        let evaluating = armed(
            sess(5, 1, 0, VERIFIED, 4),
            Cycle::Evaluating {
                seq: Seq(0),
                force: false,
            },
        );
        let (next, _) = reduce(
            evaluating,
            Event::Evaluated {
                seq: Seq(0),
                verdict: Verdict::Unchanged,
            },
            &CFG,
        );
        assert_eq!(next.session().retry_attempt, 0);
    }

    // ------------------------------------------------------ property strategies

    const ERRORS: [ErrorKind; 7] = [
        ErrorKind::Capture,
        ErrorKind::Ocr,
        ErrorKind::Network,
        ErrorKind::Provider,
        ErrorKind::Credential,
        ErrorKind::Budget,
        ErrorKind::Internal,
    ];
    const EXCLUSIONS: [ExclusionState; 5] = [
        ExclusionState::Unknown,
        ExclusionState::Applied,
        ExclusionState::Verified,
        ExclusionState::Compromised,
        ExclusionState::Unsupported,
    ];

    /// Every event kind, with ids drawn from a deliberately tiny range so
    /// that matches and mismatches both happen often.
    fn any_event() -> impl Strategy<Value = Event> {
        (
            0_u8..20,
            0_u64..4,
            0_u64..4,
            any::<i8>(),
            0_u32..4,
            0_u8..7,
            0_u8..5,
        )
            .prop_map(
                |(kind, seq, request, jitter_pct, count, error, exclusion)| {
                    let seq = Seq(seq);
                    let request = RequestId(request);
                    let error = ERRORS[usize::from(error)];
                    match kind {
                        0 => Event::Arm,
                        1 => Event::Disarm,
                        2..=4 => Event::Tick,
                        5 => Event::ForceCapture,
                        6 => Event::Captured { seq },
                        7 => Event::CaptureFailed {
                            seq,
                            error,
                            jitter_pct,
                        },
                        8 => Event::Recognized {
                            seq,
                            text_len: 1024,
                            mean_conf: 0.9,
                        },
                        9 => Event::RecognizeFailed {
                            seq,
                            error,
                            jitter_pct,
                        },
                        10 => Event::Evaluated {
                            seq,
                            verdict: Verdict::Unchanged,
                        },
                        11 => Event::Evaluated {
                            seq,
                            verdict: Verdict::Pending,
                        },
                        12 => Event::Evaluated {
                            seq,
                            verdict: Verdict::Changed,
                        },
                        13 => Event::InferenceChunk {
                            request,
                            chunk_index: count,
                        },
                        14 => Event::InferenceDone {
                            request,
                            stop: StopReason::EndTurn,
                            remaining_chunks: count,
                        },
                        15 => Event::InferenceFailed {
                            request,
                            error,
                            retryable: count % 2 == 0,
                            jitter_pct,
                        },
                        16 => Event::ChunkRendered { request },
                        17 => Event::ExclusionChanged {
                            status: EXCLUSIONS[usize::from(exclusion)],
                        },
                        18 => Event::SettingsChanged {
                            capture_interval: Ticks(count),
                        },
                        19 => Event::ScreenLocked,
                        _ => Event::MonitorChanged,
                    }
                },
            )
    }

    /// Only the stage results a cycle can be waiting on, for FR-004.
    fn any_stage_event() -> impl Strategy<Value = Event> {
        (0_u8..6, 0_u64..12).prop_map(|(kind, seq)| {
            let seq = Seq(seq);
            match kind {
                0 => Event::Captured { seq },
                1 => Event::Recognized {
                    seq,
                    text_len: 64,
                    mean_conf: 0.5,
                },
                2 => Event::Evaluated {
                    seq,
                    verdict: Verdict::Changed,
                },
                3 => Event::Evaluated {
                    seq,
                    verdict: Verdict::Unchanged,
                },
                4 => Event::Evaluated {
                    seq,
                    verdict: Verdict::Pending,
                },
                _ => Event::Captured {
                    seq: Seq(seq.0 + 1),
                },
            }
        })
    }

    fn drive(cfg: &MachineConfig, events: &[Event]) -> (State, Vec<Effect>) {
        let mut state = State::default();
        let mut effects = Vec::new();
        for event in events {
            let (next, produced) = reduce(state, event.clone(), cfg);
            state = next;
            effects.extend(produced);
        }
        (state, effects)
    }

    /// A fault is only ever wrapped around a non-fault state, so the boxed
    /// chain is at most one deep. A deeper one would mean the reducer had
    /// started nesting retries.
    fn fault_depth(state: &State) -> usize {
        match state {
            State::Fault { cycle_state, .. } => 1 + fault_depth(cycle_state),
            _ => 0,
        }
    }

    fn config() -> ProptestConfig {
        ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        }
    }

    // ------------------------------------------------------------ properties

    proptest! {
        #![proptest_config(config())]

        /// FR-001. The reducer reads nothing outside its arguments, so the
        /// same call twice is the same answer twice.
        #[test]
        fn fr_001_reduce_is_pure(
            history in proptest::collection::vec(any_event(), 0..40),
            event in any_event(),
        ) {
            let (state, _) = drive(&CFG_DEGRADED, &history);
            let first = reduce(state.clone(), event.clone(), &CFG_DEGRADED);
            let second = reduce(state, event, &CFG_DEGRADED);
            prop_assert_eq!(first, second);
        }

        /// FR-002. No `(state, event)` pair panics, over sequences of up to
        /// a thousand events, and the state stays well formed.
        #[test]
        fn fr_002_no_pair_panics(
            history in proptest::collection::vec(any_event(), 0..=1000),
        ) {
            let mut state = State::default();
            for event in history {
                let (next, _) = reduce(state, event, &CFG_DEGRADED);
                prop_assert!(fault_depth(&next) <= 1, "faults must not nest: {:?}", next);
                prop_assert!(
                    next.wants_ticks() || matches!(next, State::Disarmed { .. } | State::Degraded { cycle: None, .. }),
                    "only a state with nothing to wait for stops the timer",
                );
                state = next;
            }
        }

        /// FR-004. No sequence of stage results can start a second
        /// inference while one is in flight.
        #[test]
        fn fr_004_single_in_flight_inference(
            history in proptest::collection::vec(any_stage_event(), 0..60),
        ) {
            let mut state = armed(
                sess(12, 8, 5, VERIFIED, 0),
                Cycle::Inferencing { seq: Seq(7), request: RequestId(4), started_tick: Tick(12) },
            );
            for event in history {
                let (next, effects) = reduce(state, event, &CFG);
                prop_assert!(
                    !effects.iter().any(|e| matches!(e, Effect::StartInference { .. })),
                    "a stage result started a second inference",
                );
                prop_assert!(
                    matches!(next.cycle(), Some(Cycle::Inferencing { .. })),
                    "the cycle left the single in-flight state",
                );
                state = next;
            }
        }

        /// FR-005. Disarming from anywhere lands `Disarmed`, cancels the one
        /// inference if there was one, and releases the one frame if there
        /// was one.
        #[test]
        fn fr_005_disarm_releases_everything(
            history in proptest::collection::vec(any_event(), 0..80),
        ) {
            let (state, _) = drive(&CFG_DEGRADED, &history);
            let in_flight = state.in_flight_request();
            let held = state.held_frame();

            let (next, effects) = reduce(state, Event::Disarm, &CFG_DEGRADED);
            prop_assert!(matches!(next, State::Disarmed { .. }), "Disarm must land Disarmed");

            let cancels: Vec<&Effect> = effects
                .iter()
                .filter(|e| matches!(e, Effect::CancelInference { .. }))
                .collect();
            match in_flight {
                Some(request) => {
                    prop_assert_eq!(cancels.len(), 1);
                    prop_assert_eq!(cancels[0], &Effect::CancelInference { request });
                }
                None => prop_assert!(cancels.is_empty()),
            }

            let releases: Vec<&Effect> = effects
                .iter()
                .filter(|e| matches!(e, Effect::ReleaseFrame { .. }))
                .collect();
            match held {
                Some(seq) => {
                    prop_assert_eq!(releases.len(), 1);
                    prop_assert_eq!(releases[0], &Effect::ReleaseFrame { seq });
                }
                None => prop_assert!(releases.is_empty()),
            }
            prop_assert!(effects.contains(&Effect::ResetDetector));
        }

        /// FR-006. Without the user's consent there is no path into
        /// `Degraded`: an unverified exclusion status disarms instead.
        #[test]
        fn fr_006_degraded_is_unreachable_without_consent(
            history in proptest::collection::vec(any_event(), 0..200),
        ) {
            let mut state = State::default();
            for event in history {
                let (next, _) = reduce(state, event, &CFG);
                prop_assert!(
                    !matches!(next, State::Degraded { .. }),
                    "reached Degraded with allow_degraded = false",
                );
                if let State::Fault { cycle_state, .. } = &next {
                    prop_assert!(
                        !matches!(**cycle_state, State::Degraded { .. }),
                        "a fault boxed a degraded state with allow_degraded = false",
                    );
                }
                state = next;
            }
        }

        /// The privacy consequence of the guards above: a frame the reducer
        /// refuses is a frame the reducer releases, in every state.
        #[test]
        fn every_refused_frame_is_released(
            history in proptest::collection::vec(any_event(), 0..80),
            seq in 0_u64..4,
        ) {
            let (state, _) = drive(&CFG_DEGRADED, &history);
            let (_, effects) = reduce(state.clone(), Event::Captured { seq: Seq(seq) }, &CFG_DEGRADED);
            let recognizing = effects.contains(&Effect::Recognize { seq: Seq(seq) });
            let released = effects.contains(&Effect::ReleaseFrame { seq: Seq(seq) });
            prop_assert!(
                recognizing ^ released,
                "a captured frame is either recognized or released, never neither or both: {effects:?}",
            );
        }
    }

    // ------------------------------------------------- AC-2: the doc diagram

    const ARCHITECTURE_DOC: &str = include_str!("../../../docs/architecture.md");
    const ARCHITECTURE_PATH: &str = "../../docs/architecture.md";
    const DOC_SECTION: &str = "## 3. The state machine (spec 009)";
    const FENCE_OPEN: &str = "```mermaid\n";
    const CYCLE_NODES: [&str; 6] = [
        "Idle",
        "Capturing",
        "Recognizing",
        "Evaluating",
        "Inferencing",
        "Rendering",
    ];

    fn is_cycle_node(name: &str) -> bool {
        CYCLE_NODES.contains(&name)
    }

    /// The diagram node a state belongs to: an armed session is drawn by its
    /// cycle, everything else by its own name.
    fn node(state: &State) -> &'static str {
        match state {
            State::Armed { cycle, .. } => cycle.name(),
            other => other.name(),
        }
    }

    fn lift(node: &'static str) -> &'static str {
        if is_cycle_node(node) { "Armed" } else { node }
    }

    fn push_edge(
        edges: &mut Vec<(&'static str, &'static str, Vec<&'static str>)>,
        from: &'static str,
        to: &'static str,
        label: &'static str,
    ) {
        if let Some(existing) = edges.iter_mut().find(|(f, t, _)| *f == from && *t == to) {
            if !existing.2.contains(&label) {
                existing.2.push(label);
            }
        } else {
            edges.push((from, to, vec![label]));
        }
    }

    /// Render the transition table as the Mermaid source `docs/architecture.md`
    /// carries. Transitions between two points of the capture cycle are drawn
    /// inside the `Armed` composite; everything else is drawn between the
    /// four top-level states.
    fn render_state_diagram(rows: &[Transition]) -> String {
        let mut inner: Vec<(&str, &str, Vec<&str>)> = Vec::new();
        let mut outer: Vec<(&str, &str, Vec<&str>)> = Vec::new();
        for row in rows {
            let Some(label) = row.edge else { continue };
            let (from, to) = (node(&row.from), node(&row.to));
            if is_cycle_node(from) && is_cycle_node(to) {
                push_edge(&mut inner, from, to, label);
            } else if lift(from) != lift(to) {
                push_edge(&mut outer, lift(from), lift(to), label);
            }
        }

        let mut out = String::from(
            "stateDiagram-v2\n    [*] --> Disarmed\n    state Armed {\n        [*] --> Idle\n",
        );
        for (from, to, labels) in &inner {
            let _ = writeln!(out, "        {from} --> {to}: {}", labels.join(" / "));
        }
        out.push_str("    }\n");
        for (from, to, labels) in &outer {
            let _ = writeln!(out, "    {from} --> {to}: {}", labels.join(" / "));
        }
        out
    }

    /// The byte range of the Mermaid body inside the state machine section.
    fn diagram_span(doc: &str) -> (usize, usize) {
        let section = doc
            .find(DOC_SECTION)
            .unwrap_or_else(|| panic!("docs/architecture.md has no `{DOC_SECTION}` heading"));
        let open = section
            + doc[section..]
                .find(FENCE_OPEN)
                .expect("the state machine section has a mermaid block")
            + FENCE_OPEN.len();
        let close = open
            + doc[open..]
                .find("```")
                .expect("the mermaid block is closed");
        (open, close)
    }

    /// AC-2. The diagram in the architecture document is this table, rendered.
    #[test]
    fn ac_002_architecture_diagram_matches_the_table() {
        let (open, close) = diagram_span(ARCHITECTURE_DOC);
        let rendered = render_state_diagram(&transitions());
        assert_eq!(
            &ARCHITECTURE_DOC[open..close],
            rendered,
            "docs/architecture.md §3 is stale; regenerate it with\n  \
             cargo test -p butler-core --test machine -- --ignored regenerate",
        );
    }

    /// The regenerator AC-2 asks for: the document's diagram is written from
    /// the table, never edited by hand. Ignored by default so an ordinary
    /// test run never touches the working tree.
    #[test]
    #[ignore = "writes docs/architecture.md; run explicitly after changing the table"]
    fn regenerate_architecture_diagram() {
        let doc = std::fs::read_to_string(ARCHITECTURE_PATH).unwrap_or_else(|e| {
            panic!("cannot read {ARCHITECTURE_PATH} from the package root: {e}")
        });
        let (open, close) = diagram_span(&doc);
        let rendered = render_state_diagram(&transitions());
        let next = format!("{}{}{}", &doc[..open], rendered, &doc[close..]);
        std::fs::write(ARCHITECTURE_PATH, next).expect("cannot write the architecture document");
    }

    // ------------------------------------------- AC-3: nothing platform-shaped

    const CRATE_MANIFEST: &str = include_str!("../Cargo.toml");
    /// Spec 009 §2 names the crate's whole dependency budget.
    const ALLOWED_DEPENDENCIES: [&str; 4] = ["serde", "strsim", "thiserror", "proptest"];
    /// Spec 009 AC-3 names what may never appear in the tree.
    const BANNED_DEPENDENCIES: [&str; 5] = ["tokio", "tauri", "windows", "objc2", "xcap"];

    fn declared_dependencies(manifest: &str) -> Vec<&str> {
        let mut names = Vec::new();
        let mut in_dependencies = false;
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_dependencies = line.ends_with("dependencies]");
            } else if in_dependencies
                && !line.is_empty()
                && !line.starts_with('#')
                && let Some((name, _)) = line.split_once('=')
            {
                names.push(name.trim());
            }
        }
        names
    }

    /// AC-3. `butler-core` declares nothing with an operating system in it.
    /// The manifest is the whole story here: at phase 1 the crate has no
    /// runtime dependencies at all, so `cargo tree -p butler-core` can only
    /// contain what this list allows.
    #[test]
    fn ac_003_no_platform_dependencies() {
        let declared = declared_dependencies(CRATE_MANIFEST);
        assert!(
            !declared.is_empty(),
            "the manifest parser found no dependencies at all"
        );
        for name in &declared {
            assert!(
                ALLOWED_DEPENDENCIES.contains(name),
                "`{name}` is outside the dependency budget spec 009 §2 sets",
            );
            for banned in BANNED_DEPENDENCIES {
                assert!(
                    !name.contains(banned),
                    "`{name}` brings `{banned}` into butler-core (spec 009 AC-3)",
                );
            }
        }
    }
}
