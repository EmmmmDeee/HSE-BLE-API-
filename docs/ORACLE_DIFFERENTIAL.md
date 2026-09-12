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

Every pure oracle contract that has a reconstruction analogue — the WiFi
channel↔frequency, band, security and is_enterprise contracts (bit-exact), the
geodesy and `wifi_distance` contracts (transcendental tolerance), and the BLE
signal contracts (verified formula with locked divergences):

| Contract | Oracle signature (from `UNIFFI_META_*`) | Result |
| --- | --- | --- |
| `wifi_channel_to_frequency` | `(i32) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain (bit-exact → `DifferentiallyVerified`) |
| `wifi_frequency_to_channel` | `(Option<i32>) -> Option<i32>` | source reproduces every executed-oracle output over its `u16` domain, incl. the 6 GHz band (bit-exact → `DifferentiallyVerified`) |
| `wifi_band` | `(Option<i32>) -> String` | reconstructed from the executed oracle (previously unmapped); source `wifi_band(u16)` splits at 3000/5900 MHz and matches every executed-oracle band over its `u16` domain (bit-exact → `DifferentiallyVerified`) |
| `haversine_m` | `(f64,f64,f64,f64) -> f64` | source matches the executed oracle to <1e-6 m over 308 pairs (transcendental libm rounding; `SourceAnalog` coverage) |
| `bearing_deg` | `(f64,f64,f64,f64) -> f64` | source matches the executed oracle to <1e-8 deg (circular) over 308 pairs (transcendental libm rounding; `SourceAnalog` coverage) |
| `ble_distance` | `(i32, Option<i32>) -> f64` | source calibration formula matches the executed oracle to 2e-16 rel in the valid region; the oracle's `[0.1,100]` m clamp + `rssi>=0`→100 sentinel are documented, locked divergences (`SourceAnalog`) |
| `proximity_label` | `(f64) -> String` | oracle bands `<1.5`/`<5`/`<15` are wider than the source's `<=1`/`<=2`/`<=5`; the banding gap is documented and locked (`SourceAnalog`) |
| `wifi_distance` | `(i32, i32) -> f64` | reconstructed from the executed oracle (previously unmapped); `10^((47.55−20·log10(f)−rssi)/27)` with an `rssi>=0`→400 m sentinel, a `2000..=7199` MHz plausible-frequency window defaulting to 2437 MHz outside it, and a `[0.1,400]` m clamp; the source replicates all of it over the full `i32` domain (no behavioural or domain divergence) and matches the executed oracle to `<1e-6` relative — transcendental `log10`/`powf` rounding, observed max 2e-15; `SourceAnalog` like the geodesy contracts |
| `wifi_is_enterprise` | `(Option<String>) -> bool` | reconstructed from the executed oracle (previously unmapped); a case-sensitive `caps.contains("EAP")` test (`None` → false); pure/deterministic, reproduced bit-for-bit over the full `String` domain → `DifferentiallyVerified` |
| `wifi_security` | `(Option<String>) -> String` | reconstructed from the executed oracle (previously unmapped); case-sensitive substring precedence `SAE`\|`WPA3`→WPA3, `WPA2`\|`RSN`→WPA2, `OWE`→OWE, `WPA`→WPA, `WEP`→WEP, else Open (`None`→`"?"`); pure/deterministic, reproduced bit-for-bit over the full `String` domain → `DifferentiallyVerified` |

For the WiFi channel↔frequency contracts the one intentional divergence is domain
width: the oracle accepts signed `i32` (and an `Option` frequency), the
reconstruction a narrower `u16`. Outside the `u16` domain the executed oracle
returns `None`, which the differential asserts explicitly. Those contracts are
consequently `ParityStatus::DifferentiallyVerified`. (`wifi_distance` instead
takes the oracle's full `i32` domain, so it has no domain divergence — see its
row above.)

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
   `cargo xtask oracle-differential` writes each executed-oracle result to a
   committed vectors file under `crates/bleradar-compat/tests/oracle/`
   (`wifi_executed_vectors.tsv`, `geodesy_executed_vectors.tsv`,
   `signal_executed_vectors.tsv`, `wifi_distance_executed_vectors.tsv`,
   `wifi_security_executed_vectors.tsv`), each with the provenance header below.
   The matching CI tests (`oracle_differential.rs`,
   `oracle_geodesy_differential.rs`, `oracle_signal_differential.rs`,
   `oracle_wifi_distance_differential.rs`, `oracle_wifi_security_differential.rs`)
   replay those files against the reconstruction with no emulator, so they run
   in ordinary CI (`cargo test` / `cargo xtask gates`).

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

## The same runtime proves the reconstruction's own JNI crate on the target

Since decision #88 the extracted Bionic sysroot is also what
`cargo xtask verify-jni-target` runs the `bleradar-jni` test suite on:
cross-compiled for `aarch64-linux-android` with the NDK and executed under
`qemu-aarch64 -L <sysroot>` (the Rust test binaries need only `libc`, `libm`
and `libdl`). `cargo xtask prepare-bionic-sysroot <dir>` extracts the runtime
once (`linker64` + `lib{c,m,dl,c++}.so`, about 3 MB) for `BIONIC_SYSROOT`,
which is what CI caches instead of the 2.6 GB image.

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
   `oracle_harness_geo.c` for geodesy, `oracle_harness_signal.c` for BLE signal,
   `oracle_harness_wifi_distance.c` for wifi_distance,
   `oracle_harness_wifi_security.c` for wifi_security/is_enterprise) for
   `aarch64` with the NDK, linking the oracle;
4. runs each under `qemu-aarch64 -L <sysroot>`;
5. compares the output to the committed vectors (`wifi_executed_vectors.tsv`,
   `geodesy_executed_vectors.tsv`, `signal_executed_vectors.tsv`,
   `wifi_distance_executed_vectors.tsv`, `wifi_security_executed_vectors.tsv`)
   and fails on any drift.

To regenerate the committed vectors after an intentional sweep change, run the
harness the same way and replace the data rows in the `.tsv` (keep the header).

## Falsification

- Mutating the reconstruction (e.g. `wifi_channel_to_frequency(14)` → `2485`)
  makes `tests/oracle_differential.rs` fail with
  `source Some(2485) != executed oracle Some(2484)`.
- Tampering a committed vector makes both the CI test fail and
  `cargo xtask oracle-differential` report the first drifting row (the executed
  oracle disagrees with the tampered value).
- For `wifi_distance`, mutating the reconstruction's formula constant
  (`47.55`→`48.55`), frequency window (`2000`→`2001`) or default (`2437`→`2438`)
  each fails `tests/oracle_wifi_distance_differential.rs` (relative 0.089 /
  0.136 / 3e-4). Because its formula region is matched to a `1e-12` relative
  tolerance (transcendental `libm`), a *one-ULP* vector tamper is correctly
  absorbed by the CI test but still caught by the byte-exact xtask drift gate;
  a larger tamper fails both. The two tiers are complementary: the tolerance CI
  test catches real regressions, the exact drift gate guarantees the committed
  vectors are faithful to the oracle bit-for-bit.
- For `wifi_security`/`wifi_is_enterprise` (pure string classifiers, bit-exact,
  no tolerance), mutating the reconstruction — flipping the `"EAP"` substring,
  reordering the security precedence (e.g. testing `OWE` before `WPA2`/`RSN`, or
  dropping the `SAE`/`WPA3` precedence) — fails
  `tests/oracle_wifi_security_differential.rs` at a named capability string, and
  relabelling any committed row fails both the CI test and the xtask drift gate.
