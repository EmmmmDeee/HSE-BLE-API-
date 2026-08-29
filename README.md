# HSE-BLE-API-

Huntsman's Radar (API)

An auditable Rust reconstruction produced from the supplied BLE Radar v0.3.0 APK. It preserves the original executable artifacts as immutable behavioral oracles and makes parity gaps explicit instead of guessing missing source behavior.

## Repository layout

- `crates/bleradar-core` — safe Rust geometry, identity, RSSI, proximity and device-tracking domain.
- `crates/bleradar-compat` — semantic parity registry for high-value observed native contracts.
- `tools/` — binary inventory, parity-report generation, and the dependency-policy and oracle-integrity gates.
- `docs/` — audit, issue/exception ledgers, parity frontier, verification record, and the autonomous-session operating documents.
- `benchmarks/` — benchmark harness notes.
- `RUST_CONVERSION.md` — analysis of what is already in safe Rust and the ranked plan for converting the remaining native/Android surface.
- `.github/workflows/gates.yml` — CI enforcement of every gate below.
- `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` — original APK oracle.
- `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip` — original migration archive; also retains the extracted native oracles (`oracle/libbleradar_core.so`, `oracle/classes.dex`) and the migration `git-history.bundle` for differential testing.

## High-value tracking capabilities represented in Rust

The core supports selected-device lock state, ordered observation histories, randomized-address classification, filtered RSSI/hot-cold trend, calibrated BLE distance estimates, coarse proximity bands, GPS uncertainty, confidence-scored observed map points, and a conservative weighted spatial-region estimate (spherical centroid, correct across the ±180° antimeridian).

`Observed`, `Inferred`, and `Predicted` are separate evidence classes by design.

## Requirements

- Rust toolchain **1.98.0** with `clippy` and `rustfmt` — pinned by `rust-toolchain.toml`; `rustup` installs it automatically on first `cargo` invocation in the repo.
- Python 3 (standard library only) for the `tools/` scripts.
- No third-party crates: the workspace is intentionally dependency-free, and CI fails if that changes without a recorded decision.

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
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

These gates run on every push and pull request via `.github/workflows/gates.yml`, together with a parity-report drift check, a zero-third-party-dependency policy check (`tools/check_dependency_policy.py`), and an oracle integrity check that verifies the retained APK and migration archive against their recorded SHA-256 values (`tools/check_oracle_integrity.py`). Autonomous maintenance sessions operate under `docs/AUTONOMOUS_ENGINE.md`.

## Parity report

```sh
python3 tools/parity_report.py
```

This regenerates `docs/PARITY_COVERAGE.md` from the packaged ABI census and semantic compatibility registry.

## Distribution packaging

Release archives are produced per `docs/PACKAGING_ASSISTANT.md`: all tracked project files (oracles included, since the integrity gate depends on them), excluding `.git/`, `target/`, and non-project local files; named `hse-ble-api-v<version>.zip` with a SHA-256 sidecar; verified by extracting to a clean directory and running every gate from the extraction before delivery.

## License

Proprietary (see the `license` field in `Cargo.toml`). All rights reserved by the project owner; the retained APK and native artifacts remain the property of their original rights holder and are included solely as behavioral verification oracles.

## Start here

Read, in order:

1. `docs/FINAL_REPORT.md`
2. `docs/ISSUE_LEDGER.md`
3. `docs/PARITY_COVERAGE.md`
4. `docs/EXCEPTION_LEDGER.md`
5. `docs/COLD_START_VERIFICATION.md`
6. `RUST_CONVERSION.md`
