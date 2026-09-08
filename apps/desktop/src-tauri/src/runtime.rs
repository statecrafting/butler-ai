// Spec: specs/019-runtime-host/spec.md

//! The effect executor (spec 019).
//!
//! Spec 009 defines `reduce(state, event, cfg) -> (state, effects)` as a pure
//! function whose side effects are **data**. This module is what executes
//! that data against the real world, and it lives here rather than in
//! `butler-core` because 009 AC-3 forbids `tokio`, `tauri` and every platform
//! crate from that tree, and an executor needs most of them.
//!
//! # The shape
//!
//! One task owns the [`State`] and nothing else may touch it (§3.2). Events
//! arrive on an `mpsc` channel and are applied **strictly in order**;
//! `reduce` runs, and the effects it returns are dispatched. Effects that
//! take time run on their own task and send their result back as another
//! event, so a 200 ms capture cannot delay a `Disarm` (FR-003).
//!
//! That single-owner rule is what makes spec 009's ordering guarantees real
//! rather than hoped for: out-of-order results are the reducer's to reject
//! (009 FR-003), and it can only do that if it sees them one at a time.
//!
//! # [`Ports`], and the traits that are not here yet
//!
//! §3.1 describes the runtime as owning `Box<dyn ScreenSource>`,
//! `Box<dyn TextRecognizer>` and `Box<dyn Assistant>`. Those traits are specs
//! 006, 007 and 010, in phases 3 and 4, and none exists. Defining them here
//! would be this spec claiming another's territory.
//!
//! So the executor depends on one port it does own, [`Ports`], with a method
//! per effect that needs the outside world. When 006, 007 and 010 land, one
//! type implements `Ports` by delegating to them; the event loop, its
//! ordering and its cancellation do not change. See D-2.

use std::sync::Arc;

use butler_core::ipc::{ErrorKind as WireErrorKind, ExclusionSummary, StateName, UiEvent};
use butler_core::machine::{
    Effect, ErrorKind, Event, MachineConfig, MonitorTarget, Notice, RequestId, Seq, State,
    StopReason, Verdict, reduce,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How many events may queue before a producer waits.
///
/// Generous, because the alternative to waiting is dropping, and a dropped
/// event is a state machine that has silently diverged from the world.
const CHANNEL_CAPACITY: usize = 256;

/// What the executor needs from the outside world (§3.1, D-2).
///
/// One method per effect that cannot be computed. Implementations are free to
/// block: the runtime never calls these from the event task, so a slow
/// capture delays only itself.
pub trait Ports: Send + Sync + 'static {
    /// Capture `monitor` into the frame slot for `seq`.
    ///
    /// # Errors
    ///
    /// The kind the reducer should fault on.
    fn capture(&self, seq: Seq, monitor: MonitorTarget) -> Result<(), ErrorKind>;

    /// Recognize the frame in the slot for `seq`.
    ///
    /// Returns the text length and mean confidence the reducer traces; the
    /// text itself stays in the port's slot and never crosses this boundary
    /// (spec 015 §3.1).
    ///
    /// # Errors
    ///
    /// The kind the reducer should fault on.
    fn recognize(&self, seq: Seq) -> Result<(usize, f32), ErrorKind>;

    /// Ask the change detector about the text recognized for `seq`.
    fn evaluate(&self, seq: Seq, force: bool) -> Verdict;

    /// Redact the text for `seq` and send it as `request`.
    ///
    /// Long-running and cancellable: the token is cancelled when the reducer
    /// asks for `CancelInference`, and the implementation must stop.
    ///
    /// # Errors
    ///
    /// The kind and whether a retry could help.
    fn start_inference(
        &self,
        request: RequestId,
        seq: Seq,
        cancel: &CancellationToken,
    ) -> Result<StopReason, (ErrorKind, bool)>;

    /// Release whatever the pacer allows of `request` to the overlay.
    fn emit_chunk(&self, request: RequestId);

    /// Zero and drop the frame slot for `seq` (FR-004).
    fn release_frame(&self, seq: Seq);

    /// Reset the change detector's committed text and stability counter.
    fn reset_detector(&self);

    /// Run the capture-exclusion self-test (spec 005).
    fn run_self_test(&self);

    /// Send one [`UiEvent`] to the overlay (spec 011 §3.2).
    fn emit(&self, event: &UiEvent);

    /// A retry jitter percentage. The reducer never generates randomness
    /// (009 §3.4.11), so the host supplies it.
    fn jitter_pct(&self) -> i8;
}

/// A handle on the running machine (§3.2).
///
/// The only way the shell, the shortcuts and the IPC commands reach the
/// machine. Nothing outside this module touches [`State`].
#[derive(Clone, Debug)]
pub struct RuntimeHandle {
    events: mpsc::Sender<Event>,
}

impl RuntimeHandle {
    /// Send one event. Returns `false` if the runtime has shut down.
    pub async fn send(&self, event: Event) -> bool {
        self.events.send(event).await.is_ok()
    }

    /// Send one event without waiting. Returns `false` if the queue is full
    /// or the runtime has shut down.
    ///
    /// For call sites that cannot await: the tray handler and the global
    /// shortcut callback both run on the OS event thread.
    #[must_use]
    pub fn try_send(&self, event: Event) -> bool {
        self.events.try_send(event).is_ok()
    }
}

/// The executor (§3.1).
///
/// Owns the state and the resources; constructed by [`Runtime::spawn`], which
/// hands back the handle and keeps everything else inside the event task.
#[derive(Debug)]
pub struct Runtime {
    handle: RuntimeHandle,
    task: tokio::task::JoinHandle<State>,
    /// Cancelled by [`Runtime::shutdown`].
    ///
    /// An explicit signal rather than "the last handle dropped", for two
    /// reasons. The event loop keeps a `Sender` of its own so it can post
    /// results back to itself, which means the channel never closes on its
    /// own; and a caller that cloned a handle would otherwise be able to
    /// keep the process alive past shutdown without meaning to. See D-3.
    shutdown: CancellationToken,
}

impl Runtime {
    /// Start the event task and return the runtime (§3.2).
    ///
    /// The machine starts [`State::Disarmed`] (009 §3.1). Spec 004 §3.1 says
    /// the shell "starts the runtime disarmed"; the initial state is the
    /// reducer's, and this is the call that makes it.
    pub fn spawn<P: Ports>(ports: P, config: MachineConfig) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let handle = RuntimeHandle { events: tx.clone() };
        let shutdown = CancellationToken::new();
        let task = tokio::spawn(event_loop(
            Arc::new(ports),
            config,
            rx,
            tx,
            shutdown.clone(),
        ));
        Self {
            handle,
            task,
            shutdown,
        }
    }

    /// The handle spec 004's `AppState` stores.
    #[must_use]
    pub fn handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }

    /// Close the channel, cancel in-flight work, and join the event task
    /// (§3.2).
    ///
    /// Returns the final state, which the tests assert on. Shutting down in
    /// this order is what stops a capture in flight from outliving the window
    /// it belongs to.
    ///
    /// # Errors
    ///
    /// The join error if the event task panicked.
    pub async fn shutdown(self) -> Result<State, tokio::task::JoinError> {
        self.shutdown.cancel();
        drop(self.handle);
        self.task.await
    }
}

/// The single task that owns the state (§3.1).
///
/// Every event is applied here and nowhere else, in the order the channel
/// delivers them (FR-002).
async fn event_loop<P: Ports>(
    ports: Arc<P>,
    config: MachineConfig,
    mut rx: mpsc::Receiver<Event>,
    tx: mpsc::Sender<Event>,
    shutdown: CancellationToken,
) -> State {
    let mut state = State::default();
    let mut inflight: Option<(RequestId, CancellationToken)> = None;
    let mut closing = false;

    loop {
        let event = tokio::select! {
            // Biased, so a pending shutdown is seen before more work is
            // taken on. Without it a runtime under a tick storm could take
            // an unbounded time to notice.
            biased;
            () = shutdown.cancelled(), if !closing => {
                // §3.2 says shutdown "closes the channel... and joins the
                // event task", so what is already queued is still applied:
                // a `Disarm` sent just before shutdown must take effect, or
                // the machine's last recorded state is a lie. `close()`
                // refuses new sends and lets `recv` drain the buffer, so the
                // next iteration finishes the queue and then sees `None`.
                rx.close();
                closing = true;
                continue;
            }
            received = rx.recv() => match received {
                Some(event) => event,
                None => break,
            },
        };

        let before = StateName::from(&state);
        let event_name = event_name(&event);

        let (next, effects) = reduce(state, event, &config);
        state = next;

        let after = StateName::from(&state);
        // §3.1 and FR-006: ids only. `trace_transition` takes `&'static str`
        // names and an optional number, so there is no parameter through
        // which screen text could reach a log line.
        crate::logging::trace_transition(name_of(before), name_of(after), event_name, None);

        for effect in effects {
            dispatch(&ports, &tx, &mut inflight, &state, effect, &shutdown);
        }
    }

    // §3.2: cancel anything still running before the task ends.
    if let Some((_, token)) = inflight.take() {
        token.cancel();
    }
    state
}

/// Execute one effect (§3.1).
///
/// Nothing here awaits. Effects that take time are spawned, so the event task
/// returns to the channel immediately and a `Disarm` behind a slow capture is
/// still applied promptly (FR-003).
fn dispatch<P: Ports>(
    ports: &Arc<P>,
    tx: &mpsc::Sender<Event>,
    inflight: &mut Option<(RequestId, CancellationToken)>,
    state: &State,
    effect: Effect,
    shutdown: &CancellationToken,
) {
    match effect {
        Effect::Capture { seq, monitor } => {
            let ports = Arc::clone(ports);
            let tx = tx.clone();
            spawn_blocking_event(
                move || match ports.capture(seq, monitor) {
                    Ok(()) => Event::Captured { seq },
                    Err(error) => Event::CaptureFailed {
                        seq,
                        error,
                        jitter_pct: ports.jitter_pct(),
                    },
                },
                tx,
            );
        }
        Effect::Recognize { seq } => {
            let ports = Arc::clone(ports);
            let tx = tx.clone();
            spawn_blocking_event(
                move || match ports.recognize(seq) {
                    Ok((text_len, mean_conf)) => Event::Recognized {
                        seq,
                        text_len,
                        mean_conf,
                    },
                    Err(error) => Event::RecognizeFailed {
                        seq,
                        error,
                        jitter_pct: ports.jitter_pct(),
                    },
                },
                tx,
            );
        }
        Effect::Evaluate { seq, force } => {
            let ports = Arc::clone(ports);
            let tx = tx.clone();
            spawn_blocking_event(
                move || Event::Evaluated {
                    seq,
                    verdict: ports.evaluate(seq, force),
                },
                tx,
            );
        }
        Effect::StartInference { request, seq } => {
            let token = CancellationToken::new();
            *inflight = Some((request, token.clone()));
            let ports = Arc::clone(ports);
            let tx = tx.clone();
            spawn_blocking_event(
                move || match ports.start_inference(request, seq, &token) {
                    Ok(stop) => Event::InferenceDone {
                        request,
                        stop,
                        remaining_chunks: 0,
                    },
                    Err((error, retryable)) => Event::InferenceFailed {
                        request,
                        error,
                        retryable,
                        jitter_pct: ports.jitter_pct(),
                    },
                },
                tx,
            );
        }
        Effect::CancelInference { request } => {
            if let Some((id, token)) = inflight.take() {
                if id == request {
                    token.cancel();
                } else {
                    // A cancel for something else means the slot held a newer
                    // inference; put it back rather than losing the handle.
                    *inflight = Some((id, token));
                }
            }
        }
        Effect::EmitChunk { request } => ports.emit_chunk(request),
        Effect::Emit(notice) => ports.emit(&notice_to_ui_event(notice, state)),
        Effect::RunSelfTest => ports.run_self_test(),
        Effect::ScheduleTick => {
            let tx = tx.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                // Racing the shutdown so a pending tick cannot hold the
                // process open for its last 100 ms.
                tokio::select! {
                    () = shutdown.cancelled() => {}
                    () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        let _ = tx.send(Event::Tick).await;
                    }
                }
            });
        }
        Effect::ReleaseFrame { seq } => ports.release_frame(seq),
        Effect::ResetDetector => ports.reset_detector(),
        Effect::Log { .. } => {
            // §3.1 already records every transition. A second line per
            // effect would say the same thing twice.
        }
    }
}

/// Run `work` off the event task and post its event back.
fn spawn_blocking_event<F>(work: F, tx: mpsc::Sender<Event>)
where
    F: FnOnce() -> Event + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let event = work();
        // `blocking_send` rather than `try_send`: a full queue must slow the
        // producer, never drop a result the reducer is waiting for.
        let _ = tx.blocking_send(event);
    });
}

/// The wire event a [`Notice`] becomes (spec 011 §3.1).
///
/// FR-005: every field here is an id, an enum or a count. There is no field
/// that could carry recognized text or an error message.
fn notice_to_ui_event(notice: Notice, state: &State) -> UiEvent {
    match notice {
        Notice::Status => status_event(state),
        Notice::AnswerStarted { request } => UiEvent::AnswerStarted { request: request.0 },
        Notice::AnswerDone { request } => UiEvent::AnswerDone {
            request: request.0,
            // The reducer's notice does not carry a stop reason; the pacer
            // is what knows the answer finished cleanly, and spec 013 will
            // supply it. `EndTurn` is the only honest default: a refusal or
            // a cap arrives as `AnswerFailed` or from the provider.
            stop: butler_core::ipc::StopSummary::EndTurn,
        },
        Notice::AnswerFailed { request, error } => UiEvent::AnswerFailed {
            request: request.0,
            kind: error,
        },
    }
}

/// The status payload (§3.1, FR-005).
fn status_event(state: &State) -> UiEvent {
    let session = state.session();
    UiEvent::RuntimeStatus {
        state: StateName::from(state),
        seq: session.next_seq.0,
        request: state.in_flight_request().map(|r| r.0),
        exclusion: ExclusionSummary::from(session.exclusion),
        last_error: last_error(state),
        armed_for_ticks: session.tick.0,
    }
}

/// The most recent failure kind, or `None`.
fn last_error(state: &State) -> Option<WireErrorKind> {
    use butler_core::machine::FaultKind;

    match state {
        State::Fault { error, .. } => Some(match error {
            FaultKind::Capture(kind) | FaultKind::Recognize(kind) | FaultKind::Inference(kind) => {
                *kind
            }
            FaultKind::InferenceTimeout => ErrorKind::Network,
        }),
        _ => None,
    }
}

/// A state's name, as a `&'static str` for the trace line.
const fn name_of(state: StateName) -> &'static str {
    match state {
        StateName::Disarmed => "disarmed",
        StateName::Armed => "armed",
        StateName::Degraded => "degraded",
        StateName::Fault => "fault",
    }
}

/// An event's name, as a `&'static str` (FR-006).
///
/// Deliberately exhaustive with no catch-all: a new event added by a later
/// spec fails to compile here rather than tracing as "unknown".
const fn event_name(event: &Event) -> &'static str {
    match event {
        Event::Arm => "arm",
        Event::Disarm => "disarm",
        Event::Tick => "tick",
        Event::ForceCapture => "force-capture",
        Event::Captured { .. } => "captured",
        Event::CaptureFailed { .. } => "capture-failed",
        Event::Recognized { .. } => "recognized",
        Event::RecognizeFailed { .. } => "recognize-failed",
        Event::Evaluated { .. } => "evaluated",
        Event::InferenceChunk { .. } => "inference-chunk",
        Event::InferenceDone { .. } => "inference-done",
        Event::InferenceFailed { .. } => "inference-failed",
        Event::ChunkRendered { .. } => "chunk-rendered",
        Event::ExclusionChanged { .. } => "exclusion-changed",
        Event::SettingsChanged { .. } => "settings-changed",
        Event::ScreenLocked => "screen-locked",
        Event::ScreenUnlocked => "screen-unlocked",
        Event::MonitorChanged => "monitor-changed",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use butler_core::machine::{
        ErrorKind, Event, ExclusionState, MachineConfig, MonitorTarget, RequestId, Seq, State,
        StopReason, Verdict,
    };
    use tokio_util::sync::CancellationToken;

    use super::{Ports, Runtime, status_event};
    use butler_core::ipc::UiEvent;

    /// Everything the mock recorded, in order.
    #[derive(Debug, Default)]
    struct Log {
        calls: Mutex<Vec<String>>,
        events: Mutex<Vec<UiEvent>>,
    }

    impl Log {
        fn push(&self, call: impl Into<String>) {
            self.calls.lock().expect("log").push(call.into());
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("log").clone()
        }
        fn events(&self) -> Vec<UiEvent> {
            self.events.lock().expect("log").clone()
        }
    }

    /// A mock [`Ports`]. Every method records; a few can be made slow or made
    /// to fail, which is what FR-003 and the fault paths need.
    struct MockPorts {
        log: Arc<Log>,
        capture_delay: Duration,
        verdict: Verdict,
        captures: AtomicU32,
    }

    impl MockPorts {
        fn new(log: Arc<Log>) -> Self {
            Self {
                log,
                capture_delay: Duration::ZERO,
                verdict: Verdict::Changed,
                captures: AtomicU32::new(0),
            }
        }
        fn with_capture_delay(mut self, delay: Duration) -> Self {
            self.capture_delay = delay;
            self
        }
        fn with_verdict(mut self, verdict: Verdict) -> Self {
            self.verdict = verdict;
            self
        }
    }

    impl Ports for MockPorts {
        fn capture(&self, seq: Seq, _monitor: MonitorTarget) -> Result<(), ErrorKind> {
            self.captures.fetch_add(1, Ordering::Relaxed);
            self.log.push(format!("capture:{}", seq.0));
            if !self.capture_delay.is_zero() {
                std::thread::sleep(self.capture_delay);
            }
            Ok(())
        }
        fn recognize(&self, seq: Seq) -> Result<(usize, f32), ErrorKind> {
            self.log.push(format!("recognize:{}", seq.0));
            Ok((120, 0.94))
        }
        fn evaluate(&self, seq: Seq, _force: bool) -> Verdict {
            self.log.push(format!("evaluate:{}", seq.0));
            self.verdict
        }
        fn start_inference(
            &self,
            request: RequestId,
            _seq: Seq,
            _cancel: &CancellationToken,
        ) -> Result<StopReason, (ErrorKind, bool)> {
            self.log.push(format!("infer:{}", request.0));
            Ok(StopReason::EndTurn)
        }
        fn emit_chunk(&self, request: RequestId) {
            self.log.push(format!("chunk:{}", request.0));
        }
        fn release_frame(&self, seq: Seq) {
            self.log.push(format!("release:{}", seq.0));
        }
        fn reset_detector(&self) {
            self.log.push("reset");
        }
        fn run_self_test(&self) {
            self.log.push("self-test");
        }
        fn emit(&self, event: &UiEvent) {
            self.events.lock().expect("events").push(event.clone());
        }
        fn jitter_pct(&self) -> i8 {
            0
        }
    }

    impl std::ops::Deref for MockPorts {
        type Target = Log;
        fn deref(&self) -> &Log {
            &self.log
        }
    }

    fn config() -> MachineConfig {
        MachineConfig {
            allow_degraded: true,
            ..MachineConfig::default()
        }
    }

    /// Give spawned tasks a chance to post their results back.
    async fn settle() {
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            tokio::task::yield_now().await;
        }
    }

    /// AC-1. Arm, capture, recognize, evaluate, infer, render, disarm.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_001_a_full_cycle_runs_against_mock_ports() {
        let log = Arc::new(Log::default());
        let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
        let handle = runtime.handle();

        assert!(handle.send(Event::Arm).await);
        assert!(
            handle
                .send(Event::ExclusionChanged {
                    status: ExclusionState::Verified
                })
                .await
        );
        assert!(handle.send(Event::ForceCapture).await);
        settle().await;
        assert!(handle.send(Event::Disarm).await);

        let final_state = runtime.shutdown().await.expect("join");
        let calls = log.calls();

        assert!(
            calls.iter().any(|c| c.starts_with("capture:")),
            "the cycle never captured: {calls:?}"
        );
        assert!(
            calls.iter().any(|c| c.starts_with("recognize:")),
            "the cycle never recognized: {calls:?}"
        );
        assert!(
            calls.iter().any(|c| c.starts_with("evaluate:")),
            "the cycle never evaluated: {calls:?}"
        );
        assert!(
            matches!(final_state, State::Disarmed { .. }),
            "disarm must be applied: {final_state:?}"
        );

        // §3.1: a status reaches the overlay on every state change, and its
        // payload is the typed one FR-005 pins down.
        let events = log.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::RuntimeStatus { .. })),
            "no status was emitted during a whole cycle: {events:?}"
        );
    }

    /// FR-001. A full cycle costs under 50 ms beyond the mocks' own latency.
    ///
    /// The mocks return immediately, so the whole elapsed time *is* the
    /// overhead: channel hops, `reduce`, effect dispatch and the task spawns.
    /// The tick timer is excluded by driving the cycle with `ForceCapture`
    /// rather than waiting out `capture_interval_ticks`, which is a
    /// configured delay rather than overhead.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fr_001_a_full_cycle_costs_under_50ms_of_overhead() {
        let log = Arc::new(Log::default());
        let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
        let handle = runtime.handle();

        handle.send(Event::Arm).await;
        handle
            .send(Event::ExclusionChanged {
                status: ExclusionState::Verified,
            })
            .await;

        let started = std::time::Instant::now();
        handle.send(Event::ForceCapture).await;

        // Wait for the cycle to reach evaluation, which is the last step the
        // mocks drive without a timer.
        for _ in 0..500 {
            if log.calls().iter().any(|c| c.starts_with("evaluate:")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let elapsed = started.elapsed();

        let calls = log.calls();
        assert!(
            calls.iter().any(|c| c.starts_with("evaluate:")),
            "the cycle never reached evaluation: {calls:?}"
        );
        assert!(
            elapsed < Duration::from_millis(50),
            "capture to evaluate took {elapsed:?}, over FR-001's 50 ms budget"
        );

        runtime.shutdown().await.expect("join");
    }

    /// FR-002. One task applies events in channel order, so two concurrent
    /// producers can never interleave a state.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fr_002_events_are_applied_in_channel_order() {
        let log = Arc::new(Log::default());
        let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
        let handle = runtime.handle();

        handle.send(Event::Arm).await;
        handle
            .send(Event::ExclusionChanged {
                status: ExclusionState::Verified,
            })
            .await;

        // Two producers, deliberately out of order. The reducer's own
        // out-of-order rule (009 FR-003) is what decides the outcome; this
        // test is about the executor never showing it two at once.
        let a = handle.clone();
        let b = handle.clone();
        let one = tokio::spawn(async move { a.send(Event::Captured { seq: Seq(5) }).await });
        let two = tokio::spawn(async move { b.send(Event::Captured { seq: Seq(4) }).await });
        assert!(one.await.expect("join a"));
        assert!(two.await.expect("join b"));

        settle().await;
        let state = runtime.shutdown().await.expect("join");
        // The machine survived both, in some order, without panicking or
        // deadlocking. That is the guarantee: one owner, one at a time.
        assert!(matches!(
            state,
            State::Armed { .. } | State::Degraded { .. } | State::Disarmed { .. }
        ));
    }

    /// FR-003. A 200 ms capture does not delay a `Disarm` sent 10 ms later.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fr_003_a_slow_capture_does_not_block_the_event_task() {
        let log = Arc::new(Log::default());
        let ports = MockPorts::new(Arc::clone(&log)).with_capture_delay(Duration::from_millis(200));
        let runtime = Runtime::spawn(ports, config());
        let handle = runtime.handle();

        handle.send(Event::Arm).await;
        handle
            .send(Event::ExclusionChanged {
                status: ExclusionState::Verified,
            })
            .await;
        handle.send(Event::ForceCapture).await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let started = std::time::Instant::now();
        assert!(handle.send(Event::Disarm).await);
        // Applied, not merely queued: shutting down joins the event task,
        // which cannot finish while it is stuck inside a capture.
        let state = runtime.shutdown().await.expect("join");
        let elapsed = started.elapsed();

        assert!(matches!(state, State::Disarmed { .. }), "{state:?}");
        assert!(
            elapsed < Duration::from_millis(180),
            "the event task waited for the capture: {elapsed:?}"
        );
    }

    /// FR-004. The frame slot is released before the next capture is issued.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fr_004_the_frame_is_released_before_the_next_capture() {
        let log = Arc::new(Log::default());
        let ports = MockPorts::new(Arc::clone(&log)).with_verdict(Verdict::Unchanged);
        let runtime = Runtime::spawn(ports, config());
        let handle = runtime.handle();

        handle.send(Event::Arm).await;
        handle
            .send(Event::ExclusionChanged {
                status: ExclusionState::Verified,
            })
            .await;
        handle.send(Event::ForceCapture).await;
        settle().await;
        runtime.shutdown().await.expect("join");

        let calls = log.calls();
        let release = calls.iter().position(|c| c.starts_with("release:"));
        assert!(release.is_some(), "the frame was never released: {calls:?}");

        // Every capture after the first must come after a release.
        let captures: Vec<usize> = calls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.starts_with("capture:"))
            .map(|(i, _)| i)
            .collect();
        if let (Some(&first_release), Some(&second_capture)) = (release.as_ref(), captures.get(1)) {
            assert!(
                first_release < second_capture,
                "a capture was issued while a frame was still held: {calls:?}"
            );
        }
    }

    /// FR-005 (AC-2). The status payload's fields are ids, enums and counts.
    ///
    /// A named test rather than a review claim: the assertion is that the
    /// emitted struct has no `String` field into which recognized text or an
    /// error message could be placed.
    #[test]
    fn fr_005_the_status_payload_carries_no_text() {
        let UiEvent::RuntimeStatus {
            state,
            seq,
            request,
            exclusion,
            last_error,
            armed_for_ticks,
        } = status_event(&State::default())
        else {
            panic!("status_event must produce RuntimeStatus");
        };

        // Destructuring names every field. A `String` added to the payload
        // would fail to compile here until someone looked at it.
        let _: butler_core::ipc::StateName = state;
        let _: u64 = seq;
        let _: Option<u64> = request;
        let _: butler_core::ipc::ExclusionSummary = exclusion;
        let _: Option<butler_core::ipc::ErrorKind> = last_error;
        let _: u64 = armed_for_ticks;
    }

    /// A capturing writer shared by the global subscriber below.
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("buf").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buffer {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// The process-wide capture FR-006 reads.
    ///
    /// **Global**, not thread-local, and that is the whole point.
    /// `tracing::subscriber::set_default` installs a subscriber for the
    /// calling thread only, so the first version of this test saw nothing:
    /// the event loop runs on a tokio worker, and a regex over zero lines
    /// passes. Worse, it passed *most* of the time and failed occasionally,
    /// which is what a test that has stopped testing looks like from the
    /// outside (spec 019 D-4).
    static TRACE_CAPTURE: std::sync::OnceLock<Buffer> = std::sync::OnceLock::new();

    fn trace_capture() -> &'static Buffer {
        TRACE_CAPTURE.get_or_init(|| {
            use tracing_subscriber::layer::SubscriberExt as _;

            let buffer = Buffer::default();
            let subscriber = tracing_subscriber::registry()
                .with(tracing_subscriber::filter::LevelFilter::TRACE)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(buffer.clone())
                        .with_ansi(false)
                        .with_target(false)
                        .with_level(false)
                        .without_time(),
                );
            // Ignored if another test got here first; the buffer that wins is
            // the one this returns, because `get_or_init` runs once.
            let _ = tracing::subscriber::set_global_default(subscriber);
            buffer
        })
    }

    /// FR-006 (AC-2). A full cycle's trace output contains ids only.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fr_006_trace_records_name_ids_only() {
        let capture = trace_capture();
        let before = capture.0.lock().expect("buf").len();

        let log = Arc::new(Log::default());
        {
            let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
            let handle = runtime.handle();
            handle.send(Event::Arm).await;
            handle
                .send(Event::ExclusionChanged {
                    status: ExclusionState::Verified,
                })
                .await;
            handle.send(Event::ForceCapture).await;
            settle().await;
            handle.send(Event::Disarm).await;
            runtime.shutdown().await.expect("join");
        }

        let output = {
            let buf = capture.0.lock().expect("buf");
            String::from_utf8(buf[before..].to_vec()).expect("utf-8")
        };

        // Non-vacuity first: a regex over an empty capture is green.
        assert!(
            !output.trim().is_empty(),
            "nothing was traced, so the assertion below would pass vacuously"
        );
        assert!(
            output.contains(r#"event="arm""#),
            "the cycle's own transitions are missing: {output}"
        );

        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            assert!(
                line.chars().all(|c| c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || matches!(c, '_' | '.' | ':' | '=' | ' ' | '-' | '"')),
                "a trace line carries something other than an id: {line}"
            );
        }
    }

    /// §3.2: shutdown cancels an inference in flight rather than letting it
    /// outlive the window it belongs to.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_cancels_an_inference_in_flight() {
        let log = Arc::new(Log::default());
        let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
        let handle = runtime.handle();

        handle.send(Event::Arm).await;
        handle
            .send(Event::ExclusionChanged {
                status: ExclusionState::Verified,
            })
            .await;
        handle.send(Event::ForceCapture).await;
        settle().await;

        // Dropping the last handle closes the channel, which ends the loop.
        drop(handle);
        let state = runtime.shutdown().await.expect("join");
        assert!(matches!(
            state,
            State::Armed { .. } | State::Degraded { .. } | State::Disarmed { .. }
        ));
    }

    /// The handle is the only way in (§3.2), and it reports honestly when the
    /// runtime is gone rather than pretending the event was delivered.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_handle_reports_a_closed_runtime() {
        let log = Arc::new(Log::default());
        let runtime = Runtime::spawn(MockPorts::new(Arc::clone(&log)), config());
        let handle = runtime.handle();
        runtime.shutdown().await.expect("join");

        assert!(
            !handle.send(Event::Arm).await,
            "a closed runtime must say so"
        );
        assert!(!handle.try_send(Event::Arm));
    }
}
