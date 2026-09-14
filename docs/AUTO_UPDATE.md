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
| Version comparison, OS/downgrade policy, manifest parsing, what a remote-manifest fetch outcome means (assess it, fall back and retry, fall back and wait), artifact integrity (size + SHA-256), the lifecycle state machine, persistence, and interruption recovery | **`bleradar_core::update` (this engine)** | Pure and deterministic, so it is exhaustively testable, falsifiable, and identical on every caller. |
| Fetching the release manifest and the artifact bytes over the network; handing the verified APK to the OS package installer | **The platform** (Android `HttpURLConnection` + `DownloadManager` + `PackageInstaller`; a host CLI + process spawn) | Inherently platform-specific and unobservable off-device; no code here can prove it flawless without a device. |

The engine never performs I/O. The caller drives it: it feeds downloaded bytes
in (so the engine can verify integrity as they stream), and it performs the
actual OS install only once the engine has reached [`UpdateStage::Verified`].

## Reaching it from the Android app (the JNI boundary)

So the decision core is not merely *implemented* but *reachable* from the app,
its pure **decision** functions (four since ABI 8, the remote-manifest
disposition since ABI 11) — and, since ABI 10, the manifest parser and the
streaming artifact verifier — are exported through the same
dependency-free JNI façade as the signal/tracking math
(`crates/bleradar-jni/src/lib.rs` ↔
`android/app/src/main/java/com/hse/bleradar/NativeRadar.java`, enforced exact by
`cargo xtask check-jni-contract`). The decisions are total functions over JNI
primitives; the manifest/artifact natives take and return Java strings through
the audited bridge in `crates/bleradar-jni/src/env.rs`. The live
`verify-jni-live` JVM harness exercises every one against the real
`libbleradar_jni.so`:

| `NativeRadar` native | Rust core | Returns |
| --- | --- | --- |
| `updateDecision(installedVersionCode, availableVersionCode, deviceSdkInt, minSdkInt)` | [`update_decision`] | an `UPDATE_*` ordinal (`0` UpToDate, `1` Available, `2` DowngradeRefused, `3` IncompatibleOs) |
| `shouldCheckForUpdate(nowSeconds, lastCheckSeconds, minIntervalSeconds)` | [`should_check_for_update`] | `boolean` |
| `downloadReadiness(network, batteryPercent, charging, freeStorageBytes, allowMetered, minBatteryPercent, storageHeadroomBytes, artifactSizeBytes)` | [`download_readiness`] | a `DOWNLOAD_*` ordinal (`0` Ready, `1` NoNetwork, `2` MeteredBlocked, `3` LowBattery, `4` InsufficientStorage); `network` is a `NETWORK_*` ordinal |
| `retryBackoffDelaySeconds(attempt, baseDelaySeconds, maxDelaySeconds)` | [`RetryPolicy::backoff_delay_secs`] | `long` seconds |
| `remoteManifestDisposition(httpStatus, failureKind)` | [`manifest_source_decision`] | a `MANIFEST_SOURCE_*` ordinal (`0` UseRemote, `1` FallbackRetry, `2` FallbackNoRetry); `failureKind` is a `MANIFEST_FETCH_*` ordinal (`0` answered, `1` transport failure, `2` too large, `3` rejected) and an unknown kind answers `2` |
| `releaseManifestCanonical(text)` / `releaseManifestError(text)` / `releaseManifestField(text, field)` | [`ReleaseManifest::parse`] / [`ReleaseManifest::serialize`] | the canonical manifest text, or `null` when rejected; the rejection reason, or `null` when accepted; one field as canonical text (a `MANIFEST_FIELD_*` selector), or `null` |
| `artifactVerifyFile(path, manifestText)` | [`ArtifactVerifier`], streamed over the file | an `ARTIFACT_*` ordinal (`0` Verified, `1` ManifestInvalid — checked before the file is touched, `2` Unreadable, `3` SizeMismatch, `4` HashMismatch) |

The app therefore makes every self-update *safety* decision — when to poll,
whether a release is a safe upgrade, which manifest to assess and whether a
failed fetch deserves a retry, whether conditions permit a download, how
long to back off, whether a manifest is acceptable at all, and whether a
downloaded artifact is exactly the intended one — in the exact verified Rust,
never a Java re-implementation. `ReleaseManifest.java` is a thin holder over
the canonical form Rust emits (Java parses nothing; the manifest is persisted in
that form), and `UpdateCheckService` installs only what `artifactVerifyFile`
reports as `ARTIFACT_VERIFIED`. Adding the decision surface bumped the JNI ABI
7 → 8, the manifest/artifact surface 9 → 10 and the remote-manifest
disposition 10 → 11 (`EXPECTED_ABI_VERSION` / `abiVersion()`), so a stale
`.so` is rejected at load.

### The manifest source

A release announces itself in one place: `release_manifest.txt` attached to
the repository's latest GitHub release, at
`UpdateCheckService.RELEASE_MANIFEST_URL`
(`https://github.com/EmmmmDeee/HSE-BLE-API-/releases/latest/download/release_manifest.txt`,
a stable URL that redirects to the asset). `ReleaseManifestSource.java` GETs
it on the service's worker thread (network I/O may not run on the main
thread) with 10 s connect and read timeouts and a 16 KiB cap, never throws,
and reports the HTTP status, a `MANIFEST_FETCH_*` failure kind and a
one-line detail. The text is then the core's to accept
(`releaseManifestCanonical`) and the outcome the core's to judge
(`remoteManifestDisposition`): the remote manifest is assessed on a `2xx`
with an accepted body; the bundled manifest (`assets/release_manifest.txt`,
the descriptor of the shipped build, so the check concludes "up to date")
is assessed otherwise — with the Rust-paced retry scheduled only when the
fault is transient (no answer, `408`/`425`/`429`/`5xx`) and the retry count
kept until a fetch succeeds, and without one when the source has nothing to
offer (`404`/`410`: no release published, which is what a repository without
releases answers), refuses the request, redirects across protocols, or
serves what is not a manifest. One log line,
`Remote manifest <url>: <what the fetch got> -> <what was decided>`, records
every attempt; `cargo xtask verify-android-emulator` requires it before the
decision, naming the URL the Java source declares, and reports it.

The [`UpdateSession`] state machine is the one part still *not* bridged:
exposing owned native state across JNI needs a handle-lifetime design that is
a larger, separate surface, and the app keeps its own restart-safe bookkeeping
(the canonical manifest, the `DownloadManager` id and the retry count in
`SharedPreferences`) around the bridged decisions. Artifact authenticity is
enforced regardless by the OS installer.

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
* [`manifest_source_decision`] — what a remote-manifest fetch outcome means
  ([`ManifestFetchFailure`] + the HTTP status → [`ManifestSourceDecision`]:
  assess the remote manifest, fall back and retry, or fall back and wait),
  so the retry budget is spent on transient faults and never on a source
  that has no release.
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

## Publishing a release

The check fetches `releases/latest/download/release_manifest.txt`, the asset of
the repository's newest release, so every release must carry both the APK and
the manifest describing it, and the manifest's `size_bytes`/`sha256` must be
those of the uploaded bytes exactly (`verify_artifact` refuses anything else).
`cargo xtask release-manifest [--url <artifact url>] [--out <path>]` writes
that manifest for the committed APK — `APP_VERSION_CODE`/`APP_VERSION_NAME`
(the one version authority, `xtask/src/main.rs`), the file's exact size and
SHA-256, `min_sdk` from `AndroidManifest.xml`, and by default the URL of the
asset on the `v<version name>` release
(`https://github.com/EmmmmDeee/HSE-BLE-API-/releases/download/v1.0.0/HSE-BLE-Radar-arm64-v1.0.0.apk`);
a non-`https` URL is refused. `cargo xtask check-app-version` (a `gates`
step) requires the bundled `assets/release_manifest.txt` to repeat the
version and the committed APK's name to carry it, `verify-android-live` reads
the built package's version back and requires the committed APK to reproduce
from the sources entry by entry (`build-apk` stores every entry at the ZIP
epoch, so a rebuild on one signing key is byte-identical), and
`verify-api-live` serves the generated manifest to the real core as its
accepted-manifest scenario — so what a release publishes is proven parseable
before it is published. The steps are listed under "Releasing" in the README.

## Verifying it

* `cargo test -p bleradar-core --test update` — 38 unit/invariant tests over every
  rule, transition, error path, the retry/backoff, rollback, and throttle logic,
  the remote-manifest disposition (every status 0–1000 × every failure kind
  against an independent restatement of the rule, plus the named boundaries),
  and the pre-download gating (every branch, boundaries, precedence, plus a
  20,000-case randomized cross-check against an independent reference).
* `cargo test -p bleradar-core --test update_campaign` — a deterministic 200,000-op
  differential campaign against an independent reference state machine (zero
  divergence) that exercises offer/download/verify/install **plus retry and
  rollback**, with a serialize→deserialize identity + `recover` idempotence check
  after every step, plus a 50,000-trial integrity oracle proving an install is
  unreachable without a genuine size+SHA-256 match.
* `cargo xtask verify-api-live` — the real `ReleaseManifestSource` against a
  scripted JDK `HttpServer` on the host JVM: a valid manifest, `404`, `503`, a
  `301` without `Location`, a body over the cap, a body the core rejects, a
  stalled answer, a refused connection and a malformed URL, each fetch's classification and its Rust
  disposition required as documented through the real `libbleradar_jni.so`.
* `cargo run -p bleradar-core --example update_flow` — the whole lifecycle over a
  real 64 KiB artifact and a real SHA-256, including a simulated crash mid-install
  (persist → restart → recover → finish), an idempotent re-install, pre-download
  gating (metered → Wi-Fi), retry with exponential backoff, a rollback, and the
  tamper / downgrade / incompatible-OS rails.

Falsified (each restored): allowing a downgrade, bypassing the SHA-256 check,
recovering `Installing` to `Installed` (unsafe), a linear (non-exponential)
backoff, an off-by-one retry give-up, a rollback that fails to consume the
previous version, an off-by-one battery or storage gate, a reordered
download-gating precedence, and a `404` that retries — each breaks the tests
or campaign.

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
[`RetryPolicy::backoff_delay_secs`]: https://docs.rs/bleradar-core
[`manifest_source_decision`]: https://docs.rs/bleradar-core
[`ManifestFetchFailure`]: https://docs.rs/bleradar-core
[`ManifestSourceDecision`]: https://docs.rs/bleradar-core
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
