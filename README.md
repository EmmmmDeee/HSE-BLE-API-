# HSE-BLE-API-

Huntsman's Radar (API)

An auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

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
- `crates/bleradar-compat` — complete native ABI runtime/reachability census plus a separate source-replacement parity registry.
- `xtask/` — dependency-free Rust-native developer tooling (`cargo xtask`): binary inventory, parity-report generation, ABI/DEX census, the JNI export-contract gate derived from `NativeRadar.java`, live Java→JNI→Rust verification, APK packaging, executed-oracle differential verification under `qemu-aarch64` (`oracle-differential`, see `docs/ORACLE_DIFFERENTIAL.md`), and the dependency-policy, oracle-integrity, `cargo audit`, and `cargo deny` gates, plus a one-command `gates` runner.
- `android/app/src/main` — the hand-built Android radar app that consumes `bleradar-core` through `crates/bleradar-jni`; its design record is `docs/ANDROID_APP.md`.
- `vendor/rustsec-advisory-db/` — vendored RustSec advisory database for fully offline `cargo audit`/`cargo deny`.
- `docs/` — verified runtime topology, behavioral contract, Rust target architecture, issue/exception ledgers, generated parity frontier, and verification records.
- `benchmarks/` — benchmark harness notes.
- `RUST_CONVERSION.md` — Rust-first migration boundary and consolidation prerequisites.
- `.github/workflows/gates.yml` — CI enforcement of every gate below.
- `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` — original APK oracle.
- `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip` — original migration archive; also retains the extracted native oracles (`oracle/libbleradar_core.so`, `oracle/classes.dex`) and the migration `git-history.bundle` for differential testing.

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

- Rust toolchain **1.98.0** with `clippy` and `rustfmt` — pinned by `rust-toolchain.toml`; `rustup` installs it automatically on first `cargo` invocation in the repo.
- No third-party crates in the shipped workspace: it is intentionally dependency-free, and CI fails if that changes without a recorded decision. `xtask/` (developer tooling) and the vendored advisory database are outside that scope; see `xtask/Cargo.toml`.
- `cargo-audit` and `cargo-deny` on `PATH` to run those two specific gates, at the versions CI pins (`cargo install --locked cargo-audit@0.22.2 cargo-deny@0.20.2`; bump them together with `.github/workflows/gates.yml`); every other gate, including `cargo xtask gates` itself, needs nothing beyond the pinned toolchain. The JNI export-contract gate inside `gates` reads the host-built `libbleradar_jni.so` with the in-tree ELF64 reader, so `gates` is proven on Linux hosts (what CI runs).
- A JDK (`javac`/`java`) only for `cargo xtask verify-jni-live`, `verify-api-live` and `verify-android-emulator` (whose `avdmanager` runs on it), an Android SDK/NDK only for `cargo xtask build-apk`/`verify-android-live`, and the SDK's emulator, platform-tools and pinned system image plus KVM only for `verify-android-emulator` (see `docs/ANDROID_APP.md`).

## Installation

```sh
# from a git clone or an extracted distribution archive
cd HSE-BLE-API-
cargo build --workspace --locked
```

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
built `bleradar-jni`, no orphans), the parity-report drift check, the
zero-third-party-dependency policy check, the oracle-integrity check, and
`cargo audit`/`cargo deny` against the vendored advisory database — with the
single local gate runner:

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
which crashes older devices at run time) fails the build. Design decisions
for the app itself are recorded in `docs/ANDROID_APP.md`.

## Web dashboard (Termux / browser on the device)

While `RadarScanService` runs, the app serves a loopback-only HTTP API on
`http://127.0.0.1:8080/` — `/api/devices`, `/api/status`, `/api/updates` as
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
scripted `ScanControl` and the host `bleradar-jni` library, answers 27 real
HTTP requests over loopback (the dashboard bytes, the three JSON documents
byte-identical to the browser fixtures, `404`/`405`/`400`, `no-store`, exact
`Content-Length`; scan control paused and resumed with every document
reflecting it, the four documented refusals as `409`, a throwing control
answered `500` with the server still up, a 64 KiB request body consumed, a
body declared beyond the 64 KiB cap refused with `413` at once), and
then renders the committed dashboard from that server in headless Chromium.
It needs a JDK and a Chromium/Chrome binary; CI's `gates` job runs it after
the live JNI proof.

```sh
cargo xtask verify-android-emulator
```

This runs the committed `HSE-BLE-Radar-arm64-v1.0.0.apk` on a real Android
runtime: a throw-away AVD from the pinned API 34 `google_apis` x86_64 system
image (whose ARM translation runs the arm64-only package as is) boots
headless on KVM; the adapter is enabled and awaited, the APK installed with
its runtime permissions granted, the activity launched and the loopback API
forwarded to the host. It then requires, over real HTTP, `GET /` byte-identical
to the committed dashboard, the three documents with their keys and
`native_available` true, `POST /api/scan/start` answered `200` with
`RadarScanService` promoted to the foreground and its "BLE Radar is scanning"
notification, `POST /api/scan/stop` keeping the foreground with "BLE Radar is
idle", a restarted service with the scan resumed after `kill -9` of the app
process, no Java or native crash of the package in logcat, and the API
unreachable after `am force-stop`; the AVD is deleted on every exit. It needs
`/dev/kvm`, the SDK's `emulator`, `platform-tools` and the pinned image
(`cargo xtask android-sdk-install --emulator` installs them: licenses
accepted, `sdkmanager` retried, every package and the tools checked)
and a JDK; CI's `android-emulator` job runs it on every pull request and
every push to `main` (2 min 33 s on the first green run).

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
cargo xtask check-jni-contract [lib.so]   # NativeRadar.java natives ↔ Java_* exports, 1:1 (host build by default)
cargo xtask verify-jni-live        # real JVM → JNI → Rust proof (needs a JDK)
cargo xtask build-apk              # cross-compile + package + sign the Android app (needs SDK/NDK)
cargo xtask verify-android-live    # verify-jni-live + build-apk + APK/DEX/export checks
cargo xtask verify-android-emulator   # the committed APK on a headless API 34 emulator (needs KVM + SDK emulator)
cargo xtask android-sdk-packages [--system-image|--emulator]   # the pinned sdkmanager package set CI installs
cargo xtask android-sdk-install [--system-image|--emulator]    # install that set: licenses, sdkmanager retried, every package and the pinned tools checked (what CI runs)
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
