# Benchmark harness

## Legacy (oracle) benchmarks

No trustworthy legacy source-level benchmark can be reconstructed from the
release APK alone. The original APK and native library are retained as
immutable oracles for on-device A/B measurement. Suggested critical paths:
scan ingest throughput, `ui_radar_points`, session serialization,
multilateration, and map overlay generation. No before/after numbers are
claimed without an executable Android test environment (MIG-003).

## Reconstructed hot path (host, reproducible)

`crates/bleradar-jni/examples/scan_result_cost.rs` measures the Rust-owned
math behind one BLE scan result in `BleScanEngine.recordResult`, which calls
eight `NativeRadar.tracking*` natives that each recompute the same
`tracking_snapshot`. It is dependency-free, deterministic (1024 rotating
inputs, best of 5 rounds × 10,000,000 iterations), compiled by every
`cargo clippy --all-targets` gate, and run with:

```sh
cargo run --release -p bleradar-jni --example scan_result_cost
```

Baseline observed 2026-09-10 (Linux x86_64 sandbox, 4 vCPUs, rustc 1.98.0,
`bleradar-core` 0.6.1; the JVM → native transition itself is excluded and is
roughly 0.1 µs per call on Android):

| Measurement | ns |
|---|---|
| `tracking_snapshot` (one call) | 129.2 per op |
| eight `tracking*` wrapper calls per scan result (the `BleScanEngine` pattern) | 1,075.5 per result |
| `filtered_rssi` (EMA only) | 8.6 per op |

Decision recorded in `docs/AUTONOMOUS_DECISIONS.md` #58: even with the JNI
transitions added, one scan result costs on the order of 2 µs of native
work, i.e. about 0.02 % of one core at 100 scan results per second, so
consolidating the eight calls into one packed JNI call is rejected as below
marginal value. Re-run the example before revisiting that decision; a
material change to these numbers, not intuition, is the trigger.

## Engine load cost (host, reproducible)

`crates/bleradar-core/examples/engine_load.rs` measures the per-operation cost
of the canonical-store engines as the store grows: `observe` and one
`correlate` for the website and infrastructure engines, and `execute_result`
for the OSINT engine, at 1,000 / 2,000 / 4,000 / 8,000 records. It is
dependency-free and compiled by every `cargo clippy --all-targets` gate:

```sh
cargo run --release -p bleradar-core --example engine_load
```

Observed 2026-09-10 (Linux x86_64 sandbox, rustc 1.98.0), before and after
`EvidenceStore::transaction` replaced the clone-per-operation pattern
(`docs/AUTONOMOUS_DECISIONS.md` #66, COR-027):

| records | website `observe` before | after | infrastructure `observe` after | OSINT `execute_result` before → after |
|---|---|---|---|---|
| 1,000 | 234 µs | 3.7 µs | 2.8 µs | 68 µs → 1.9 µs |
| 2,000 | 477 µs | 2.7 µs | 2.9 µs | — |
| 4,000 | 1.01 ms | 2.9 µs | 3.3 µs | 68 µs → 2.2 µs |
| 8,000 | 2.63 ms (21.0 s per load) | 3.2 µs (25.8 ms per load) | 3.5 µs | — |

The "after" cost is flat with store size, as the journal makes it. One
`correlate` over the two websites/nodes in that load (16,000 comparable pairs
at 8,000 records) takes 585 ms / 428 ms and is dominated by ranking, not
persistence.
