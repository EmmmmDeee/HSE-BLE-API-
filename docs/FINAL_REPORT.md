# Final Migration Report — Critically Enhanced

## Executive status

This deliverable is a **binary-grounded Rust reconstruction and migration foundation**, critically enhanced from the first pass. It does **not** falsely claim complete functional-parity source recovery from an R8-obfuscated APK.

The supplied APK already contains a substantial Rust core. The original APK, DEX and ARM64 `libbleradar_core.so` remain immutable behavioral oracles. The reconstructed workspace now implements a substantially broader safe-Rust domain for device-centric tracking and map interaction while explicitly separating observed measurements from inference.

## Before architecture

Android Compose UI → Android BLE/location/map services → generated Kotlin UniFFI/JNA → shipped Rust `libbleradar_core.so`.

## Enhanced reconstructed architecture

Rust workspace:
- `bleradar-core::evidence` — canonical, provenance-preserving evidence records and
  referentially validated claim/transformation traces.
- `bleradar-core::advancement` — evaluation-gated metamorphic software
  advancement with explicit ranking factors, benchmark gates, and integration
  state.
- `bleradar-core::osint` — execution-feedback adaptive OSINT search with
  representation-aware pivots, adaptive ranking, and canonical provenance
  persistence.
- `bleradar-core::infrastructure` — temporal metamorphic infrastructure
  correlation with explicit shared-infrastructure versus common-control
  classifications, temporal continuity, dependency collapse, and falsification.
- `bleradar-core::website` — website lineage and ecosystem analysis with
  raw-capture preservation, feature-family extraction, temporal comparison,
  calibrated competing explanations, dependency collapse, falsification, and
  canonical relationship persistence.
- `bleradar-core::fusion` — bounded calibrated evidence scoring, dependency
  collapse, competing-hypothesis fusion, and adversarial falsification.
- `bleradar-core::verification` — required-semantics contracts, metamorphic
  relations, baseline/candidate differential comparison, failure minimization,
  repair records, regression locks, and family-yield feedback.
- `bleradar-core::geo` — validated coordinates, haversine distance, bearing.
- `bleradar-core::identity` — canonical MAC handling, randomized/private address classification, conservative identity evidence.
- `bleradar-core::signal` — deterministic EMA filtering, hot/cold trend, coarse proximity bands, calibrated BLE range estimation.
- `bleradar-core::tracking` — observations, selected-device lock state, histories, confidence, map points, GPS uncertainty and conservative spatial estimates.
- `bleradar-compat` — complete ABI implementation/reachability/evidence census plus a source-parity registry distinguishing differential proof, source analogues, oracle-only behavior and blocked contracts.
- root APK and retained migration archive — immutable oracle, including the archived extracted DEX and native core.
- `xtask/` — dependency-free Rust-native APK/native inventory, parity-coverage generation, and gate runner (supersedes the former `tools/*.py`).

## Material improvements over the initial reconstruction

1. Replaced the monolithic helper file with explicit domain modules.
2. Added persistent selected-device tracking state.
3. Added timestamp-ordered device observation history.
4. Added validation for non-finite RSSI, invalid GPS accuracy and time reversal.
5. Added GPS uncertainty to map observations rather than treating coordinates as exact.
6. Added explicit `Observed | Inferred | Predicted` evidence classes for visual layers.
7. Added conservative weighted spatial-region estimation using signal and GPS accuracy.
8. Added randomized/private MAC classification so rotating addresses are not silently treated as stable identities.
9. Added calibrated BLE log-distance estimation while documenting that it is not exact ranging.
10. Expanded regression coverage for identity, mapping, tracking, filtering, proximity and selection state.
11. Replaced flat ABI-name inventory semantics with a parity-status registry.
12. Added a generated parity coverage report to make the remaining migration frontier measurable.
13. Added bounded calibrated evidence fusion with explicit quality dimensions,
    dependent-source collapse, and adversarial support-removal checks.
14. Added metamorphic and differential verification with observable-surface
    comparison, failure minimization, explicit repair/regression state, and
    transformation-family feedback.
15. Added evaluation-gated software advancement that ranks changes by explicit
    benefit/confidence/reachability/reversibility factors and rejects candidates
    without verified semantics, measurable improvement, falsification resistance,
    reproducibility, or explained regression behavior.
16. Added execution-feedback adaptive OSINT search with eleven query
    representations, explicit outcome classification, feedback-driven frontier
    ranking, duplicate-pivot suppression, and transactional evidence/action
    persistence.
17. Added temporal metamorphic infrastructure correlation across eleven
    infrastructure observation families, with raw/normalized preservation,
    temporal intervals, source-dependency collapse, calibrated competing
    explanations, adversarial falsification, and canonical relationship edges.
18. Added website lineage and ecosystem analysis across twelve feature
    families, preserving raw HTML and public inputs, distinguishing content,
    platform, asset, development, and operational explanations, collapsing
    provider-dependent support, and preventing website similarity from being
    represented as proof of common operator control.
19. Reconstructed the actual material runtime paths, classified all 124 native
    ABI contracts, froze sampled oracle semantics and known gaps in executable
    tests, and assigned a single Rust-first target owner to every core
    responsibility.

## Functional parity status

- Native Rust core: original compiled artifact preserved exactly as oracle.
- Pure geometry/address utilities: available as safe-Rust source analogues, but
  not labeled parity where sampled oracle behavior differs.
- Device-centric tracking/map domain: implemented as an enhancement layer; not falsely labeled exact legacy parity where coefficients or UI semantics are unknown.
- Generated Kotlin bridge: all 124 function/method/constructor contracts are
  classified; exact private record layouts are not guessed.
- Android Compose UI / BLE lifecycle / permission timing: material call paths
  are statically mapped, but exact behavior still requires Android traces.

See `docs/PARITY_COVERAGE.md` for the current semantic frontier.

## Security/dependencies

The reconstructed Rust workspace has no third-party crate dependencies. It therefore introduces no crates.io dependency graph. `cargo audit` and `cargo deny` have been run (2026-08-31) fully offline against a vendored RustSec advisory database and are green with zero findings. The legacy Android APK dependency graph cannot be reconstructed completely from the binary metadata alone.

## Credentials

No populated user/API credential values were found in the supplied APK. No values were invented or imported from unrelated sources. See `docs/RETENTION_MANIFEST.md`.

## Verification status

Package-level verification performed at packaging time:
- original oracle files retained and checksummed;
- parity report generation executes successfully under `cargo xtask parity-report`;
- Git recovery history regenerated and tagged at the critical enhancement point;
- ZIP is re-extracted and all packaged SHA-256 entries are checked before delivery.

Execution gates, observed green on 2026-08-28 (Linux x86_64, pinned rustc/cargo 1.98.0) and enforced continuously by `.github/workflows/gates.yml`, now run as one command (`cargo xtask gates`):
- `cargo fmt --all --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo build --workspace --locked`;
- `cargo test --workspace --locked`;
- `cargo xtask parity-report` followed by a drift check against the committed `docs/PARITY_COVERAGE.md`;
- `cargo audit` / `cargo deny`, fully offline against the vendored RustSec advisory database (`vendor/rustsec-advisory-db/`) — closed 2026-08-31, see `docs/AUTONOMOUS_DECISIONS.md` #28.

Still blocked, not reported green:
- all Android-target execution (see MIG-003).

## Known risks

The largest remaining risk is semantic overreach. The 124-contract surface is
fully registered, but registration and Rust-native oracle implementation do
not prove that workspace source preserves behavior. DEX-owned control logic,
competing state, and 27 unknown-reachability contracts remain explicit
migration defects/unknowns.

## Recommended continuation

On an ARM64 Android/Bionic host, first run the standard gates, then execute the
differential harness around the immutable native oracle. Promote a source
contract to `DifferentiallyVerified` only after generated-input comparisons
prove inputs, outputs, errors, state, persistence, side effects, ordering,
resources, and termination. Follow the removal gates in
`BEHAVIORAL_CONTRACT.md`; never leave parallel live writers or schedulers.
