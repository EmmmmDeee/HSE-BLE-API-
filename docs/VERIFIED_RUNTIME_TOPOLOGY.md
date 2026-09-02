# Verified Runtime Topology and Rust Migration Boundary

## Scope and authority

This map describes the code that actually exists at commit `7a1ae2d` and the
immutable v0.3.0 APK (`dc1c5129625e53646f3cb644c6c8e060c4527d366aa1e284d0393284d467e255`).
It does not infer a runnable Android product from the reconstructed source.

There are two disconnected execution graphs:

1. **Shipped Android product:** Android framework → R8-obfuscated DEX →
   generated UniFFI/JNA → the APK's ARM64 Rust `libbleradar_core.so`.
2. **Source workspace:** Cargo/test callers → `bleradar-core` and
   `bleradar-compat`; `cargo xtask` is the only built first-party executable.

The source workspace has no Gradle project, Android source, Android target,
UniFFI exports, or link from the APK. Therefore, a source analogue is not a
replacement for the shipped native contract.

## Evidence and method

| Evidence | Result |
|---|---|
| APK manifest inspection | `BleRadarApp`, exported launcher `MainActivity`, non-exported `BleScanService`, `FileProvider`, AndroidX startup provider, and profile receiver |
| DEX call-site analysis | 90 of 124 application contracts have direct non-generated DEX callers |
| UniFFI metadata | 99 functions, 24 `RadarStore` methods, and one constructor |
| Instrumented native trace | 19 deterministic functions executed under ARM64 QEMU compatibility loading |
| Rust source/call-site analysis | Library modules are caller-owned and synchronous; only `xtask` is an executable |
| APK signature/alignment checks | v2/v3 signature valid and ZIP alignment valid; retained APK was not modified |

The complete machine-readable ABI result is
`bleradar_compat::RUNTIME_CONTRACTS`. Its current counts are:

- `VERIFIED_RUNTIME`: 19;
- `STATICALLY_REACHABLE`: 78;
- `CONDITIONALLY_REACHABLE`: 0 at individual ABI-contract granularity;
- `UNREACHABLE`: 0;
- `UNKNOWN`: 27.

`VERIFIED_RUNTIME` records execution of deterministic native code, not Android
lifecycle parity. The compatibility loader changed Android library names and
used minimal Bionic-to-glibc symbol shims. Pure results were stable; stateful
`RadarStore` runs exposed allocator corruption and are excluded. Android,
concurrency, persistence, and lifecycle behavior still require a real Bionic
device/emulator trace.

## Entry points

| ID | Entry point | Language | Reachability | Classification |
|---|---|---|---|---|
| E1 | `BleRadarApp.onCreate` | DEX/JVM | `STATICALLY_REACHABLE` (manifest + DEX) | `RUST_MIGRATION_REQUIRED` for initialization policy; Android `Application` adapter is justified |
| E2 | `MainActivity` launcher and Compose actions | DEX/JVM | `STATICALLY_REACHABLE` (exported `MAIN`/`LAUNCHER`) | UI policy/actions require Rust; Android activity/render adapter is justified |
| E3 | `BleScanService.onStartCommand` | DEX/JVM | `STATICALLY_REACHABLE` (manifest + UI call sites) | Scheduling and decisions require Rust; Android `Service`/callback adapter is justified |
| E4 | BLE scan callback | DEX/JVM | `CONDITIONALLY_REACHABLE` (permission, radio, service state) | Parsing/normalization require Rust |
| E5 | classic-Bluetooth `ACTION_FOUND` receiver | DEX/JVM | `CONDITIONALLY_REACHABLE` | Parsing/normalization require Rust |
| E6 | Wi-Fi `SCAN_RESULTS` receiver | DEX/JVM | `CONDITIONALLY_REACHABLE` | Parsing/normalization require Rust |
| E7 | GPS/network location listener | DEX/JVM | `CONDITIONALLY_REACHABLE` | Validation/state policy require Rust |
| E8 | GATT UI actions/callbacks | DEX/JVM | `CONDITIONALLY_REACHABLE` | Protocol decisions require Rust; Android callback adapter is justified |
| E9 | import/export/session UI actions | DEX/JVM | `CONDITIONALLY_REACHABLE` | Parsing, serialization, and persistence policy are Rust responsibilities |
| E10 | OSINT UI action | DEX/JVM → Rust | `CONDITIONALLY_REACHABLE` (user action/options/network) | Existing oracle execution is `RUST_NATIVE` |
| E11 | AndroidX providers/profile receiver | vendor DEX | `STATICALLY_REACHABLE` (manifest) | `NON_RUST_JUSTIFIED_BOUNDARY`; no application policy |
| E12 | `cargo xtask <command>` | Rust | `VERIFIED_RUNTIME` through CI/tests | `RUST_NATIVE`; Cargo/Git child processes are toolchain boundaries |
| E13 | public `bleradar-core` APIs | Rust | `STATICALLY_REACHABLE` from tests/external callers | `RUST_NATIVE`, but not product-runtime reachable |

No HTTP API handler, application queue, plugin loader, alternate first-party
worker process, alarm/job scheduler, or first-party Android subprocess was
found. Cargo features do not select application runtime paths.

## Material Android paths

The tables use these boundary codes:

- **A:** coroutine/callback async boundary;
- **F:** generated UniFFI/JNA FFI;
- **P:** Android process boundary (all first-party Android paths remain in the
  main app process);
- **Q:** queue boundary;
- **R:** retry boundary;
- **B:** fallback boundary;
- **X:** recursion/follow-up edge.

| Path | Input → call chain → result | Control owner | State read/write | External/persistence I/O | Boundaries | Termination |
|---|---|---|---|---|---|---|
| P1 startup | process → `BleRadarApp.onCreate` → map cache/config → alias preferences → `RadarStore.setAliasFile/importAliases` → preload thread → notification channel | DEX | reads SharedPreferences/files dir; writes Rust aliases, StateFlow, map cache config | `aliases` preferences, `aliases.json`, osmdroid cache | A thread; F; P none; Q none; B invalid preference values skipped | `onCreate` returns; preload thread returns |
| P2 launcher/UI | launcher intent → `MainActivity` → Compose route/action → permission checks/service, GATT, session, OSINT, map, or export action | DEX | reads/writes Compose/StateFlow selection, filters, messages | Android permissions, share/document picker, map network/cache | A UI/coroutines; F; B UI fallbacks | activity/process lifecycle or action completion |
| P3 service start | intent/null → action parse (`STOP`, `SET_MODE`, start/default) → foreground notification → wake lock → permissions/location/radio receivers → loop launch | DEX | reads running/mode; writes running/error/counters/listeners/job/wake lock | Android notification, power, Bluetooth, Wi-Fi, location | A coroutine/listeners; F permission policy; B missing service/provider/security exceptions | success returns `START_STICKY` (1); stop/failure returns `START_NOT_STICKY` (2) |
| P4 mode change | `SET_MODE` + string → enum-name match → mode StateFlow → if running stop/restart BLE | DEX | reads running; writes mode/scanner | Bluetooth LE scanner | A callback; B invalid/missing mode ignored | returns `START_STICKY` |
| P5 hidden scan loop | coroutine tick → Rust `scanTickPlan` → Wi-Fi/classic/threat/correlation/prune/rearm dispatch → tick+1 → 5 s delay → repeat | DEX dispatch + Rust plan | reads mode/tick/store; mutates radios/store/groups and mirror state | radio scans and native calls | A delay; F; X unbounded loop while scope active; B catch/log continues | coroutine cancellation/service cleanup |
| P6 BLE ingest | `ScanResult` → DEX manufacturer/service/UUID/name/tx/PHY parsing → `RadarStore.ingestBle` → mirror publish/counter | DEX parsing, Rust store | reads permission/name cache; writes Rust store, JVM mirror, StateFlow | Bluetooth APIs | A callback; F; B absent fields/defaults/security exception | callback returns or throws outside caught regions |
| P7 classic ingest | `ACTION_FOUND` → parcel/name/class/bond/RSSI extraction → `RadarStore.ingestClassic` → mirror/counter | DEX parsing, Rust store | writes Rust store and JVM mirror | Bluetooth broadcast/device APIs | A receiver; F; B RSSI defaults `-90`, unavailable optional fields become absent | receiver returns |
| P8 Wi-Fi ingest | scan broadcast/tick → results → timestamp conversion → DEX SSID quote trim/hidden fallback → width mapping → `RadarStore.ingestWifi` → mirror/counter | DEX parsing, Rust store | writes Rust store and JVM mirror | `WifiManager`; scan results | A receiver; F; B null/blank SSID becomes `<hidden>`; security exception ends pass | result iteration completes |
| P9 location | GPS/network update → accuracy gate → `RadarStore.updateObserver` → observer StateFlows | DEX gate, Rust store | writes current observer/track/version and mirror | `LocationManager` every ≥2 s and ≥1.5 m | A listener; F; B missing provider/exception/log | listener return; removed on cleanup |
| P10 threat refresh | every third tick → mirrored devices with sightings ≥3 → `sessionToTrack` → per-device `assessThreat` → batch `applyThreats` → mirror refresh | DEX candidate loop, Rust scoring/store | reads JVM mirror; writes Rust threats then republishes mirror | FFI only | F; B failed assessment logged and omitted | finite candidate iteration |
| P11 correlation | every fourth tick → mirrored devices → `sessionToTrack` → background `correlate` → `RadarStore.setGroups` → group StateFlow | DEX candidate/async dispatch, Rust correlation/store | reads mirror; writes Rust/JVM groups | FFI only | A background dispatcher; F; B failure → empty groups | fewer than two devices yields empty groups; otherwise future completion |
| P12 prune/rearm | tick 59 mod 60 → `RadarStore.prune(now, 30 min, 3)`; tick 239 mod 240 also stop/start BLE | DEX dispatch, Rust predicate/store | mutates store then republishes mirror; scanner handle | Bluetooth scanner | F; X future ticks; B tick catch/log | loop cancellation |
| P13 sessions | UI → choose app `files/sessions` → Rust list/save/load/delete → JVM conversion/load → mirror refresh | DEX path/action, Rust serialization/filesystem/store | snapshots mirror/observer; writes files/store/mirror | private files | A background dispatcher; F; B exceptions surface inconsistently | action completion |
| P14 import/export | UI/document URI/share → read text or snapshot → Rust parse/JSON/CSV/HTML → Android content/share adapter | DEX I/O adapter, Rust format logic | reads store mirror and observer; import writes store/mirror | ContentResolver/FileProvider/files | A; F; B open failure becomes empty list in one path | action completion |
| P15 aliases | app start/UI edit → Rust alias file + Rust alias map + SharedPreferences + mirror refresh | competing Rust/DEX persistence | writes three representations | `aliases.json` and `aliases` preferences | F; B absent editor silently skips preference write | UI action completion |
| P16 GATT | selected BLE device → permission/connect → callbacks/services/chars → Rust name/decode/property helpers → UI state | DEX orchestration, Rust decoding helpers | writes GATT/UI state | Android BluetoothGatt | A callback; F; R platform reconnect behavior not characterized; B callback errors | disconnect/close/activity lifecycle |
| P17 OSINT | seed/kind/options → Rust `osintScan` → bounded BFS/modules/network → result/reports → UI/export | Rust after UI dispatch | native local frontier/cache/result state | HTTPS/DNS endpoints; report share | A background dispatcher; F; X BFS depth 1..3; B module errors produce partial/error result | 45 s default global budget, entity limit, frontier exhaustion, or cancellation boundary |
| P18 map/UI projection | store/mirror/observer → Rust radar/geo/spark/status helpers plus DEX filtering/projection/fallback → Compose/osmdroid | competing DEX/Rust | reads mirrors and UI selection; map cache mutation | map tiles/network/cache | A rendering/network; F; B null/empty visual fallbacks | frame/action/lifecycle end |
| P19 stop/destroy | `STOP`, failure, or `onDestroy` → cancel loop → stop BLE/classic → unregister receivers/location → release wake lock → running false → stop foreground/self | DEX | clears job/listener/scanner/wake-lock/running handles | Android services/radios | A cancellation; B cleanup exceptions suppressed around unregister/platform calls | deterministic cleanup sequence then component termination |

The only application recursion found is the bounded native OSINT frontier. The
service loop is repetition, not call recursion. No queue boundary exists;
coroutines and Android callbacks are the concurrency boundaries.

## Scheduler contract

The instrumented oracle trace and DEX loop agree on a five-second base period:

| Mode | BLE scan mode | Wi-Fi | Classic discovery |
|---|---:|---:|---:|
| `AGGRESSIVE` | 2 | every 15 s | every 6 ticks |
| `BALANCED` (also invalid/empty fallback) | 1 | every 30 s | every 12 ticks |
| `LOW_POWER` | 0 | every 60 s | every 24 ticks |

Threat refresh occurs every three ticks, correlation every four, pruning every
60, and BLE rearm every 240. Tick zero starts Wi-Fi. These cadences are
compatibility requirements until deliberately changed under a separate,
tested contract.

## State and mutation topology

| State | Authoritative runtime owner today | Duplicate/bypass |
|---|---|---|
| devices, observer, groups, aliases, version, session start | native Rust `RadarStore` | DEX `sq.m` `LinkedHashMap` and multiple StateFlows mirror snapshots |
| aliases | Rust store/file | SharedPreferences is a second persistent writer |
| service mode/running/error/counters | DEX static StateFlows | direct UI and service writes bypass one state-transition API |
| scanner/listener/job/wake-lock/GATT handles | Android component objects | cleanup is distributed across stop, failure, and destroy |
| saved sessions | Rust native file functions | DEX chooses paths and converts through JVM models |
| reconstructed evidence | separate Rust `EvidenceStore` values owned by callers | `osint`, `infrastructure`, and `website` do not share one runtime store |

This is not harmless caching: `RadarStore` and `sq.m` are independently
mutable, so readers can observe different versions. Alias persistence has the
same split-brain risk. Both are migration defects.

## Network, cache, retry, and health boundaries

- Native OSINT references BGPView, GitHub, GitLab, Archive.org, Cloudflare DNS,
  crt.sh, Gravatar, Shodan InternetDB, ipapi, and Reddit. The default is online,
  depth 2, 45,000 ms, and 250 entities. Seventeen modules and eight seed kinds
  were runtime-enumerated.
- osmdroid owns map-tile networking and an app cache under `cache/osmdroid`.
- No application-wide rate limiter or circuit breaker was identified.
  OSINT's wall-clock/entity limits are the verified resource gates; individual
  client retry/cache semantics remain `UNKNOWN`.
- Radio eligibility is permission- and provider-gated. Location-off state is a
  health warning, not a hard service termination.
- Scan-tick exceptions are logged and the loop continues after five seconds.
  Several UI native-call failures become empty/null results; these fallbacks
  must be characterized because failure must not silently become success.

## Bypasses, dead surfaces, and unknowns

- DEX directly invokes scan, threat, correlation, prune, and rearm actions after
  obtaining a Rust tick plan. Rust does not own the control plane.
- BLE advertisement and Wi-Fi SSID parsing bypass Rust normalization.
- Threat and correlation candidates come from the JVM mirror instead of an
  atomic Rust snapshot.
- Alias edits write Rust and SharedPreferences separately.
- The 27 `UNKNOWN` ABI entries have exports but no direct non-generated DEX
  caller and no pure trace. They are not declared dead. Generated bindings,
  reflection, native-internal calls, or unsupported UI routes remain possible.
- The 106 ABI contracts outside the high-value semantic registry were
  reachable-but-unregistered before this audit; all 124 now have runtime-map
  entries, but semantic parity still requires characterization.
- No alternative scheduler, application job/alarm worker, plugin registry,
  first-party subprocess, or compile-time application feature branch was found.

## Explicit Rust-first boundary

### `RUST_NATIVE`

The retained native core owns its 124 ABI contracts, including store,
formatting, sessions, threat/correlation, geo, permission policy, GATT helpers,
and OSINT. The reconstructed Rust crates own their source-only analytical
engines and developer tooling. Binary ownership does not satisfy source
migration: behavior must be recreated and verified in buildable Rust source.

### `RUST_MIGRATION_REQUIRED`

The following live DEX responsibilities have no hard reason to remain outside
Rust:

- service state machine, tick advancement, scheduling, retry/fallback policy,
  health gates, and termination decisions;
- BLE/classic/Wi-Fi parsing, normalization, deduplication, and timestamp policy;
- candidate generation and eligibility for threat/correlation;
- all authoritative device, alias, group, observer, selection, and session
  state;
- persistence schemas, migrations, atomic writes, recovery, and cache policy;
- OSINT/network orchestration, limits, retries, and error classification;
- GATT protocol decisions and value parsing;
- filtering, sorting, projection, display-policy decisions, and export/import
  validation.

Leaving any of these in DEX without a new hard-platform proof is an unresolved
migration defect.

### `NON_RUST_JUSTIFIED_BOUNDARY`

The thinnest defensible boundary is:

- manifest, resources, signing/package metadata, and Gradle/Android packaging
  syntax required by the platform;
- JVM classes Android must instantiate (`Application`, `Activity`, `Service`,
  receivers/providers/listeners) and callback-to-Rust event adaptation;
- Compose/osmdroid rendering declarations and Android document/share/permission
  presentation;
- generated UniFFI/JNI/JNA glue and Android framework handle marshalling;
- unavoidable Android framework calls for radios, GATT, location,
  notification, wake lock, lifecycle, and content URIs.

Android requires JVM component identities and framework-owned objects. Rust
cannot be the manifest-instantiated `Activity`/`Service`, and raw NDK APIs do
not replace these SDK contracts. This justifies adaptation only—not policy,
validation, state, scheduling, parsing, or decisions.

### `UNKNOWN`

Exact behavior of untraced ABI exports, Android lifecycle races, platform retry
semantics, stateful native allocator/concurrency behavior, persistence failure
atomicity, cache freshness, and network-module retry/rate policy remains
unknown. A real ARM64/Bionic device or emulator is required to reduce these
unknowns safely.

