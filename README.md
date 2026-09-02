# HSE-BLE-API-

Huntsman's Radar (API)

An auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

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
- `crates/bleradar-compat` — complete native ABI runtime/reachability census plus a separate source-replacement parity registry.
- `xtask/` — dependency-free Rust-native developer tooling (`cargo xtask`): binary inventory, parity-report generation, ABI/DEX census, and the dependency-policy, oracle-integrity, `cargo audit`, and `cargo deny` gates, plus a one-command `gates` runner.
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

The runtime registry classifies all 124 contracts: 19 observed executing, 78
statically reached from non-generated DEX call sites, and 27 of unknown
reachability. All shipped ABI implementations are Rust-native, but none of the
similarly named workspace replacements is yet differentially verified across
its complete observable contract.

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
this store rather than maintaining parallel evidence histories.

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

## Requirements

- Rust toolchain **1.98.0** with `clippy` and `rustfmt` — pinned by `rust-toolchain.toml`; `rustup` installs it automatically on first `cargo` invocation in the repo.
- No third-party crates in the shipped workspace: it is intentionally dependency-free, and CI fails if that changes without a recorded decision. `xtask/` (developer tooling) and the vendored advisory database are outside that scope; see `xtask/Cargo.toml`.
- `cargo-audit` and `cargo-deny` on `PATH` to run those two specific gates (`cargo install cargo-audit cargo-deny`); every other gate, including `cargo xtask gates` itself, needs nothing beyond the pinned toolchain.

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

Or run every gate — the above plus the parity-report drift check, the
zero-third-party-dependency policy check, the oracle-integrity check, and
`cargo audit`/`cargo deny` against the vendored advisory database — with the
single local gate runner:

```sh
cargo xtask gates
```

These same gates run on every push and pull request via
`.github/workflows/gates.yml`. Autonomous maintenance sessions operate under
`docs/AUTONOMOUS_ENGINE.md`.

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
cargo xtask audit                  # cargo audit, offline, vendored advisory db
cargo xtask deny                   # cargo deny check, offline, vendored advisory db
cargo xtask gates                  # every gate, one command
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
