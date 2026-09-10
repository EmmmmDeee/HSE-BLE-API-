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

## Verifying it

* `cargo test -p bleradar-core --test update` — 23 unit/invariant tests over every
  rule, transition, and error path.
* `cargo test -p bleradar-core --test update_campaign` — a deterministic 200,000-op
  differential campaign against an independent reference state machine (zero
  divergence), plus a 50,000-trial integrity oracle proving an install is
  unreachable without a genuine size+SHA-256 match, and a serialize→deserialize
  identity + `recover` idempotence check after every step.
* `cargo run -p bleradar-core --example update_flow` — the whole lifecycle over a
  real 64 KiB artifact and a real SHA-256, including a simulated crash mid-install
  (persist → restart → recover → finish), an idempotent re-install, and the
  tamper / downgrade / incompatible-OS rails.

Falsified (each restored): allowing a downgrade, bypassing the SHA-256 check, and
recovering `Installing` to `Installed` (unsafe) each break the tests or campaign.

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
