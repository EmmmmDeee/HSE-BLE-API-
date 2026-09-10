//! Robust, deterministic automatic-update engine.
//!
//! An in-app auto-update has two halves: platform plumbing (fetch bytes over the
//! network, hand an APK to the OS package installer) and the *decisions* that
//! make an update safe — is this release newer, is it compatible, is the
//! downloaded file exactly the intended artifact, and can the flow survive a
//! restart mid-way. The plumbing is inherently platform-specific and untestable
//! off-device; the decisions are pure, so they live here where they can be
//! exhaustively tested, falsified, and reused verbatim by every caller.
//!
//! This module owns the whole decision surface:
//!
//! * [`Version`] — the monotonic Android `versionCode` (the authoritative update
//!   key) plus the human-facing `versionName`.
//! * [`ReleaseManifest`] — a strictly-parsed, HTTPS-only release descriptor with
//!   a pinned SHA-256 and size, round-trippable via [`ReleaseManifest::serialize`].
//! * [`update_decision`] / [`check_update`] — the single source of truth for
//!   whether to offer an update ([`UpdateDecision`]), keyed on `versionCode` and
//!   minimum OS, refusing silent downgrades.
//! * [`ArtifactVerifier`] / [`verify_artifact`] — streaming integrity
//!   verification (exact size **and** SHA-256) that rejects an over-long or
//!   tampered download before it can be installed.
//! * [`UpdateSession`] — a restart-safe state machine over the whole lifecycle
//!   (`Idle → Available → Downloading → Downloaded → Verified → Installing →
//!   Installed`), which [`serialize`](UpdateSession::serialize)s to a string,
//!   [`deserialize`](UpdateSession::deserialize)s back exactly, and
//!   [`recover`](UpdateSession::recover)s any interrupted stage to a safe,
//!   resumable point. Illegal transitions are rejected, never panic; installing
//!   an unverified artifact is unrepresentable; a completed install is
//!   idempotent.
//!
//! Every guard is a permanent invariant locked by `tests/update.rs` and the
//! randomized `tests/update_campaign.rs`; `examples/update_flow.rs` exercises the
//! whole lifecycle over real bytes.

use crate::entity::{Sha256, hex_encode};
use std::fmt;

/// An application version: the monotonic `versionCode` (the update key Android
/// itself compares) and the display `versionName`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Android `versionCode`: strictly increasing across releases.
    pub code: u64,
    /// Android `versionName`: human-facing, e.g. `"1.2.3"`.
    pub name: String,
}

impl Version {
    /// A version from its code and name.
    #[must_use]
    pub fn new(code: u64, name: impl Into<String>) -> Self {
        Self {
            code,
            name: name.into(),
        }
    }
}

/// The outcome of comparing an installed version to an available release.
///
/// Keyed on `versionCode` (like the Android package manager) and minimum OS, so
/// a build that is not strictly newer, or that the device is too old to run, is
/// never offered — and a downgrade is refused rather than silently applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    /// The available release is the same `versionCode` as installed.
    UpToDate,
    /// The release is strictly newer and OS-compatible: offer it.
    Available,
    /// The release is older than installed: refuse (no silent downgrade).
    DowngradeRefused,
    /// The release is newer but the device OS is below the release minimum.
    IncompatibleOs,
}

impl UpdateDecision {
    /// A stable ordinal for FFI/JNI (`0..=3`).
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::UpToDate => 0,
            Self::Available => 1,
            Self::DowngradeRefused => 2,
            Self::IncompatibleOs => 3,
        }
    }

    /// Decodes a stable ordinal produced by [`ordinal`](Self::ordinal).
    #[must_use]
    pub const fn from_ordinal(ordinal: i32) -> Option<Self> {
        match ordinal {
            0 => Some(Self::UpToDate),
            1 => Some(Self::Available),
            2 => Some(Self::DowngradeRefused),
            3 => Some(Self::IncompatibleOs),
            _ => None,
        }
    }

    /// Whether this decision means an install should be offered.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// The authoritative update decision from raw version codes and OS levels.
///
/// This is the numeric core shared by [`check_update`] and the JNI bridge, so
/// the app and the tests exercise exactly one implementation.
///
/// # Examples
/// ```
/// use bleradar_core::{update_decision, UpdateDecision};
/// // Strictly newer and OS-compatible.
/// assert_eq!(update_decision(41, 42, 34, 26), UpdateDecision::Available);
/// // Same versionCode.
/// assert_eq!(update_decision(42, 42, 34, 26), UpdateDecision::UpToDate);
/// // Older build is refused.
/// assert_eq!(update_decision(42, 41, 34, 26), UpdateDecision::DowngradeRefused);
/// // Newer but the device is too old.
/// assert_eq!(update_decision(41, 42, 24, 26), UpdateDecision::IncompatibleOs);
/// ```
#[must_use]
pub fn update_decision(
    installed_code: u64,
    available_code: u64,
    device_sdk: u32,
    min_sdk: u32,
) -> UpdateDecision {
    if available_code == installed_code {
        UpdateDecision::UpToDate
    } else if available_code < installed_code {
        UpdateDecision::DowngradeRefused
    } else if device_sdk < min_sdk {
        UpdateDecision::IncompatibleOs
    } else {
        UpdateDecision::Available
    }
}

/// The update decision for a manifest against the installed version and device.
///
/// # Examples
/// ```
/// use bleradar_core::update::{check_update, ReleaseManifest, Version, UpdateDecision};
/// let manifest = ReleaseManifest::parse(SAMPLE).unwrap();
/// let installed = Version::new(41, "1.2.2");
/// assert_eq!(check_update(&installed, 34, &manifest), UpdateDecision::Available);
/// # const SAMPLE: &str = "\
/// # version_code = 42\n\
/// # version_name = 1.2.3\n\
/// # url = https://example.com/app-1.2.3.apk\n\
/// # size_bytes = 8\n\
/// # sha256 = 2c624232cdd221771294dfbb310aca000a0df6ac8b66b696d90ef06fdefb64a3\n\
/// # min_sdk = 26\n\
/// # mandatory = false\n";
/// ```
#[must_use]
pub fn check_update(
    installed: &Version,
    device_sdk: u32,
    manifest: &ReleaseManifest,
) -> UpdateDecision {
    update_decision(
        installed.code,
        manifest.version.code,
        device_sdk,
        manifest.min_sdk,
    )
}

/// A parsed, validated release descriptor.
///
/// Produced by [`ReleaseManifest::parse`] from the line-oriented `key = value`
/// format (one field per line, first `=` splits key from value, `#` comment
/// lines and blank lines ignored). Required fields: `version_code`,
/// `version_name`, `url`, `size_bytes`, `sha256`, `min_sdk`, `mandatory`; `notes`
/// is optional. The URL must be HTTPS, the size non-zero, and the SHA-256 exactly
/// 64 hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseManifest {
    /// The release version.
    pub version: Version,
    /// HTTPS download URL for the artifact.
    pub url: String,
    /// Exact artifact size in bytes.
    pub size_bytes: u64,
    /// Expected SHA-256 of the artifact.
    pub sha256: [u8; 32],
    /// Minimum device SDK level required to install.
    pub min_sdk: u32,
    /// Whether the update must be applied (cannot be deferred).
    pub mandatory: bool,
    /// Optional single-line release notes.
    pub notes: String,
}

impl ReleaseManifest {
    /// Parses and validates a manifest from the line-oriented format.
    ///
    /// # Errors
    /// Returns an [`UpdateError`] for a missing, duplicate, unknown, or malformed
    /// field, a non-HTTPS URL, a zero size, or a SHA-256 that is not 64 hex chars.
    pub fn parse(text: &str) -> Result<Self, UpdateError> {
        let mut version_code: Option<u64> = None;
        let mut version_name: Option<String> = None;
        let mut url: Option<String> = None;
        let mut size_bytes: Option<u64> = None;
        let mut sha256: Option<[u8; 32]> = None;
        let mut min_sdk: Option<u32> = None;
        let mut mandatory: Option<bool> = None;
        let mut notes: Option<String> = None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| UpdateError::MalformedLine(raw.trim().to_string()))?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "version_code" => set_once(
                    &mut version_code,
                    "version_code",
                    parse_u64(value, "version_code")?,
                )?,
                "version_name" => set_once(&mut version_name, "version_name", value.to_string())?,
                "url" => set_once(&mut url, "url", value.to_string())?,
                "size_bytes" => set_once(
                    &mut size_bytes,
                    "size_bytes",
                    parse_u64(value, "size_bytes")?,
                )?,
                "sha256" => set_once(&mut sha256, "sha256", parse_hex32(value)?)?,
                "min_sdk" => set_once(&mut min_sdk, "min_sdk", parse_u32(value, "min_sdk")?)?,
                "mandatory" => {
                    set_once(&mut mandatory, "mandatory", parse_bool(value, "mandatory")?)?
                }
                "notes" => set_once(&mut notes, "notes", value.to_string())?,
                other => return Err(UpdateError::UnknownField(other.to_string())),
            }
        }

        let version_name = version_name.ok_or(UpdateError::MissingField("version_name"))?;
        if version_name.is_empty() {
            return Err(UpdateError::MalformedField {
                key: "version_name",
                reason: "must not be empty",
            });
        }
        let url = url.ok_or(UpdateError::MissingField("url"))?;
        if !url.starts_with("https://") {
            return Err(UpdateError::InsecureUrl);
        }
        let size_bytes = size_bytes.ok_or(UpdateError::MissingField("size_bytes"))?;
        if size_bytes == 0 {
            return Err(UpdateError::EmptyArtifact);
        }

        Ok(Self {
            version: Version::new(
                version_code.ok_or(UpdateError::MissingField("version_code"))?,
                version_name,
            ),
            url,
            size_bytes,
            sha256: sha256.ok_or(UpdateError::MissingField("sha256"))?,
            min_sdk: min_sdk.ok_or(UpdateError::MissingField("min_sdk"))?,
            mandatory: mandatory.ok_or(UpdateError::MissingField("mandatory"))?,
            notes: notes.unwrap_or_default(),
        })
    }

    /// Serializes the manifest back to the canonical line-oriented format.
    ///
    /// [`parse`](Self::parse) of the result reproduces an equal manifest.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("version_code = {}\n", self.version.code));
        out.push_str(&format!("version_name = {}\n", self.version.name));
        out.push_str(&format!("url = {}\n", self.url));
        out.push_str(&format!("size_bytes = {}\n", self.size_bytes));
        out.push_str(&format!("sha256 = {}\n", hex_encode(&self.sha256)));
        out.push_str(&format!("min_sdk = {}\n", self.min_sdk));
        out.push_str(&format!("mandatory = {}\n", self.mandatory));
        out.push_str(&format!("notes = {}\n", self.notes));
        out
    }
}

/// A recoverable fault in the update flow. Every variant is actionable and names
/// what was wrong; no variant leaks the raw artifact bytes or hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// A required manifest field was absent.
    MissingField(&'static str),
    /// A field appeared more than once.
    DuplicateField(&'static str),
    /// An unrecognized manifest key.
    UnknownField(String),
    /// A line without a `key = value` separator.
    MalformedLine(String),
    /// A field's value could not be interpreted.
    MalformedField {
        /// The field name.
        key: &'static str,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The download URL was not HTTPS.
    InsecureUrl,
    /// The artifact size was zero.
    EmptyArtifact,
    /// The download exceeded the manifest's declared size.
    Overrun {
        /// Declared size.
        expected: u64,
        /// Bytes received before the overrun was detected.
        received: u64,
    },
    /// The completed download's size did not match the manifest.
    SizeMismatch {
        /// Declared size.
        expected: u64,
        /// Actual size.
        actual: u64,
    },
    /// The artifact's SHA-256 did not match the manifest.
    HashMismatch,
    /// An operation was attempted from a stage that does not allow it.
    IllegalTransition {
        /// The stage the session was in.
        from: &'static str,
        /// The operation that was rejected.
        op: &'static str,
    },
    /// A serialized session string could not be parsed back.
    MalformedSession(&'static str),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(k) => write!(f, "manifest is missing required field `{k}`"),
            Self::DuplicateField(k) => write!(f, "manifest field `{k}` appears more than once"),
            Self::UnknownField(k) => write!(f, "manifest has unknown field `{k}`"),
            Self::MalformedLine(l) => write!(f, "manifest line is not `key = value`: {l:?}"),
            Self::MalformedField { key, reason } => {
                write!(f, "manifest field `{key}` is invalid: {reason}")
            }
            Self::InsecureUrl => write!(f, "manifest url must be https://"),
            Self::EmptyArtifact => write!(f, "manifest size_bytes must be non-zero"),
            Self::Overrun { expected, received } => {
                write!(
                    f,
                    "download exceeded declared size: {received} > {expected} bytes"
                )
            }
            Self::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "artifact size mismatch: expected {expected}, got {actual} bytes"
                )
            }
            Self::HashMismatch => write!(f, "artifact SHA-256 does not match the manifest"),
            Self::IllegalTransition { from, op } => {
                write!(f, "cannot `{op}` from stage `{from}`")
            }
            Self::MalformedSession(r) => write!(f, "serialized update session is invalid: {r}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Streaming integrity verifier for a downloading artifact.
///
/// Bytes are [`feed`](Self::feed)'d as they arrive; the verifier rejects an
/// over-long stream immediately (so a mismatched or malicious artifact cannot
/// grow without bound) and [`finish`](Self::finish) confirms the exact declared
/// size and SHA-256. This is the only path by which an artifact becomes
/// installable.
#[derive(Clone)]
pub struct ArtifactVerifier {
    expected_size: u64,
    expected_hash: [u8; 32],
    hasher: Sha256,
    received: u64,
}

impl ArtifactVerifier {
    /// A verifier for the given manifest's size and hash.
    #[must_use]
    pub fn new(manifest: &ReleaseManifest) -> Self {
        Self {
            expected_size: manifest.size_bytes,
            expected_hash: manifest.sha256,
            hasher: Sha256::new(),
            received: 0,
        }
    }

    /// Feeds one downloaded chunk.
    ///
    /// # Errors
    /// [`UpdateError::Overrun`] if the accumulated length would exceed the
    /// declared size (the chunk is not hashed in that case).
    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), UpdateError> {
        let next = self.received.saturating_add(chunk.len() as u64);
        if next > self.expected_size {
            return Err(UpdateError::Overrun {
                expected: self.expected_size,
                received: next,
            });
        }
        self.hasher.update(chunk);
        self.received = next;
        Ok(())
    }

    /// Bytes fed so far.
    #[must_use]
    pub const fn received(&self) -> u64 {
        self.received
    }

    /// Finalizes verification.
    ///
    /// # Errors
    /// [`UpdateError::SizeMismatch`] if fewer bytes than declared were fed, or
    /// [`UpdateError::HashMismatch`] if the SHA-256 differs from the manifest.
    pub fn finish(self) -> Result<(), UpdateError> {
        if self.received != self.expected_size {
            return Err(UpdateError::SizeMismatch {
                expected: self.expected_size,
                actual: self.received,
            });
        }
        if self.hasher.finalize() != self.expected_hash {
            return Err(UpdateError::HashMismatch);
        }
        Ok(())
    }
}

/// One-shot integrity check of a fully-buffered artifact against a manifest.
///
/// # Errors
/// [`UpdateError::SizeMismatch`] or [`UpdateError::HashMismatch`] on any
/// discrepancy.
///
/// # Examples
/// ```
/// use bleradar_core::update::{verify_artifact, ReleaseManifest};
/// # const M: &str = "\
/// # version_code = 2\nversion_name = 0.2\nurl = https://e/x.apk\n\
/// # size_bytes = 5\nsha256 = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n\
/// # min_sdk = 21\nmandatory = false\n";
/// let manifest = ReleaseManifest::parse(M).unwrap();
/// assert!(verify_artifact(b"hello", &manifest).is_ok());
/// assert!(verify_artifact(b"hellp", &manifest).is_err());
/// ```
pub fn verify_artifact(bytes: &[u8], manifest: &ReleaseManifest) -> Result<(), UpdateError> {
    let mut verifier = ArtifactVerifier::new(manifest);
    // A single feed can itself overrun; surface that as a size mismatch for the
    // one-shot API rather than the streaming Overrun.
    if bytes.len() as u64 > manifest.size_bytes {
        return Err(UpdateError::SizeMismatch {
            expected: manifest.size_bytes,
            actual: bytes.len() as u64,
        });
    }
    verifier.feed(bytes).and_then(|()| verifier.finish())
}

/// The lifecycle stage of an [`UpdateSession`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStage {
    /// No update in flight.
    Idle,
    /// A newer release has been offered.
    Available,
    /// The artifact is downloading.
    Downloading,
    /// The full artifact has been received (not yet verified).
    Downloaded,
    /// The artifact's size and SHA-256 have been verified.
    Verified,
    /// The verified artifact is being installed.
    Installing,
    /// The update has been installed.
    Installed,
    /// The flow failed and is awaiting reset/retry.
    Failed,
}

impl UpdateStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Available => "Available",
            Self::Downloading => "Downloading",
            Self::Downloaded => "Downloaded",
            Self::Verified => "Verified",
            Self::Installing => "Installing",
            Self::Installed => "Installed",
            Self::Failed => "Failed",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Idle" => Self::Idle,
            "Available" => Self::Available,
            "Downloading" => Self::Downloading,
            "Downloaded" => Self::Downloaded,
            "Verified" => Self::Verified,
            "Installing" => Self::Installing,
            "Installed" => Self::Installed,
            "Failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// A restart-safe state machine driving one update through its whole lifecycle.
///
/// Transitions are the only way to change stage and each rejects an illegal
/// starting stage with [`UpdateError::IllegalTransition`] rather than panicking.
/// The session [`serialize`](Self::serialize)s to a string that
/// [`deserialize`](Self::deserialize)s back to an equal session, and
/// [`recover`](Self::recover) maps any stage that a crash could have interrupted
/// to a safe, resumable one — so persisting the session across a process restart
/// never loses progress or installs an unverified artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSession {
    stage: UpdateStage,
    installed: Version,
    target: Option<ReleaseManifest>,
    received_bytes: u64,
    fail_reason: Option<String>,
}

impl UpdateSession {
    /// A fresh, idle session for the currently-installed version.
    #[must_use]
    pub fn new(installed: Version) -> Self {
        Self {
            stage: UpdateStage::Idle,
            installed,
            target: None,
            received_bytes: 0,
            fail_reason: None,
        }
    }

    /// The current stage.
    #[must_use]
    pub const fn stage(&self) -> UpdateStage {
        self.stage
    }

    /// The installed version.
    #[must_use]
    pub const fn installed(&self) -> &Version {
        &self.installed
    }

    /// The release being applied, if any.
    #[must_use]
    pub const fn target(&self) -> Option<&ReleaseManifest> {
        self.target.as_ref()
    }

    /// Bytes downloaded so far for the in-flight artifact.
    #[must_use]
    pub const fn received_bytes(&self) -> u64 {
        self.received_bytes
    }

    /// Why the flow failed, if it is in [`UpdateStage::Failed`].
    #[must_use]
    pub fn fail_reason(&self) -> Option<&str> {
        self.fail_reason.as_deref()
    }

    /// Whether the in-flight update may be deferred (false for a mandatory one).
    #[must_use]
    pub fn can_defer(&self) -> bool {
        !self.target.as_ref().is_some_and(|m| m.mandatory)
    }

    /// Offers a release, given the device SDK. Returns the decision; only an
    /// [`UpdateDecision::Available`] moves the session to [`UpdateStage::Available`].
    ///
    /// Allowed from `Idle`, `Available`, or `Failed` (re-offer after a fault).
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] from any other stage.
    pub fn offer(
        &mut self,
        manifest: ReleaseManifest,
        device_sdk: u32,
    ) -> Result<UpdateDecision, UpdateError> {
        if !matches!(
            self.stage,
            UpdateStage::Idle | UpdateStage::Available | UpdateStage::Failed
        ) {
            return Err(UpdateError::IllegalTransition {
                from: self.stage.as_str(),
                op: "offer",
            });
        }
        let decision = check_update(&self.installed, device_sdk, &manifest);
        if decision.is_available() {
            self.stage = UpdateStage::Available;
            self.target = Some(manifest);
            self.received_bytes = 0;
            self.fail_reason = None;
        }
        Ok(decision)
    }

    /// Begins (or resumes) downloading the offered artifact.
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Available` or `Downloading`.
    pub fn begin_download(&mut self) -> Result<(), UpdateError> {
        match self.stage {
            UpdateStage::Available | UpdateStage::Downloading => {
                self.stage = UpdateStage::Downloading;
                Ok(())
            }
            other => Err(UpdateError::IllegalTransition {
                from: other.as_str(),
                op: "begin_download",
            }),
        }
    }

    /// Records `n` more downloaded bytes, saturating at the declared size so
    /// progress is monotonic and bounded.
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Downloading`.
    pub fn record_progress(&mut self, n: u64) -> Result<u64, UpdateError> {
        if self.stage != UpdateStage::Downloading {
            return Err(UpdateError::IllegalTransition {
                from: self.stage.as_str(),
                op: "record_progress",
            });
        }
        let size = self.target.as_ref().map_or(0, |m| m.size_bytes);
        self.received_bytes = self.received_bytes.saturating_add(n).min(size);
        Ok(self.received_bytes)
    }

    /// Marks the download complete once every byte has arrived.
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Downloading`;
    /// [`UpdateError::SizeMismatch`] if fewer than the declared bytes were recorded.
    pub fn finish_download(&mut self) -> Result<(), UpdateError> {
        if self.stage != UpdateStage::Downloading {
            return Err(UpdateError::IllegalTransition {
                from: self.stage.as_str(),
                op: "finish_download",
            });
        }
        let size = self.target.as_ref().map_or(0, |m| m.size_bytes);
        if self.received_bytes != size {
            return Err(UpdateError::SizeMismatch {
                expected: size,
                actual: self.received_bytes,
            });
        }
        self.stage = UpdateStage::Downloaded;
        Ok(())
    }

    /// Verifies the downloaded artifact bytes against the manifest.
    ///
    /// On success the session advances to [`UpdateStage::Verified`]; on an
    /// integrity failure it moves to [`UpdateStage::Failed`] (the artifact must
    /// be re-downloaded) and the error is returned.
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Downloaded`; otherwise the
    /// verification error, which also transitions the session to `Failed`.
    pub fn verify(&mut self, bytes: &[u8]) -> Result<(), UpdateError> {
        if self.stage != UpdateStage::Downloaded {
            return Err(UpdateError::IllegalTransition {
                from: self.stage.as_str(),
                op: "verify",
            });
        }
        let manifest = self
            .target
            .as_ref()
            .ok_or(UpdateError::MalformedSession("verify without a target"))?;
        match verify_artifact(bytes, manifest) {
            Ok(()) => {
                self.stage = UpdateStage::Verified;
                Ok(())
            }
            Err(e) => {
                self.stage = UpdateStage::Failed;
                self.fail_reason = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Begins installing the verified artifact.
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Verified`.
    pub fn begin_install(&mut self) -> Result<(), UpdateError> {
        if self.stage != UpdateStage::Verified {
            return Err(UpdateError::IllegalTransition {
                from: self.stage.as_str(),
                op: "begin_install",
            });
        }
        self.stage = UpdateStage::Installing;
        Ok(())
    }

    /// Completes the install, adopting the target version as installed.
    ///
    /// Idempotent: calling it again once already `Installed` at the target
    /// version is a no-op success (a restart that re-runs the OS installer for an
    /// already-applied `versionCode` must not error).
    ///
    /// # Errors
    /// [`UpdateError::IllegalTransition`] unless in `Installing` (or already
    /// `Installed` at the target version).
    pub fn finish_install(&mut self) -> Result<(), UpdateError> {
        match self.stage {
            UpdateStage::Installing => {
                if let Some(manifest) = self.target.take() {
                    self.installed = manifest.version;
                }
                self.stage = UpdateStage::Installed;
                self.received_bytes = 0;
                self.fail_reason = None;
                Ok(())
            }
            UpdateStage::Installed => Ok(()),
            other => Err(UpdateError::IllegalTransition {
                from: other.as_str(),
                op: "finish_install",
            }),
        }
    }

    /// Records a failure with a human-readable reason, from any stage.
    pub fn fail(&mut self, reason: impl Into<String>) {
        self.stage = UpdateStage::Failed;
        self.fail_reason = Some(reason.into());
    }

    /// Discards any in-flight update and returns to [`UpdateStage::Idle`],
    /// keeping the installed version. Used to retry or roll back to a clean state.
    pub fn reset(&mut self) {
        self.stage = UpdateStage::Idle;
        self.target = None;
        self.received_bytes = 0;
        self.fail_reason = None;
    }

    /// Maps a stage that a crash could have interrupted to a safe, resumable one,
    /// without losing progress:
    ///
    /// * `Installing` → `Verified`: the OS installer may or may not have run;
    ///   re-installing the same verified `versionCode` is idempotent, so drop
    ///   back to the last provably-safe point and re-install.
    /// * `Downloaded` → `Downloading`: bytes may be on disk but were never
    ///   verified in this process; re-verify by continuing the download/verify path.
    /// * every other stage is already safe and is returned unchanged.
    ///
    /// Idempotent: `recover(recover(x)) == recover(x)`.
    #[must_use]
    pub fn recover(mut self) -> Self {
        match self.stage {
            UpdateStage::Installing => {
                self.stage = UpdateStage::Verified;
            }
            UpdateStage::Downloaded => {
                self.stage = UpdateStage::Downloading;
            }
            _ => {}
        }
        self
    }

    /// Serializes the session to the line-oriented format for persistence.
    ///
    /// [`deserialize`](Self::deserialize) of the result reproduces an equal session.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("stage = {}\n", self.stage.as_str()));
        out.push_str(&format!("installed_code = {}\n", self.installed.code));
        out.push_str(&format!("installed_name = {}\n", self.installed.name));
        out.push_str(&format!("received_bytes = {}\n", self.received_bytes));
        if let Some(reason) = &self.fail_reason {
            out.push_str(&format!("fail_reason = {reason}\n"));
        }
        if let Some(target) = &self.target {
            for line in target.serialize().lines() {
                out.push_str("target_");
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }

    /// Parses a session previously produced by [`serialize`](Self::serialize).
    ///
    /// # Errors
    /// [`UpdateError`] if the text is malformed, or if a non-idle stage is
    /// missing its target manifest.
    pub fn deserialize(text: &str) -> Result<Self, UpdateError> {
        let mut stage: Option<UpdateStage> = None;
        let mut installed_code: Option<u64> = None;
        let mut installed_name: Option<String> = None;
        let mut received_bytes: Option<u64> = None;
        let mut fail_reason: Option<String> = None;
        let mut target_lines = String::new();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or(UpdateError::MalformedSession("line is not key = value"))?;
            let key = key.trim();
            let value = value.trim();
            if let Some(rest) = key.strip_prefix("target_") {
                target_lines.push_str(rest);
                target_lines.push_str(" = ");
                target_lines.push_str(value);
                target_lines.push('\n');
                continue;
            }
            match key {
                "stage" => {
                    stage = Some(
                        UpdateStage::from_str(value)
                            .ok_or(UpdateError::MalformedSession("unknown stage"))?,
                    );
                }
                "installed_code" => {
                    installed_code = Some(
                        value
                            .parse()
                            .map_err(|_| UpdateError::MalformedSession("installed_code"))?,
                    );
                }
                "installed_name" => installed_name = Some(value.to_string()),
                "received_bytes" => {
                    received_bytes = Some(
                        value
                            .parse()
                            .map_err(|_| UpdateError::MalformedSession("received_bytes"))?,
                    );
                }
                "fail_reason" => fail_reason = Some(value.to_string()),
                _ => return Err(UpdateError::MalformedSession("unknown session key")),
            }
        }

        let stage = stage.ok_or(UpdateError::MalformedSession("missing stage"))?;
        let installed = Version::new(
            installed_code.ok_or(UpdateError::MalformedSession("missing installed_code"))?,
            installed_name.ok_or(UpdateError::MalformedSession("missing installed_name"))?,
        );
        let target = if target_lines.is_empty() {
            None
        } else {
            Some(ReleaseManifest::parse(&target_lines).map_err(|_| {
                UpdateError::MalformedSession("embedded target manifest is invalid")
            })?)
        };
        // Available/Downloading/Downloaded/Verified/Installing describe an
        // in-flight update and therefore require a target; Idle, Installed, and
        // Failed can legitimately have none (Failed may be reached from Idle).
        if matches!(
            stage,
            UpdateStage::Available
                | UpdateStage::Downloading
                | UpdateStage::Downloaded
                | UpdateStage::Verified
                | UpdateStage::Installing
        ) && target.is_none()
        {
            return Err(UpdateError::MalformedSession(
                "in-flight stage requires a target manifest",
            ));
        }
        Ok(Self {
            stage,
            installed,
            target,
            received_bytes: received_bytes.unwrap_or(0),
            fail_reason,
        })
    }
}

// ---- small strict value parsers (dependency-free) ----

fn set_once<T>(slot: &mut Option<T>, key: &'static str, value: T) -> Result<(), UpdateError> {
    if slot.is_some() {
        return Err(UpdateError::DuplicateField(key));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_u64(value: &str, key: &'static str) -> Result<u64, UpdateError> {
    value.parse().map_err(|_| UpdateError::MalformedField {
        key,
        reason: "expected a non-negative integer",
    })
}

fn parse_u32(value: &str, key: &'static str) -> Result<u32, UpdateError> {
    value.parse().map_err(|_| UpdateError::MalformedField {
        key,
        reason: "expected a non-negative integer",
    })
}

fn parse_bool(value: &str, key: &'static str) -> Result<bool, UpdateError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(UpdateError::MalformedField {
            key,
            reason: "expected `true` or `false`",
        }),
    }
}

/// Parses exactly 64 lowercase/uppercase hex characters into 32 bytes.
fn parse_hex32(value: &str) -> Result<[u8; 32], UpdateError> {
    if value.len() != 64 {
        return Err(UpdateError::MalformedField {
            key: "sha256",
            reason: "expected 64 hex characters",
        });
    }
    let mut out = [0u8; 32];
    let bytes = value.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> Result<u8, UpdateError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(UpdateError::MalformedField {
            key: "sha256",
            reason: "expected hex characters only",
        }),
    }
}
