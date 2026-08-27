# BLE Radar — Binary-Grounded Rust Migration (Enhanced)

This archive is an auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

## Workspace

- `crates/bleradar-core` — safe Rust geometry, identity, RSSI, proximity and device-tracking domain.
- `crates/bleradar-compat` — semantic parity registry for high-value observed native contracts.
- `oracle/` — original APK, DEX and ARM64 Rust core for differential testing.
- `tools/` — binary inventory and parity-report generation.
- `docs/` — audit, issue/exception ledgers, parity frontier and verification record.

## High-value tracking capabilities represented in Rust

The enhanced core supports selected-device lock state, ordered observation histories, randomized-address classification, filtered RSSI/hot-cold trend, calibrated BLE distance estimates, coarse proximity bands, GPS uncertainty, confidence-scored observed map points, and a conservative weighted spatial-region estimate.

`Observed`, `Inferred`, and `Predicted` are separate evidence classes by design.

## Standard gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo audit
```

The preparation host did not contain Cargo/Rust, so these commands are documented but not falsely reported as passed. See `docs/COLD_START_VERIFICATION.md`.

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
