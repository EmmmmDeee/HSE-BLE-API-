# Executed-oracle differential verification

This document records how the immutable v0.3.0 native oracle is *executed* — not
merely disassembled or modeled by hand — so the safe-Rust reconstruction can be
proved equivalent to it contract by contract, and how to reproduce that proof.

## Why this exists

`crates/bleradar-compat` classifies every reconstructed contract with a
`ParityStatus`. Most pure contracts were `SourceAnalog`: a source analogue
existed and matched a **hand-reconstructed model** of the oracle
(`tests/oracle_characterization.rs::oracle_wifi_*`), but the oracle binary
itself had never been run, because it is an `aarch64` Android library and the
sandbox has no device or emulator (`docs/ISSUE_LEDGER.md` MIG-001/002/003 —
`/dev/kvm` absent, no ARM64/Bionic host).

That last gap is now closed for pure contracts. The oracle is a UniFFI library
whose scaffolding exports (`uniffi_bleradar_core_fn_func_*`) are plain C-ABI
functions — no JVM, no `JNIEnv` — and it needs only `libc`/`libm`/`libdl`. It
can therefore be run under **`qemu-aarch64` user-mode emulation** against a real
Android **Bionic** runtime, and its outputs read back through the UniFFI ABI.

## What is verified

The WiFi channel↔frequency contracts (bit-exact, over a comprehensive input
sweep) and the pure geodesy contracts (physical-tolerance, over a 308-pair
sweep):

| Contract | Oracle signature (from `UNIFFI_META_*`) | Result |
| --- | --- | --- |
| `wifi_channel_to_frequency` | `(i32) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain (bit-exact → `DifferentiallyVerified`) |
| `wifi_frequency_to_channel` | `(Option<i32>) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain, incl. the 6 GHz band (bit-exact → `DifferentiallyVerified`) |
| `haversine_m` | `(f64,f64,f64,f64) -> f64` | source matches the executed oracle to <1e-6 m over 308 pairs (transcendental libm rounding; `SourceAnalog` coverage) |
| `bearing_deg` | `(f64,f64,f64,f64) -> f64` | source matches the executed oracle to <1e-8 deg (circular) over 308 pairs (transcendental libm rounding; `SourceAnalog` coverage) |

The one intentional divergence is domain width: the oracle accepts signed `i32`
(and an `Option` frequency), the reconstruction a narrower `u16`. Outside the
`u16` domain the executed oracle returns `None`, which the differential asserts
explicitly. Both contracts are consequently `ParityStatus::DifferentiallyVerified`.

The pure geodesy contracts `haversine_m` and `bearing_deg` are also
executed-oracle differentials, over 308 coordinate pairs (edge cases + a
fixed-seed sweep), but with a **physical tolerance** rather than bit-exactness:
Bionic `libm` and the host `libm` are not bit-identical for transcendentals
(`sin`/`cos`/`asin`/`atan2`), so the reconstruction matches the executed oracle
to under a micrometre (haversine) and under a nanodegree (bearing, circular) —
observed maxima ≈ 11 nm and ≈ 1e-13°, the latter amplified only at
ill-conditioned near-antipodal pairs. That is broad differential COVERAGE (it
would catch a changed Earth-radius constant or a reworked formula, which move by
metres/degrees), not a bit-exact promotion, so both stay `SourceAnalog`. The
tolerance is far tighter than any real change yet absorbs last-bit `libm`
divergence, and is robust across host `libm` versions (a degree/metre bound, not
a bit bound).

## Two tiers

1. **Baked ground truth + CI test (always runs).**
   `cargo xtask oracle-differential` writes each executed-oracle result to
   `crates/bleradar-compat/tests/oracle/wifi_executed_vectors.tsv` (committed,
   with the provenance header below). `tests/oracle_differential.rs` replays
   that file against the reconstruction with no emulator, so it runs in ordinary
   CI (`cargo test` / `cargo xtask gates`).

2. **Live regeneration + drift gate (needs the runtime).**
   `cargo xtask oracle-differential` re-extracts the oracle, rebuilds the
   harness, re-executes it under qemu, and fails if the output differs from the
   committed vectors. It is a live command like `verify-android-live` and is not
   part of `gates`.

## Provenance

- oracle `oracle/libbleradar_core.so` SHA-256:
  `d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d`
  (from the migration archive, itself pinned by `check-oracle-integrity`).
- Bionic runtime: `system-images;android-24;default;arm64-v8a`
  (`Android/sdk_phone_arm64/generic_arm64:7.0/NYC/8695085`).
- CPU emulator: `qemu-aarch64` 8.2.2 (user-mode).
- Toolchain: Android NDK 27.3.13750724 (`aarch64-linux-android24-clang`).

## Reproducing

Prerequisites (external, like the SDK/NDK that `build-apk` needs):

```sh
# 1. qemu user-mode aarch64
apt-get install -y qemu-user-static

# 2. an aarch64 Bionic runtime (a system image; ~300 MB)
sdkmanager "system-images;android-24;default;arm64-v8a"

# 3. point xtask at the SDK (holds the NDK and the system image)
export ANDROID_HOME=/path/to/android-sdk
```

Then, from the repo root:

```sh
cargo xtask oracle-differential
```

The command:

1. extracts `oracle/libbleradar_core.so` from the migration archive (via `jar`)
   and refuses to continue unless its SHA-256 matches the pin above;
2. extracts `linker64` + `lib{c,m,dl,c++}.so` from the system image with
   `debugfs` into a temporary Bionic sysroot (no root, no loopback mount) —
   or uses `BIONIC_SYSROOT` if you exported a prepared one;
3. compiles each committed harness (`xtask/src/oracle_harness.c` for WiFi,
   `xtask/src/oracle_harness_geo.c` for geodesy) for `aarch64` with the NDK,
   linking the oracle;
4. runs each under `qemu-aarch64 -L <sysroot>`;
5. compares the output to the committed vectors (`wifi_executed_vectors.tsv`,
   `geodesy_executed_vectors.tsv`) and fails on any drift.

To regenerate the committed vectors after an intentional sweep change, run the
harness the same way and replace the data rows in the `.tsv` (keep the header).

## Falsification

- Mutating the reconstruction (e.g. `wifi_channel_to_frequency(14)` → `2485`)
  makes `tests/oracle_differential.rs` fail with
  `source Some(2485) != executed oracle Some(2484)`.
- Tampering a committed vector makes both the CI test fail and
  `cargo xtask oracle-differential` report the first drifting row (the executed
  oracle disagrees with the tampered value).
