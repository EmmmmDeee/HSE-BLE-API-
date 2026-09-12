# Cold-Start Verification

The deliverable is verified in two layers.

## Layer 1 — package integrity (executed here)

1. Create the final ZIP.
2. Extract it to a fresh directory.
3. Verify every `SHA256SUMS` entry.
4. Run `cargo xtask parity-report` from the extracted copy.
5. Confirm the Git history bundle lists the recovery tags and enhancement commit.

## Layer 2 — Rust execution gates (executed 2026-08-28)

Run from the clean extraction:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo xtask gates
```

Observed on Linux x86_64 with the pinned toolchain (rustc/cargo 1.98.0, per
`rust-toolchain.toml`): the four cargo gates exit 0. Test counts grow with the
workspace and are intentionally not re-asserted as a fixed number here (that
would itself drift silently); the authoritative count on any given commit is
whatever `cargo test --workspace --locked` and
`cargo test --manifest-path xtask/Cargo.toml --locked` report, re-proven on
every push and pull request by `.github/workflows/gates.yml`'s
`cargo xtask gates`, which runs both plus the parity-report drift check.

`cargo audit` and `cargo deny` now run fully offline against the vendored
RustSec advisory database (`vendor/rustsec-advisory-db/`, materialized into a
throwaway git commit by `cargo xtask vendor-advisory-db`) and are green: the
workspace's `Cargo.lock` contains zero third-party crates, so the advisory
surface is empty, and both tools confirm it rather than leaving the command
unrun. See `docs/AUTONOMOUS_DECISIONS.md` #28.

Since 2026-09-10 (`docs/AUTONOMOUS_DECISIONS.md` #54–#55) `cargo xtask gates`
also runs the JNI export-contract check (`cargo xtask check-jni-contract`:
every `static native` in `NativeRadar.java` ↔ exactly one `Java_*` export in
the host-built `libbleradar_jni.so`, no orphans; observed 21 ↔ 21, exact
match), and CI additionally runs `cargo xtask verify-jni-live` on a pinned
Temurin 21 JDK with `cargo-audit` 0.22.2 / `cargo-deny` 0.20.2 pinned and
cached. Observed on this host (rustc 1.98.0, OpenJDK 21.0.10): `gates` exit 0
in 11 s with warm caches; `verify-jni-live` failure path
`UnsatisfiedLinkError`, success path `linked-natives=25`, `abi=8`.
Re-observed 2026-09-12 after decision #86 added the device-map policy
natives: `check-jni-contract` 27 ↔ 27, `verify-jni-live` `linked-natives=27`,
`abi=9`.

## Layer 3 — reconstructed Android APK live build (executed 2026-09-09)

The hand-built radar app under `android/app/src/main` is packaged without
Gradle by the Rust-native runner:

```sh
cargo xtask build-apk
cargo xtask verify-android-live   # JNI dual-path + build-apk + entry/DEX/JNI gates
```

Live results, first on the 2026-09-09 host (Android SDK at
`/usr/local/lib/android/sdk`) and re-executed 2026-09-10 on a fresh host
(SDK installed to `/opt/android-sdk` from `commandlinetools-linux-11076708`,
build-tools 37.0.0, NDK 27.3.13750724, platform android-36, OpenJDK 21.0.10;
`verify-android-live` exit 0 in 23 s cold, 5 s warm), and again 2026-09-12
on this host from the decision #86 sources (exit 0; APK SHA-256
`e69204c7…60acfa`; the `android-apk` CI job now repeats this build on every
push and pull request):

| Check | Outcome |
|---|---|
| Cross-compile `bleradar-jni` → `aarch64-linux-android` release | **Validated** |
| aapt2 compile/link, javac, d8, zipalign, apksigner (v2+v3) | **Validated** |
| Output `HSE-BLE-Radar-arm64-v1.0.0.apk` | **Validated** (installable package artifact) |
| Required APK entries (manifest, classes.dex, arm64 `.so`, resources.arsc) | **Validated** |
| Required DEX classes (MainActivity, NativeRadar, RadarScanService, BleScanEngine) | **Validated** |
| JNI export contract derived from `NativeRadar.java` (`check-jni-contract`: every `static native` ↔ one `Java_com_hse_bleradar_NativeRadar_*` export, no orphans) | **Validated** (2026-09-11, 25 ↔ 25 against the then-committed APK's `lib/arm64-v8a/libbleradar_jni.so`, SHA-256 `29d89f14…e8de02`; 2026-09-12, 27 ↔ 27 against the regenerated APK's `.so`, SHA-256 `c9d9e99a…d4b84`) |
| `cargo xtask verify-jni-live` failure + success paths (host JVM) | **Validated** (abi=8, `linked-natives=25`, 2026-09-11; abi=9, `linked-natives=27`, 2026-09-12; run by CI) |
| On-device install / BLE scan / original-oracle differential | **Unverified** — no emulator, physical device, or original signing key (MIG-003) |

Package identity from live `aapt dump badging`:
`com.hse.bleradar` versionName `1.0.0`, minSdk 26, targetSdk 34, native-code
`arm64-v8a`, launchable `com.hse.bleradar.MainActivity`.

**Reproducibility note (live 2026-09-09):** two successive `cargo xtask build-apk`
runs produced **bit-identical** zip payloads for every entry
(`AndroidManifest.xml`, `classes.dex`, `lib/arm64-v8a/libbleradar_jni.so`,
resources, icons). The outer APK SHA-256 can still differ between builds
because the APK Signature Block embeds a wall-clock signing time that
`apksigner` does not fully pin even under `SOURCE_DATE_EPOCH`. Treat
per-entry content hashes (or `cargo xtask verify-android-live`) as the
authoritative completeness proof, not a single whole-file digest.

**Cross-host reproducibility (live 2026-09-10):** rebuilding the then-committed
APK on a different host reproduced `libbleradar_jni.so`
(SHA-256 `20e49084…d3aeb5`), the manifest, `resources.arsc`, and both icon
resources byte-for-byte; `classes.dex` differed only by JDK-version metadata
(JDK 21 `javac` emits `MethodParameters` attributes; identical class/method
inventory and instruction stream under `dexdump -d`). The DEX is reproducible
per JDK major version, so the JDK is pinned to 21 in CI and recorded in
`docs/ANDROID_APP.md`. The committed APK was then regenerated from the
COR-017/018/019 sources (decision #57): `classes.dex` 37,704 bytes,
`resources.arsc` 3,084 bytes, native library unchanged, whole file 360,898
bytes, signed with a fresh ephemeral debug identity as every rebuild is.
Regenerated again for `bleradar-core` 0.6.1 (decision #58): only the
`lib/arm64-v8a/libbleradar_jni.so` entry changed (318,536 → 318,504 bytes);
`classes.dex`, `resources.arsc`, the manifest, and both icons are byte-identical
to the previous build, and `verify-android-live` re-ran green. Rebuilt once more
from the `bleradar-core` 0.6.2 sources (decision #64, COR-024..026): every
entry, the native library included (SHA-256 `86103a9b…`, 318,504 bytes), is
byte-identical to the committed APK, because the JNI library does not link the
website or infrastructure engines and LTO strips them; the committed APK was
therefore left unchanged rather than re-signed for no content change.
Rebuilt again from the `bleradar-core` 0.6.3 sources (decision #66, COR-027):
this time the `lib/arm64-v8a/libbleradar_jni.so` entry differed (SHA-256
`e48ac57d…`, still 318,504 bytes) by 151 bytes confined to `.dynsym`,
`.dynstr`, `.rela.dyn` and the ELF headers — the address-normalized
disassembly (56,740 lines), the section sizes, the 21 `Java_*` exports and
the 47 imports are identical, so the change is symbol-table layout, not
code — and `verify-android-live` re-ran green. The committed APK was
regenerated so that a rebuild from the integrated sources reproduces its
native library byte-for-byte; `classes.dex`, `resources.arsc`, the manifest,
and both icons are byte-identical to the previous build. Rebuilt once more
after decision #69 (indexed pair matching in the correlation engines, which
the JNI library does not call): the native library grew by 32 bytes to
318,536 (SHA-256 `b6225924…`) with 73 of 56,739 address-normalized
disassembly lines differing — the size-optimising LTO build inlined shared
standard-library code slightly differently — while the 21 exports, the
manifest, `classes.dex` and `resources.arsc` are unchanged and
`verify-android-live` re-ran green; the committed APK was regenerated again
for the same reproducibility reason.

This layer proves the **reconstructed** APK builds and packages correctly. It
does **not** claim differential parity with
`BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` on Bionic.
