# Automatic-update engine

`bleradar_core::update` is the authoritative, dependency-free core of the app's
automatic-update feature. It owns every decision that makes a self-update *safe*
— is a release newer, is it compatible, is the downloaded file exactly the
intended artifact, and can the flow survive a restart — so those decisions are
made once, in verified Rust, rather than re-implemented per platform.

## What is in Rust vs. at the platform boundary

An in-app auto-update has two halves:

| Half | Owner | Why |
| --- | --- | --- |
| Version comparison, OS/downgrade policy, manifest parsing, artifact integrity (size + SHA-256), the lifecycle state machine, persistence, and interruption recovery | **`bleradar_core::update` (this engine)** | Pure and deterministic, so it is exhaustively testable, falsifiable, and identical on every caller. |
| Fetching bytes over the network; handing the verified APK to the OS package installer | **The platform** (Android `DownloadManager`/OkHttp + `PackageInstaller`; a host CLI + process spawn) | Inherently platform-specific and unobservable off-device; no code here can prove it flawless without a device. |

The engine never performs I/O. The caller drives it: it feeds downloaded bytes
in (so the engine can verify integrity as they stream), and it performs the
actual OS install only once the engine has reached [`UpdateStage::Verified`].

## The decision surface

* [`Version`] — the monotonic Android `versionCode` (the update key the OS
  itself compares) plus the display `versionName`.
* [`ReleaseManifest`] — a strictly-parsed release descriptor. The manifest format
  is line-oriented `key = value` (one field per line; first `=` splits key from
  value; `#` and blank lines ignored). Required: `version_code`, `version_name`,
  `url`, `size_bytes`, `sha256`, `min_sdk`, `mandatory`; optional: `notes`. The
  URL **must be HTTPS**, the size non-zero, and the SHA-256 exactly 64 hex chars.
  [`ReleaseManifest::serialize`] round-trips.
* [`update_decision`] / [`check_update`] → [`UpdateDecision`] — the single source
  of truth: `Available` only when the release is *strictly newer* by
  `versionCode` **and** the device SDK meets the release minimum; `UpToDate` when
  equal; `DowngradeRefused` when older (no silent downgrade); `IncompatibleOs`
  when newer but the OS is too old.
* [`ArtifactVerifier`] / [`verify_artifact`] — streaming integrity. Bytes are fed
  as they arrive; an over-long stream is rejected immediately, and finalization
  requires the exact declared size **and** SHA-256. This is the only path by
  which an artifact becomes installable.
* [`UpdateSession`] — a restart-safe state machine over the whole lifecycle.
* [`RetryPolicy`] / [`UpdateSession::retry`] — bounded exponential-backoff
  recovery from a transient download/verify fault ([`RetryDecision`]).
* [`UpdateSession::rollback`] — revert to the previous known-good version after a
  bad update.
* [`should_check_for_update`] — a re-check throttle (minimum poll interval).
* [`download_readiness`] — pre-download gating on network / battery / free storage
  ([`DownloadPolicy`] + [`DownloadConditions`] → [`DownloadReadiness`]).

## Lifecycle and safety invariants

```
Idle ── offer(Available) ──▶ Available ── begin_download ──▶ Downloading
                                                               │ record_progress*
                                                               ▼ finish_download
   Installed ◀── finish_install ── Installing ◀── begin_install ── Verified ◀── verify ── Downloaded
```

Guaranteed (locked by `tests/update.rs` and the randomized
`tests/update_campaign.rs`):

* **No install without verification.** `begin_install` is only legal from
  `Verified`, and `Verified` is only reachable through a genuine size+SHA-256
  match. Tampered or truncated bytes drive the session to `Failed`, never to
  install.
* **No silent downgrade.** An older `versionCode` is `DowngradeRefused`; a
  session never adopts a lower version.
* **Illegal transitions are errors, not panics.** Every method rejects a wrong
  starting stage with `UpdateError::IllegalTransition`.
* **Monotonic, bounded progress.** `record_progress` saturates at the declared
  size; `finish_download` requires every byte.
* **Idempotent install.** `finish_install` on an already-`Installed` session at
  the target version is a no-op success (a duplicate OS callback cannot error).
* **Mandatory updates cannot be deferred** (`can_defer()` is false).

## Persistence and recovery

`UpdateSession::serialize` writes the session (stage, installed version, target
manifest, download progress) to a string the caller persists; `deserialize`
reads it back to an equal session. `recover` maps any stage a crash could have
interrupted to a safe, resumable one without losing progress:

* `Installing → Verified` — the OS installer may or may not have run; re-installing
  the same verified `versionCode` is idempotent, so drop back to the last
  provably-safe point and re-install.
* `Downloaded → Downloading` — bytes were never verified in this process; re-drive
  the verify path.

So a process killed at any point resumes correctly on next launch, and never
installs an unverified artifact.

## Resilience: retry, rollback, and re-check throttle

A robust auto-update must survive transient faults, recover from a bad release,
and not hammer the server:

* **Retry with bounded exponential backoff.** On a dropped connection or corrupt
  download the caller invokes [`UpdateSession::retry`] with a [`RetryPolicy`]; it
  increments the attempt counter, re-arms the session for a fresh download, and
  returns [`RetryDecision::RetryAfter`] with the backoff delay
  (`base · 2^(attempt-1)`, saturating and capped at `max_delay_secs`) — until the
  budget is spent, when it returns [`RetryDecision::GaveUp`] and the session is
  `Failed`. The attempt counter resets on a new `offer` or a successful install.
* **Rollback to the previous known-good version.** Every `finish_install` records
  the version it upgraded *from*; if the new version fails its post-install
  health check the caller calls [`UpdateSession::rollback`], which reverts
  `installed` to that previous version and returns to `Idle` (or
  [`UpdateError::NothingToRollBack`] if there is no prior version). This models
  the OS rollback path a caller drives when a fresh install misbehaves.
* **Re-check throttle.** [`should_check_for_update`] answers whether enough time
  has elapsed since the last check to poll again, robust against a clock that
  went backwards.
* **Pre-download gating.** [`download_readiness`] checks a sampled
  [`DownloadConditions`] snapshot against a [`DownloadPolicy`] before a download
  is started, returning the first unmet precondition in a fixed precedence — no
  network → metered blocked → low battery (unless charging) → insufficient
  storage (artifact + headroom) → ready. This keeps the engine from starting a
  download that would fail or cost the user (mobile data, a drained battery, or a
  volume that runs out of space mid-write).

Both `attempts` and the rollback target survive `serialize`/`deserialize`, so a
retry budget and the previous known-good version persist across a restart.

## Verifying it

* `cargo test -p bleradar-core --test update` — 37 unit/invariant tests over every
  rule, transition, error path, the retry/backoff, rollback, and throttle logic,
  and the pre-download gating (every branch, boundaries, precedence, plus a
  20,000-case randomized cross-check against an independent reference).
* `cargo test -p bleradar-core --test update_campaign` — a deterministic 200,000-op
  differential campaign against an independent reference state machine (zero
  divergence) that exercises offer/download/verify/install **plus retry and
  rollback**, with a serialize→deserialize identity + `recover` idempotence check
  after every step, plus a 50,000-trial integrity oracle proving an install is
  unreachable without a genuine size+SHA-256 match.
* `cargo run -p bleradar-core --example update_flow` — the whole lifecycle over a
  real 64 KiB artifact and a real SHA-256, including a simulated crash mid-install
  (persist → restart → recover → finish), an idempotent re-install, pre-download
  gating (metered → Wi-Fi), retry with exponential backoff, a rollback, and the
  tamper / downgrade / incompatible-OS rails.

Falsified (each restored): allowing a downgrade, bypassing the SHA-256 check,
recovering `Installing` to `Installed` (unsafe), a linear (non-exponential)
backoff, an off-by-one retry give-up, a rollback that fails to consume the
previous version, an off-by-one battery or storage gate, and a reordered
download-gating precedence — each breaks the tests or campaign.

[`Version`]: https://docs.rs/bleradar-core
[`ReleaseManifest`]: https://docs.rs/bleradar-core
[`ReleaseManifest::serialize`]: https://docs.rs/bleradar-core
[`update_decision`]: https://docs.rs/bleradar-core
[`check_update`]: https://docs.rs/bleradar-core
[`UpdateDecision`]: https://docs.rs/bleradar-core
[`ArtifactVerifier`]: https://docs.rs/bleradar-core
[`verify_artifact`]: https://docs.rs/bleradar-core
[`UpdateSession`]: https://docs.rs/bleradar-core
[`UpdateStage::Verified`]: https://docs.rs/bleradar-core
[`RetryPolicy`]: https://docs.rs/bleradar-core
[`RetryDecision`]: https://docs.rs/bleradar-core
[`RetryDecision::RetryAfter`]: https://docs.rs/bleradar-core
[`RetryDecision::GaveUp`]: https://docs.rs/bleradar-core
[`UpdateSession::retry`]: https://docs.rs/bleradar-core
[`UpdateSession::rollback`]: https://docs.rs/bleradar-core
[`UpdateError::NothingToRollBack`]: https://docs.rs/bleradar-core
[`should_check_for_update`]: https://docs.rs/bleradar-core
[`download_readiness`]: https://docs.rs/bleradar-core
[`DownloadConditions`]: https://docs.rs/bleradar-core
[`DownloadPolicy`]: https://docs.rs/bleradar-core
[`DownloadReadiness`]: https://docs.rs/bleradar-core
