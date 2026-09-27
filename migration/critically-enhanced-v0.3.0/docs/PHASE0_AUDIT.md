# Phase 0 Audit and Execution Plan

## Authoritative input

The only authoritative project material supplied is the release APK `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk`, SHA-256 `dc1c5129625e53646f3cb644c6c8e060c4527d366aa1e284d0393284d467e255`.
No original Kotlin/Compose source tree, Gradle project, Rust source, Cargo manifests, signing key, source maps, or pre-obfuscation mapping were supplied.

## Binary inventory

- one Android binary manifest (`AndroidManifest.xml`)
- one DEX (`classes.dex`, R8/obfuscated application UI)
- one ARM64 Rust native core (`lib/arm64-v8a/libbleradar_core.so`), Android API 24 baseline
- JNA native dispatch (`libjnidispatch.so`)
- AndroidX graphics-path native support
- Jetpack Compose/AndroidX runtime metadata
- OpenStreetMap/osmdroid map implementation visible in DEX strings
- UniFFI/JNA bridge generated into the DEX for the Rust core

The Rust core exposes a large, named UniFFI ABI including `RadarStore`, BLE/Wi-Fi ingest, sessions, exports, geographic calculations, multilateration, radar points, threat assessment, correlation, GATT decoding, OSINT functions, formatting and permission policy. The complete observed native symbol census is `NATIVE_ABI.txt`.

## Dependency graph

Observed high-level graph:

`Android MainActivity/Compose UI`
→ `BLE scan foreground service + Android Bluetooth/Location APIs`
→ `generated UniFFI/JNA Kotlin bridge`
→ `libbleradar_core.so (Rust)`
→ `RadarStore / sessions / correlation / geo / export / OSINT domain`

Map UI additionally depends on osmdroid and Android location/network APIs.

## High-risk constructs

1. Android framework lifecycle and permission behavior — platform-specific and side-effectful.
2. BLE foreground scanning service — concurrency, callbacks, radio state and permission errors.
3. Generated UniFFI/JNA bridge — ABI/layout-sensitive.
4. R8-obfuscated Compose UI — names and some higher-level intent are destroyed.
5. Native Rust library is stripped — source-level control flow and private types cannot be reconstructed exactly.
6. Map networking/location — asynchronous external side effects.
7. Session persistence and exports — exact serialization/error behavior must be characterized before replacement.

No evidence of `eval`-style dynamic code generation was found in the supplied binary.

## Toolchain

Target reconstruction toolchain is pinned to Rust 1.98.0, edition 2024. The reconstructed crates intentionally use no crates.io dependencies, minimizing supply-chain exposure and allowing deterministic offline builds once the pinned Rust toolchain is present.

## Batch order

1. Preserve binary oracle and census ABI/classes/dependencies.
2. Reconstruct mathematically unambiguous pure functions and types.
3. Build compatibility inventory around the observed Rust ABI.
4. Characterize Android-facing behavior only where a runnable Android environment exists.
5. Replace platform UI/service behavior only after characterization tests exist.
6. Remediate ledger issues only after parity is demonstrated.

## Interop strategy

The original `libbleradar_core.so` is retained as a behavior oracle, not linked into the reconstructed portable crates. A future Android migration can run old and new implementations side-by-side behind a narrow adapter and differential-test each UniFFI contract before switching callers.

## Critical limitation

Strict whole-codebase functional parity cannot be proved from an obfuscated release APK alone. Compiled code is not an information-preserving representation of the original source. Unknown behavior is therefore logged as an exception instead of guessed.
