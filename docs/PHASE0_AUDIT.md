# Phase 0 Audit

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

Verified high-level shipped graph:

`BleRadarApp / MainActivity / BleScanService`
→ `Android radio, location, GATT, content and lifecycle callbacks`
→ `DEX parsing, normalization, scheduling, candidate generation and UI policy`
→ `generated UniFFI/JNA Kotlin bridge`
→ `libbleradar_core.so (Rust)`
→ `RadarStore / sessions / correlation / geo / export / OSINT`.

Map UI additionally depends on osmdroid and Android location/network APIs. The
DEX also maintains a competing device mirror and duplicate alias persistence;
not all state/control flows through `RadarStore`.

The reconstructed Cargo workspace is a separate source-only topology. Its
libraries are reached by tests/library callers and are not linked from the APK.
`xtask` is its only executable.

`crates/bleradar-compat` records the complete 124-entry ABI census. Ninety
contracts have non-generated DEX call-site evidence; 12 of those plus seven
additional pure exports have stronger instrumented-trace evidence, producing
the final buckets 19 `VERIFIED_RUNTIME`, 78 `STATICALLY_REACHABLE`, and 27
`UNKNOWN`. Every shipped ABI implementation is native Rust; that fact does not
prove parity for reconstructed functions with similar names.

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

## Phase 0 result

The verified path map and migration boundary are in
`VERIFIED_RUNTIME_TOPOLOGY.md`. Required observable semantics and removal gates
are in `BEHAVIORAL_CONTRACT.md`. Authoritative target ownership is in
`RUST_TARGET_ARCHITECTURE.md`.

Pure native functions were provisionally exercised under QEMU with
compatibility shims. Stateful results were discarded after allocator
incompatibility was observed. Real Android/Bionic characterization remains
required for state, persistence, lifecycle, callback, GATT, concurrency, and
network behavior.

## Interop strategy

The original `libbleradar_core.so` is retained as a behavior oracle, not linked
into the reconstructed portable crates. A differential harness may run isolated
old/new cases against the same fixture. The production migration must switch
one responsibility at a time and immediately remove its competing writer or
scheduler; it must not operate parallel live application cores.

## Critical limitation

Strict whole-codebase functional parity cannot be proved from an obfuscated
release APK alone. Compiled code is not an information-preserving
representation of the original source. Unknown behavior is classified and
blocks old-implementation removal rather than being guessed or treated as a
permanent non-Rust exception.
