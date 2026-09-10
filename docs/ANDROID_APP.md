# Android app design record (`android/app/src/main`)

This is the design record that the app's own sources cite
(`AndroidManifest.xml`, `res/values/colors.xml`, `MainActivity.java`,
`NativeRadar.java`, and `xtask/src/main.rs`). Every statement below names the
tree evidence it rests on, so the app can be extended without re-deriving the
decisions. It was cited from PR #13 (commit `ea842c1`) onward but never
committed on any branch until 2026-09-10 (`docs/AUTONOMOUS_DECISIONS.md`
decision #56), which is why the earlier Android decisions are recorded here
retroactively rather than in the append-only log's 2026-09-09 entries.

## What it is, and what it is not

- A from-scratch Android app, package `com.hse.bleradar`, that renders nearby
  BLE advertisers as radar blips. Every distance, range bound, proximity band,
  trend, confidence, and freshness value is computed by `bleradar-core`
  (`tracking_snapshot` and the signal helpers) through the `bleradar-jni`
  bridge; the Java side never reimplements the math
  (`BleScanEngine.java` class comment, `crates/bleradar-jni/src/lib.rs` module
  docs).
- It is **not** a rebuild of the retained oracle APK
  (`BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk`, package
  `com.huntsman.bleradar`). The oracle stays an immutable behavioral oracle
  (`docs/VERIFIED_RUNTIME_TOPOLOGY.md`, MIG-010); this app is the Rust-first
  adapter layer that `RUST_CONVERSION.md` calls for, scoped to BLE scanning.
- Ownership follows `docs/EXCEPTION_LEDGER.md` EXT-001..EXT-003: Java owns only
  the platform plumbing (scanner session, service lifecycle, permissions,
  rendering); Rust owns policy and math.

| Source | Role |
|---|---|
| `NativeRadar.java` | The only JNI façade: `static native` declarations, ordinal/sentinel constants, `EXPECTED_ABI_VERSION`, and a never-throwing `ensureLoaded()`. It is the single authority for the export contract enforced by `cargo xtask check-jni-contract`. |
| `BleScanEngine.java` | Owns the `BluetoothLeScanner` session and the address-keyed `Blip` map; feeds every scan result through the `tracking*` natives; prunes stale devices using the Rust freshness class. |
| `Blip.java` | One tracked device: last filtered RSSI, distance and bounds, proximity, trend, freshness, confidence, retained TX power, an 8-sample filtered-RSSI window for spread, and a stable per-address display angle (RSSI carries no bearing). |
| `RadarScanService.java` | Foreground service (`connectedDevice` type) that owns the single engine so scanning survives activity recreation; posts the oracle's own "BLE Radar is scanning" notification copy. |
| `MainActivity.java` | Binds to the service, requests permissions, renders the status line, toggle button, radar view, and device list on a 400 ms timer; UI built from `android.widget` views in code. |
| `RadarView.java` | Pure rendering of the snapshot: range rings with distance labels, a rotating sweep, and blips colour-coded by `NativeRadar.PROXIMITY_*`. |

## Why there is no Gradle project, AndroidX, or Compose

`cargo xtask build-apk` packages the app without Gradle: aapt2 compile/link
→ `javac -source 8 -target 8` → d8 → zip (the native library and
`resources.arsc` stored uncompressed) → `zipalign -p 4` → `apksigner`
(v2 + v3) with an ephemeral, non-secret debug keystore
(`xtask/src/main.rs::cmd_build_apk`). It needs only the Android SDK
build-tools, one platform `android.jar`, the NDK, and a JDK, all invoked as
plain external processes exactly like `cargo audit`/`cargo deny`.

Reasons, in priority order:

1. **No dependency resolution.** Gradle, AndroidX, and Compose require Maven
   resolution of hundreds of artifacts. The workspace policy is zero
   third-party code for everything shipped and Rust-native tooling for the
   rest (`docs/AUTONOMOUS_DECISIONS.md` #9, #25), and the autonomous build
   sandboxes had no Maven access. The app therefore uses only the platform
   `android.*` classes present in `android.jar`.
2. **Inspectable, reproducible artifacts.** With no build plugin in the loop,
   two successive builds produce bit-identical zip payloads for every entry
   (`docs/COLD_START_VERIFICATION.md`, layer 3); only the signature block's
   wall-clock time differs.
3. **A minimal resource pipeline.** The only resources are the adaptive
   launcher icon (`mipmap-anydpi-v26/ic_launcher.xml` +
   `drawable/ic_launcher_foreground.xml`), `values/strings.xml`, and
   `values/colors.xml`; there are no XML layouts, so `aapt2 link` generates
   only the `R.string`/`R.color`/`R.mipmap`/`R.drawable` glue and the UI is
   constructed in `MainActivity.onCreate`.

## Why `arm64-v8a` is the only shipped ABI

- The oracle itself ships exactly one ABI: `unzip -l` on the oracle APK lists
  native code only under `lib/arm64-v8a/` (`libbleradar_core.so`,
  `libjnidispatch.so`, `libandroidx.graphics.path.so`). The reconstruction
  targets the same hardware class.
- One cross-compile target (`aarch64-linux-android`, API 26 clang from the
  NDK) keeps `build-apk` deterministic and the package small. The committed
  `HSE-BLE-Radar-arm64-v1.0.0.apk` (360,898 bytes) holds 366,232 bytes
  uncompressed in six entries — `classes.dex` 37,704, `resources.arsc` 3,084,
  the manifest and two icon resources, and a 318,536-byte `libbleradar_jni.so`
  built under the root `Cargo.toml` release profile (`opt-level = "z"`, LTO,
  one codegen unit, `panic = "abort"`, stripped) — against the oracle's
  10,808,624-byte `libbleradar_core.so`.
- `android:extractNativeLibs="false"` plus the uncompressed, page-aligned
  library entry lets the loader map `libbleradar_jni.so` straight from the
  APK, so there is no extraction step at install time.
- On a device without `arm64-v8a`, `System.loadLibrary` fails inside
  `NativeRadar.ensureLoaded()`; the error is retained in `loadError()` and
  `isAvailable()` returns `false`. `BleScanEngine` then records raw RSSI with
  `NaN` distances and `PROXIMITY_FAR`, and every status line the activity
  renders goes through `MainActivity.setStatus`, which appends
  `status_native_unavailable` (naming the load error's class) so the degraded
  mode is never presented as a normal scan (COR-017).

## Permissions: the subset of the oracle's manifest this app uses

The oracle's binary manifest (string pool of the `AndroidManifest.xml` entry
of the retained APK) declares the permissions below. The rebuilt manifest keeps
exactly those the BLE-only scope exercises and omits the rest deliberately.

| Oracle `uses-permission` | Rebuilt | Reason |
|---|---|---|
| `BLUETOOTH`, `BLUETOOTH_ADMIN` | kept, `maxSdkVersion="30"` | Legacy scan permissions for API 26–30 only; superseded by the `BLUETOOTH_*` runtime permissions on API 31+. |
| `BLUETOOTH_SCAN` | kept, `usesPermissionFlags="neverForLocation"` | Scanning on API 31+. The flag asserts scan results are never used to derive physical location, which is true: the app has no location code. |
| `BLUETOOTH_CONNECT` | kept | Needed to read device names (`ScanResult.getDevice().getName()`) on API 31+. |
| `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION` | kept | Required by the platform for BLE scan results on API 26–30. |
| `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_CONNECTED_DEVICE` | kept | `RadarScanService` runs as a `connectedDevice` foreground service (API 34 requires the typed permission). |
| `POST_NOTIFICATIONS` | kept | The foreground-service notification on API 33+. |
| `WAKE_LOCK` | kept | Declared so the scan session can be held across screen-off in a future change; currently unused by code. |
| `FOREGROUND_SERVICE_LOCATION` | omitted | The service declares only the `connectedDevice` type; there is no location foreground use. |
| `INTERNET`, `ACCESS_NETWORK_STATE` | omitted | The app makes no network calls; OSINT/network features are out of the BLE-only scope. |
| `ACCESS_WIFI_STATE`, `CHANGE_WIFI_STATE`, `NEARBY_WIFI_DEVICES` | omitted | The reconstructed scan loop is BLE-only; the oracle's Wi-Fi scanning is not reproduced here. |
| `DUMP` | omitted | A signature/privileged debugging permission that a third-party app cannot be granted. |
| `com.huntsman.bleradar.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` | omitted | Generated by AndroidX, which this app does not use. |

`uses-feature`: the oracle declares `bluetooth_le`, `location.gps`,
`location.network`, `telephony`, and `wifi`; the rebuilt manifest requires
only `android.hardware.bluetooth_le`.

Runtime permission flow (`BleScanEngine.requiredPermissions()`): API 31+
requests `BLUETOOTH_SCAN`, `BLUETOOTH_CONNECT`, and `ACCESS_FINE_LOCATION`;
API 26–30 requests `ACCESS_FINE_LOCATION`. `MainActivity` additionally
requests `POST_NOTIFICATIONS` on API 33+. Because `BLUETOOTH_SCAN` carries
`neverForLocation`, the platform does not require location permission for
scanning on API 31+, so the runtime gate over-requests fine location there.
This is deliberately conservative until on-device behaviour can be observed
(MIG-003); it is a known over-request, not a correctness defect.

## Palette

`res/values/colors.xml` is extracted read-only from the oracle's compiled
launcher-icon vectors (`aapt2 dump xmltree` on its `res/E4.xml`/`res/BW.xml`,
see the file's own comment): background `#0F1419`, ring/sweep `#39D98A`,
alert blip `#FF7A45`, info blip `#4CC2FF`, primary text `#E7F3ED`, secondary
text `#7E9C90`, panel `#162026`. `drawable/ic_launcher_foreground.xml`
recreates the oracle's adaptive-icon foreground (three concentric rings, a
sweep needle, two accent blips). **Known duplication:** `MainActivity` and
`RadarView` also hard-code several of these values as `Color.parseColor`
literals instead of reading `R.color.*`; the resource file is the intended
authority.

## Lifecycle and restart behaviour

- `MainActivity.onCreate` calls `NativeRadar.ensureLoaded()`; `onStart` only
  **binds** to `RadarScanService` (`BIND_AUTO_CREATE`), which creates the
  service without promoting it, so nothing is shown in the notification shade
  while the app is idle. Rotation is handled through `configChanges` so the
  activity is not recreated.
- A scan request is the only thing that promotes the service. The Start
  action first checks `hasRequiredPermissions` and the adapter, then issues
  `startForegroundService` and `startScanning()`. `onStartCommand` therefore
  runs only after the Bluetooth runtime permissions were granted, which is
  what the `connectedDevice` foreground type requires on API 34+ before
  `startForeground` (COR-019); it starts the scan and promotes the service
  with the "BLE Radar is scanning" notification. If the scan cannot start
  (adapter off), it leaves the foreground again and stops itself so no
  misleading notification lingers.
- `START_STICKY` now means recovery: if the process is killed while scanning,
  the system restarts the service with a `null` intent, `onStartCommand` runs
  again, and scanning resumes with its notification (COR-018). A user Stop
  calls `stopForeground(STOP_FOREGROUND_REMOVE)` and `stopSelf()`, so a
  deliberate stop is never resurrected. A restart that finds the permissions
  revoked stops itself with `START_NOT_STICKY` instead of throwing.
- `refreshUiLoop` reflects the real engine state every 400 ms: if the scan
  ended without a toggle (scan failure, or a restart that could not resume),
  the button and status return to idle.
- Stale devices are pruned by the Rust `FreshnessClass` derived from the
  tracking profile's windows (`Standard`: live ≤ 5 s, recent ≤ 30 s), so the
  UI never applies its own timeout policy while the native library is loaded.
- Evidence classification: the contract above is compile-verified (`javac`
  against `android-36`, `d8`, DEX inspection) and packaged in the committed
  APK; first-launch, process-kill, and permission-revocation behaviour on a
  device remain unobserved (MIG-003), so REQ-ANDROID-002/003 stay
  `IMPLEMENTED_UNVERIFIED` in `docs/REQUIREMENTS_LEDGER.md` until then.

## Verification

| Proof | Command | Needs | Runs in CI |
|---|---|---|---|
| Java façade ↔ Rust exports match 1:1 (no missing, no orphan) | `cargo xtask check-jni-contract [lib.so]` (also inside `cargo xtask gates`) | pinned toolchain; an ELF64 little-endian library (host Linux build or the Android cross-compile) | yes (`gates`) |
| Real JVM loads the host library, links every declared native, checks `abiVersion`, and exercises the tracking surface | `cargo xtask verify-jni-live` | a JDK (`javac`/`java`) | yes |
| Cross-compile, package, sign, and inspect the APK (entries, DEX classes, exports) | `cargo xtask build-apk`, `cargo xtask verify-android-live` | Android SDK build-tools, platform `android.jar`, NDK, JDK | no (runner has no SDK) |
| Install, scan, and differential comparison against the oracle on a device | — | an ARM64 Android/Bionic device or emulator, and the original signing key for update identity | no (MIG-003, EXT-006) |

`ANDROID_HOME`/`ANDROID_SDK_ROOT` locate the SDK for the last two commands
(`xtask/src/main.rs::discover_sdk_root`). The committed
`HSE-BLE-Radar-arm64-v1.0.0.apk` is the output of `build-apk` at the commit
that last changed the Android sources or the JNI crate; the export contract
of its `lib/arm64-v8a/libbleradar_jni.so` was re-verified against
`NativeRadar.java` on 2026-09-10 (21 natives ↔ 21 exports).

Reproducibility, observed 2026-09-10 by rebuilding the then-committed APK on
a different host (build-tools 37.0.0, NDK 27.3.13750724, platform
android-36, OpenJDK 21.0.10): `AndroidManifest.xml`, `resources.arsc`, both
icon resources, and `libbleradar_jni.so` were byte-identical to the committed
entries, so the Rust cross-compile is bit-reproducible across hosts and
sessions. `classes.dex` differed by 164 bytes with an identical class/method
inventory and identical instruction stream: JDK 21's `javac` emits
`MethodParameters` attributes for mandated inner-class constructor
parameters, which the previous host's JDK did not. The DEX is therefore
reproducible per JDK major version, which is why CI and this record pin
JDK 21; a rebuild on another JDK is a metadata difference, not source drift,
and `dexdump -d` on both files is the way to tell the two apart.
