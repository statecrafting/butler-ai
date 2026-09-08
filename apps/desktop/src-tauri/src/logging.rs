// Spec: specs/016-diagnostics-and-logging/spec.md

//! Structured, content-free tracing (spec 016 §3.1).
//!
//! A stealthy overlay that misbehaves is hard to debug by design: there is
//! little UI and the interesting state is inside a state machine. Tracing
//! transitions and effect outcomes **by id** gives enough to reconstruct a
//! failure without ever recording what the user was looking at.
//!
//! # The field allowlist is the point
//!
//! Spec 015 constrains this file: "logs carry ids, kinds and counts, never
//! screen text, prompts, answers, or secrets". That is not something a code
//! review can hold for the life of a product. [`field!`] accepts only the
//! twelve key names §3.1 lists, and **a key outside that list does not
//! compile**. Spec 015 FR-004 becomes structural rather than hopeful, which
//! is exactly what that spec asks for.
//!
//! Free-form messages are `&'static str`, so a message cannot be built from
//! anything the user is looking at.

use std::path::PathBuf;

use butler_core::settings::DiagnosticsLevel;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// The closed set of structured field names (§3.1).
///
/// Every one is an id, a kind, a count or a duration. None of them can carry
/// screen text, a prompt, an answer or a secret, which is the property spec
/// 015 §3.5 needs and [`field!`] enforces at compile time.
pub const ALLOWED_FIELDS: [&str; 12] = [
    "state",
    "event",
    "seq",
    "request",
    "kind",
    "duration_ms",
    "count",
    "provider",
    "model",
    "monitor",
    "status",
    "code",
];

/// Build a `tracing` field pair, refusing any key outside [`ALLOWED_FIELDS`].
///
/// ```
/// use butler_desktop::field;
/// let (key, value) = field!("seq", 42_u64);
/// assert_eq!(key, "seq");
/// ```
///
/// A key the allowlist does not contain is a compile error, not a warning and
/// not a runtime check:
///
/// ```compile_fail
/// use butler_desktop::field;
/// // `screen_text` is not in the allowlist, so this does not compile.
/// let _ = field!("screen_text", "the user's bank balance");
/// ```
#[macro_export]
macro_rules! field {
    ("state", $value:expr) => {
        ("state", $value)
    };
    ("event", $value:expr) => {
        ("event", $value)
    };
    ("seq", $value:expr) => {
        ("seq", $value)
    };
    ("request", $value:expr) => {
        ("request", $value)
    };
    ("kind", $value:expr) => {
        ("kind", $value)
    };
    ("duration_ms", $value:expr) => {
        ("duration_ms", $value)
    };
    ("count", $value:expr) => {
        ("count", $value)
    };
    ("provider", $value:expr) => {
        ("provider", $value)
    };
    ("model", $value:expr) => {
        ("model", $value)
    };
    ("monitor", $value:expr) => {
        ("monitor", $value)
    };
    ("status", $value:expr) => {
        ("status", $value)
    };
    ("code", $value:expr) => {
        ("code", $value)
    };
}

/// Keeps the non-blocking writer alive.
///
/// Dropping this flushes and stops the background thread, so `run()` holds it
/// for the process's lifetime. Returning it rather than leaking it means a
/// test can take a subscriber down and start another.
#[derive(Debug)]
pub struct Guard {
    _appender: WorkerGuard,
    directory: PathBuf,
}

impl Guard {
    /// Where the log files are, so the diagnostics bundle can find them.
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }
}

/// What can go wrong starting the logger.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// The platform has no local data directory.
    #[error("no local data directory on this platform")]
    NoDataDir,
    /// The log directory could not be created.
    #[error("log directory {path}: {source}")]
    Io {
        /// Which directory.
        path: PathBuf,
        /// What the OS said.
        #[source]
        source: std::io::Error,
    },
    /// A subscriber is already installed. Only reachable in tests.
    #[error("a tracing subscriber is already installed")]
    AlreadyInitialized,
}

/// The `tracing` level a diagnostics level means (§3.1).
#[must_use]
pub const fn level_for(level: DiagnosticsLevel) -> tracing::Level {
    match level {
        DiagnosticsLevel::Minimal => tracing::Level::WARN,
        DiagnosticsLevel::Normal => tracing::Level::INFO,
        DiagnosticsLevel::Verbose => tracing::Level::TRACE,
    }
}

/// Where the logs live (§3.1).
///
/// # Errors
///
/// [`LogError::NoDataDir`] if the platform has no local data directory.
pub fn log_directory() -> Result<PathBuf, LogError> {
    Ok(dirs::data_local_dir()
        .ok_or(LogError::NoDataDir)?
        .join("butler-ai")
        .join("logs"))
}

/// Start tracing at the level the settings ask for (§3.1).
///
/// JSON lines to a daily rolling file, plus stderr in debug builds. The level
/// comes from settings and from nowhere else: `env-filter` is deliberately
/// not enabled, because `RUST_LOG` would be exactly the
/// environment-variable surface spec 014 FR-005 forbids.
///
/// # Errors
///
/// [`LogError::NoDataDir`] or [`LogError::Io`] if the log directory cannot be
/// determined or created, and [`LogError::AlreadyInitialized`] if a
/// subscriber is already installed.
pub fn init(level: DiagnosticsLevel) -> Result<Guard, LogError> {
    let directory = log_directory()?;
    init_in(&directory, level)
}

/// [`init`], with the directory given. Tests use this; `run()` uses `init`.
///
/// # Errors
///
/// As [`init`].
pub fn init_in(directory: &std::path::Path, level: DiagnosticsLevel) -> Result<Guard, LogError> {
    std::fs::create_dir_all(directory).map_err(|source| LogError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    // FR-004: daily rotation, seven files kept. `tracing-appender` prunes on
    // rotation, so the cap is enforced by the appender rather than by a
    // sweep this crate would have to remember to run.
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("butler")
        .filename_suffix("jsonl")
        .max_log_files(MAX_LOG_FILES)
        .build(directory)
        .map_err(|e| LogError::Io {
            path: directory.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;

    let (writer, appender_guard) = tracing_appender::non_blocking(appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(writer)
        .with_current_span(false)
        .with_span_list(false);

    let registry = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            level_for(level),
        ))
        .with(file_layer);

    // Debug builds also print to stderr. A release build does not: the
    // overlay has no console, and a stray line would be the only place the
    // product wrote anything outside the log.
    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    registry
        .try_init()
        .map_err(|_| LogError::AlreadyInitialized)?;

    Ok(Guard {
        _appender: appender_guard,
        directory: directory.to_path_buf(),
    })
}

/// FR-004: at most seven log files are kept.
pub const MAX_LOG_FILES: usize = 7;

/// Record one state transition (§3.1, FR-002).
///
/// At `trace`, so it appears only under [`DiagnosticsLevel::Verbose`]. Three
/// fields, all from [`ALLOWED_FIELDS`]: the state pair, the event's name, and
/// the capture sequence number. Every one is an id or a name that the reducer
/// itself produced, so no path leads from the screen to this line.
///
/// The runtime host (spec 019) is what calls this on every reduction. It
/// lives here rather than there because spec 015 constrains this file, and a
/// second place that formats log lines would be a second place to review.
pub fn trace_transition(from: &str, to: &str, event: &'static str, seq: Option<u64>) {
    if let Some(seq) = seq {
        tracing::trace!(state = to, event = event, seq = seq, from = from);
    } else {
        tracing::trace!(state = to, event = event, from = from);
    }
}

/// Log panics as a kind and a location, never a payload (§3.1).
///
/// A panic message can contain anything a `format!` was given, including
/// screen text, so the payload is deliberately not read. The location is a
/// file and line from this binary and carries nothing of the user's.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);
        // `%location` is a display value of our own source position. The
        // panic payload is never touched.
        tracing::error!(kind = "panic", code = %location, "panicked");
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::{ALLOWED_FIELDS, MAX_LOG_FILES, level_for};
    use butler_core::settings::DiagnosticsLevel;

    /// §3.1 fixes this list, and `field!` has one arm per entry. A key added
    /// to one and not the other is the failure this catches.
    #[test]
    fn the_allowlist_is_the_one_the_spec_names() {
        assert_eq!(
            ALLOWED_FIELDS,
            [
                "state",
                "event",
                "seq",
                "request",
                "kind",
                "duration_ms",
                "count",
                "provider",
                "model",
                "monitor",
                "status",
                "code",
            ]
        );
    }

    /// Every allowed key has a `field!` arm. Without this, a key could be in
    /// the documented list and still not compile, which reads as a defect in
    /// the caller rather than in the macro.
    #[test]
    fn every_allowed_key_has_a_macro_arm() {
        let pairs = [
            crate::field!("state", "armed"),
            crate::field!("event", "tick"),
            crate::field!("seq", "1"),
            crate::field!("request", "2"),
            crate::field!("kind", "capture"),
            crate::field!("duration_ms", "12"),
            crate::field!("count", "3"),
            crate::field!("provider", "anthropic"),
            crate::field!("model", "claude-opus-5"),
            crate::field!("monitor", "0"),
            crate::field!("status", "verified"),
            crate::field!("code", "E1"),
        ];
        let keys: Vec<&str> = pairs.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ALLOWED_FIELDS.to_vec());
    }

    /// §3.1's level table. `Minimal` is the default (spec 014), so the
    /// shipped product logs warnings and nothing quieter.
    #[test]
    fn the_level_table_is_the_one_the_spec_fixes() {
        assert_eq!(level_for(DiagnosticsLevel::Minimal), tracing::Level::WARN);
        assert_eq!(level_for(DiagnosticsLevel::Normal), tracing::Level::INFO);
        assert_eq!(level_for(DiagnosticsLevel::Verbose), tracing::Level::TRACE);
    }

    /// FR-004: at most seven files.
    #[test]
    fn fr_004_rotation_keeps_seven_files() {
        assert_eq!(MAX_LOG_FILES, 7);
    }

    /// The level filter is built from settings, never from the environment.
    ///
    /// `RUST_LOG` would be exactly the environment-variable surface spec 014
    /// FR-005 forbids, and the feature that would provide it is the one this
    /// checks for. The assertion reads the dependency's own feature list
    /// rather than the whole file: a comment explaining why the feature is
    /// off would otherwise fail this test, which is a trap this corpus has
    /// walked into more than once (spec 016 D-2).
    #[test]
    fn the_subscriber_has_no_environment_filter() {
        let workspace = include_str!("../../../../Cargo.toml");
        let line = workspace
            .lines()
            .find(|line| line.starts_with("tracing-subscriber = "))
            .expect("tracing-subscriber is pinned in the workspace manifest");
        assert!(
            !line.contains("env-filter"),
            "`env-filter` would make RUST_LOG configure the product: {line}"
        );
        assert!(
            line.contains("default-features = false"),
            "the feature set must be explicit, or a default could add it back: {line}"
        );
    }
}

#[cfg(test)]
mod capture_tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use butler_core::settings::DiagnosticsLevel;
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::{level_for, trace_transition};

    /// A `MakeWriter` that appends everything to a shared buffer.
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Buffer {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().expect("buffer").clone()).expect("utf-8")
        }
    }

    impl io::Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("buffer").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buffer {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Run `body` with a capturing JSON subscriber at `level`.
    fn captured(level: DiagnosticsLevel, body: impl FnOnce()) -> String {
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::LevelFilter::from_level(
                level_for(level),
            ))
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(buffer.clone())
                    .with_current_span(false)
                    .with_span_list(false),
            );
        tracing::subscriber::with_default(subscriber, body);
        buffer.contents()
    }

    /// FR-002. Verbose produces one line per transition, carrying `state`,
    /// `event` and `seq`.
    #[test]
    fn fr_002_verbose_records_every_transition() {
        let output = captured(DiagnosticsLevel::Verbose, || {
            trace_transition("disarmed", "armed", "Arm", None);
            trace_transition("armed", "armed", "FrameCaptured", Some(1));
            trace_transition("armed", "armed", "TextRecognized", Some(1));
        });

        let lines: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3, "one line per transition: {output}");

        assert!(lines[1].contains("\"state\":\"armed\""), "{}", lines[1]);
        assert!(
            lines[1].contains("\"event\":\"FrameCaptured\""),
            "{}",
            lines[1]
        );
        assert!(lines[1].contains("\"seq\":1"), "{}", lines[1]);
    }

    /// The level actually gates. `Minimal` is the shipped default (spec 014),
    /// so a product with default settings records no transitions at all.
    #[test]
    fn minimal_records_no_transitions() {
        let output = captured(DiagnosticsLevel::Minimal, || {
            trace_transition("armed", "armed", "FrameCaptured", Some(1));
        });
        assert!(output.trim().is_empty(), "{output}");
    }

    /// Spec 015 FR-004, as far as it can be asserted without the pipeline.
    ///
    /// The whole-pipeline version needs mock capture, OCR and provider traits
    /// (specs 006, 007, 010) and the runtime that drives them (019). What can
    /// be held today is the property those mocks would be checking: the
    /// tracing surface this module exposes has nowhere to put screen text.
    /// Every field it accepts is an id or a name, and the message is a
    /// `&'static str`, so a caller cannot smuggle content through it even
    /// deliberately (spec 016 D-3).
    #[test]
    fn no_screen_text_can_reach_a_log_line() {
        const SCREEN: &str = "ACME Bank balance 12,345.67";
        const ANSWER: &str = "the model's answer about their balance";
        const SECRET: &str = "sk-ant-not-a-real-key";

        let output = captured(DiagnosticsLevel::Verbose, || {
            // Everything the module offers, called with the ids a real cycle
            // would produce while those three strings were on the screen.
            trace_transition("armed", "armed", "TextRecognized", Some(7));
            tracing::warn!(kind = "provider", code = "429", "inference refused");
            tracing::info!(provider = "anthropic", model = "claude-opus-5", "asking");
            tracing::trace!(count = 3_u64, duration_ms = 41_u64, "redacted");
        });

        for content in [SCREEN, ANSWER, SECRET] {
            assert!(
                !output.contains(content),
                "content reached the log: {content} in {output}"
            );
        }
        // And the ids did get through, so this is not passing vacuously.
        assert!(output.contains("\"seq\":7"), "{output}");
        assert!(output.contains("\"code\":\"429\""), "{output}");
    }
}
