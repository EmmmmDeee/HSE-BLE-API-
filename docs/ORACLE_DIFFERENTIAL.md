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

The WiFi channel↔frequency contracts, over a comprehensive input sweep:

| Contract | Oracle signature (from `UNIFFI_META_*`) | Result |
| --- | --- | --- |
| `wifi_channel_to_frequency` | `(i32) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain |
| `wifi_frequency_to_channel` | `(Option<i32>) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain, incl. the 6 GHz band |

The one intentional divergence is domain width: the oracle accepts signed `i32`
(and an `Option` frequency), the reconstruction a narrower `u16`. Outside the
`u16` domain the executed oracle returns `None`, which the differential asserts
explicitly. Both contracts are consequently `ParityStatus::DifferentiallyVerified`.

Floating-point geodesy contracts (`haversine_m`, `bearing_deg`) were also
executed and agree with the reconstruction to within 1–2 ULP, but Bionic `libm`
and the host `libm` are not bit-identical for transcendentals, so those remain
`SourceAnalog` (a bit-exact baked assertion would be non-portable). Extending
them would require an ULP-tolerant criterion.

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
3. compiles `xtask/src/oracle_harness.c` for `aarch64` with the NDK, linking the
   oracle;
4. runs it under `qemu-aarch64 -L <sysroot>`;
5. compares the output to `wifi_executed_vectors.tsv` and fails on any drift.

To regenerate the committed vectors after an intentional sweep change, run the
harness the same way and replace the data rows in the `.tsv` (keep the header).

## Falsification

- Mutating the reconstruction (e.g. `wifi_channel_to_frequency(14)` → `2485`)
  makes `tests/oracle_differential.rs` fail with
  `source Some(2485) != executed oracle Some(2484)`.
- Tampering a committed vector makes both the CI test fail and
  `cargo xtask oracle-differential` report the first drifting row (the executed
  oracle disagrees with the tampered value).
