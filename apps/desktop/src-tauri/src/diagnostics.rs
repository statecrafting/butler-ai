// Spec: specs/016-diagnostics-and-logging/spec.md

//! The diagnostics bundle (spec 016 §3.2).
//!
//! A user-initiated archive of what is needed to reconstruct a failure, and
//! nothing else. Spec 015 §3.5 says what may be in it: the log (which by
//! construction carries only ids, kinds and counts), the settings (which
//! carry no secret, because credentials live in the OS keychain), the recent
//! machine transitions, and the platform's own facts.
//!
//! FR-003 fixes the file list at exactly four names. That is asserted rather
//! than described, because "and nothing else" is the requirement: an archive
//! that quietly grew a fifth member would be a privacy change nobody
//! reviewed.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use butler_core::ipc::{ExclusionSummary, StateName};
use butler_core::settings::Settings;
use serde::Serialize;

/// The four members FR-003 fixes, in the order they are written.
pub const BUNDLE_MEMBERS: [&str; 4] = [
    "log.jsonl",
    "settings.toml",
    "transitions.json",
    "system.json",
];

/// How many recent transitions the bundle carries (§3.2).
pub const TRANSITION_CAPACITY: usize = 50;

/// How many log lines the bundle carries (§3.2).
pub const LOG_LINES: usize = 500;

/// One state transition, as the bundle records it.
///
/// Ids and names only. Spec 015 §3.5: a transition is exactly the kind of
/// thing that *can* be recorded, because it names states and events rather
/// than what was on the screen when they happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransitionRecord {
    /// The state before.
    pub from: StateName,
    /// The state after.
    pub to: StateName,
    /// The event's name, a `&'static str` from the reducer.
    pub event: &'static str,
    /// The capture sequence number, if the transition had one.
    pub seq: Option<u64>,
}

/// Platform facts, for support triage (§3.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SystemInfo {
    /// The app's version, from its manifest.
    pub app_version: &'static str,
    /// The target triple this binary was built for.
    pub target: &'static str,
    /// The OS family, as Rust names it.
    pub os: &'static str,
    /// The CPU architecture.
    pub arch: &'static str,
    /// What capture exclusion last reported.
    pub exclusion: ExclusionSummary,
}

impl SystemInfo {
    /// The facts this build knows about itself.
    #[must_use]
    pub fn collect(exclusion: ExclusionSummary) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION"),
            target: std::env::consts::FAMILY,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            exclusion,
        }
    }
}

/// Everything the bundle contains, before it is written (§3.2).
#[derive(Clone, Debug)]
pub struct DiagnosticsBundle {
    /// The most recent log lines.
    log: Vec<String>,
    /// The configuration, as canonical TOML.
    settings: Settings,
    /// The most recent machine transitions.
    transitions: Vec<TransitionRecord>,
    /// Platform facts.
    system: SystemInfo,
}

/// What can go wrong assembling or writing a bundle.
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    /// A file could not be read or written.
    #[error("diagnostics io at {path}: {source}")]
    Io {
        /// Which file.
        path: PathBuf,
        /// What the OS said.
        #[source]
        source: std::io::Error,
    },
    /// A member could not be serialized. A defect, not a user error.
    #[error("diagnostics serialize: {0}")]
    Serialize(String),
}

impl DiagnosticsBundle {
    /// Assemble a bundle from what the process knows (§3.2).
    ///
    /// The log is read from disk rather than buffered in memory: the file is
    /// the record, and reading it back means the bundle contains exactly what
    /// the log contains, with no second path that could include more.
    #[must_use]
    pub fn collect(
        log_directory: &Path,
        settings: Settings,
        transitions: Vec<TransitionRecord>,
        exclusion: ExclusionSummary,
    ) -> Self {
        Self {
            log: recent_log_lines(log_directory),
            settings,
            transitions: transitions
                .into_iter()
                .rev()
                .take(TRANSITION_CAPACITY)
                .rev()
                .collect(),
            system: SystemInfo::collect(exclusion),
        }
    }

    /// Write the bundle as a single `.zip` (§3.2).
    ///
    /// # Errors
    ///
    /// [`BundleError::Io`] if the archive cannot be written,
    /// [`BundleError::Serialize`] if a member cannot be rendered.
    pub fn write(&self, path: &Path) -> Result<(), BundleError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| BundleError::Io { path, source }
        };

        let settings = toml::to_string_pretty(&self.settings)
            .map_err(|e| BundleError::Serialize(e.to_string()))?;
        let transitions = serde_json::to_string_pretty(&self.transitions)
            .map_err(|e| BundleError::Serialize(e.to_string()))?;
        let system = serde_json::to_string_pretty(&self.system)
            .map_err(|e| BundleError::Serialize(e.to_string()))?;

        let file = std::fs::File::create(path).map_err(io(path))?;
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // FR-003: exactly these four, in this order, and nothing else. The
        // list is a constant so the test and the writer cannot disagree.
        let bodies = [self.log.join("\n"), settings, transitions, system];
        for (name, body) in BUNDLE_MEMBERS.iter().zip(bodies) {
            zip.start_file(*name, options)
                .map_err(|e| BundleError::Serialize(e.to_string()))?;
            zip.write_all(body.as_bytes()).map_err(io(path))?;
        }

        zip.finish()
            .map_err(|e| BundleError::Serialize(e.to_string()))?;
        Ok(())
    }

    /// The log lines the bundle carries. Tests read this.
    #[must_use]
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// The transitions the bundle carries.
    #[must_use]
    pub fn transitions(&self) -> &[TransitionRecord] {
        &self.transitions
    }
}

/// The last [`LOG_LINES`] lines across the rolling log files.
///
/// A missing or unreadable directory yields no lines rather than an error: a
/// bundle without a log is still worth having, and refusing to produce one
/// because the log is missing would withhold the other three members exactly
/// when something is wrong.
fn recent_log_lines(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    // The appender's names end in the date, so lexical order is chronological.
    files.sort();

    let mut lines = Vec::new();
    for file in files {
        if let Ok(body) = std::fs::read_to_string(&file) {
            lines.extend(body.lines().map(ToOwned::to_owned));
        }
    }

    let start = lines.len().saturating_sub(LOG_LINES);
    lines.split_off(start)
}

#[cfg(test)]
mod tests {
    use super::{
        BUNDLE_MEMBERS, DiagnosticsBundle, LOG_LINES, TRANSITION_CAPACITY, TransitionRecord,
    };
    use butler_core::ipc::{ExclusionSummary, StateName};
    use butler_core::settings::Settings;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("butler-diag-{name}-{unique}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn transition(seq: u64) -> TransitionRecord {
        TransitionRecord {
            from: StateName::Armed,
            to: StateName::Armed,
            event: "FrameCaptured",
            seq: Some(seq),
        }
    }

    /// FR-003. Exactly four members, with exactly these names.
    ///
    /// "And nothing else" is the requirement, so the archive is opened and
    /// its entries are compared to the list. An archive that quietly grew a
    /// fifth member would be a privacy change nobody reviewed.
    #[test]
    fn fr_003_the_archive_contains_exactly_four_named_members() {
        let logs = TempDir::new("members-logs");
        let out = TempDir::new("members-out");
        let path = out.0.join("bundle.zip");

        DiagnosticsBundle::collect(
            &logs.0,
            Settings::default(),
            vec![transition(1)],
            ExclusionSummary::Verified,
        )
        .write(&path)
        .expect("write the bundle");

        let file = std::fs::File::open(&path).expect("open the archive");
        let mut zip = zip::ZipArchive::new(file).expect("read the archive");

        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).expect("entry").name().to_owned())
            .collect();
        names.sort();

        let mut expected: Vec<String> = BUNDLE_MEMBERS.iter().map(|s| (*s).to_owned()).collect();
        expected.sort();

        assert_eq!(names, expected, "FR-003 fixes the member list");
    }

    #[test]
    fn the_settings_member_is_the_canonical_toml() {
        let logs = TempDir::new("toml-logs");
        let out = TempDir::new("toml-out");
        let path = out.0.join("bundle.zip");

        DiagnosticsBundle::collect(
            &logs.0,
            Settings::default(),
            Vec::new(),
            ExclusionSummary::Unknown,
        )
        .write(&path)
        .expect("write");

        let file = std::fs::File::open(&path).expect("open");
        let mut zip = zip::ZipArchive::new(file).expect("read");
        let mut body = String::new();
        {
            use std::io::Read as _;
            zip.by_name("settings.toml")
                .expect("settings member")
                .read_to_string(&mut body)
                .expect("read settings");
        }

        assert_eq!(
            body,
            toml::to_string_pretty(&Settings::default()).expect("serialize"),
            "the bundle's settings must be what the store would write"
        );
        // Spec 015 §3.4: there is nothing secret in the file by construction,
        // and this is the assertion that says so out loud.
        assert!(!body.contains("sk-"), "no credential may appear: {body}");
    }

    /// §3.2 caps the transitions. The *most recent* are kept: a bundle taken
    /// after a fault should contain the fault, not the launch.
    #[test]
    fn the_most_recent_transitions_are_the_ones_kept() {
        let logs = TempDir::new("transitions");
        let many: Vec<TransitionRecord> = (0..(TRANSITION_CAPACITY as u64 + 20))
            .map(transition)
            .collect();

        let bundle = DiagnosticsBundle::collect(
            &logs.0,
            Settings::default(),
            many,
            ExclusionSummary::Verified,
        );

        assert_eq!(bundle.transitions().len(), TRANSITION_CAPACITY);
        assert_eq!(
            bundle.transitions().last().and_then(|t| t.seq),
            Some(TRANSITION_CAPACITY as u64 + 19),
            "the newest transition must survive the cap"
        );
        assert_eq!(
            bundle.transitions().first().and_then(|t| t.seq),
            Some(20),
            "the oldest kept is exactly `capacity` from the end"
        );
    }

    #[test]
    fn a_missing_log_directory_still_produces_a_bundle() {
        let out = TempDir::new("nolog-out");
        let missing = out.0.join("does-not-exist");
        let path = out.0.join("bundle.zip");

        let bundle = DiagnosticsBundle::collect(
            &missing,
            Settings::default(),
            Vec::new(),
            ExclusionSummary::Unknown,
        );
        assert!(bundle.log().is_empty());
        // The other three members are still worth having, and refusing to
        // produce them because the log is missing would withhold evidence
        // exactly when something is wrong.
        bundle.write(&path).expect("write without a log");
        assert!(path.exists());
    }

    #[test]
    fn only_the_most_recent_log_lines_are_carried() {
        let logs = TempDir::new("loglines");
        let mut body = String::new();
        for i in 0..(LOG_LINES + 50) {
            use std::fmt::Write as _;
            let _ = writeln!(body, "{{\"n\":{i}}}");
        }
        std::fs::write(logs.0.join("butler.2026-09-07.jsonl"), body).expect("write log");

        let bundle = DiagnosticsBundle::collect(
            &logs.0,
            Settings::default(),
            Vec::new(),
            ExclusionSummary::Unknown,
        );

        assert_eq!(bundle.log().len(), LOG_LINES);
        assert_eq!(bundle.log().last().map(String::as_str), Some("{\"n\":549}"));
    }
}
