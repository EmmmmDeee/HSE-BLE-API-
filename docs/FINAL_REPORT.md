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
- `bleradar-core::fusion` — bounded calibrated evidence scoring, dependency
  collapse, competing-hypothesis fusion, and adversarial falsification.
- `bleradar-core::verification` — required-semantics contracts, metamorphic
  relations, baseline/candidate differential comparison, failure minimization,
  repair records, regression locks, and family-yield feedback.
- `bleradar-core::geo` — validated coordinates, haversine distance, bearing.
- `bleradar-core::identity` — canonical MAC handling, randomized/private address classification, conservative identity evidence.
- `bleradar-core::signal` — deterministic EMA filtering, hot/cold trend, coarse proximity bands, calibrated BLE range estimation.
- `bleradar-core::tracking` — observations, selected-device lock state, histories, confidence, map points, GPS uncertainty and conservative spatial estimates.
- `bleradar-compat` — semantic parity-status registry distinguishing reconstructed, oracle-only and blocked contracts.
- `oracle/` — immutable original APK, DEX and native core.
- `tools/` — repeatable APK/native inventory and parity-coverage generation.

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

## Functional parity status

- Native Rust core: original compiled artifact preserved exactly as oracle.
- High-confidence pure geometry/address utilities: reconstructed in safe Rust.
- Device-centric tracking/map domain: implemented as an enhancement layer; not falsely labeled exact legacy parity where coefficients or UI semantics are unknown.
- Generated Kotlin bridge: contract names inventoried; exact private record layouts are not guessed.
- Android Compose UI / BLE lifecycle / permission timing: cannot be proven source-identical from R8 output alone.

See `docs/PARITY_COVERAGE.md` for the current semantic frontier.

## Security/dependencies

The reconstructed Rust workspace has no third-party crate dependencies. It therefore introduces no crates.io dependency graph. A real `cargo audit` invocation still requires a host with Cargo and cargo-audit installed. The legacy Android APK dependency graph cannot be reconstructed completely from the binary metadata alone.

## Credentials

No populated user/API credential values were found in the supplied APK. No values were invented or imported from unrelated sources. See `docs/RETENTION_MANIFEST.md`.

## Verification status

Package-level verification performed at packaging time:
- original oracle files retained and checksummed;
- parity report generation executes successfully under Python;
- Git recovery history regenerated and tagged at the critical enhancement point;
- ZIP is re-extracted and all packaged SHA-256 entries are checked before delivery.

Execution gates, observed green on 2026-08-28 (Linux x86_64, pinned rustc/cargo 1.98.0) and enforced continuously by `.github/workflows/gates.yml`:
- `cargo fmt --all --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo build --workspace --locked`;
- `cargo test --workspace --locked`;
- `python3 tools/parity_report.py` followed by a drift check against the committed `docs/PARITY_COVERAGE.md`.

Still blocked, not reported green:
- `cargo audit` (cargo-audit binary unavailable; the lockfile currently contains zero third-party crates, so the advisory surface is empty);
- all Android-target execution (see MIG-003).

## Known risks

The largest remaining risk is semantic overreach. The original native binary exports a much larger UniFFI surface than can be faithfully reconstructed from symbol names alone. The package now makes that limitation mechanically visible instead of burying it in prose.

## Recommended continuation

On a Rust/Android-capable host, first run the standard gates, then build a differential harness around the immutable native oracle. Promote a contract from `Blocked` or `OracleOnly` to `Reconstructed` only after generated-input comparisons prove inputs, outputs, errors and side effects. Android service/UI parity should be frozen with instrumentation tests before replacing lifecycle-sensitive behavior.
