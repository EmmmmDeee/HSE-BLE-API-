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
every pull request and every push to `main` by `.github/workflows/gates.yml`'s
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
`abi=9`; and after decision #87 added the string bridge and the
manifest/artifact natives: 32 ↔ 32, `linked-natives=32`, `abi=11` (decision #96), with the
new `jni.h` slot gate reporting 235 `JNINativeInterface_` members in the JDK
21 header and all 4 `SLOT_*` constants matching. Decision #88 then executed
the crate's own test suite on the target: `cargo xtask verify-jni-target` →
3 executables, 36 tests passed on `aarch64-linux-android` under
`qemu-aarch64` against the android-24 Bionic runtime (25 s), and the
`android-apk` CI job now runs it with the ~3 MB extracted runtime cached.

## Layer 3 — reconstructed Android APK live build (executed 2026-09-09)

The hand-built radar app under `android/app/src/main` is packaged without
Gradle by the Rust-native runner:

```sh
cargo xtask build-apk
cargo xtask verify-android-live   # JNI dual-path + build-apk + entry/DEX/JNI gates
cargo xtask verify-android-emulator   # the committed APK on a headless API 34 emulator (KVM)
```

Live results, first on the 2026-09-09 host (Android SDK at
`/usr/local/lib/android/sdk`) and re-executed 2026-09-10 on a fresh host
(SDK installed to `/opt/android-sdk` from `commandlinetools-linux-11076708`,
build-tools 37.0.0, NDK 27.3.13750724, platform android-36, OpenJDK 21.0.10;
`verify-android-live` exit 0 in 23 s cold, 5 s warm), and again 2026-09-12
on this host from the decision #86 sources (exit 0; APK SHA-256
`e69204c7…60acfa`) and the decision #87 sources (exit 0; APK SHA-256
`3e3670fb…daa64`; the `android-apk` CI job now repeats this build on every
pull request and every push to `main`), and once more from the decision #89 sources (exit 0
in 12 s warm; APK SHA-256 `db4fb288…61b8`, 406,094 bytes, the first build to
carry `assets/`), then from the decision #90 sources (exit 0; APK SHA-256
`ad14aff1…536b`, 406,094 bytes; `classes.dex` 64,948 bytes; the pinned SDK
set chosen by xtask's discovery without a fallback notice), from the
decision #91 sources (exit 0; APK SHA-256 `3559634d…353e`, 406,094 bytes;
`classes.dex` 67,680 bytes with the server's seams and `Json` added), and
from the decision #92 sources (exit 0 in 11 s warm; APK SHA-256
`9e179cb1…db2f`, 406,094 bytes; `classes.dex` 70,468 bytes with scan
control added), from the decision #93 sources (exit 0; APK SHA-256
`f67958c8…afca`, 406,094 bytes; `classes.dex` 70,848 bytes with the
request-body cap and the idempotent accepted start), and — after the #94,
#95 and #96 regenerations — from the decision #97 sources (exit 0; APK
SHA-256 `f049dce0…3aff`, 406,094 bytes; `classes.dex` 75,864 bytes; every
entry stored at the ZIP epoch, the committed package's 8 entries reproduced
by the fresh build with equal sizes and CRC-32s, the built version read back
as `versionCode 1, versionName 1.0.0`), and from the decision #98 sources
(the first run refused the #97 package — `classes.dex: size 75864 → 75956,
crc32 67ee51d9 → db6dd772`, the entries gate's first catch of a real source
change, `ReleaseManifest` now logging through `java.util.logging` — and the
second run passed on the rebuilt package: exit 0; APK SHA-256
`cc182b57…cbba`, 406,094 bytes; `classes.dex` 75,956 bytes; native library
unchanged):

| Check | Outcome |
|---|---|
| Cross-compile `bleradar-jni` → `aarch64-linux-android` release | **Validated** |
| aapt2 compile/link (with `-A assets/`), javac, d8, zipalign, apksigner (v2+v3) | **Validated** |
| Output `HSE-BLE-Radar-arm64-v1.0.0.apk` | **Validated** (installable package artifact) |
| Required APK entries (manifest, classes.dex, arm64 `.so`, resources.arsc, `assets/dashboard.html`, `assets/release_manifest.txt`) | **Validated** (2026-09-12; the two asset entries failed on the pre-#89 pipeline, which never packaged `assets/` — COR-029) |
| Required DEX classes (MainActivity, NativeRadar, RadarScanService, BleScanEngine, ApiHttpServer) | **Validated** |
| Android lint `NewApi`: no `java.*`/`android.*` call newer than `minSdkVersion` 26 | **Validated** (2026-09-12, "No issues found."; the pre-#89 sources failed on `InputStream#readAllBytes` at API 33 — COR-028) |
| The web dashboard renders live data in headless Chromium against a mock of the JSON contract (`cargo xtask verify-dashboard-live`) | **Validated** (2026-09-12, Chromium 141: healthy `polls=4`, every device in order and escaped; a `500` or wrong-shape `/api/devices`, a wrong-shape `/api/status` and a `500` `/api/updates` each show the banner (five scenarios); run by the `web-dashboard` CI job) |
| The app's real `ApiHttpServer` on the host JVM: 27 real HTTP requests answered as documented — the three JSON documents byte-identical to the browser fixtures; scan control through a scripted `ScanControl` paused and resumed with every document reflecting it, refused four ways as `409`, a throwing control answered `500` with the server still up, a 64 KiB request body consumed before a clean close, a body declared beyond the 64 KiB cap refused with `413` at once — and the committed dashboard rendered from that server in headless Chromium with Start disabled and Stop offered; the real `ReleaseManifestSource` classifying nine scripted fetches as documented, each dispositioned by the Rust core (`cargo xtask verify-api-live`) | **Validated** (2026-09-13: 27 requests, `polls=4`, 3.0 s; 2026-09-14: the 9 manifest-source scenarios, decision #96; run by the `gates` CI job) |
| The app's unit tests (`BlipTest`, `ReleaseManifestTest`: 64 `@Test` methods) executed on the host JVM against the real native core through xtask's generated JUnit-subset runner, the test directory held to the listed classes (`cargo xtask verify-android-unit`) | **Validated** (2026-09-14, decision #98: "64 tests in 2 classes passed on the host JVM against the real native core" — the first execution of tests that had been dormant since they were written; falsified by a `getVersionName()` mutation — `FAIL ReleaseManifestTest#parses_valid_manifest: java.lang.AssertionError: version name expected:<1.0.1> but was:<1.0.1x>` —, by a `Blip.recordFilteredRssi` keeping non-finite samples (`FAIL BlipTest#record_filtered_rssi_ignores_invalid_values … expected:<0> but was:<3>`, exit 1), by the library path emptied (`native core=unavailable`, exit 3) and by a dormant `DormantTest.java` (refused by name); run by the `gates` CI job) |
| JNI export contract derived from `NativeRadar.java` (`check-jni-contract`: every `static native` ↔ one `Java_com_hse_bleradar_NativeRadar_*` export, no orphans) | **Validated** (2026-09-11, 25 ↔ 25 against the then-committed APK's `lib/arm64-v8a/libbleradar_jni.so`, SHA-256 `29d89f14…e8de02`; 2026-09-12, 27 ↔ 27 against the regenerated APK's `.so`, SHA-256 `c9d9e99a…d4b84`, then 31 ↔ 31 against the `.so` regenerated for decision #87, SHA-256 `e27e42ac…9253`; 2026-09-14, 32 ↔ 32 against the `.so` regenerated for decision #96, SHA-256 `2c62e06f…`) |
| `cargo xtask verify-jni-live` failure + success paths (host JVM) | **Validated** (abi=8, `linked-natives=25`, 2026-09-11; abi=9, `linked-natives=27`, then abi=10, `linked-natives=31` with the string bridge exercised through real Java strings, 2026-09-12; abi=11, `linked-natives=32` with the remote-manifest disposition surface, 2026-09-14; run by CI) |
| The committed APK on a real Android runtime — install, first launch, the native library loading (`native_available` true), the dashboard and the documents served, scan control, the foreground promotion with its notification, a virtual advertiser (a second controller on netsimd's HCI socket) listed by `/api/devices` with its Rust-computed row and pruned once gone, the pause that keeps the foreground, the sticky restart after `kill -9`, the first launch's update check fetching the repository's release manifest (the outcome reported) and finishing, the API ending with the app, a runtime permission revoked while scanning, no crash, no leaked `ServiceConnection`, then the app upgrading itself against a stand-in `github.com` the guest trusts — the production URL fetched, the remote manifest accepted, the download, the core's verification, the installer hand-off, the installer's `Update` tapped, the successor installed and running (`cargo xtask verify-android-emulator`: headless API 34 `google_apis` x86_64 emulator with ARM translation, on KVM; the upgrade packages from `cargo xtask build-update-proof`) | **Validated** (2026-09-13, decision #93: boot 42 s, the API 0.3 s after the launch, the page byte-identical (19,974 bytes), promotion 2.5 s after the start, the restarted service resumed the scan 2.2 s after the kill, no crash; 2 min 33 s in the `android-emulator` CI job. Decision #94: the update check reached its decision and its service finished; `pm revoke BLUETOOTH_SCAN` → the platform killed the app, the sticky restart found the permissions revoked and stopped the service, the API unreachable 22.8 s later; proof step 1 min 54 s. Decision #95: a virtual advertiser on netsimd's HCI socket (emulator package 37.1.11) listed 1.0 s after its start with the core's row — `rssi_dbm` 20.0, `distance_m` 1.12e-4, `IMMEDIATE`, `STABLE`, `LIVE`, confidence 85 — and pruned 30.2 s after its removal; no leaked `ServiceConnection`; proof step 2 min 12 s, job 4 min 00 s. Decision #96 (2026-09-14): the first launch's update check fetched the repository's release manifest URL on the runtime — `HTTP 404 -> using the bundled manifest; no retry` — and finished; proof step 1 min 58 s, job 3 min 32 s. Decision #99 (2026-09-14, run 171, the first end-to-end run): the trust anchor `7c1c59e8.0` listed by the shadowed store, `remote manifest HTTP 200, 311 bytes -> using the remote manifest` on the production URL, decision 1, download 13 verified by the core and handed to the installer 5.8 s after the launch, `PackageInstallerActivity` in front with `Update`, versionCode 2 installed 2.5 s after the tap, the successor's API up 0.1 s after its launch with the native core; run 172, green: handed to the installer 2.5 s after the launch, installed 2.5 s after the tap, the upgrade phase 21 s, job 3 min 31 s) |
| The app version has one authority (`APP_VERSION_CODE`/`APP_VERSION_NAME` in `xtask/src/main.rs`): `cargo xtask check-app-version` (a `gates` step) requires the bundled `release_manifest.txt` and the committed APK's name to repeat it, `verify-android-live` reads the built package's version back from `aapt2 dump badging` and requires the committed APK's entries (name, size, CRC-32) to reproduce from the sources; `cargo xtask release-manifest` writes the manifest a release publishes (the committed APK's exact size and SHA-256), which `verify-api-live` feeds to the real core as its accepted manifest | **Validated** (2026-09-14, decision #97: "app version 1 (1.0.0): the bundled release manifest and the committed APK's name agree"; "versionCode 1, versionName 1.0.0"; "committed APK reproduced: 8 entries with equal sizes and CRC-32s"; two successive builds byte-identical, SHA-256 `f049dce0…3aff`; the generated manifest — `size_bytes = 406094`, `sha256 = f049dce0…` — accepted through the real `.so`; falsified by one changed Java string literal, `classes.dex: size 75864 → 75868, crc32 67ee51d9 → f9ed9f04`; run by the `gates` and `android-apk` CI jobs) |
| BLE scan of real advertisers on a physical radio / original-oracle differential on a physical device | **Unverified** — the scan → Rust row path is verified with a virtual advertiser on the emulator (decision #95) and the update download/verification/install path against a stand-in release host there (decision #99); a radio's RSSI physics and the original signing key need a physical device and the key (MIG-003, MIG-002) |

Package identity from live `aapt dump badging`:
`com.hse.bleradar` versionName `1.0.0`, minSdk 26, targetSdk 34, native-code
`arm64-v8a`, launchable `com.hse.bleradar.MainActivity`.

**Reproducibility note (live 2026-09-09; cause found and removed 2026-09-14):**
two successive `cargo xtask build-apk` runs produced **bit-identical** zip
payloads for every entry (`AndroidManifest.xml`, `classes.dex`,
`lib/arm64-v8a/libbleradar_jni.so`, resources, icons) while the outer APK
SHA-256 still differed, which this note had attributed to a wall-clock
signing time in the APK Signature Block. Decision #97 read the two files
entry by entry and found the actual cause: `build-apk` staged `classes.dex`
and the native library as fresh copies carrying the build's mtime, which
`zip` stored in those two entries (`2026-09-14 09:17:42` in the #96 build),
while every entry `aapt2 link` wrote already sat at the ZIP epoch. Both
staged files are now set to `1980-01-01 00:00:00` and `zip` runs under
`TZ=UTC`, and two successive builds are **byte-identical, signature block
included** (SHA-256 `f049dce0…3aff`, 2026-09-14). `cargo xtask
verify-android-live` requires the fresh build's entries (name, size, CRC-32;
`META-INF/` aside, since the debug key is per machine) to equal the committed
APK's, so a committed package that no longer matches the sources fails CI
naming the entry — falsified by one changed Java string literal
(`classes.dex: size 75864 → 75868, crc32 67ee51d9 → f9ed9f04`). A rebuild on
another machine still differs in `META-INF/` (its own debug key, MIG-002);
the whole-file digest a release manifest carries is therefore the digest of
the committed bytes (`cargo xtask release-manifest`), and reproduction is
checked entry by entry.

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
