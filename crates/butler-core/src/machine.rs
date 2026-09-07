// Spec: specs/009-pipeline-state-machine/spec.md

//! The pipeline state machine.
//!
//! The pipeline (capture, OCR, evaluate, infer, render) is a strict state
//! machine expressed as one pure function, [`reduce`]. It performs no I/O,
//! reads no clock, spawns no threads and holds no screen data: the pixels,
//! the recognized text and the model's chunks live in the runtime host's
//! typed slots, keyed by [`Seq`] and [`RequestId`], and the machine only ever
//! sees the ids. Everything the pipeline must actually *do* is returned as a
//! list of [`Effect`] values, which the runtime host (spec 019) executes and
//! reports back as [`Event`] values.
//!
//! The guarantees the reducer gives by construction:
//!
//! - **Out of order is impossible.** A `Captured`, `Recognized` or
//!   `Evaluated` whose `seq` is not the one the current cycle is waiting for
//!   is dropped, with the frame released.
//! - **One inference at a time.** While `Inferencing`, no sequence of capture
//!   or recognition results can produce a second `StartInference`. A frame
//!   taken during inference is stale by definition; the cycle that follows
//!   rendering evaluates against the committed text, so a real change is not
//!   lost.
//! - **Disarm always releases.** `Disarm` (and a screen lock) from any state
//!   cancels an in-flight inference exactly once, releases any held frame,
//!   and resets the change detector.
//! - **Totality.** Every `(state, event)` pair returns a state. A pair with
//!   no transition returns the same state and a single warning log effect,
//!   never a panic.
//!
//! # Time without a clock
//!
//! The runtime host ticks the machine every 100 ms. [`Ticks`] is a count of
//! those ticks (a duration) and [`Tick`] is the running total (an instant).
//! The machine keeps its own tick counter in [`Session`], so [`Event::Tick`]
//! carries no payload and the reducer stays a function of its arguments
//! alone. Values the reducer must not invent, notably the retry jitter and
//! the pacer's remaining chunk count, arrive as event fields.

use core::fmt;

/// A frame sequence number, allocated by the machine and increasing for the
/// lifetime of the process, never reset by disarming.
///
/// Monotonicity across arm and disarm is what makes the out-of-order guard
/// sound: a capture that completes after its session ended can never collide
/// with a sequence number the next session hands out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seq(pub u64);

/// An inference request id, allocated by the machine on the same terms as
/// [`Seq`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

/// A point in time, counted in runtime ticks since the process started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

/// A duration, counted in runtime ticks (one tick is 100 ms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticks(pub u32);

impl Tick {
    /// The next tick.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Ticks elapsed since `earlier`, saturating at zero.
    #[must_use]
    pub const fn since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// Which monitor a capture targets.
///
/// The machine never enumerates displays: this is an opaque selector the
/// runtime host resolves against `butler_capture::monitor::MonitorId`
/// (spec 006) at the moment it executes [`Effect::Capture`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MonitorTarget {
    /// Whatever the platform reports as the primary display (spec 006 §3.4).
    #[default]
    Primary,
    /// A specific display, by the opaque handle the runtime host assigned.
    Id(u64),
}

/// The last capture-exclusion status the runtime host reported.
///
/// This is the machine's operating-system-free mirror of spec 005's
/// `ExclusionStatus`: the kind only, without the `Instant`, the reason string
/// or the self-test evidence that type carries, none of which may cross into
/// a crate with no clock and no screen data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExclusionState {
    /// Nothing reported yet; the self-test has not run.
    #[default]
    Unknown,
    /// The platform accepted the request; the self-test has not confirmed it.
    Applied,
    /// Applied and confirmed absent from a capture of the display.
    Verified,
    /// Applied, but the self-test saw the overlay's own pixels.
    Compromised,
    /// The platform cannot exclude this window.
    Unsupported,
}

impl ExclusionState {
    /// Whether arming may proceed without the user's degraded-mode consent.
    #[must_use]
    pub const fn is_verified(self) -> bool {
        matches!(self, Self::Verified)
    }
}

/// Why the machine is running in degraded mode.
///
/// One reason today, carrying the status that caused it, so the overlay can
/// name which of the four non-verified states the user is living with. It is
/// an enum rather than a bare [`ExclusionState`] because "degraded" is a
/// property of the machine, not of the platform, and a later spec may add a
/// reason that has nothing to do with capture exclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DegradedReason {
    /// The capture exclusion status is not `Verified` (spec 005).
    Exclusion(ExclusionState),
}

impl DegradedReason {
    /// The degraded reason a non-`Verified` exclusion status implies.
    #[must_use]
    pub const fn from_exclusion(status: ExclusionState) -> Self {
        Self::Exclusion(status)
    }
}

/// The closed set of failure kinds the pipeline reports.
///
/// A kind, never a message: an error string built from screen content would
/// carry the user's data across the privacy boundary (spec 015). Spec 011's
/// IPC contract re-exports this enum as the wire `ErrorKind`, which is why it
/// carries the serde and specta derives the rest of this module does not: it
/// is the one reducer type that is also a wire type (spec 011 D-1).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    specta::Type,
)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// The display could not be captured.
    Capture,
    /// Text recognition failed.
    Ocr,
    /// The request never reached the provider.
    Network,
    /// The provider answered with an error.
    Provider,
    /// No usable credential for the configured provider.
    Credential,
    /// The spend guard refused the request.
    Budget,
    /// A defect on our side.
    Internal,
}

/// Which stage faulted, and with what kind of error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FaultKind {
    /// Capture failed for the frame the cycle was waiting on.
    Capture(ErrorKind),
    /// Recognition failed for the frame the cycle was waiting on.
    Recognize(ErrorKind),
    /// The provider reported a retryable failure.
    Inference(ErrorKind),
    /// No terminal event arrived within `inference_timeout_ticks`.
    InferenceTimeout,
}

/// The change detector's answer, reduced to the discriminant the machine
/// branches on.
///
/// Spec 008's `delta::Verdict` carries the similarity ratio and the stability
/// counter as well; the runtime host maps it down to this before the event
/// reaches the reducer, so the machine's state stays free of floating point
/// and remains exactly comparable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// The screen still says what it said when inference last ran.
    Unchanged,
    /// Changed, but not yet stable across enough frames.
    Pending,
    /// Changed and stable: worth asking about.
    Changed,
}

/// Why the model stopped producing tokens.
///
/// The machine's mirror of spec 010's `StopReason`, without the refusal
/// category and the free-form provider string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StopReason {
    /// The model finished its answer.
    EndTurn,
    /// The output token cap was reached.
    MaxTokens,
    /// The model declined to answer.
    Refusal,
    /// Anything else the provider reported.
    Other,
}

/// Severity for [`Effect::Log`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Per-transition detail.
    Trace,
    /// Developer detail.
    Debug,
    /// Lifecycle milestones.
    Info,
    /// A dropped event or a refused transition.
    Warn,
    /// A fault.
    Error,
}

/// What the runtime host should tell the overlay.
///
/// Spec 011 defines the wire types (`UiEvent`) and spec 019 builds them: a
/// `RuntimeStatus` payload needs the exclusion summary and the error kind the
/// runtime holds, not just what the reducer knows, so the machine names the
/// occasion and the runtime fills in the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Notice {
    /// The machine's state changed in a way the user should see.
    Status,
    /// An answer is being requested.
    AnswerStarted {
        /// The inference this notice is about.
        request: RequestId,
    },
    /// An answer finished rendering.
    AnswerDone {
        /// The inference this notice is about.
        request: RequestId,
    },
    /// An answer will not arrive.
    AnswerFailed {
        /// The inference this notice is about.
        request: RequestId,
        /// Why it failed.
        error: ErrorKind,
    },
}

/// Memory that outlives one capture cycle.
///
/// Spec 009 §3.1 sketches [`State`] as four phases; a reducer with no storage
/// outside its state still needs somewhere to keep the monotonic counters,
/// the tick total, the last reported exclusion status and the consecutive
/// fault count, so every phase carries this record and passes it along
/// unchanged when it has nothing to say about it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Runtime ticks observed since the process started.
    pub tick: Tick,
    /// The sequence number the next capture will use.
    pub next_seq: Seq,
    /// The id the next inference will use.
    pub next_request: RequestId,
    /// The last exclusion status the runtime host reported.
    pub exclusion: ExclusionState,
    /// Consecutive faults with no successful cycle in between; the exponent
    /// in the retry backoff.
    pub retry_attempt: u32,
}

impl Session {
    fn take_seq(&mut self) -> Seq {
        let seq = self.next_seq;
        self.next_seq = Seq(seq.0.saturating_add(1));
        seq
    }

    fn take_request(&mut self) -> RequestId {
        let request = self.next_request;
        self.next_request = RequestId(request.0.saturating_add(1));
        request
    }
}

/// Where an armed session is in the capture cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cycle {
    /// Waiting out the capture interval.
    Idle {
        /// Ticks remaining before the next capture.
        next_capture_in: Ticks,
    },
    /// A capture is in flight.
    Capturing {
        /// The frame being captured.
        seq: Seq,
        /// Whether the user asked for this cycle explicitly.
        force: bool,
    },
    /// Recognition is in flight for a captured frame.
    Recognizing {
        /// The frame being recognized.
        seq: Seq,
        /// Whether the user asked for this cycle explicitly.
        force: bool,
    },
    /// Change detection is in flight for recognized text.
    Evaluating {
        /// The frame being evaluated.
        seq: Seq,
        /// Whether the user asked for this cycle explicitly.
        force: bool,
    },
    /// An inference is in flight. This is the single-in-flight state.
    Inferencing {
        /// The frame the question was built from.
        seq: Seq,
        /// The inference in flight.
        request: RequestId,
        /// When it started, for the timeout.
        started_tick: Tick,
    },
    /// The answer is complete and the pacer is still releasing it.
    Rendering {
        /// The frame the question was built from.
        seq: Seq,
        /// The inference being rendered.
        request: RequestId,
        /// Chunks the pacer has still to release.
        remaining_chunks: u32,
    },
}

impl Cycle {
    /// The frame this cycle is holding in the runtime's slot, if any.
    ///
    /// The frame is released as soon as the text has been evaluated, so the
    /// later stages hold nothing.
    #[must_use]
    pub const fn held_frame(&self) -> Option<Seq> {
        match self {
            Self::Capturing { seq, .. }
            | Self::Recognizing { seq, .. }
            | Self::Evaluating { seq, .. } => Some(*seq),
            Self::Idle { .. } | Self::Inferencing { .. } | Self::Rendering { .. } => None,
        }
    }

    /// The inference that would have to be cancelled to stop this cycle.
    #[must_use]
    pub const fn in_flight_request(&self) -> Option<RequestId> {
        match self {
            Self::Inferencing { request, .. } => Some(*request),
            _ => None,
        }
    }

    /// A stable name for logs, traces and the IPC status payload.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Idle { .. } => "Idle",
            Self::Capturing { .. } => "Capturing",
            Self::Recognizing { .. } => "Recognizing",
            Self::Evaluating { .. } => "Evaluating",
            Self::Inferencing { .. } => "Inferencing",
            Self::Rendering { .. } => "Rendering",
        }
    }
}

/// The machine's state. It always starts [`State::Disarmed`]: nothing about
/// a session survives a launch (spec 009 §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Not watching the screen. The only state the process starts in.
    Disarmed {
        /// Cross-cycle memory.
        session: Session,
    },
    /// Watching the screen with exclusion verified.
    Armed {
        /// Cross-cycle memory.
        session: Session,
        /// Where the capture cycle is.
        cycle: Cycle,
    },
    /// Watching the screen although exclusion is not verified, because the
    /// user opted into degraded mode.
    Degraded {
        /// Cross-cycle memory.
        session: Session,
        /// Which exclusion status put the machine here.
        reason: DegradedReason,
        /// The cycle carried over from the armed session, if there was one.
        cycle: Option<Cycle>,
    },
    /// A stage failed; the machine is counting down to a retry.
    Fault {
        /// Which stage failed, and how.
        error: FaultKind,
        /// Ticks remaining before the retry.
        retry_in: Ticks,
        /// The state to resume, boxed because it is a `State`.
        cycle_state: Box<State>,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Disarmed {
            session: Session::default(),
        }
    }
}

impl State {
    /// The cross-cycle memory this state carries.
    ///
    /// [`State::Fault`] keeps no copy of its own: it reads through to the
    /// state it will resume, so there is exactly one record and it cannot go
    /// stale while a fault is counting down.
    #[must_use]
    pub fn session(&self) -> Session {
        match self {
            Self::Disarmed { session }
            | Self::Armed { session, .. }
            | Self::Degraded { session, .. } => *session,
            Self::Fault { cycle_state, .. } => cycle_state.session(),
        }
    }

    /// This state with its cross-cycle memory replaced.
    #[must_use]
    pub fn with_session(self, next: Session) -> Self {
        match self {
            Self::Disarmed { .. } => Self::Disarmed { session: next },
            Self::Armed { cycle, .. } => Self::Armed {
                session: next,
                cycle,
            },
            Self::Degraded { reason, cycle, .. } => Self::Degraded {
                session: next,
                reason,
                cycle,
            },
            Self::Fault {
                error,
                retry_in,
                cycle_state,
            } => Self::Fault {
                error,
                retry_in,
                cycle_state: Box::new(cycle_state.with_session(next)),
            },
        }
    }

    /// The capture cycle this state is running, if it is running one.
    #[must_use]
    pub fn cycle(&self) -> Option<Cycle> {
        match self {
            Self::Disarmed { .. } => None,
            Self::Armed { cycle, .. } => Some(*cycle),
            Self::Degraded { cycle, .. } => *cycle,
            Self::Fault { cycle_state, .. } => cycle_state.cycle(),
        }
    }

    /// The frame held in the runtime's slot, if any.
    #[must_use]
    pub fn held_frame(&self) -> Option<Seq> {
        self.cycle().and_then(|cycle| cycle.held_frame())
    }

    /// The inference that would have to be cancelled to stop this state.
    #[must_use]
    pub fn in_flight_request(&self) -> Option<RequestId> {
        self.cycle().and_then(|cycle| cycle.in_flight_request())
    }

    /// Whether the runtime host should keep ticking the machine.
    ///
    /// `Disarmed` is the only state that does not need time to pass, so it is
    /// also the only state after which the reducer stops asking for ticks.
    #[must_use]
    pub fn wants_ticks(&self) -> bool {
        match self {
            Self::Disarmed { .. } => false,
            Self::Armed { .. } | Self::Fault { .. } => true,
            Self::Degraded { cycle, .. } => cycle.is_some(),
        }
    }

    /// A stable name for logs, traces and the IPC status payload.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Disarmed { .. } => "Disarmed",
            Self::Armed { .. } => "Armed",
            Self::Degraded { .. } => "Degraded",
            Self::Fault { .. } => "Fault",
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.cycle() {
            Some(cycle) => write!(f, "{}({})", self.name(), cycle.name()),
            None => f.write_str(self.name()),
        }
    }
}

/// Everything that can happen to the machine.
///
/// Events carry ids and small metadata only. The frames, the recognized text
/// and the model's chunks stay in the runtime host's slots, so an event
/// stream is cheap to trace and safe to replay (spec 015).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The user started a session.
    Arm,
    /// The user ended the session.
    Disarm,
    /// 100 ms passed.
    Tick,
    /// The user asked for an answer now, whatever the detector thinks.
    ForceCapture,
    /// A capture completed.
    Captured {
        /// The frame that was captured.
        seq: Seq,
    },
    /// A capture failed.
    CaptureFailed {
        /// The frame that was being captured.
        seq: Seq,
        /// Why it failed.
        error: ErrorKind,
        /// Retry jitter, in percent, drawn by the runtime host: the reducer
        /// never generates randomness (spec 009 §3.4.11).
        jitter_pct: i8,
    },
    /// Recognition completed.
    Recognized {
        /// The frame that was recognized.
        seq: Seq,
        /// How much text came back, for tracing.
        text_len: usize,
        /// Mean OCR confidence, for tracing.
        mean_conf: f32,
    },
    /// Recognition failed.
    RecognizeFailed {
        /// The frame that was being recognized.
        seq: Seq,
        /// Why it failed.
        error: ErrorKind,
        /// Retry jitter, in percent, drawn by the runtime host.
        jitter_pct: i8,
    },
    /// Change detection answered.
    Evaluated {
        /// The frame that was evaluated.
        seq: Seq,
        /// What the detector decided.
        verdict: Verdict,
    },
    /// The provider produced a chunk of the answer.
    InferenceChunk {
        /// The inference it belongs to.
        request: RequestId,
        /// Its position in the stream, for tracing.
        chunk_index: u32,
    },
    /// The provider's stream ended.
    InferenceDone {
        /// The inference that ended.
        request: RequestId,
        /// Why it ended.
        stop: StopReason,
        /// What the pacer (spec 013) still has to release. The reducer cannot
        /// consult the pacer, so the runtime host supplies the count.
        remaining_chunks: u32,
    },
    /// The inference failed.
    InferenceFailed {
        /// The inference that failed.
        request: RequestId,
        /// Why it failed.
        error: ErrorKind,
        /// Whether retrying could help.
        retryable: bool,
        /// Retry jitter, in percent, drawn by the runtime host.
        jitter_pct: i8,
    },
    /// The overlay rendered one paced chunk.
    ChunkRendered {
        /// The inference it belonged to.
        request: RequestId,
    },
    /// The exclusion status changed (spec 005).
    ExclusionChanged {
        /// The new status.
        status: ExclusionState,
    },
    /// Settings changed (spec 014). The runtime host passes the new
    /// [`MachineConfig`] on every later call; the capture interval is carried
    /// here so a shortened interval takes effect without waiting out the old
    /// one.
    SettingsChanged {
        /// The new capture interval.
        capture_interval: Ticks,
    },
    /// The session locked the screen.
    ScreenLocked,
    /// The session unlocked the screen. Deliberately not a re-arm.
    ScreenUnlocked,
    /// The target display changed.
    MonitorChanged,
}

/// What the runtime host must do. Effects are data: the machine decides,
/// the runtime host (spec 019) acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Capture the display into the frame slot for `seq`.
    Capture {
        /// The frame to capture.
        seq: Seq,
        /// Which display to capture.
        monitor: MonitorTarget,
    },
    /// Run OCR over the frame in the slot for `seq`.
    Recognize {
        /// The frame to recognize.
        seq: Seq,
    },
    /// Ask the change detector about the text recognized for `seq`.
    Evaluate {
        /// The frame to evaluate.
        seq: Seq,
        /// Whether the user forced this cycle, which the detector honours by
        /// answering `Changed` regardless (spec 008 §3.2).
        force: bool,
    },
    /// Redact the text for `seq` and send it to the provider as `request`.
    StartInference {
        /// The inference to start.
        request: RequestId,
        /// The frame its question comes from.
        seq: Seq,
    },
    /// Cancel an inference in flight.
    CancelInference {
        /// The inference to cancel.
        request: RequestId,
    },
    /// Release whatever the pacer allows of `request` to the overlay.
    EmitChunk {
        /// The inference being released.
        request: RequestId,
    },
    /// Tell the overlay something.
    Emit(Notice),
    /// Run the capture-exclusion self-test (spec 005).
    RunSelfTest,
    /// Deliver another [`Event::Tick`] in 100 ms. The machine asks for one
    /// tick at a time, so a state that stops asking stops the timer.
    ScheduleTick,
    /// Zero and drop the frame slot for `seq`.
    ReleaseFrame {
        /// The frame to release.
        seq: Seq,
    },
    /// Reset the change detector's committed text and stability counter.
    ResetDetector,
    /// Record a fixed message. Never interpolated, so no screen content can
    /// reach a log through this effect (spec 015).
    Log(Level, &'static str),
}

/// The tunables the reducer reads. Derived from the user's settings
/// (spec 014) by the runtime host and passed on every call, so the machine
/// holds no configuration of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MachineConfig {
    /// Ticks between captures while idle (2500 ms by default).
    pub capture_interval_ticks: Ticks,
    /// Ticks an inference may run before it is treated as a fault.
    pub inference_timeout_ticks: Ticks,
    /// Ticks to wait after an answer before capturing again, so the user can
    /// read it.
    pub post_answer_delay_ticks: Ticks,
    /// The first retry delay; each consecutive fault doubles it.
    pub retry_backoff_base_ticks: Ticks,
    /// The ceiling on the retry delay.
    pub retry_backoff_max_ticks: Ticks,
    /// Whether the user consented to running without verified exclusion.
    pub allow_degraded: bool,
    /// Which display to capture.
    pub monitor: MonitorTarget,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            capture_interval_ticks: Ticks(25),
            inference_timeout_ticks: Ticks(600),
            post_answer_delay_ticks: Ticks(30),
            retry_backoff_base_ticks: Ticks(10),
            retry_backoff_max_ticks: Ticks(600),
            allow_degraded: false,
            monitor: MonitorTarget::Primary,
        }
    }
}

/// Apply one event to the machine.
///
/// A pure function: the same `(state, event, cfg)` always produces the same
/// `(state, effects)`, and it neither reads nor writes anything outside its
/// arguments. Every pair is total; an event with no transition from the given
/// state returns that state unchanged with a single warning log.
///
/// The effects are ordered, and the order matters: releases and cancellations
/// come before the work that replaces them, so the runtime host can execute
/// the list front to back without holding two frames or two inferences.
// The by-value `event` is the normative signature (spec 009 §3.4) and the
// right ownership story for a queue that hands each event over exactly once,
// even though the reducer itself only ever reads it.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn reduce(state: State, event: Event, cfg: &MachineConfig) -> (State, Vec<Effect>) {
    let was_ticking = state.wants_ticks();
    let is_tick = matches!(event, Event::Tick);

    let (next, mut effects) = dispatch(state, &event, cfg);

    // One tick at a time: re-arm the timer while handling a tick, and start it
    // when entering a state that needs time from one that did not.
    if next.wants_ticks() && (is_tick || !was_ticking) {
        effects.push(Effect::ScheduleTick);
    }
    (next, effects)
}

fn dispatch(state: State, event: &Event, cfg: &MachineConfig) -> (State, Vec<Effect>) {
    match *event {
        Event::Arm => arm(state, cfg),
        Event::Disarm | Event::ScreenLocked => disarm(&state),
        Event::ScreenUnlocked => (
            state,
            vec![Effect::Log(Level::Info, "screen-unlocked-not-rearmed")],
        ),
        Event::ExclusionChanged { status } => exclusion_changed(state, status, cfg),
        Event::SettingsChanged { capture_interval } => settings_changed(state, capture_interval),
        Event::MonitorChanged => monitor_changed(state),
        _ => in_cycle(state, event, cfg),
    }
}

// §3.4.1: arming needs verified exclusion, or the user's consent to run
// without it. FR-006: with `allow_degraded` false there is no path into
// `Degraded`, so a machine that cannot verify simply stays disarmed. The
// self-test runs either way, because it is the only thing that can change the
// answer.
fn arm(state: State, cfg: &MachineConfig) -> (State, Vec<Effect>) {
    let mut session = match state {
        State::Disarmed { session } => session,
        already_armed => {
            return (
                already_armed,
                vec![Effect::Log(Level::Warn, "already-armed")],
            );
        }
    };
    session.retry_attempt = 0;
    let cycle = Cycle::Idle {
        next_capture_in: Ticks(0),
    };
    let status = session.exclusion;

    if status.is_verified() {
        (
            State::Armed { session, cycle },
            vec![Effect::RunSelfTest, Effect::Emit(Notice::Status)],
        )
    } else if cfg.allow_degraded {
        let reason = DegradedReason::from_exclusion(status);
        (
            State::Degraded {
                session,
                reason,
                cycle: Some(cycle),
            },
            vec![Effect::RunSelfTest, Effect::Emit(Notice::Status)],
        )
    } else {
        (
            State::Disarmed { session },
            vec![
                Effect::RunSelfTest,
                Effect::Emit(Notice::Status),
                Effect::Log(Level::Warn, "arm-refused-exclusion-not-verified"),
            ],
        )
    }
}

// §3.4.8 and §3.4.9: every effect that could hold user data is released.
fn disarm(state: &State) -> (State, Vec<Effect>) {
    let mut session = state.session();
    let mut effects = Vec::new();
    if let Some(request) = state.in_flight_request() {
        effects.push(Effect::CancelInference { request });
    }
    if let Some(seq) = state.held_frame() {
        effects.push(Effect::ReleaseFrame { seq });
    }
    effects.push(Effect::ResetDetector);
    effects.push(Effect::Emit(Notice::Status));
    session.retry_attempt = 0;
    (State::Disarmed { session }, effects)
}

// §3.4.10, and FR-006 for the branch that refuses degraded mode.
fn exclusion_changed(
    state: State,
    status: ExclusionState,
    cfg: &MachineConfig,
) -> (State, Vec<Effect>) {
    let mut session = state.session();
    session.exclusion = status;
    let status_effects = vec![Effect::Emit(Notice::Status)];

    match state {
        State::Disarmed { .. } => (State::Disarmed { session }, status_effects),
        State::Armed { cycle, .. } => {
            if status.is_verified() {
                (State::Armed { session, cycle }, status_effects)
            } else if cfg.allow_degraded {
                let reason = DegradedReason::from_exclusion(status);
                (
                    State::Degraded {
                        session,
                        reason,
                        cycle: Some(cycle),
                    },
                    status_effects,
                )
            } else {
                disarm(&State::Armed { session, cycle })
            }
        }
        State::Degraded { cycle, .. } => {
            if status.is_verified() {
                let cycle = cycle.unwrap_or(Cycle::Idle {
                    next_capture_in: Ticks(0),
                });
                (State::Armed { session, cycle }, status_effects)
            } else if cfg.allow_degraded {
                let reason = DegradedReason::from_exclusion(status);
                (
                    State::Degraded {
                        session,
                        reason,
                        cycle,
                    },
                    status_effects,
                )
            } else {
                disarm(&State::Degraded {
                    session,
                    reason: DegradedReason::from_exclusion(status),
                    cycle,
                })
            }
        }
        State::Fault {
            error,
            retry_in,
            cycle_state,
        } => {
            let (resumed, effects) = exclusion_changed(*cycle_state, status, cfg);
            if matches!(resumed, State::Disarmed { .. }) {
                (resumed, effects)
            } else {
                (
                    State::Fault {
                        error,
                        retry_in,
                        cycle_state: Box::new(resumed),
                    },
                    effects,
                )
            }
        }
    }
}

// §3.4.12: the new configuration reaches the reducer as `cfg` on the next
// call, so the only thing to do here is make sure a countdown already under
// way does not outlast a shortened interval.
fn settings_changed(state: State, capture_interval: Ticks) -> (State, Vec<Effect>) {
    let clamp = |cycle: Cycle| match cycle {
        Cycle::Idle { next_capture_in } => Cycle::Idle {
            next_capture_in: Ticks(next_capture_in.0.min(capture_interval.0)),
        },
        other => other,
    };
    let effects = vec![Effect::Log(Level::Info, "settings-changed")];
    match state {
        State::Armed { session, cycle } => (
            State::Armed {
                session,
                cycle: clamp(cycle),
            },
            effects,
        ),
        State::Degraded {
            session,
            reason,
            cycle,
        } => (
            State::Degraded {
                session,
                reason,
                cycle: cycle.map(clamp),
            },
            effects,
        ),
        other => (other, effects),
    }
}

// §3.4.12: a new display invalidates the detector's committed text, so the
// cycle restarts from `Idle` with no delay and the detector is reset.
fn monitor_changed(state: State) -> (State, Vec<Effect>) {
    let mut effects = Vec::new();
    if let Some(request) = state.in_flight_request() {
        effects.push(Effect::CancelInference { request });
    }
    if let Some(seq) = state.held_frame() {
        effects.push(Effect::ReleaseFrame { seq });
    }
    effects.push(Effect::ResetDetector);

    let restart = Cycle::Idle {
        next_capture_in: Ticks(0),
    };
    let next = match state {
        State::Disarmed { session } => State::Disarmed { session },
        State::Armed { session, .. } => State::Armed {
            session,
            cycle: restart,
        },
        State::Degraded {
            session,
            reason,
            cycle,
        } => State::Degraded {
            session,
            reason,
            cycle: cycle.map(|_| restart),
        },
        State::Fault {
            error,
            retry_in,
            cycle_state,
        } => {
            let session = cycle_state.session();
            let resumed = match *cycle_state {
                State::Degraded { reason, cycle, .. } => State::Degraded {
                    session,
                    reason,
                    cycle: cycle.map(|_| restart),
                },
                _ => State::Armed {
                    session,
                    cycle: restart,
                },
            };
            State::Fault {
                error,
                retry_in,
                cycle_state: Box::new(resumed),
            }
        }
    };
    (next, effects)
}

/// The envelope an advancing cycle is sealed back into.
#[derive(Clone, Copy)]
enum Envelope {
    Armed,
    Degraded(DegradedReason),
}

/// What advancing a cycle produced.
#[derive(Clone, Copy)]
enum CycleNext {
    Stay(Cycle),
    Fault { kind: FaultKind, jitter_pct: i8 },
}

/// What to do with a stage result that arrives when no cycle is waiting for
/// it: a capture that completed after the session ended still put pixels in
/// the runtime host's slot, and those must be released whatever the machine
/// thinks (constitution §V).
fn stray(event: &Event) -> Vec<Effect> {
    match *event {
        Event::Captured { seq }
        | Event::CaptureFailed { seq, .. }
        | Event::Recognized { seq, .. }
        | Event::RecognizeFailed { seq, .. }
        | Event::Evaluated { seq, .. } => vec![
            Effect::ReleaseFrame { seq },
            Effect::Log(Level::Warn, "stray-frame-released"),
        ],
        _ => vec![Effect::Log(Level::Warn, "ignored")],
    }
}

fn in_cycle(state: State, event: &Event, cfg: &MachineConfig) -> (State, Vec<Effect>) {
    match state {
        State::Disarmed { mut session } => {
            if matches!(event, Event::Tick) {
                session.tick = session.tick.next();
                (State::Disarmed { session }, Vec::new())
            } else {
                (State::Disarmed { session }, stray(event))
            }
        }
        State::Armed { session, cycle } => {
            let (session, next, effects) = advance(session, cycle, event, cfg);
            (seal(session, Envelope::Armed, next, cfg), effects)
        }
        State::Degraded {
            session,
            reason,
            cycle: Some(cycle),
        } => {
            let (session, next, effects) = advance(session, cycle, event, cfg);
            (
                seal(session, Envelope::Degraded(reason), next, cfg),
                effects,
            )
        }
        State::Degraded {
            session,
            reason,
            cycle: None,
        } => (
            State::Degraded {
                session,
                reason,
                cycle: None,
            },
            stray(event),
        ),
        State::Fault {
            error,
            retry_in,
            cycle_state,
        } => in_fault(error, retry_in, cycle_state, event),
    }
}

// §3.4.11: the fault counts down and then resumes the state it interrupted,
// which is always that state's `Idle`. The delay itself was computed when the
// fault was entered, from the consecutive-fault count and the jitter the
// runtime host supplied.
fn in_fault(
    error: FaultKind,
    retry_in: Ticks,
    cycle_state: Box<State>,
    event: &Event,
) -> (State, Vec<Effect>) {
    if !matches!(event, Event::Tick) {
        return (
            State::Fault {
                error,
                retry_in,
                cycle_state,
            },
            stray(event),
        );
    }
    let mut session = cycle_state.session();
    session.tick = session.tick.next();
    let resumed = cycle_state.with_session(session);
    let remaining = retry_in.0.saturating_sub(1);
    if remaining == 0 {
        (resumed, vec![Effect::Log(Level::Info, "fault-retry")])
    } else {
        (
            State::Fault {
                error,
                retry_in: Ticks(remaining),
                cycle_state: Box::new(resumed),
            },
            Vec::new(),
        )
    }
}

fn seal(mut session: Session, envelope: Envelope, next: CycleNext, cfg: &MachineConfig) -> State {
    match next {
        CycleNext::Stay(cycle) => wrap(session, envelope, cycle),
        CycleNext::Fault { kind, jitter_pct } => {
            session.retry_attempt = session.retry_attempt.saturating_add(1);
            let retry_in = backoff(session.retry_attempt, jitter_pct, cfg);
            let resume = wrap(
                session,
                envelope,
                Cycle::Idle {
                    next_capture_in: Ticks(0),
                },
            );
            State::Fault {
                error: kind,
                retry_in,
                cycle_state: Box::new(resume),
            }
        }
    }
}

fn wrap(session: Session, envelope: Envelope, cycle: Cycle) -> State {
    match envelope {
        Envelope::Armed => State::Armed { session, cycle },
        Envelope::Degraded(reason) => State::Degraded {
            session,
            reason,
            cycle: Some(cycle),
        },
    }
}

/// The retry delay for the `attempt`-th consecutive fault: the base doubled
/// once per attempt, capped, then moved by the jitter percentage the runtime
/// host drew. The reducer never generates randomness of its own.
fn backoff(attempt: u32, jitter_pct: i8, cfg: &MachineConfig) -> Ticks {
    let doublings = attempt.saturating_sub(1).min(31);
    let factor = 1_u64 << doublings;
    let grown = u64::from(cfg.retry_backoff_base_ticks.0).saturating_mul(factor);
    let capped = grown.min(u64::from(cfg.retry_backoff_max_ticks.0)).max(1);

    let capped = i64::try_from(capped).unwrap_or(i64::MAX);
    let pct = i64::from(jitter_pct.clamp(-50, 50));
    let delta = capped.saturating_mul(pct) / 100;
    let jittered = capped.saturating_add(delta).max(1);
    Ticks(u32::try_from(jittered).unwrap_or(u32::MAX))
}

fn advance(
    mut session: Session,
    cycle: Cycle,
    event: &Event,
    cfg: &MachineConfig,
) -> (Session, CycleNext, Vec<Effect>) {
    let (next, effects) = match *event {
        Event::Tick => {
            session.tick = session.tick.next();
            on_tick(&mut session, cycle, cfg)
        }
        Event::ForceCapture => on_force_capture(&mut session, cycle, cfg),
        Event::Captured { seq } => on_captured(cycle, seq),
        Event::CaptureFailed {
            seq,
            error,
            jitter_pct,
        } => on_capture_failed(cycle, seq, error, jitter_pct),
        Event::Recognized { seq, .. } => on_recognized(cycle, seq),
        Event::RecognizeFailed {
            seq,
            error,
            jitter_pct,
        } => on_recognize_failed(cycle, seq, error, jitter_pct),
        Event::Evaluated { seq, verdict } => on_evaluated(&mut session, cycle, seq, verdict, cfg),
        Event::InferenceChunk { request, .. } => on_inference_chunk(cycle, request),
        Event::InferenceDone {
            request,
            remaining_chunks,
            ..
        } => on_inference_done(&mut session, cycle, request, remaining_chunks, cfg),
        Event::InferenceFailed {
            request,
            error,
            retryable,
            jitter_pct,
        } => on_inference_failed(
            &mut session,
            cycle,
            request,
            error,
            retryable,
            jitter_pct,
            cfg,
        ),
        Event::ChunkRendered { request } => on_chunk_rendered(&mut session, cycle, request, cfg),
        _ => (
            CycleNext::Stay(cycle),
            vec![Effect::Log(Level::Warn, "ignored")],
        ),
    };
    (session, next, effects)
}

fn start_capture(
    session: &mut Session,
    force: bool,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    let seq = session.take_seq();
    (
        CycleNext::Stay(Cycle::Capturing { seq, force }),
        vec![Effect::Capture {
            seq,
            monitor: cfg.monitor,
        }],
    )
}

// §3.4.2 for the idle countdown, §3.4.6 for the inference timeout. Everywhere
// else a tick is a no-op that only advances the clock.
fn on_tick(session: &mut Session, cycle: Cycle, cfg: &MachineConfig) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Idle { next_capture_in } if next_capture_in.0 > 0 => (
            CycleNext::Stay(Cycle::Idle {
                next_capture_in: Ticks(next_capture_in.0 - 1),
            }),
            Vec::new(),
        ),
        Cycle::Idle { .. } => start_capture(session, false, cfg),
        Cycle::Inferencing {
            request,
            started_tick,
            ..
        } if session.tick.since(started_tick) >= u64::from(cfg.inference_timeout_ticks.0) => (
            CycleNext::Fault {
                kind: FaultKind::InferenceTimeout,
                jitter_pct: 0,
            },
            vec![
                Effect::CancelInference { request },
                Effect::Emit(Notice::AnswerFailed {
                    request,
                    error: ErrorKind::Network,
                }),
            ],
        ),
        other => (CycleNext::Stay(other), Vec::new()),
    }
}

// §3.4.2: "ask now" starts a fresh forced cycle from anywhere except an
// inference, which it must not interrupt (that would be the second in-flight
// request FR-004 forbids).
fn on_force_capture(
    session: &mut Session,
    cycle: Cycle,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    if let Cycle::Inferencing { .. } = cycle {
        return (
            CycleNext::Stay(cycle),
            vec![Effect::Log(Level::Warn, "force-capture-during-inference")],
        );
    }
    let mut effects = Vec::new();
    if let Some(seq) = cycle.held_frame() {
        effects.push(Effect::ReleaseFrame { seq });
    }
    if let Cycle::Rendering { request, .. } = cycle {
        effects.push(Effect::Emit(Notice::AnswerDone { request }));
    }
    let (next, capture) = start_capture(session, true, cfg);
    effects.extend(capture);
    (next, effects)
}

// §3.4.3: the out-of-order guard. A frame the machine is not waiting for is
// released rather than recognized, whatever state it arrives in.
fn on_captured(cycle: Cycle, seq: Seq) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Capturing {
            seq: current,
            force,
        } if seq == current => (
            CycleNext::Stay(Cycle::Recognizing { seq, force }),
            vec![Effect::Recognize { seq }],
        ),
        other => (
            CycleNext::Stay(other),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "frame-dropped"),
            ],
        ),
    }
}

fn on_capture_failed(
    cycle: Cycle,
    seq: Seq,
    error: ErrorKind,
    jitter_pct: i8,
) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Capturing { seq: current, .. } if seq == current => (
            CycleNext::Fault {
                kind: FaultKind::Capture(error),
                jitter_pct,
            },
            vec![Effect::ReleaseFrame { seq }, Effect::Emit(Notice::Status)],
        ),
        other => (
            CycleNext::Stay(other),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "stale-capture-failure"),
            ],
        ),
    }
}

// §3.4.4.
fn on_recognized(cycle: Cycle, seq: Seq) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Recognizing {
            seq: current,
            force,
        } if seq == current => (
            CycleNext::Stay(Cycle::Evaluating { seq, force }),
            vec![Effect::Evaluate { seq, force }],
        ),
        other => (
            CycleNext::Stay(other),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "stale-recognition"),
            ],
        ),
    }
}

fn on_recognize_failed(
    cycle: Cycle,
    seq: Seq,
    error: ErrorKind,
    jitter_pct: i8,
) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Recognizing { seq: current, .. } if seq == current => (
            CycleNext::Fault {
                kind: FaultKind::Recognize(error),
                jitter_pct,
            },
            vec![Effect::ReleaseFrame { seq }, Effect::Emit(Notice::Status)],
        ),
        other => (
            CycleNext::Stay(other),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "stale-recognition-failure"),
            ],
        ),
    }
}

// §3.4.5: the frame is released either way, so nothing derived from the
// screen outlives the decision it informed.
fn on_evaluated(
    session: &mut Session,
    cycle: Cycle,
    seq: Seq,
    verdict: Verdict,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    let Cycle::Evaluating { seq: current, .. } = cycle else {
        return (
            CycleNext::Stay(cycle),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "stale-evaluation"),
            ],
        );
    };
    if seq != current {
        return (
            CycleNext::Stay(cycle),
            vec![
                Effect::ReleaseFrame { seq },
                Effect::Log(Level::Warn, "stale-evaluation"),
            ],
        );
    }
    session.retry_attempt = 0;
    match verdict {
        Verdict::Unchanged | Verdict::Pending => (
            CycleNext::Stay(Cycle::Idle {
                next_capture_in: cfg.capture_interval_ticks,
            }),
            vec![Effect::ReleaseFrame { seq }],
        ),
        Verdict::Changed => {
            let request = session.take_request();
            (
                CycleNext::Stay(Cycle::Inferencing {
                    seq,
                    request,
                    started_tick: session.tick,
                }),
                vec![
                    Effect::StartInference { request, seq },
                    Effect::ReleaseFrame { seq },
                    Effect::Emit(Notice::AnswerStarted { request }),
                ],
            )
        }
    }
}

// §3.4.6: the pacer decides how much of the buffer actually leaves, so the
// effect is "release what is due", not "send this chunk".
fn on_inference_chunk(cycle: Cycle, request: RequestId) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Inferencing {
            request: current, ..
        } if request == current => (CycleNext::Stay(cycle), vec![Effect::EmitChunk { request }]),
        other => (
            CycleNext::Stay(other),
            vec![Effect::Log(Level::Warn, "stale-chunk")],
        ),
    }
}

fn on_inference_done(
    session: &mut Session,
    cycle: Cycle,
    request: RequestId,
    remaining_chunks: u32,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Inferencing {
            seq,
            request: current,
            ..
        } if request == current => {
            session.retry_attempt = 0;
            if remaining_chunks == 0 {
                (
                    CycleNext::Stay(Cycle::Idle {
                        next_capture_in: cfg.post_answer_delay_ticks,
                    }),
                    vec![Effect::Emit(Notice::AnswerDone { request })],
                )
            } else {
                (
                    CycleNext::Stay(Cycle::Rendering {
                        seq,
                        request,
                        remaining_chunks,
                    }),
                    Vec::new(),
                )
            }
        }
        other => (
            CycleNext::Stay(other),
            vec![Effect::Log(Level::Warn, "stale-inference-done")],
        ),
    }
}

fn on_inference_failed(
    session: &mut Session,
    cycle: Cycle,
    request: RequestId,
    error: ErrorKind,
    retryable: bool,
    jitter_pct: i8,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Inferencing {
            request: current, ..
        } if request == current => {
            let effects = vec![Effect::Emit(Notice::AnswerFailed { request, error })];
            if retryable {
                (
                    CycleNext::Fault {
                        kind: FaultKind::Inference(error),
                        jitter_pct,
                    },
                    effects,
                )
            } else {
                session.retry_attempt = 0;
                (
                    CycleNext::Stay(Cycle::Idle {
                        next_capture_in: cfg.capture_interval_ticks,
                    }),
                    effects,
                )
            }
        }
        other => (
            CycleNext::Stay(other),
            vec![Effect::Log(Level::Warn, "stale-inference-failure")],
        ),
    }
}

// §3.4.7.
fn on_chunk_rendered(
    session: &mut Session,
    cycle: Cycle,
    request: RequestId,
    cfg: &MachineConfig,
) -> (CycleNext, Vec<Effect>) {
    match cycle {
        Cycle::Rendering {
            seq,
            request: current,
            remaining_chunks,
        } if request == current => {
            let remaining = remaining_chunks.saturating_sub(1);
            if remaining == 0 {
                session.retry_attempt = 0;
                (
                    CycleNext::Stay(Cycle::Idle {
                        next_capture_in: cfg.post_answer_delay_ticks,
                    }),
                    vec![Effect::Emit(Notice::AnswerDone { request })],
                )
            } else {
                (
                    CycleNext::Stay(Cycle::Rendering {
                        seq,
                        request,
                        remaining_chunks: remaining,
                    }),
                    Vec::new(),
                )
            }
        }
        other => (
            CycleNext::Stay(other),
            vec![Effect::Log(Level::Warn, "stale-chunk-rendered")],
        ),
    }
}
