// Spec: specs/014-user-configuration/spec.md

//! The settings file (spec 014 §3.2).
//!
//! `butler-core` owns the model and its validation and has no filesystem;
//! this is the half that touches disk. Three properties are the reason it is
//! its own module rather than two `fs` calls at a call site.
//!
//! **The write is atomic.** Serialize to `settings.toml.tmp`, `fsync`, then
//! rename over the real file. A crash or a full disk halfway through leaves
//! the previous settings intact; a plain `write` leaves a truncated file that
//! the next launch cannot parse, which is how a configuration is lost.
//!
//! **A bad file is preserved, not overwritten.** On a parse error the file is
//! moved to `settings.toml.bad` and defaults are used. Silently rewriting it
//! would destroy whatever the user had, including the typo that would tell
//! them what went wrong.
//!
//! **The file is the user's, and only theirs.** `0600` on Unix. It carries no
//! secret by construction (spec 010 puts credentials in the OS keychain, spec
//! 015 §3.4 keeps them out of here), but it does describe what the user
//! watches and where they send it.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use butler_core::settings::{Settings, SettingsError};

/// What can go wrong reading or writing the settings file.
///
/// Paths are carried because the user needs to be told which file to look at;
/// spec 015 §3.5 forbids content, and a path is not content.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The platform has no config directory.
    #[error("no config directory on this platform")]
    NoConfigDir,
    /// The file could not be read or written.
    #[error("settings io at {path}: {source}")]
    Io {
        /// Which file.
        path: PathBuf,
        /// What the OS said.
        #[source]
        source: std::io::Error,
    },
    /// The file parsed but is not a valid configuration.
    #[error("settings invalid: {0:?}")]
    Invalid(Vec<SettingsError>),
    /// The settings could not be serialized. A defect, not a user error.
    #[error("settings serialize: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// How a load ended, so the caller can tell the user (§3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The file was read and is valid.
    Loaded,
    /// No file yet. Defaults, and nothing to report.
    Missing,
    /// The file was unreadable or invalid; it was moved aside and defaults
    /// are in use. The UI shows this once (§3.2).
    Recovered {
        /// Where the unreadable file was moved to.
        backup: PathBuf,
    },
}

/// The settings file, and the operations on it (§3.2).
#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// The store at the platform's config location.
    ///
    /// `~/Library/Application Support/butler-ai/settings.toml` on macOS,
    /// `%APPDATA%\butler-ai\settings.toml` on Windows.
    ///
    /// # Errors
    ///
    /// [`StoreError::NoConfigDir`] if the platform has no config directory,
    /// which in practice means an environment with no home.
    pub fn platform() -> Result<Self, StoreError> {
        let dir = dirs::config_dir().ok_or(StoreError::NoConfigDir)?;
        Ok(Self::at(dir.join("butler-ai").join("settings.toml")))
    }

    /// A store at an explicit path. Tests use this; the app uses
    /// [`Self::platform`].
    #[must_use]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the file is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the settings, recovering from anything unreadable (§3.2).
    ///
    /// Never returns a parse error: a broken file is moved aside and the
    /// defaults are returned with [`LoadOutcome::Recovered`], because the app
    /// starting with a banner is better than the app not starting.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] only when the file exists, is readable, and yet
    /// cannot be moved aside, which means the directory itself is not
    /// writable and nothing later would work either.
    pub fn load(&self) -> Result<(Settings, LoadOutcome), StoreError> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Settings::default(), LoadOutcome::Missing));
            }
            Err(source) => {
                return Err(StoreError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };

        // FR-004: an unknown key is a file from a newer build, or a typo that
        // would otherwise do nothing at all. Both are worth refusing.
        let parsed = toml::from_str::<Settings>(&raw)
            .map_err(|_| ())
            .and_then(|settings| settings.validate().map(|()| settings).map_err(|_| ()));

        if let Ok(settings) = parsed {
            return Ok((settings, LoadOutcome::Loaded));
        }

        // Moved, not deleted: whatever the user had is preserved, including
        // the mistake that explains why this branch was taken.
        let backup = self.backup_path();
        fs::rename(&self.path, &backup).map_err(|source| StoreError::Io {
            path: backup.clone(),
            source,
        })?;
        Ok((Settings::default(), LoadOutcome::Recovered { backup }))
    }

    /// Write the settings atomically (§3.2).
    ///
    /// Validated first, so an invalid configuration cannot reach the disk
    /// even if a caller skipped the check.
    ///
    /// # Errors
    ///
    /// [`StoreError::Invalid`] if the settings do not validate,
    /// [`StoreError::Io`] if the directory cannot be created or the file
    /// cannot be written or renamed.
    pub fn save(&self, settings: &Settings) -> Result<(), StoreError> {
        settings.validate().map_err(StoreError::Invalid)?;

        let body = toml::to_string_pretty(settings)?;

        let dir = self.path.parent().ok_or_else(|| StoreError::Io {
            path: self.path.clone(),
            source: std::io::Error::other("settings path has no parent directory"),
        })?;
        fs::create_dir_all(dir).map_err(|source| StoreError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        // Written next to the target, not in a temp directory: `rename` is
        // only atomic within one filesystem, and a temp directory can be on
        // another one.
        let tmp = self.path.with_extension("toml.tmp");
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| StoreError::Io { path, source }
        };

        {
            let mut file = fs::File::create(&tmp).map_err(io(&tmp))?;
            file.write_all(body.as_bytes()).map_err(io(&tmp))?;
            // Without this the rename can land before the bytes do, and a
            // power loss leaves an empty file where the settings were.
            file.sync_all().map_err(io(&tmp))?;
        }

        set_owner_only(&tmp)?;
        fs::rename(&tmp, &self.path).map_err(io(&self.path))?;
        Ok(())
    }

    /// Where an unreadable file is moved to.
    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("toml.bad")
    }
}

/// `0600` on Unix; the default user ACL on Windows (§3.2).
#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Windows inherits the user's ACL from the directory, which is what §3.2
/// asks for. Setting a mode here would be a no-op that read as protection.
#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the `Result` matches the Unix counterpart, where the call can \
              genuinely fail, so the one call site in `save` is written once \
              rather than behind a `cfg`. Narrowing this to `()` would move \
              the platform difference from here, where it is explained, into \
              the middle of the write path."
)]
fn set_owner_only(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{LoadOutcome, SettingsStore};
    use butler_core::settings::Settings;

    /// A store in a fresh directory under the OS temp dir, removed on drop.
    struct TempStore {
        dir: std::path::PathBuf,
        store: SettingsStore,
    }

    impl TempStore {
        fn new(name: &str) -> Self {
            // A counter rather than a random: the tests run in one process and
            // a name collision between two of them would be a flake nobody
            // could reproduce.
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);

            let dir = std::env::temp_dir().join(format!("butler-settings-{name}-{unique}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp dir");
            let store = SettingsStore::at(dir.join("settings.toml"));
            Self { dir, store }
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_missing_file_is_defaults_and_not_an_error() {
        let t = TempStore::new("missing");
        let (settings, outcome) = t.store.load().expect("load");
        assert_eq!(settings, Settings::default());
        assert_eq!(outcome, LoadOutcome::Missing);
        assert!(!t.store.path().exists(), "load must not create the file");
    }

    #[test]
    fn a_saved_file_round_trips() {
        let t = TempStore::new("roundtrip");
        let mut settings = Settings::default();
        settings.capture.interval_ms = 4000;
        settings.privacy.allow_degraded_mode = true;

        t.store.save(&settings).expect("save");
        let (loaded, outcome) = t.store.load().expect("load");

        assert_eq!(loaded, settings);
        assert_eq!(outcome, LoadOutcome::Loaded);
    }

    /// FR-004. An unknown key is refused, the file is kept, and the app still
    /// starts. Overwriting it would destroy the typo that explains the
    /// problem.
    #[test]
    fn fr_004_an_unknown_key_is_moved_aside_and_defaults_are_used() {
        let t = TempStore::new("unknown-key");
        std::fs::write(t.store.path(), "schema = 1\nnonsense = true\n").expect("write");

        let (settings, outcome) = t.store.load().expect("load");

        assert_eq!(settings, Settings::default());
        let LoadOutcome::Recovered { backup } = outcome else {
            panic!("an unknown key must be recovered from, got {outcome:?}");
        };
        assert!(backup.exists(), "the original must be kept");
        assert!(
            !t.store.path().exists(),
            "the bad file is moved, not copied"
        );
        assert!(
            std::fs::read_to_string(&backup)
                .expect("read backup")
                .contains("nonsense"),
            "the backup must be the file the user wrote, verbatim"
        );
    }

    #[test]
    fn a_file_that_parses_but_does_not_validate_is_also_recovered() {
        let t = TempStore::new("invalid");
        // Parses as TOML and as `Settings`, but 10 ms is below the documented
        // minimum. Recovering rather than running with it is the point: an
        // out-of-range interval would drive the capture loop.
        std::fs::write(
            t.store.path(),
            "schema = 1\n\n[capture]\ninterval_ms = 10\n",
        )
        .expect("write");

        let (settings, outcome) = t.store.load().expect("load");
        assert_eq!(settings, Settings::default());
        assert!(matches!(outcome, LoadOutcome::Recovered { .. }));
    }

    #[test]
    fn save_refuses_an_invalid_configuration() {
        let t = TempStore::new("refuse");
        let mut settings = Settings::default();
        settings.window.opacity = 5.0;

        let error = t.store.save(&settings).expect_err("5.0 opacity is invalid");
        assert!(matches!(error, super::StoreError::Invalid(_)));
        assert!(
            !t.store.path().exists(),
            "nothing may reach the disk when validation fails"
        );
    }

    /// §3.2: the write is atomic, so the scratch file is never left behind.
    #[test]
    fn the_temporary_file_does_not_survive_a_save() {
        let t = TempStore::new("atomic");
        t.store.save(&Settings::default()).expect("save");

        let tmp = t.store.path().with_extension("toml.tmp");
        assert!(!tmp.exists(), "the temp file must be renamed, not left");
        assert!(t.store.path().exists());
    }

    #[test]
    fn a_second_save_replaces_the_first() {
        let t = TempStore::new("replace");
        t.store.save(&Settings::default()).expect("first");

        let mut settings = Settings::default();
        settings.ui.font_scale = 1.5;
        t.store.save(&settings).expect("second");

        let (loaded, _) = t.store.load().expect("load");
        assert!((loaded.ui.font_scale - 1.5).abs() < f64::EPSILON);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let t = TempStore::new("perms");
        t.store.save(&Settings::default()).expect("save");

        let mode = std::fs::metadata(t.store.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "§3.2: 0600 on macOS");
    }

    /// FR-003. The serialized defaults are byte-stable.
    ///
    /// A golden rather than a shape check: §3.2 requires the file to diff
    /// cleanly, which means key order and formatting are part of the
    /// contract, not an accident of the serializer. If this fails after a
    /// `toml` upgrade, that is the test doing its job.
    #[test]
    fn fr_003_the_default_file_is_byte_stable() {
        let once = toml::to_string_pretty(&Settings::default()).expect("serialize");
        let twice = toml::to_string_pretty(&Settings::default()).expect("serialize");
        assert_eq!(once, twice, "two serializations of one value differed");

        // The golden. Every line is a default the user would see on first run.
        let expected = "\
schema = 1

[capture]
interval_ms = 2500

[capture.monitor]
kind = \"primary\"

[detection]
threshold = 0.85
stability_frames = 2
max_compare_chars = 6000

[assistant]
provider = \"anthropic\"
model = \"claude-opus-5\"
effort = \"medium\"
answer_style = \"short\"
max_output_tokens = 1024

[assistant.budget]
daily_usd = 2.0
monthly_usd = 20.0
max_input_tokens = 8000

[pacing]
words_per_minute = 300

[shortcuts]
arm_disarm = \"CmdOrCtrl+Shift+B\"
interact = \"CmdOrCtrl+Shift+Space\"
toggle_visibility = \"CmdOrCtrl+Shift+H\"
ask_now = \"CmdOrCtrl+Shift+Enter\"

[privacy]
redaction_enabled = true
redact_pii = false
allow_degraded_mode = false
diagnostics_level = \"minimal\"
region_only = false

[window]
anchor = \"top-right\"
width_px = 420
max_height_px = 600
opacity = 0.92

[ui]
theme = \"system\"
font_scale = 1.0
";
        assert_eq!(once, expected, "the default settings file changed shape");
    }
}
