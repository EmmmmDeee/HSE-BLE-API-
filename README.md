# HSE-BLE-API-

Huntsman's Radar (API)

An auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

## Repository layout

- `crates/bleradar-core` — safe Rust geometry, identity, RSSI, proximity and device-tracking domain.
- `crates/bleradar-compat` — semantic parity registry for high-value observed native contracts.
- `tools/` — binary inventory and parity-report generation.
- `docs/` — audit, issue/exception ledgers, parity frontier and verification record.
- `benchmarks/` — benchmark harness notes.
- `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` — original APK oracle.
- `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip` — original migration archive; also retains the extracted native oracles (`oracle/libbleradar_core.so`, `oracle/classes.dex`) and the migration `git-history.bundle` for differential testing.

## High-value tracking capabilities represented in Rust

The core supports selected-device lock state, ordered observation histories, randomized-address classification, filtered RSSI/hot-cold trend, calibrated BLE distance estimates, coarse proximity bands, GPS uncertainty, confidence-scored observed map points, and a conservative weighted spatial-region estimate.

`Observed`, `Inferred`, and `Predicted` are separate evidence classes by design.

## Standard gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

## Parity report

```sh
python tools/parity_report.py
```

This regenerates `docs/PARITY_COVERAGE.md` from the packaged ABI census and semantic compatibility registry.

## Start here

Read, in order:

1. `docs/FINAL_REPORT.md`
2. `docs/ISSUE_LEDGER.md`
3. `docs/PARITY_COVERAGE.md`
4. `docs/EXCEPTION_LEDGER.md`
5. `docs/COLD_START_VERIFICATION.md`
