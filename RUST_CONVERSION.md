# Rust Conversion Analysis

The repository contains two distinct systems:

1. the shipped Android APK, whose live topology is obfuscated DEX → generated
   UniFFI/JNA → an ARM64 Rust native core; and
2. a portable Rust reconstruction used as a migration foundation, which is not
   built into or called by the APK.

Language is therefore not proof of replacement. The question is which
implementation actually owns each reachable responsibility and whether the
workspace implementation preserves the shipped behavioral contract.

## Current architecture

- **Shipped runtime:** Android `Application`, `MainActivity`, and
  `BleScanService`; platform callbacks; generated binding code; and
  `libbleradar_core.so`.
- **Shipped control split:** DEX owns lifecycle, callbacks, parsing,
  normalization, scheduling, candidate generation, UI projection, and several
  fallbacks. Rust owns every one of the 124 exported native ABI implementations
  and `RadarStore`.
- **Reconstructed workspace:** dependency-free safe Rust libraries plus
  `xtask`; no Android application source or build project links them to the
  shipped app.
- **Immutable oracle:** the root APK and retained migration archive are
  integrity-gated. Extracted DEX/native artifacts in the archive support
  analysis but are not a substitute for running on Android/Bionic.

The machine-readable census in `bleradar-compat` classifies all **124**
contracts: 41 `VERIFIED_RUNTIME`, 78 `STATICALLY_REACHABLE`, and 5 `UNKNOWN`;
all 124 shipped implementations are `RUST_NATIVE`. The separate
source-replacement registry has 0 `DifferentiallyVerified`, 6 `SourceAnalog`,
5 `OracleOnly`, and 7 `Blocked` entries. This distinction prevents “written in
Rust” from being confused with “verified replacement.”

## Authoritative analysis

Read these as one migration specification:

1. `docs/VERIFIED_RUNTIME_TOPOLOGY.md` — path-level execution, I/O, state,
   process/FFI, recursion, fallback, and termination map;
2. `docs/BEHAVIORAL_CONTRACT.md` — required semantics, sampled executable
   fixtures, unknowns, confirmed gaps, and removal conditions;
3. `docs/RUST_TARGET_ARCHITECTURE.md` — one Rust owner per core
   responsibility and the minimum external Android boundary;
4. `docs/PARITY_COVERAGE.md` — generated census totals.

## Rust-first boundary

Rust must own application/domain policy, parsing, normalization, scheduling,
dispatch, state, persistence formats and migration, eligibility, ranking,
analysis, network planning, retries, rate limits, caching, recursion,
presentation decisions, health, and termination.

The justified non-Rust remainder is limited to:

- Android-instantiated component classes and callback interfaces;
- execution of Android-only API objects/handles;
- Compose rendering, accessibility, and navigation mechanics;
- manifest/resources/Gradle/signing metadata;
- generated ABI marshalling.

These boundaries copy platform data into typed Rust events and execute
Rust-decided commands. They do not retain validation, policy, scheduling,
state, or success/failure decisions.

## Required characterization before replacement

QEMU compatibility probing supplied provisional fixed-environment fixtures for
stateless functions, including confirmed mismatches in haversine radius,
proximity input semantics, BLE range, and 6 GHz Wi-Fi channels. It is not
authoritative for allocator-, thread-, persistence-, lifecycle-, callback-, or
network-dependent behavior.

A real ARM64 Android/Bionic harness must record:

- all service intents, restart and cleanup paths;
- callback races, duplicate delivery, permission denial, and cancellation;
- store transactions and snapshots under concurrent mutation;
- import/export/session/alias schemas and interrupted persistence;
- threat/correlation boundaries and failure fallbacks;
- GATT ordering/timeouts and OSINT request/retry/cache/rate-limit behavior;
- UI-visible ordering, empty/partial/error states, and termination.

Every fixture compares output, typed error, state mutation, side effect,
ordering where contractual, persistence/network observation, resource release,
and termination. Only that evidence can promote a source contract to
`DifferentiallyVerified`.

## Consolidation order

1. Complete the behavioral firewall and Android traces.
2. Introduce one Rust runtime/state facade and versioned generated FFI.
3. Move lifecycle scheduling and radio/location normalization, deleting each
   competing JVM mutation as its Rust owner becomes live.
4. Migrate aliases and sessions with rollback-safe schema conversion.
5. Move threat/correlation candidate generation and typed failures.
6. Move OSINT orchestration, GATT state, and presentation policy.
7. Remove compatibility exports only when each ledger row passes.

No step may leave two live writers, two schedulers, or a fallback that turns
failure into empty success. No retained non-Rust core logic is accepted without
a hard external blocker recorded in `docs/EXCEPTION_LEDGER.md`.
