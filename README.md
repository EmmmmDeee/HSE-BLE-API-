# HSE-BLE-API-

Huntsman's Radar (API)

The **HSE BLE Radar**: a standalone Android ARM64 wireless-intelligence app (an installable `HSE-BLE-Radar-arm64-<version>.apk`, attached to a GitHub release; release policy is pre-releases only, see [Releasing](#releasing)) and the safe-Rust engine library behind it, `bleradar-core`. The app scans BLE on the device, decodes what it hears, names the manufacturer and services, keeps an identity across address rotation and remembers devices across sessions — all decided in Rust, with Android providing only the platform glue.

It is one of two repositories with distinct purposes. The other is the [Huntsman Search Engine](https://github.com/EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-) (HSE), a recon CLI (`huntsman-recon`) for Termux. The two repositories are currently independent: this repository depends on nothing from HSE, and HSE does not depend on `bleradar-core` (checked against HSE `main`; the SHA and date are in the boundary document). Some rules exist in both (for example `tower_id`) rather than one depending on the other. What each owns, the duplicated rules, and how a seam could be re-established: [`docs/REPOSITORY_BOUNDARY.md`](docs/REPOSITORY_BOUNDARY.md).

Origin: an auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

## Core Principle: Real Code and Verified Results Only

This repository accepts **only real, executed code and verified results as work product**. Planning documents, design proposals, and intent are never deliverables. A feature is complete only when:
- Code is written and committed to the repository
- Tests pass and CI is green on the committed code
- The change is merged into the `main` branch
- Execution is verified on that exact merged commit

Code that works in principle but was never executed is incomplete. Features that pass tests locally but fail remotely are not done. Improvements that disappear after rebuild, reinstall, or restart were never permanent. Every claim of completion must have real, observable evidence: green test runs, passing CI gates, and merged commits on `origin/main`.

## Repository layout

- `crates/bleradar-core` — safe Rust geometry, identity, RSSI, proximity and device-tracking domain.
- `crates/bleradar-core::evidence` — canonical observations, provenance records, representations,
  transformations, claims, and an authoritative evidence store.
- `crates/bleradar-core::advancement` — evaluation-gated metamorphic software
  advancement with formula-based ranking and explicit integration state.
- `crates/bleradar-core::osint` — execution-feedback adaptive OSINT search with
  representation-aware pivots, provenance-preserving findings, and canonical
  retrieval actions.
- `crates/bleradar-core::infrastructure` — temporal metamorphic infrastructure
  correlation across domains, DNS, addresses, certificates, hosting, HTTP,
  public assets, application structure, and archived states.
- `crates/bleradar-core::pipeline` — the investigation control loop (discover →
  … → stop on diminishing information gain): a temporal+geo graph with
  bridge/cluster detection, a round-over-round diminishing-information-gain
  stop criterion, and explicit stage tracking across the whole loop.
- `crates/bleradar-core::sweep` — the Huntsman Search Engine's multi-sensor
  radar sweep domain, consolidated here as the single authority for the sensor
  rules HSE's `signal_radar` had reimplemented: real-vs-placeholder device
  addresses, locally-administered (randomized) vs trackable MAC classification,
  the Wi-Fi RSSI reliability tiers (a positive dBm reading is corrupt input and
  scores the worst tier, never the best), 802.11 channel and coarse RSSI
  proximity derivation, per-radio cell identity (`cid`/`ci`/`nci` with the
  `Integer.MAX_VALUE` and zero unavailable filters), the canonical
  `mcc-mnc-lac-cid` tower id, and the sighting key. Dependency-free and
  regression-locked so the radar's sensor math has one home rather than a
  reimplementation that can silently drift.
- `crates/bleradar-core::{entity,coords,tags}` — the Huntsman Search Engine
  (HSE) dependency-free entity model imported for the radar domain: SHA-256
  deterministic UIDs, per-kind normalisation, a cross-source corroboration
  confidence model with derived classification tiers, GREATEST-semantics
  merge, a universal coordinate parser (decimal/DMS/DDM/`geo:` URI/Plus
  Code/Maidenhead), and the canonical tag vocabulary. See
  `docs/HSE_IMPORT.md`.
- `crates/bleradar-core::update` — the automatic-update engine: `versionCode`-keyed
  update decisions (no silent downgrade, OS-gated), a strict HTTPS-only release
  manifest, streaming SHA-256 + size integrity verification, and a restart-safe
  lifecycle state machine that persists, recovers from a crash mid-update, and
  makes installing an unverified or tampered artifact unrepresentable. It also
  carries the resilience a robust updater needs: bounded exponential-backoff
  retry of transient download faults, rollback to the previous known-good version
  after a bad release, a re-check throttle, and pre-download gating on network
  (metered/Wi-Fi), battery, and free storage so a download that would fail or
  cost the user is never started. Verified by a 200,000-op
  differential campaign (offer/download/verify/install/retry/rollback) and a
  50,000-trial integrity oracle; the network fetch and OS installer are the
  documented platform boundary. Its pure decision core (update decision,
  re-check throttle, pre-download gating, retry backoff) is reachable from the
  Android app through the JNI façade (`NativeRadar.updateDecision` /
  `shouldCheckForUpdate` / `downloadReadiness` / `retryBackoffDelaySeconds`),
  so the app makes those safety decisions in verified Rust. See
  `docs/AUTO_UPDATE.md`.
- `crates/bleradar-core::capability` — the ATT&CK capability ledger (v0): defensive coverage claims whose status is derived from an evidence chain only (no manual promotion; `Verified` needs a regression lock). See `docs/CAPABILITY_LEDGER.md`.
- `crates/bleradar-compat` — complete native ABI runtime/reachability census plus a separate source-replacement parity registry.
- `xtask/` — dependency-free Rust-native developer tooling (`cargo xtask`): binary inventory, parity-report generation, ABI/DEX census, the JNI export-contract gate derived from `NativeRadar.java`, live Java→JNI→Rust verification, APK packaging, executed-oracle differential verification under `qemu-aarch64` (`oracle-differential`, see `docs/ORACLE_DIFFERENTIAL.md`), and the dependency-policy, oracle-integrity, `cargo audit`, and `cargo deny` gates, plus a one-command `gates` runner.
- `android/app/src/main` — the hand-built Android radar app that consumes `bleradar-core` through `crates/bleradar-jni`; its design record is `docs/ANDROID_APP.md`.
- `vendor/rustsec-advisory-db/` — vendored RustSec advisory database for fully offline `cargo audit`/`cargo deny`.
- `docs/` — verified runtime topology, behavioral contract, Rust target architecture, issue/exception ledgers, generated parity frontier, the generated capability supersession matrix (`docs/CAPABILITY_MATRIX.md`, rendered by `bleradar_core::registry`), and verification records. Host toolchain / PATH setup: `docs/DEVELOPMENT.md`.
- `benchmarks/` — benchmark harness notes.
- `RUST_CONVERSION.md` — Rust-first migration boundary and consolidation prerequisites.
- `.github/workflows/gates.yml` — CI enforcement of every gate below.
- `.github/workflows/release.yml` — publishes the committed APK after `gates` passes on `main` (see [Releasing](#releasing)).
- `CHANGELOG.md` — notable changes, Keep a Changelog format.
- `oracle/` — the one canonical home of every immutable v0.3.0 input, each file pinned by `oracle/SHA256SUMS` and verified by `cargo xtask check-oracle-integrity` (a `gates` step). Layout, provenance and storage follow-ups: [`docs/ORACLE_STORAGE.md`](docs/ORACLE_STORAGE.md).
  - `oracle/BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` — original APK oracle (also pinned by `docs/INPUT_SHA256.txt`).
  - `oracle/BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0.zip` — byte-pinned migration archive (the `check-oracle-integrity` baseline; it was committed at the root as `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip`, same bytes). Its contents are checked in, not only zipped:
  - `oracle/libbleradar_core.so` and `oracle/classes.dex` — the immutable native oracle and DEX, same bytes as inside the archive.
  - `oracle/git-history.bundle` — the recovery history (`archive/migration-v0.3.0`, tags `migration/*` and `recovery/*`).
- `migration/critically-enhanced-v0.3.0/` — the v0.3.0 reconstruction from that archive, checked in as Rust. Inventory and parity generation live in `crates/bleradar-tools` (no Python, no shell). The snapshot is **not** a workspace member (`exclude = ["migration"]`) and must not replace the crates on `main`, which have moved past it. Its `SHA256SUMS` lists every other file in this Rust tree (`check-oracle-integrity` rejects an unlisted one; only untracked build output in the gitignored `target/` directory is skipped, so a file force-added there with `git add -f` must be listed too), including its copies of the canonical files (`oracle/BLE-Radar-v0.3.0-original.apk`, `oracle/classes.dex`, `oracle/libbleradar_core.so`, `git-history.bundle`). `check-oracle-integrity` maps each copy to its own counterpart above (the APK copy to `oracle/BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk`, the others to the file of the same name) and requires it to be listed and byte-identical to that file; the pinned zip remains the byte-for-byte original archive.
- The original Python and shell helpers survive only inside the pinned zip, which `check-oracle-integrity` must not modify. The source that builds is Rust: `cargo xtask` on `main`, and `bleradar-tools` inside the v0.3.0 snapshot.

## Developer setup

You need **rustup** and **Rust 1.98**. Put `~/.cargo/bin` ahead of `/usr/bin` on `PATH` so the pin in `rust-toolchain.toml` wins over a system `rustc` (1.85 and friends fail the MSRV check with exit 101). Full steps: [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

```sh
export PATH="$HOME/.cargo/bin:$PATH"
rustc --version          # must show 1.98.x
cargo check --workspace
```

## Verified runtime status

The shipped APK and this reconstructed workspace are separate execution
topologies. In the APK, Android DEX owns lifecycle, scheduling, platform-event
normalization, candidate generation, UI projection, and fallbacks; generated
bindings call 124 ABI contracts implemented by the shipped Rust native core.
The workspace libraries are reached by Cargo callers/tests and are not linked
into that APK.

The runtime registry classifies all 124 contracts: 41 observed executing, 78
statically reached from non-generated DEX call sites, and 5 of unknown
reachability. The five unknowns are read-only `RadarStore` methods that require
trustworthy Android/Bionic state. All shipped ABI implementations are
Rust-native. Differential verification against the oracle *binary* has now
begun: the immutable native core is executed under `qemu-aarch64` against a real
Bionic runtime, and the WiFi `channel_to_frequency`, `frequency_to_channel`,
`band`, `security` and `is_enterprise` contracts are `DifferentiallyVerified` —
the safe-Rust reconstruction reproduces every executed-oracle output bit-for-bit
over its input domain (`docs/ORACLE_DIFFERENTIAL.md`, `cargo xtask
oracle-differential`); `wifi_band`, `wifi_security` and `wifi_is_enterprise` were
previously-unmapped, statically-reachable shipped contracts reconstructed from
the executed oracle (`wifi_security` classifies a capabilities string by a
case-sensitive substring precedence; `wifi_is_enterprise` tests for `EAP`). The pure geodesy contracts (`haversine_m`,
`bearing_deg`) also have a broad executed-oracle differential — the
reconstruction matches the executed oracle to under a micrometre / nanodegree
over 308 coordinate pairs — but stay `SourceAnalog` because Bionic and host
`libm` are not bit-identical for transcendentals (coverage, not a bit-exact
promotion). The core BLE signal contracts (`ble_distance`, `proximity_label`)
also have an executed-oracle differential: the reconstruction's distance
calibration formula matches the oracle to machine precision in the valid region,
while the oracle's `[0.1,100]` m clamp + `rssi>=0` sentinel and its wider
proximity bands (`<1.5`/`<5`/`<15` vs the source's `<=1`/`<=2`/`<=5`) are
documented, locked `SourceAnalog` divergences. `wifi_distance` — another
previously-unmapped shipped contract — is reconstructed faithfully from the
executed oracle (the log-distance formula, an `rssi>=0`→400 m sentinel, a
`2000..=7199` MHz plausible-frequency window defaulting to 2437 MHz, and a
`[0.1,400]` m clamp) with no behavioural or domain divergence, and stays
`SourceAnalog` because its `log10`/`powf` step is transcendental (matched to
`<1e-12` relative, observed max 2e-15). Together these complete the
reconstruction of the entire WiFi ABI family — all six `wifi_*` contracts are now
reconstructed and executed-oracle-differentiated (five `DifferentiallyVerified`,
`wifi_distance` `SourceAnalog`).

See `docs/VERIFIED_RUNTIME_TOPOLOGY.md`,
`docs/BEHAVIORAL_CONTRACT.md`, and `docs/RUST_TARGET_ARCHITECTURE.md` before
changing runtime ownership.

## High-value tracking capabilities represented in Rust

The core supports selected-device lock state, ordered observation histories, randomized-address classification, filtered RSSI/hot-cold trend, calibrated BLE distance estimates, coarse proximity bands, GPS uncertainty, confidence-scored observed map points, and a conservative weighted spatial-region estimate (spherical centroid, correct across the ±180° antimeridian).

`Observed`, `Inferred`, and `Predicted` are separate evidence classes by design.

## Canonical evidence and provenance

The evidence core keeps raw observations separate from normalized values and
records `source`, `source_type`, `retrieval_method`, `observed_at`, `first_seen`,
`last_seen`, and `derivation_history` for every observation. `EvidenceStore`
rejects missing references and exposes trace APIs for:

- `claim → hypothesis → evidence → observation → source`;
- `input representation → transformation → output representation → features → verification`.

Raw observations are immutable through the public API: normalization returns a
new record and cannot replace the captured value. Other engines should write to
this store rather than maintaining parallel evidence histories. Multi-record
writes go through `EvidenceStore::transaction`, the store's single
all-or-nothing mechanism (an undo journal that is nested and panic-safe), which
every engine uses instead of copying the store; a refused engine operation
therefore leaves the store exactly as it was, at a cost that does not grow
with the store. A caller composing engines over one investigation moves the
canonical store from one engine into the next with `into_evidence()` (zero
copy) rather than cloning it; `crates/bleradar-core/tests/composition.rs`
threads one store through the OSINT, infrastructure, website and fusion
engines and validates the composed result.

`VerificationEngine` keeps required semantics separate from implementation and
supports metamorphic relations for invariance, idempotence, commutativity,
monotonicity, reversibility, round trips, partition recombination,
normalization, and permutation. It compares observable outputs, state, side
effects, errors, exit codes, ordering, concurrency, restart, recovery, and
contractual performance; failing inputs are minimized and classified, while
family yield, repairs, and regression locks remain explicit. Reports can be
persisted back into the canonical store as provenance-linked metamorphic test
records, and missing contractual measurements remain inconclusive rather than
being treated as proof.

`MetamorphicSoftwareAdvancementEngine` ranks proposed changes by expected net
benefit × correctness confidence × reachability × reversibility, divided by
implementation cost × regression risk. It accepts a candidate only after
baseline/candidate verification, differential equivalence, measurable
improvement, explained-regression review, falsification resistance, and
reproducibility all pass; integration and ranking recomputation remain explicit.

`CalibratedEvidenceFusion` scores reliability, specificity, rarity,
discriminative power, source independence, temporal compatibility,
transformation resistance, provenance quality, and reproducibility on an
explicit bounded calibration scale. It collapses dependent evidence groups and
can falsify a leading hypothesis by removing high-base-rate or strongest
support, checking contradictions, missing expected evidence, and uncertain
assumptions; it does not claim Bayesian precision without defensible
probabilities.

`ExecutionFeedbackAdaptiveOsintSearchEngine` treats search as an executable
frontier rather than a fixed list of expansions. It supports exact, normalized,
alias, historical, semantic, structural, temporal, relational, technical,
provenance, and graph-neighbor representations. Each execution records its
query, observed feedback, classification, adaptive family statistics, generated
or suppressed pivots, and complete control-loop phases; useful families receive
more ranking pressure while repeated or unproductive families are penalized.
Raw queries and source values remain separate from normalized forms, and
source-backed findings plus retrieval actions are persisted transactionally in
`EvidenceStore`.

`TemporalMetamorphicInfrastructureCorrelationEngine` treats infrastructure
relationships as competing explanations rather than proof of common control.
It preserves raw and normalized values, source metadata, dependency groups, and
first/last-seen intervals for eleven infrastructure observation families.
Correlation rankings down-weight common CDN, hosting, ASN, and HTTP signals,
collapse copied/provider-dependent support, reward rare features, independent
sources, and temporal continuity, and run adversarial passes before persisting
a provenance-linked relationship edge.

`WebsiteLineageEcosystemAnalysisEngine` extracts normalized text, distinctive
phrases, HTML structure, public assets, scripts, styles, identifiers, contacts,
certificates, links, application characteristics, and archived states while
retaining each raw capture and its source and temporal interval. It compares
websites through competing coincidence, platform, template, reuse,
development, and operational explanations; collapses provider-dependent
support; and applies bounded calibration, temporal alignment, and
support-removal falsification before persisting a canonical lineage edge.
Website similarity can yield a possible common-operator assessment, but this
engine never treats similarity alone as proof of common operation.

`InvestigationPipeline` names and tracks the loop that ties every other engine
together: discover → normalise → entity-resolve → trace source lineage →
geolocate → generate pivots → score frontier → expand best candidates →
corroborate/contradict → build temporal+geo graph → detect bridges/clusters →
generate competing hypotheses → seek discriminating evidence → promote/demote
claims → recompute frontier → stop on diminishing information gain. Fourteen of
the sixteen stages are domain work already performed by `entity`, `evidence`,
`coords`, `osint`, `fusion`, `infrastructure`, and `website`; this module adds
only the two capabilities the loop names but nothing else implements: a
`TemporalGeoGraph` that unifies caller-supplied edges with per-node temporal
and geographic annotations and reports connected components and bridge edges
(via an iterative, multigraph-safe Tarjan bridge search), and a
`DiminishingGainStopCriterion` that halts the loop once a full sliding window
of consecutive rounds all yield low marginal information gain — distinct from
`osint`'s static per-pivot ranking factor and hard search-limit counts, neither
of which measures actual round-over-round yield.

## Requirements

- Rust toolchain **1.98.0** with `clippy` and `rustfmt` — pinned by `rust-toolchain.toml`; `rustup` installs it automatically on first `cargo` invocation in the repo. Put `~/.cargo/bin` first on `PATH` (a system `/usr/bin/rustc` below 1.98 fails MSRV with exit 101). See `docs/DEVELOPMENT.md`.
- No third-party crates in the shipped workspace: it is intentionally dependency-free, and CI fails if that changes without a recorded decision. `xtask/` (developer tooling) and the vendored advisory database are outside that scope; see `xtask/Cargo.toml`.
- `cargo-audit` and `cargo-deny` on `PATH` to run those two specific gates, at the versions CI pins (`cargo install --locked cargo-audit@0.22.2 cargo-deny@0.20.2`; bump them together with `.github/workflows/gates.yml`); every other gate, including `cargo xtask gates` itself, needs nothing beyond the pinned toolchain. The JNI export-contract gate inside `gates` reads the host-built `libbleradar_jni.so` with the in-tree ELF64 reader, so `gates` is proven on Linux hosts (what CI runs).
- A JDK (`javac`/`java`/`keytool`, and `jar` for `oracle-differential`) only for `cargo xtask verify-jni-live`, `verify-api-live`, `verify-android-unit`, `build-apk`, `verify-android-live`, `build-update-proof`, `oracle-differential` and `verify-android-emulator` (whose `avdmanager` runs on it). CI uses Temurin 21.
- An Android SDK/NDK only for `cargo xtask build-apk`/`verify-android-live`/`build-update-proof`/`verify-jni-target`/`oracle-differential`/`prepare-bionic-sysroot` (pinned in `xtask/src/main.rs`: platform `android-36`, build-tools `37.0.0`, NDK `27.3.13750724`; `cargo xtask android-sdk-install` installs them).
- For the qemu proofs `oracle-differential` and `verify-jni-target`: `qemu-aarch64` (or `qemu-aarch64-static`, or `QEMU_AARCH64`) and a Bionic runtime. That runtime is `BIONIC_SYSROOT`, or `debugfs` (e2fsprogs) plus the pinned `system-images;android-24;default;arm64-v8a` (`cargo xtask android-sdk-install --system-image`), which `prepare-bionic-sysroot` also needs. See `docs/ORACLE_DIFFERENTIAL.md`.
- The SDK's emulator (pinned `37.2.12`), platform-tools and the pinned `system-images;android-34;google_apis;x86_64` plus KVM only for `verify-android-emulator` (`cargo xtask android-sdk-install --emulator`; see `docs/ANDROID_APP.md` and `docs/DEVELOPMENT.md`).

## Installation

```sh
# from a git clone or an extracted distribution archive
cd HSE-BLE-API-
export PATH="$HOME/.cargo/bin:$PATH"   # rustup before any system rustc
rustc --version                       # must show 1.98.x
cargo build --workspace --locked
```

If `rustc --version` is below 1.98, fix PATH / install rustup before building.
Details: [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

## Usage example

```rust
use bleradar_core::{DeviceObservation, DeviceTrack, LatLon, ProximityBand};

let mut track = DeviceTrack::new(0.4).unwrap();
track
    .push(DeviceObservation {
        timestamp_ms: 0,
        observer_position: Some(LatLon::new(-26.8000, 152.8000).unwrap()),
        gps_accuracy_m: Some(5.0),
        rssi_dbm: -63.0,
        tx_power_dbm: None,
    })
    .unwrap();
assert_eq!(track.proximity(), Some(ProximityBand::Near));
// With two or more positioned observations, track.spatial_estimate()
// yields a conservative confidence-scored region estimate.
```

## Standard gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Or run every gate — the above plus the JNI export-contract check (every
`static native` in `NativeRadar.java` ↔ exactly one `Java_*` export in the
built `bleradar-jni`, no orphans), `xtask`'s own fmt/clippy/build/test, the
parity-report drift check, the zero-third-party-dependency policy check, the
oracle-integrity check, `check-app-version`, and `cargo audit`/`cargo deny`
against the vendored advisory database — with the single local gate runner:

```sh
cargo xtask gates
```

These same gates run on every pull request and on every push to `main` via
`.github/workflows/gates.yml` (the workflow's `push` trigger is limited to
`main`; feature branches are covered through their pull requests), followed
by the live JVM → JNI → Rust proof
(`cargo xtask verify-jni-live`) on a pinned Temurin 21 JDK; the workflow pins
`cargo-audit`/`cargo-deny` to exact versions and caches their binaries, so a
run is reproducible and does not rebuild them from source every time.
Autonomous maintenance sessions operate under `docs/AUTONOMOUS_ENGINE.md`.

## Live JNI proof

```sh
cargo xtask verify-jni-live
```

This compiles the host `bleradar-jni` library, verifies its export contract
against the repository's
`android/app/src/main/java/com/hse/bleradar/NativeRadar.java`
(`cargo xtask check-jni-contract`), checks the string bridge's JNI
function-table slots against the JDK's `jni.h`, compiles that façade, then
executes a real JVM → JNI → Rust smoke harness. It proves both the failure
path (wrong library path yields `UnsatisfiedLinkError`) and the success path
(library loads, ABI version matches, every declared native is resolved and
invoked by the JVM through reflection with the count cross-checked against
the Java source, and JNI calls — Java strings included — return the expected
values). CI runs it on every pull request and every push to `main`.

```sh
cargo xtask verify-jni-target
```

This cross-compiles the `bleradar-jni` test suite — the unit tests, the
`jni_bridge` regression tests and the 20,000-iteration export campaign — for
`aarch64-linux-android` with the NDK and executes it under `qemu-aarch64`
against a real Android Bionic runtime extracted from the
`system-images;android-24;default;arm64-v8a` image: the architecture and libc
the shipped `libbleradar_jni.so` runs on, rather than the x86_64 host the JVM
proof uses. It needs `qemu-user-static`, `debugfs` (e2fsprogs) and the system
image, or a prepared runtime via `BIONIC_SYSROOT` (`cargo xtask
prepare-bionic-sysroot <dir>` extracts one, about 3 MB, which is what CI
caches). It needs the `aarch64-linux-android` Rust target, which the command
installs through `rustup` when missing, exactly as `build-apk` does. CI's
`android-apk` job runs it on every pull request and every push to `main`.

## Strongest current Android live proof

```sh
cargo xtask verify-android-live
```

This runs the strongest end-to-end proof currently possible in this sandbox:
the live JVM → JNI → Rust proof above, a full `cargo xtask build-apk`, then
post-build verification that the generated APK contains the required manifest,
DEX, and JNI library entries, that the built DEX defines the critical Android
classes, and that the cross-compiled native library exports exactly the JNI
entrypoints `NativeRadar.java` declares (the same export-contract rule as
`gates`, applied to the `aarch64-linux-android` build). It also runs Android
lint's `NewApi` check over the app sources, so a `java.*`/`android.*` call
newer than the manifest's `minSdkVersion` (which `javac` and `d8` accept and
which crashes older devices at run time) fails the build. It reads the built
package's version back (`aapt2 dump badging` ↔ `APP_VERSION_CODE`/
`APP_VERSION_NAME`, the one version authority in `xtask/src/main.rs`) and
requires the fresh build's entries (name, size, CRC-32) to equal the
committed `HSE-BLE-Radar-arm64-v1.0.0.apk`'s, so a committed package that no
longer matches the sources fails on every pull request; `build-apk` stores
every entry at the ZIP epoch, so two builds on one signing key are
byte-identical. Design decisions for the app itself are recorded in
`docs/ANDROID_APP.md`.

## Web dashboard (Termux / browser on the device)

While `RadarScanService` runs, the app serves a loopback-only HTTP API on
`http://127.0.0.1:8080/` — `/api/devices`, `/api/wifi` (the passive Wi-Fi
survey), `/api/status` (which also says whether the scan is `idle`, `scanning`,
`recovering` — wanted but waiting for Bluetooth or a retry — or `failed`, and why),
`/api/updates` as
JSON — and, at `/`, a self-contained web dashboard (`android/app/src/main/assets/dashboard.html`)
that polls them every second and renders the status, a canvas radar and the
ranked device table. Open it in any browser on the device (from Termux:
`termux-open-url http://127.0.0.1:8080/`, or `curl http://127.0.0.1:8080/api/devices`
for the JSON); from another machine, tunnel it with
`ssh -L 8080:127.0.0.1:8080 <device>` — nothing off-device can reach the
port directly.

The scan is controlled through the same API — the dashboard's Start/Stop
buttons, or a Termux shell:

```sh
curl -X POST http://127.0.0.1:8080/api/scan/start   # {"scanning":true}
curl -X POST http://127.0.0.1:8080/api/scan/stop    # {"scanning":false}
```

A refused start answers `409` with the reason (the Bluetooth permissions
were never granted — open the app once — Bluetooth is off, the scanner is
unavailable, or Android refused a background start). A stop pauses the scan
but keeps the service, its idle notification and this API alive, so a
headless session can resume later without the screen. The API lives as long
as the service: open the app once (its status line shows the dashboard URL)
and start a scan, and the service stays up — scanning or paused — until the
app's own Stop.

```sh
cargo xtask verify-dashboard-live
```

This renders the committed page in headless Chromium against a mock of the
documented JSON contract and checks the DOM it produced: every fixture device
in the server's order with its name rendered as text (never markup), more
than one completed poll, and the error banner when the API answers `500` or a
response of the wrong shape. Before rendering, it locks the contract itself:
the field names `ApiHttpServer.java` writes per endpoint must equal the
mock's keys and cover every property the page reads (the same check runs as
an xtask unit test inside `gates`). It needs a Chromium/Chrome binary
(`BLERADAR_CHROMIUM=<path>`, one of `chromium`/`chromium-browser`/`google-chrome`
on `PATH`, or Playwright's browser cache); CI's `web-dashboard` job runs it
on every pull request and every push to `main`.

```sh
cargo xtask verify-api-live
```

This runs the app's real `ApiHttpServer` — the class the APK ships, which
references nothing in `android.*` — on the host JVM with fixture sources, a
scripted `ScanControl` and the host `bleradar-jni` library, answers 48 real
HTTP requests over loopback (the dashboard bytes, the three JSON documents
byte-identical to the browser fixtures, `404`/`405`/`400`, `no-store`, exact
`Content-Length`; scan control paused and resumed with every document
reflecting it, the four documented refusals as `409`, a throwing control
answered `500` with the server still up, a 64 KiB request body consumed, a
body declared beyond the 64 KiB cap refused with `413` at once), then
renders the committed dashboard from that server in headless Chromium, and
then runs the real `ReleaseManifestSource` — the class that fetches the
app's release manifest from the repository's latest release — against a
scripted local server (a valid manifest, no release, a server fault, a
redirect it cannot follow, an oversized body, a body the Rust core rejects,
a stalled answer, a refused connection, a malformed URL), requiring each fetch's classification and its
Rust disposition through the real library.
It needs a JDK and a Chromium/Chrome binary; CI's `gates` job runs it after
the live JNI proof.

```sh
cargo xtask verify-android-unit
```

This runs the app's unit tests — `android/app/src/test/java/com/hse/bleradar`,
written against JUnit 4's `@Test` and `Assert` — on the host JVM against the
real host `bleradar-jni` library, with no JUnit jar and no Gradle: xtask
generates the `org.junit.Test` marker, the `Assert` overloads the tests call
(resolved by `javac` as JUnit would resolve them) and a reflection runner,
compiles them with the host-executable app sources, and requires the native
core loaded (half of `ReleaseManifestTest` asserts `null` results, which a
JVM without the library would produce for the wrong reason), every listed
class run with its pinned test count, and zero failures. The directory may
hold only the listed classes — a test file the runner does not execute fails
the gate — because until decision #98 every file there was dormant: 2,731
lines of JUnit-style tests that had never run, 2,091 of them asserting
literals against themselves. `BlipTest` (the RSSI window, the spread, the
address-derived angle) and `ReleaseManifestTest` (the manifest contract
through the real core) survived that cut; the runner now executes seven
classes pinned in `HOST_TEST_CLASSES` (`xtask/src/javaunit.rs`) —
`ApiHttpServerGuardTest` 4, `BlipTest` 14, `DeviceHistoryTest` 10,
`ReleaseManifestTest` 50, `ScanSupervisorTest` 21, `WifiApTest` 12,
`WifiSurveyTest` 4: 115 tests. It needs a JDK; CI's
`gates` job runs it after `verify-api-live`.

```sh
cargo xtask verify-android-emulator
```

This runs the committed `HSE-BLE-Radar-arm64-v1.0.0.apk` on a real Android
runtime: a throw-away AVD from the pinned API 34 `google_apis` x86_64 system
image (whose ARM translation runs the arm64-only package as is) boots
headless on KVM, on the pinned emulator 37.2.12 started with
`-feature -WiFiPacketStream` (the guest's Wi-Fi then uses the emulator's own
globally administered access point, not netsimd's randomized one; Bluetooth
stays on netsimd); the adapter is enabled and awaited, the APK installed with
its runtime permissions granted, the activity launched and the loopback API
forwarded to the host. It then requires, over real HTTP, `GET /` byte-identical
to the committed dashboard, the three documents with their keys and
`native_available` true, `POST /api/scan/start` answered `200` with
`RadarScanService` promoted to the foreground and its "BLE Radar is scanning"
notification, a virtual advertiser — a second Bluetooth controller the proof
connects to netsimd's HCI socket (`xtask/src/hci.rs`; the emulator's radios
are simulated there), advertising as `bleradar-beacon` — listed by
`/api/devices` with its Rust-computed row (RSSI, a finite distance) within
30 s and pruned by the core's freshness policy within 75 s of its removal,
`POST /api/scan/stop` keeping the foreground with "BLE Radar is
idle", a restarted service with the scan resumed after `kill -9` of the app
process, Bluetooth turned off and back on with `/api/status` reporting
`scan_state: recovering` and why while it is gone and the beacon sighted
again with no request once the adapter returns, the first launch's update
check finished (its fetch of the
repository's release manifest logged with its outcome — a `404` until a
release is published —, its decision logged, no service record or
notification left), the API unreachable after
`am force-stop`, and — after a relaunch and a new scan — no scan and no
foreground service surviving `pm revoke … BLUETOOTH_SCAN` (the report names
the branch the platform took; on the first run the sticky restart found the
permissions revoked and stopped the service), with no Java or native crash
of the package and no leaked `ServiceConnection` in logcat. Then the app
upgrades itself through its own pathway (decision #99): the committed
package is replaced by `build-update-proof`'s build of the same version
(one signing key with its successor), a `keytool` certificate for
`github.com` is injected into the guest's system trust store (a tmpfs copy
of the Conscrypt APEX's store, bind-mounted over it in every zygote's
namespace), a generated JDK `HttpsServer` on that certificate serves the
release URL's redirect, the successor's manifest and its APK behind an xtask
CONNECT proxy set as the guest's global proxy, and a relaunch must fetch the
production URL (`HTTP 200 -> using the remote manifest`), decide 1, download,
verify and hand the successor to the installer, whose `Update` the proof
taps; the successor's versionCode must be installed and its API up with the
native core, and the stand-in's log must show the redirect, the manifest and
the artifact in order (on the first end-to-end run: handed to the installer
5.8 s after the launch, installed 2.5 s after the tap). The AVD is
deleted on every exit. It needs
`/dev/kvm`, the SDK's `emulator`, `platform-tools` and the pinned image
(`cargo xtask android-sdk-install --emulator` installs them: licenses
accepted, `sdkmanager` retried, every package and the tools checked, and
the emulator pinned to 37.2.12: kept only when its `package.xml` and
`source.properties` name 37.2.12 and the SHA-256 of seven of its files
matches the pinned archive, else replaced by that archive, fetched over
HTTPS only, its size and SHA-256 checked before it is unpacked and swapped
in) and a JDK; CI's `android-emulator` job runs it on every pull request and
every push to `main` (2 min 33 s on the first green run; 4 min 00 s with
the virtual advertiser, decision #95). The emulator is launched with
`-feature -WiFiPacketStream`, so the survey sees its own virtio-wifi access
point (`00:13:10:85:fe:01`, globally administered, `TRACKABLE`) rather than
netsimd's locally administered one; the run fails if netsimd's Wi-Fi comes
up anyway.

## Parity report

```sh
cargo xtask parity-report
```

This regenerates `docs/PARITY_COVERAGE.md` from the packaged ABI census and
the semantic source-parity/runtime registries.

## Developer tooling (`cargo xtask`)

`xtask/` is a dependency-free, Rust-native replacement for the former
`tools/*.py`/`tools/native_abi.sh` scripts — no Python or `readelf` needed.
It is a separate Cargo workspace, so it never joins `--workspace` scope or
the root `Cargo.lock`.

```sh
cargo xtask                        # list subcommands
cargo xtask parity-report          # regenerate docs/PARITY_COVERAGE.md
cargo xtask check-dependency-policy
cargo xtask check-oracle-integrity
cargo xtask apk-inventory <apk>
cargo xtask native-abi <lib.so>
cargo xtask dex-classes <classes.dex>
cargo xtask vendor-advisory-db         # materialize the offline cargo-deny advisory db
cargo xtask oracle-differential        # execute the immutable oracle under qemu-aarch64 and check the committed vectors (docs/ORACLE_DIFFERENTIAL.md)
cargo xtask verify-jni-target          # bleradar-jni tests cross-compiled for aarch64-linux-android, run under qemu against Bionic
cargo xtask prepare-bionic-sysroot <dir>   # extract the android-24 arm64 Bionic runtime for BIONIC_SYSROOT
cargo xtask verify-dashboard-live      # dashboard.html in headless Chromium against a mock of the JSON contract
cargo xtask verify-api-live            # the real ApiHttpServer on the host JVM, every HTTP contract, then the dashboard from it
cargo xtask sync-company-ids           # refresh the Bluetooth SIG company-identifier registry (crates/bleradar-core/data)
cargo xtask check-jni-contract [lib.so]   # NativeRadar.java natives ↔ Java_* exports, 1:1 (host build by default)
cargo xtask verify-jni-live        # real JVM → JNI → Rust proof (needs a JDK)
cargo xtask build-apk              # cross-compile + package + sign the Android app (needs SDK/NDK)
cargo xtask verify-android-live    # verify-jni-live + build-apk + APK/DEX/export checks
cargo xtask verify-apk-rebuild <committed.apk> <rebuilt.apk>  # equal once the APK Signing Block is stripped (signer-agnostic; CI runs it after the build)
cargo xtask verify-android-unit    # the app's unit tests (android/app/src/test) on the host JVM against the real native core (needs a JDK)
cargo xtask verify-android-emulator   # the committed APK on a headless API 34 emulator, then the app upgrading itself against a stand-in github.com (needs KVM + SDK emulator + target/android-apk/proof from build-update-proof)
cargo xtask build-update-proof     # the current version and its successor on one key + the successor's release manifest, under target/android-apk/proof (needs SDK/NDK)
cargo xtask android-sdk-packages [--system-image|--emulator]   # the pinned sdkmanager package set CI installs
cargo xtask android-sdk-install [--system-image|--emulator]    # install that set: licenses, sdkmanager retried, every package and the pinned tools checked, the emulator verified against (or replaced by) its pinned archive (what CI runs)
cargo xtask check-app-version      # the bundled release manifest repeats APP_VERSION_CODE/NAME, the committed APK's name carries APP_VERSION_NAME, no artifact of another version remains (a gates step)
cargo xtask release-plan           # the release identity (tag, apk, manifest, version) as key=value lines, after checking the committed APK; what the release workflow reads
cargo xtask release-manifest [--url <artifact url>] [--out <path>]   # the manifest a release publishes: the committed APK's version, exact size and SHA-256
cargo xtask audit                  # cargo audit, offline, vendored advisory db
cargo xtask deny                   # cargo deny check, offline, vendored advisory db
cargo xtask gates                  # every gate, one command
cargo run --release -p bleradar-jni --example scan_result_cost   # host hot-path baseline, see benchmarks/README.md
cargo run --release -p bleradar-core --example engine_load        # per-operation engine cost vs store size, see benchmarks/README.md
BLERADAR_CAMPAIGN_ITERATIONS=5000000 cargo test -p bleradar-core --release --test falsification_campaign   # scale the randomised campaign
BLERADAR_EVIDENCE_CAMPAIGN_SEQUENCES=20000 cargo test -p bleradar-core --release --test evidence_campaign   # scale the evidence-store campaign
BLERADAR_OSINT_CAMPAIGN_SEQUENCES=50000 cargo test -p bleradar-core --release --test osint_campaign   # scale the OSINT engine campaign
BLERADAR_WEBSITE_CAMPAIGN_SEQUENCES=20000 cargo test -p bleradar-core --release --test website_campaign   # scale the website lineage engine campaign
BLERADAR_INFRASTRUCTURE_CAMPAIGN_SEQUENCES=20000 cargo test -p bleradar-core --release --test infrastructure_campaign   # scale the infrastructure correlation campaign
BLERADAR_JNI_CAMPAIGN_ITERATIONS=2000000 cargo test -p bleradar-jni --release --test jni_campaign   # scale the JNI export differential campaign
BLERADAR_FUSION_CAMPAIGN_ITERATIONS=300000 cargo test -p bleradar-core --release --test fusion_campaign   # scale the fusion differential campaign
BLERADAR_VERIFICATION_CAMPAIGN_SEQUENCES=20000 cargo test -p bleradar-core --release --test verification_campaign   # scale the verification engine campaign
BLERADAR_ADVANCEMENT_CAMPAIGN_SEQUENCES=50000 cargo test -p bleradar-core --release --test advancement_campaign   # scale the advancement engine campaign
```

## Releasing

**Release policy:** pre-releases only, as `main-<sha7>` plus a rolling
`latest`. A stable (non-pre-release) release is made only with the owner's
explicit approval and is never automatic.

The app checks `https://github.com/EmmmmDeee/HSE-BLE-API-/releases/latest/download/release_manifest.txt`
daily (`docs/AUTO_UPDATE.md`), so a stable release is a GitHub release (not a
pre-release) carrying two assets — the APK and the manifest that describes it — and the version has one
authority, `APP_VERSION_CODE`/`APP_VERSION_NAME` in `xtask/src/main.rs`:

```sh
# 1. bump APP_VERSION_CODE / APP_VERSION_NAME in xtask/src/main.rs and the
#    version_code / version_name lines of android/app/src/main/assets/release_manifest.txt
cargo xtask build-apk                      # HSE-BLE-Radar-arm64-v<version name>.apk, byte-reproducible on one key
git rm HSE-BLE-Radar-arm64-v<previous>.apk # one artifact is committed: check-app-version refuses a stale one
cargo xtask check-app-version              # the bundled manifest, the artifact's name, no stale artifact (also a `gates` step)
cargo xtask verify-android-live            # the built version read back; the package's entries reproduced by a second build
git add HSE-BLE-Radar-arm64-v<version name>.apk
# 2. merge to main — the `release` workflow publishes that commit as the
#    pre-releases main-<sha7> and `latest` (never a stable release)
# 3. cut the stable release by hand, from a main commit `gates` passed on:
cargo xtask release-plan                                 # tag v<version name>, the APK and manifest names
cargo xtask release-manifest --out release_manifest.txt  # url = the asset on the v<version name> release
scan="$(mktemp -d)" && unzip -q -d "$scan" HSE-BLE-Radar-arm64-v<version name>.apk
bash scripts/scan-for-keys.sh HSE-BLE-Radar-arm64-v<version name>.apk "$scan"  # must report 0 findings (exit 2 = could not scan: fix, never ignore)
gh release create v<version name> HSE-BLE-Radar-arm64-v<version name>.apk release_manifest.txt \
  --target <commit> --title "HSE BLE Radar <version name>" --latest
```

Every main build is published automatically, as pre-releases only: once a
commit is on `main` and `gates` passes, the `release` workflow publishes the
committed APK (with a `release_manifest.txt` pointing at that pre-release's
own asset, the zero-finding `key-scan-report.txt` and `SHA256SUMS`) as the
immutable `main-<sha7>` pre-release and moves the rolling `latest`
pre-release to it, so a tester only ever downloads and installs the APK. It
never creates, edits or re-tags a stable release and never marks anything
latest, and GitHub's `releases/latest/download/` URL skips pre-releases, so
installed apps are updated only by a stable release cut by hand (step 3) —
see "Publishing a release" in `docs/AUTO_UPDATE.md` for exactly when the
workflow runs and what it refuses.

`release-manifest` refuses a non-`https` URL, and `verify-api-live` serves
the manifest it generates to the real core as the accepted-manifest scenario,
so the text a release publishes is proven parseable before it is published;
`verify-android-emulator` runs the whole pathway past it — the fetch of the
production URL, the download, the core's verification, the installer, the
install — against a stand-in `github.com` on every pull request
(`build-update-proof`, decision #99), so a real release's only untested
link is GitHub itself.

## Distribution packaging

Release archives are produced per `docs/PACKAGING_ASSISTANT.md`: all tracked project files (oracles included, since the integrity gate depends on them), excluding `.git/`, `target/`, and non-project local files; named `hse-ble-api-v<version>.zip` with a SHA-256 sidecar; verified by extracting to a clean directory and running every gate from the extraction before delivery.

## License

Proprietary (see the `license` field in `Cargo.toml`). All rights reserved by the project owner; the retained APK and native artifacts remain the property of their original rights holder and are included solely as behavioral verification oracles.

## Start here

Read, in order:

1. `docs/VERIFIED_RUNTIME_TOPOLOGY.md`
2. `docs/BEHAVIORAL_CONTRACT.md`
3. `docs/RUST_TARGET_ARCHITECTURE.md`
4. `docs/ISSUE_LEDGER.md`
5. `docs/EXCEPTION_LEDGER.md`
6. `docs/PARITY_COVERAGE.md`
7. `RUST_CONVERSION.md`
8. `docs/FINAL_REPORT.md`
9. `docs/REQUIREMENTS_LEDGER.md`
10. `docs/COLD_START_VERIFICATION.md`
11. `docs/ANDROID_APP.md`
12. `docs/CAPABILITY_LEDGER.md`
13. `docs/DEVELOPMENT.md`
14. `CHANGELOG.md`
