# Behavioral Contract and Migration Ledger

## Contract rule

Required observable semantics, not source structure, are the migration
firewall. No old implementation may be removed until its target passes the
listed characterization and differential checks. A sampled fixture proves only
that sample.

Classification meanings:

- **REQUIRED:** necessary for correctness, safety, or a public contract;
- **INTENTIONAL:** evidenced policy that may change only deliberately;
- **COMPATIBILITY_REQUIRED:** externally visible oracle behavior to preserve
  during replacement;
- **ACCIDENTAL:** observable implementation detail with no evidence of intent;
- **DEFECTIVE:** reproduced behavior with a separately defined correction;
- **UNKNOWN:** insufficient trustworthy evidence.

## Executable evidence

`crates/bleradar-compat/tests/oracle_characterization.rs` locks sampled oracle
facts, an exact Wi-Fi mapping model, and known source gaps. Its ignored
`wifi_frequency_oracle_parity_removal_gate` is deliberately red until the
replacement implements the captured contract.
`crates/bleradar-compat/tests/stateless_oracle_characterization.rs` locks exact
Bluetooth bitfields, text normalization, projection math, and the expanded
trace census. `crates/bleradar-compat/tests/contracts.rs` locks the 124-entry
census, reachability counts, and registry semantics. Timestamp defects remain
recorded trace observations pending executable target rejection/boundary tests.
These tests deliberately do not load the Android `.so` in normal CI.

The compatibility trace executed stateless native functions from a copied
oracle after changing imported Android library names and removing Android
symbol-version metadata. It supplied only `__sF`, `__errno`, and
`__register_atfork` compatibility symbols. Two fixed-environment runs were
byte-identical. A three-timezone run isolated `times_parse_wigle` as dependent
on the process timezone; its absolute glibc values are observations, not Android
parity evidence. Stateful store results are excluded because Bionic allocation
and threading were not reproduced.

All 22 formerly unknown stateless exports were exercised using their generated
UniFFI wire layouts. This raises the instrumented bucket to 41 and leaves only
five untraced, read-only `RadarStore` methods. Execution proves reachability,
not complete semantics: structured parsers exercised only selected empty and
malformed inputs. The Bluetooth, CSV/text, fingerprint, and projection helpers
have executable models below; timestamp behavior remains trace-only evidence.

The Wi-Fi model has stronger evidence than a sampled fixture. The generated DEX
bindings establish `Int → Int?` for channel-to-frequency and `Int? → Int?` for
frequency-to-channel. A native sweep covered frequencies `0..=8000` and channels
`0..=300`; a separate run covered `None`, signed extrema, negatives, and every
transition boundary. ARM64 control flow at native offsets `0x5dce54..0x5dcea4` and
`0x5dd318..0x5dd3ac` independently establishes the complete range checks and
integer formulas. The extracted native oracle used for both has SHA-256
`d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d`.

### Captured native fixtures

| Contract/input | Oracle output | Classification |
|---|---|---|
| `core_version()` | `0.3.0 (oui 58049 blocks)` | `COMPATIBILITY_REQUIRED` |
| haversine `(0,0)→(1,0)` | `111195.08023353291` m | `COMPATIBILITY_REQUIRED`; source-radius mismatch recorded |
| antipodal haversine | `20015114.442035925` m | `COMPATIBILITY_REQUIRED` within target-specific floating tolerance |
| `proximity_label(1/2/5/25)` | `immediate/near/mid/far` | `COMPATIBILITY_REQUIRED`; argument is distance, not RSSI |
| BLE RSSI `-70` | `2.8729848333536645` m for absent or supplied tx power | `COMPATIBILITY_REQUIRED`; fixed `-59`, exponent `2.4`, 100 m cap |
| BLE RSSI `0` | `100` m | `COMPATIBILITY_REQUIRED` sentinel behavior |
| Wi-Fi channel → frequency | `1..=13 → 2407 + 5c`; `14 → 2484`; `32..=177 → 5000 + 5c`; otherwise `None` | `COMPATIBILITY_REQUIRED`; exact signed-input mapping |
| Wi-Fi frequency → channel | `2412..=2472 → floor((f-2407)/5)`; `2484 → 14`; `5160..=5885 → floor((f-5000)/5)`; `5955..=7115 → floor((f-5950)/5)`; otherwise `None` | `COMPATIBILITY_REQUIRED`; `None → None`; ranges are inclusive |
| Bluetooth class helpers | major `(class >> 8) & 31`; minor `(class >> 2) & 63`; majors 0..9 map to the captured platform labels and categories | `COMPATIBILITY_REQUIRED`; exact signed-`Int` bit extraction |
| `export_csv_field` | absent → empty; comma, quote, CR, or LF triggers quoting and doubled quotes; tabs and formula prefixes remain raw | CSV quoting is `COMPATIBILITY_REQUIRED`; unneutralized formula-leading attacker input is security-sensitive and not promoted to desired semantics |
| `fmt_rssi(i32)` | decimal integer plus ` dBm`, including signed extrema | `COMPATIBILITY_REQUIRED` |
| `gatt_short` | Unicode-lowercase input; shorten a 36-byte `0000hhhh` Bluetooth-base-shaped string to `0xhhhh`; otherwise return the lowercased input | `COMPATIBILITY_REQUIRED`; `hhhh` is not hex-validated |
| `session_fingerprint(transport,address)` | transport unchanged, `:`, Unicode-uppercase address; `straße → STRASSE` | `COMPATIBILITY_REQUIRED`; punctuation is not canonicalized |
| `ui_point_alpha(now,last_seen)` | `1 - 0.7 × clamp(now-last_seen,0,60000)/60000` in `f32` operation order | `COMPATIBILITY_REQUIRED` for valid timestamps; wrapping subtraction at signed extrema is `DEFECTIVE` |
| `ui_stable_angle(address)` | wrapping `u64` seed `1125899906842597`, multiply by 31 per UTF-16 unit, then `((hash >> 1) % 3600) / 10` as `f32` | `COMPATIBILITY_REQUIRED`; exact Unicode/code-unit behavior |
| `times_iso` | ordinary signed milliseconds format as UTC `YYYY-MM-DDTHH:MM:SS.mmmZ`; `i64::MIN/MAX` wrap to epoch-adjacent `.192/.807` | ordinary range is `COMPATIBILITY_REQUIRED`; overflow output is `DEFECTIVE` |
| ISO timestamp parse | trims outer whitespace and accepts variable-width components/fractions; impossible dates/times normalize forward | invalid-date normalization is `DEFECTIVE`, not desired compatibility |
| WiGLE timestamp parse | seconds precision; permissive components; interprets text in the process timezone | local-time intent may be compatible; hidden timezone dependency is `DEFECTIVE` |
| MAC/OUI samples | delimiter variants normalize to uppercase colon form; flags/address kind and optional vendor fields are returned; malformed input remains an invalid record | sampled normalization is `COMPATIBILITY_REQUIRED`; all 58,049 OUI blocks are not differentially frozen |
| empty structured helpers | empty JSON/WiGLE parse and empty session summary/filter inputs return empty lists; empty CSV split returns one empty field | sampled behavior only; malformed/schema semantics remain `UNKNOWN` |
| signal bars thresholds | `<-90:0`, `-90..-72:1`, `-71..-62:2`, `-61..-52:3`, `>=-51:4` | `COMPATIBILITY_REQUIRED` |
| `fmt_coord(1.23456789)` | `1.23457` | `COMPATIBILITY_REQUIRED` |
| `fmt_distance(999/1000/nonfinite)` | `999 m` / `1.0 km` / `?` | `COMPATIBILITY_REQUIRED` |
| `scan_mode_params` | aggressive `(2,15000,2)`, balanced `(1,30000,4)`, low power `(0,60000,8)` | `COMPATIBILITY_REQUIRED` |
| unknown/empty scan mode | balanced parameters | `INTENTIONAL` fallback evidenced by trace |
| default OSINT options | online, depth 2, 45,000 ms, 250 entities | `COMPATIBILITY_REQUIRED` |
| OSINT inventories | 17 modules, 8 seed kinds | `COMPATIBILITY_REQUIRED` for v0.3.0 |
| empty session export | pretty JSON, format `huntsman-ble-radar-session`, schema version `2` | `COMPATIBILITY_REQUIRED` |
| empty/blank/malformed import samples | empty device vector, no native error | `UNKNOWN`; do not promote silent acceptance to desired semantics |

## Invariants for every target

1. One accepted platform event causes at most one logical ingest. Retries carry
   an operation identifier and cannot duplicate a committed side effect.
2. Failure is represented as failure. Empty, partial, not-found, timeout,
   cancellation, and execution failure are distinct.
3. State mutation is atomic with its published version/snapshot.
4. Candidate generation reads one consistent snapshot; threat/correlation
   results apply only to the version they assessed or are explicitly rebased.
5. A retry is bounded by count, elapsed budget, and idempotency policy.
6. Cache entries include origin, age, and policy; stale reuse is explicit.
7. Persisted data is versioned, validated before mutation, and written
   atomically. Unsupported future schemas fail without overwriting live state.
8. Restart reconstructs only valid persisted state and never silently revives
   transient scanner/GATT handles.
9. Cancellation reaches radio/network work and leaves resource ownership
   observable. Wake locks, listeners, receivers, GATT clients, and foreground
   state are released exactly once.
10. Termination has a reason and a reproducible final state.

These are `REQUIRED`. Where the oracle violates one, the corrected contract
must land as a named defect change rather than incidental migration drift.

## Surface contracts

### Application/service lifecycle

- Start, null, or unknown service intents follow the normal-start path.
  Successful/already-running starts return Android `START_STICKY` (`1`).
  Explicit stop and foreground-start failure return `START_NOT_STICKY` (`2`).
  This is `COMPATIBILITY_REQUIRED`.
- `SET_MODE` accepts the three enum names. An invalid/missing value does not
  change mode. A valid change while running stops/restarts BLE scanning.
- Startup creates foreground state, acquires a non-reference-counted partial
  wake lock with a six-hour timeout, evaluates permissions, registers available
  listeners/receivers, and launches one loop.
- `STOP`, start failure, and destruction converge on cancellation, BLE stop,
  classic cancellation, receiver/location removal, wake-lock release, running
  false, and foreground stop. Exact ordering is `REQUIRED` where Android
  resource ownership depends on it.
- Repeated start must not launch a second loop. This is `REQUIRED`; an Android
  integration test is pending.

### Scheduling and orchestration

- Base delay is five seconds after each tick, including a caught tick failure.
- Wi-Fi/classic cadence follows the mode table in
  `VERIFIED_RUNTIME_TOPOLOGY.md`; threat is every three ticks, correlation
  every four, prune every 60, and BLE rearm every 240.
- Tick actions are independently eligible but currently execute in fixed
  order: Wi-Fi, classic, threat, correlation, prune, rearm. Preserve this as
  `COMPATIBILITY_REQUIRED` until tests show no observable ordering dependency.
- The loop has no retry queue. Catch/log/delay is an `ACCIDENTAL` retry-like
  behavior; the target must classify the failure before deliberately
  continuing.

### BLE, classic Bluetooth, Wi-Fi, and location

- BLE input includes address, optional name, RSSI, optional tx power,
  manufacturer entries, lower-cased service-data UUIDs, lower-cased service
  UUIDs, flags, appearance, address/legacy/connectable/PHY metadata, and
  timestamp. Manufacturer/service iteration order may affect fingerprints and
  is `COMPATIBILITY_REQUIRED` until differential tests prove invariance.
- The DEX name cache and permission fallbacks are observable but
  `ACCIDENTAL`; target Rust normalization owns a typed optional name.
- Classic missing RSSI defaults to `-90`; inaccessible name/class/bond fields
  become absent. Preserve during parity, then change only as an explicit
  contract revision.
- Wi-Fi ignores null BSSID, converts Android microsecond timestamps, trims SDK
  33+ quoted SSIDs, maps null/blank to `<hidden>`, and sends frequency,
  channel-width mapping, capabilities, and timestamp to Rust.
- Native frequency-to-channel conversion floors every in-range integer rather
  than accepting only channel centers. Its 6 GHz range maps `5955..=7115` to
  `1..=233`; the reverse function still maps channel `1` to 2412 MHz and rejects
  `178..=233`, so the pair is intentionally recorded as non-bijective pending a
  separately versioned contract decision.
- Location updates use GPS/network providers, at least 2,000 ms and 1.5 m;
  updates without accuracy or with accuracy above the observed gate are
  ignored. The exact threshold requires a Bionic/Android fixture and is
  `UNKNOWN`.
- Permission denial must remain distinguishable from no results. Current
  empty-result fallbacks are not evidence of success.

### Store and concurrency

- `RadarStore` is the intended native owner of devices, observer track, groups,
  aliases, session start, and a monotonic version.
- Duplicate observations for the same canonical fingerprint update one device
  and increment sightings; case-normalized BLE addresses coalesce in sampled
  traces. Whether identical `(fingerprint,timestamp,payload)` events should
  count twice is `UNKNOWN`.
- Today, DEX updates a separate `LinkedHashMap`/StateFlow mirror. Its ordering
  is observable; independent mutability is `DEFECTIVE` architecture, not a
  parity requirement. The correction is one Rust state owner plus immutable
  versioned snapshots.
- Concurrent callback, tick, UI edit, save, and clear interleavings have not
  been executed on Android. Until race/model tests exist, store parity cannot
  be claimed.

### Threat, correlation, map, and GATT

- Threat candidates currently require at least three sightings. Per-device
  assessment failures are logged and omitted before a batch apply.
- Correlation with fewer than two devices writes an empty group set. Native
  failure also falls back to empty; conflating these is `DEFECTIVE`. The target
  output is `Result<CorrelationSet, CorrelationError>` and only an actual empty
  success clears groups.
- Threat/correlation candidate order comes from the JVM mirror and is
  `ACCIDENTAL`; contractual ordering must be established by differential
  fixtures.
- Rust helper outputs used for radar points, geo sketch, rings, sparkline,
  status, GATT names/properties/value decode, and labels are public UI
  contracts. Compose styling itself is not a domain contract.
- GATT connect/discover/read/disconnect timeout, retry, cancellation, and
  callback-order behavior is `UNKNOWN` pending device tests.

### OSINT/network

- Supported seed kinds are `mac_address`, `ssid`, `ip`, `domain`, `hostname`,
  `email`, `username`, and `url`.
- The native scan is a bounded breadth-first expansion with depth 1..3,
  wall-clock budget, and maximum-entity gate. Default values are captured
  above.
- Offline mode runs local OUI/MAC/SSID logic only. Online modules add DNS,
  certificate, archive, IP, routing, identity, code-host, and Reddit lookups.
- Result status distinguishes complete, partial reason, and error reason.
  Module errors, network-visible request ordering, per-request timeout/retry,
  cache hit/miss/staleness, rate limits, cancellation latency, and partial
  result ordering are `UNKNOWN`.
- The target must retain partial evidence with per-module errors; dependency
  failure cannot erase successful independent findings.

### Import, export, persistence, and restart

- Private sessions live under the Android app's `files/sessions` directory.
  Rust receives the directory/path and owns list/save/load/delete format logic.
- Session JSON v2 top-level order observed for an empty export is `format`,
  `version`, `title`, `exportedAt`, `app`, `core`, `observerTrack`, `devices`;
  it is pretty-printed with two spaces and a trailing newline. Field order is
  compatibility-required because byte-level export consumers may exist.
- `format` is `huntsman-ble-radar-session`; `version` is numeric `2`; time is
  UTC ISO-8601 with milliseconds.
- Exact device schema, unknown-field handling, malformed/partial file errors,
  size limits, atomicity, interrupted writes, schema upgrades, and recovery are
  `UNKNOWN`. They require generated v1/v2/future-schema fixtures and fault
  injection before replacement.
- WiGLE CSV and report formats are public export contracts. Quoting,
  line-ending, non-finite, Unicode, and spreadsheet-formula cases require
  fixtures.
- Alias state is currently persisted in both native `aliases.json` and Android
  SharedPreferences. This duplication is `DEFECTIVE`; import-on-start is a
  compatibility bridge, not the target architecture.
- Timestamp parsing currently accepts impossible values such as
  `2023-02-29`, month 13, hour 24, minute 60, and second 60 by normalizing them
  into later dates. The target rejects these with a typed parse error.
- WiGLE text has no offset. The oracle consults ambient process timezone:
  parsing `1970-01-01 00:00:00` produced `0` in UTC, `36,000,000` in
  `Pacific/Honolulu`, and `-3,600,000` in `Europe/Berlin`. The target receives
  an explicit timezone policy rather than reading hidden process state.
- Timestamp formatting must define a supported date range and reject values
  outside it; epoch-adjacent output produced by signed-extreme arithmetic is
  not a compatibility requirement.
- A process restart reconstructs aliases but not an unsaved scan automatically.
  Manual saved-session load is a separate action. Exact sticky-service restart
  behavior is Android-dependent and `UNKNOWN`.
- Content URI, document picker, FileProvider, and share intent mechanics are
  justified Android boundaries. Data validation and naming policy are Rust.

## Characterization matrix

| Case family | Current executable protection | Required next evidence |
|---|---|---|
| valid/empty/boundary scalar inputs | pure oracle fixtures for geo, range, formatting, scheduler, Bluetooth, text helpers, projection math; complete Wi-Fi integer model and source-domain sweep | exhaustive tables for remaining scalars and target differential runner |
| invalid/non-finite inputs | signed-extreme scalar fixtures, timestamp defects, selected floating fixtures, source validators | each remaining ABI error/status and panic containment |
| malformed import | selected JSON/WiGLE/record/CSV helper traces | typed error tests, truncation/encoding/size fuzz corpus |
| duplicate input/execution | sampled store trace, not trusted for parity | real-Bionic idempotency and callback replay |
| timeouts/retries/rate limits | global OSINT budget metadata only | fake-clock/fake-client integration tests |
| cancellation | DEX cleanup call analysis | service, GATT, and network cancellation tests |
| dependency failure/partial results | DEX/native fallback call analysis | deterministic fault injection per dependency |
| cache hit/miss/stale | map cache path only | cache policy and restart tests |
| concurrent mutation | none authoritative | model/property tests plus Android stress trace |
| restart/recovery | startup/save/load call analysis | process-death and interrupted-write tests |
| resource exhaustion | entity/wall-clock OSINT limits | file/network/memory/radio exhaustion tests |
| schema evolution/partial persistence | empty v2 export fixture | v1/v2/future fixtures and atomic-write faults |
| termination | cleanup call analysis | instrumented lifecycle integration test |

An uncovered row is not waived. It blocks removal of the corresponding old
implementation.

## Confirmed migration defects

| ID | Reproducer | Correct contract | Executable guard |
|---|---|---|---|
| BF-001 | registry labeled source analogues `Reconstructed` despite oracle mismatches | only differential proof may produce `DifferentiallyVerified` | `no_source_analogue_is_mislabeled_as_differentially_verified` |
| BF-002 | oracle 1° haversine differs from source by its radius constant | target explicitly chooses compatibility or a versioned correction | `oracle_haversine_fixture_exposes_radius_gap` |
| BF-003 | oracle proximity accepts metres; source analogue accepts dBm | distinct typed APIs; no name-based substitution | `oracle_proximity_fixture_exposes_input_semantics_gap` |
| BF-004 | source rejects 1,789 oracle-accepted `u16` frequencies: 628 off-center 2.4/5 GHz values and all 1,161 6 GHz values | target preserves the verified inclusive ranges, floor division, optional input, and asymmetric reverse mapping unless deliberately versioned | `oracle_wifi_boundary_trace_locks_ranges_and_flooring`; `oracle_wifi_frequency_gap_is_exhaustively_classified_over_source_domain`; ignored parity removal gate |
| BF-005 | Rust store plus independently mutable JVM mirror and duplicate alias persistence | one authoritative Rust state/persistence transaction | architecture guard pending Android source |
| BF-006 | correlation execution failure becomes empty groups in DEX fallback | failure leaves prior groups intact and is observable | fault-injection test required before migration |
| BF-007 | ISO/WiGLE parsers normalize impossible calendar/time components | reject invalid components with a typed error before persistence/state mutation | QEMU trace observation; executable target rejection test required |
| BF-008 | WiGLE parse reads ambient timezone; timestamp arithmetic wraps at signed extrema | explicit timezone input and validated timestamp range | QEMU trace observation; executable target boundary tests required |

No defect is silently fixed or promoted to desired compatibility in this
phase. Existing guards lock the reproducer; corrected-target gates remain
mandatory where no replacement exists yet. Run
`cargo test -p bleradar-compat --test oracle_characterization
wifi_frequency_oracle_parity_removal_gate -- --ignored` to exercise BF-004's
currently failing removal gate directly.

## Migration ledger

| Component/source | Target Rust owner | Required semantics/compatibility | Data migration | Verification | Old removal condition |
|---|---|---|---|---|---|
| DEX service lifecycle/scheduler | `runtime::scan_controller` | lifecycle states, cadence, action order, bounded continuation, cleanup | none | fake-clock state-machine + Android adapter tests | every intent/tick/termination trace matches required contract |
| DEX BLE/classic/Wi-Fi parsing | `radio::{ble,classic,wifi}` | field optionality, ordering where contractual, timestamp/SSID rules | none | captured Android event corpus + differential ingest | adapters contain only field copying |
| native `RadarStore` + `sq.m` | `state::RadarState` | canonical identity, atomic mutation/version/snapshot, no duplicate commit | convert v2 sessions/aliases | model, concurrency, replay, restart tests | no independent JVM state writer/read model |
| DEX threat candidates + native policy | `analysis::threat` | ≥3 eligibility unless versioned change, typed failures | preserve persisted assessments | dense boundary/differential fixtures | all callers use one Rust snapshot transaction |
| DEX correlation dispatch + native correlation | `analysis::correlation` | empty vs failure distinction, ordering policy | preserve groups if persisted | fault injection + differential corpus | empty fallback and JVM candidate loop removed |
| DEX location gate + native geo | `geo`/`state::observer` | accuracy/timestamp validation and observer history | session v2 observer track | scalar, track, Android callback tests | JVM validation is absent |
| native session/import/export | `persistence::{session,import,export}` | v2/CSV/report bytes, typed errors, limits, atomicity | readers for every supported schema | golden, round-trip, malformed/fault tests | native oracle parity and upgrade path pass |
| duplicate alias stores | `persistence::aliases` | one normalized map, atomic persistence | one-time SharedPreferences/JSON merge with conflict report | restart and interrupted-write tests | compatibility importer has completed/been retired |
| native OSINT | `network::osint` | bounded BFS, partial evidence/errors, options, module contracts | cache schema if any is discovered | fake HTTP/DNS, clock, rate, cache, cancellation tests | all 17 modules pass offline/differential fixtures |
| DEX GATT orchestration + native helpers | `gatt` | typed connection state, decode, deadlines, cancellation | none | fake adapter + device integration | JVM owns callbacks/handles only |
| DEX filtering/projection/UI decisions | `presentation` | stable ordering, labels, points, permission/status decisions | UI preferences if found | golden projection and Compose adapter tests | JVM performs rendering only |
| Android component/callback/content adapters | no Rust replacement; `android-adapter` boundary | exact event/command marshalling and lifecycle/resource calls | platform-managed | instrumentation tests | retained permanently at minimum size |
| generated UniFFI/JNI/JNA | generated `ffi` boundary | ABI ownership, error/panic mapping, cancellation | none | generated-binding and ABI checksum tests | retained or replaced only by equivalent generated glue |
| reconstructed source-only analytical engines | one application `Runtime`/`EvidenceStore` owner | preserve existing public Rust tests while integrating state ownership | explicit store merge if adopted | workspace gates + integration topology test | no parallel live store or competing engine |
