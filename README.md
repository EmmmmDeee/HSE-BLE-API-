# BLE Radar binary-grounded Rust migration workspace

This archive is the auditable reconstruction produced from the supplied BLE Radar v0.3.0 APK.

Start with `docs/FINAL_REPORT.md`, `docs/ISSUE_LEDGER.md`, and `docs/EXCEPTION_LEDGER.md`.

## Standard gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo audit
```

The workspace has no third-party Rust dependencies. `cargo audit` must still be installed separately on a verification host because it is a Cargo subcommand, not part of the Rust standard toolchain.

## Oracle preservation

`oracle/BLE-Radar-v0.3.0-original.apk`, `oracle/classes.dex`, and `oracle/libbleradar_core.so` are immutable inputs retained for differential testing and auditability.
