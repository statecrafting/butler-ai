// Spec: specs/010-assistant-inference/spec.md

//! The API key, and the type that stops it leaking (spec 010 §3.4).
//!
//! Spec 015 constrains this file. The key lives in the **OS keychain** and
//! nowhere else: not in the settings file, not in a log, not in a diagnostics
//! bundle. `Secret` is what makes that structural rather than careful.
//!
//! # What `Secret` cannot do
//!
//! No `Debug`, no `Display`, no `Clone`. Every logging and tracing macro
//! formats through one of the first two, so a `Secret` cannot reach a log line
//! by any route a caller could take, deliberately or otherwise (FR-004). No
//! `Clone`, so there is one copy to zero. And the buffer is zeroed on drop, so
//! the key does not survive in freed memory.

use zeroize::Zeroize as _;

use crate::assistant::ProviderId;

/// The keychain service name (§3.4).
pub const SERVICE: &str = "dev.butler-ai.desktop";

/// An API key.
///
/// The single accessor is [`Secret::expose`], named so that a reader of a
/// call site sees what is happening.
///
/// # FR-004: it cannot be formatted
///
/// No `Debug`:
///
/// ```compile_fail
/// use butler_llm::Secret;
/// let secret = Secret::new("sk-ant-not-a-real-key".to_owned());
/// println!("{secret:?}");
/// ```
///
/// No `Display`:
///
/// ```compile_fail
/// use butler_llm::Secret;
/// let secret = Secret::new("sk-ant-not-a-real-key".to_owned());
/// println!("{secret}");
/// ```
///
/// No `Clone`, so there is one copy and one drop to zero it:
///
/// ```compile_fail
/// use butler_llm::Secret;
/// let secret = Secret::new("sk-ant-not-a-real-key".to_owned());
/// let _second = secret.clone();
/// ```
///
/// The positive control, so a passing `compile_fail` above cannot be passing
/// because the import path is wrong:
///
/// ```
/// use butler_llm::Secret;
/// let secret = Secret::new("sk-ant-not-a-real-key".to_owned());
/// assert_eq!(secret.expose(), "sk-ant-not-a-real-key");
/// ```
pub struct Secret(String);

impl Secret {
    /// Wrap a key.
    #[must_use]
    pub fn new(key: String) -> Self {
        Self(key)
    }

    /// The key, for the one place that needs it: the request header.
    ///
    /// Named `expose` rather than `as_str` on purpose. `as_str` reads as
    /// harmless and invites use; this one reads as a decision.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the key is empty, which the keychain can return.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Zero the key before the allocation is freed (§3.4).
impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// What can go wrong talking to the keychain.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform keychain refused or is unavailable.
    ///
    /// Carries the keychain's own words, never the key.
    #[error("keychain: {0}")]
    Keychain(String),
}

/// The OS keychain (§3.4).
///
/// Keychain on macOS, Credential Manager on Windows.
#[derive(Clone, Copy, Debug, Default)]
pub struct SecretStore;

impl SecretStore {
    /// A store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Read the key for `provider`, if one is stored.
    ///
    /// A missing entry is `Ok(None)`, not an error: no key yet is an ordinary
    /// state on first run, and §3.4 turns it into a UI prompt rather than a
    /// fault.
    ///
    /// # Errors
    ///
    /// [`SecretError::Keychain`] if the keychain itself is unavailable.
    pub fn get(self, provider: ProviderId) -> Result<Option<Secret>, SecretError> {
        let entry = Self::entry(provider)?;
        match entry.get_password() {
            Ok(key) => Ok(Some(Secret::new(key))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }

    /// Store the key for `provider`, replacing any previous one.
    ///
    /// # Errors
    ///
    /// [`SecretError::Keychain`] if the keychain refuses.
    pub fn set(self, provider: ProviderId, secret: &Secret) -> Result<(), SecretError> {
        Self::entry(provider)?
            .set_password(secret.expose())
            .map_err(|e| SecretError::Keychain(e.to_string()))
    }

    /// Remove the key for `provider`. Removing a key that is not there
    /// succeeds: the caller asked for it to be gone, and it is.
    ///
    /// # Errors
    ///
    /// [`SecretError::Keychain`] if the keychain refuses.
    pub fn delete(self, provider: ProviderId) -> Result<(), SecretError> {
        match Self::entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }

    fn entry(provider: ProviderId) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, provider.as_str())
            .map_err(|e| SecretError::Keychain(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{SERVICE, Secret};

    #[test]
    fn the_service_name_is_the_one_the_spec_names() {
        assert_eq!(SERVICE, "dev.butler-ai.desktop");
    }

    /// FR-006. One notice per key, however many requests fail.
    #[test]
    fn fr_006_a_rejected_key_is_reported_once_per_key() {
        let alarm = super::CredentialAlarm::new();

        assert!(alarm.should_report(), "the first rejection is news");
        for _ in 0..50 {
            assert!(
                !alarm.should_report(),
                "the same bad key must not prompt again"
            );
        }

        // A new key is new news, whether it works or not.
        alarm.key_changed();
        assert!(alarm.should_report());
        assert!(!alarm.should_report());
    }

    #[test]
    fn a_secret_exposes_only_through_the_named_accessor() {
        let secret = Secret::new("sk-ant-not-a-real-key".to_owned());
        assert_eq!(secret.expose(), "sk-ant-not-a-real-key");
        assert!(!secret.is_empty());
        assert!(Secret::new(String::new()).is_empty());
    }
}

/// FR-006: a rejected key is reported **once per key**, not per request.
///
/// A user whose key has expired should be asked for a new one once, not on
/// every capture cycle for as long as the app is armed. The identity of "the
/// key" is a **generation counter** bumped whenever the stored key changes,
/// rather than anything derived from the key itself: a hash of a secret is
/// still a function of a secret, and there is no reason to keep one.
#[derive(Debug, Default)]
pub struct CredentialAlarm {
    generation: std::sync::atomic::AtomicU64,
    reported: std::sync::atomic::AtomicU64,
}

impl CredentialAlarm {
    /// A fresh alarm, nothing reported.
    #[must_use]
    pub fn new() -> Self {
        Self {
            generation: std::sync::atomic::AtomicU64::new(1),
            reported: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Note that the stored key changed, so the next rejection is new news.
    pub fn key_changed(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    /// Whether this rejection should reach the user.
    ///
    /// `true` exactly once per generation, however many requests fail.
    pub fn should_report(&self) -> bool {
        let generation = self.generation.load(std::sync::atomic::Ordering::Acquire);
        self.reported
            .swap(generation, std::sync::atomic::Ordering::AcqRel)
            != generation
    }
}
