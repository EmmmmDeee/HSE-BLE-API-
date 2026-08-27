# Final Migration Report

## Executive status

This deliverable is a **binary-grounded Rust reconstruction and migration foundation**, not a truthful claim of complete functional-parity source recovery.

The shipped APK already contains a substantial Rust core. That core's public UniFFI surface has been preserved as an ABI census and the original binary is retained as an oracle. High-confidence pure behavior has been reconstructed into dependency-free, safe Rust with regression tests. The unrecoverable Android/Compose behavior and stripped private native behavior are explicitly captured in the exception and issue ledgers.

## Before architecture

Android Compose UI → Android BLE/location/map services → generated Kotlin UniFFI/JNA → shipped Rust `libbleradar_core.so`.

## After architecture represented here

Rust workspace:
- `bleradar-core`: safe reconstructed domain primitives.
- `bleradar-compat`: observed contract inventory.
- `oracle/`: original APK, DEX and Rust native core for differential testing.
- `tools/`: repeatable binary inventory/ABI extraction.

## Functional parity status

- Native Rust core: original compiled artifact preserved exactly as oracle.
- Reconstructed mathematical helpers: implemented and regression-tested in source.
- Generated Kotlin bridge: contract names inventoried; not falsely recreated without exact record layout evidence.
- Android UI, BLE service, map interactions: not source-recoverable to strict parity from R8 output alone.

## Security/dependencies

The new Rust source has no third-party crate dependencies. Therefore it adds no crates.io dependency advisory surface. A full `cargo audit` invocation could not be run on this host because Cargo is absent; the generated workspace itself has no external dependency graph to audit. The legacy APK's complete transitive dependency graph cannot be reconstructed from version marker files alone.

## Credentials

No populated user/API credential values were found. Generic token variable names/placeholders are documented in `RETENTION_MANIFEST.md`.

## Known risks

The principal risk is overclaiming parity from a binary-only input. This deliverable avoids that: every area whose behavior cannot be demonstrated is an explicit exception.

## Recommended continuation

Run the workspace on a host with Rust 1.98.0 and Android build tools; add a device/emulator differential harness that calls the original UniFFI functions and the reconstructed equivalents with generated inputs. Port contracts only when that harness demonstrates exact parity, then replace Android service/UI surfaces using instrumentation tests that freeze existing observable behavior.
