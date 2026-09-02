# Authoritative Rust Target Architecture

## Scope and constraints

This is the ownership design for replacing the shipped v0.3.0 runtime after
the behavioral firewall is complete. It is not approval for a mechanical port.
The immutable APK remains the behavior oracle; the current workspace remains a
source reconstruction until it is built into an Android application and
differentially verified.

The target has one live application core. It does not run the legacy state
machine beside a new Rust state machine, and it does not adopt source-only
analytical engines as product behavior without an explicit contract.

## Current implementation census

| Responsibility | Current implementations | Classification |
|---|---|---|
| Android lifecycle/resources | DEX application/activity/service | `JUSTIFIED_EXTERNAL_BOUNDARY` for component methods; mixed with `TARGET_FOR_RUST_MIGRATION` policy |
| scheduling/dispatch | DEX `BleScanService` coroutine loop and direct callback paths | `BYPASSING`, `TARGET_FOR_RUST_MIGRATION` |
| BLE/classic/Wi-Fi/location normalization | DEX callbacks/helpers | `TARGET_FOR_RUST_MIGRATION` |
| canonical device/session/group state | native `RadarStore`; DEX `sq.m` mirror | native `AUTHORITATIVE_RUST`, JVM mirror `COMPETING` |
| aliases | native store/`aliases.json`; DEX mirror/SharedPreferences | `DUPLICATED` |
| threat/correlation eligibility and dispatch | DEX tick/candidate loops plus native functions | `COMPETING`, `TARGET_FOR_RUST_MIGRATION` |
| geo, formatting, GATT decoding | native functions, with non-equivalent source analogues | oracle `AUTHORITATIVE_RUST`; source `COMPATIBILITY_LAYER` |
| OSINT orchestration/network/cache | native functions | oracle `AUTHORITATIVE_RUST`; source engines are not verified substitutes |
| UI filtering/projection/status decisions | DEX/Compose helpers plus native formatting helpers | `TARGET_FOR_RUST_MIGRATION` |
| session/import/export | native serialization plus DEX path/content handling | native `AUTHORITATIVE_RUST`; path adapter `JUSTIFIED_EXTERNAL_BOUNDARY` |
| Android callbacks, intents, permissions, content URIs, rendering | DEX/platform | `JUSTIFIED_EXTERNAL_BOUNDARY` after policy extraction |
| UniFFI/JNA/JNI glue | generated binding classes and native exports | `COMPATIBILITY_LAYER` |
| reconstructed engines 1–10 | `bleradar-core` source modules | `LEGACY`/source-only until selected and verified; potentially `COMPETING` if wired blindly |
| build/audit/parity tooling | `xtask`, Python checks, Cargo/GitHub configuration | external build syntax or developer tooling, not application runtime |

No alternate scheduler, application queue, plugin loader, worker process, or
subprocess is a target component because none was found in the shipped
runtime.

## Target dependency direction

The target retains one application crate, `bleradar-core`, with cohesive
modules rather than one crate per concept:

```text
Android component and rendering adapter
                 │ platform events / platform commands
                 ▼
          generated safe FFI facade
                 │
                 ▼
      bleradar_core::runtime::Runtime
       │          │             │
       ▼          ▼             ▼
 radio/geo    analysis/gatt  network/osint
       │          │             │
       └──────────┴──────┬──────┘
                         ▼
                state::RadarState
                         │
                         ▼
          persistence / immutable snapshots
```

`bleradar-compat` remains verification metadata and test support. It is never
linked as a second policy engine. `xtask` remains developer tooling. If an
Android adapter project is recovered or recreated, its non-Rust source is a
separate thin packaging target, not another application core.

Lower modules do not call the Android adapter, render UI, or mutate state
through FFI. Platform capabilities are injected as typed ports implemented by
the adapter.

## One owner per responsibility

| Responsibility | Sole target owner | Permitted delegates |
|---|---|---|
| lifecycle state and transition validity | `runtime::Runtime` | Android invokes component callbacks only |
| scheduling, cadence, retry, budget, cancellation | `runtime::scheduler` | injected monotonic clock/timer |
| dispatch and follow-up work | `runtime::Runtime` | typed capability ports execute commands |
| BLE/classic/Wi-Fi parsing and normalization | `radio` | adapter copies platform fields without policy |
| location validation and observer track | `geo` and `state::RadarState` | provider registration remains Android |
| canonical IDs/fingerprints/entity merge | `model` and `state::RadarState` | none |
| eligibility, ranking, threat, correlation | `analysis` | workers compute against immutable snapshots |
| GATT state, deadlines, decoding, read policy | `gatt` | Android owns framework handles/callback transport |
| OSINT frontier, module selection, rate/retry/cache policy | `network::osint` | DNS/HTTP/filesystem executors perform I/O |
| all mutable application state/versioning | `state::RadarState` | persistence commits approved snapshots |
| session, alias, cache, export formats and upgrades | `persistence` | Android supplies private directories/content streams |
| UI filtering, sorting, projection, labels, status | `presentation` | Compose renders immutable view models |
| permission decision and degraded-mode policy | `runtime::capabilities` | Android reports grants/capabilities |
| resource acquisition/release decision | `runtime::Runtime` | Android executes wake-lock/radio/receiver commands |
| FFI ownership/error mapping | generated `ffi` facade | generated bindings only |
| logging/metrics/audit event schema | `observability` | platform sink writes emitted records |
| terminal reason and final snapshot | `runtime::Runtime` | Android performs final platform teardown |

Direct external calls from UI helpers, callbacks, or background code are
forbidden. Every material action enters `Runtime`, which returns explicit
commands and records its state transition.

## Domain model

The target uses strong identifiers for device, observation, operation,
session, group, module, job, state version, and monotonic tick. Addresses,
fingerprints, SSIDs, frequencies, channels, RSSI, coordinates, timestamps,
durations, distances, and confidence are validated domain types rather than
interchangeable strings/numbers.

Finite lifecycle states are closed enums:

- application: created, foreground, background, terminating, terminated;
- scan controller: stopped, starting, running by mode, stopping, failed;
- permission/capability: unavailable, denied, restricted, granted;
- GATT: idle, connecting, discovering, reading, disconnecting, complete,
  cancelled, failed;
- OSINT job: queued, running, partial, complete, cancelled, failed;
- persistence transaction: validating, staging, committed, failed.

State transitions consume a command plus the current state and return either a
new state/effects or a typed rejection. Optional data is represented by
`Option`; expected failures use `Result`. Sentinel strings and empty
collections do not encode failure.

Interchange records are schema-versioned and separate from domain types.
Conversions validate all fields and return path-aware errors. Unknown future
enum values remain representable at the interchange boundary but cannot enter
the domain silently.

## Runtime control model

`runtime::Runtime` is a single-writer event processor. Inputs are typed:

- lifecycle and intent events;
- permission/capability snapshots;
- timer expirations;
- BLE/classic/Wi-Fi/location observations;
- GATT callbacks;
- network/module completions;
- persistence completions;
- user commands.

Each accepted input receives an `OperationId`. Processing yields:

1. a state transition and new `StateVersion`;
2. zero or more platform/I/O commands carrying the originating operation,
   cancellation scope, deadline, and idempotency class;
3. an immutable presentation snapshot;
4. structured audit events.

I/O completion re-enters through the same event processor. No callback mutates
application state directly. Follow-up execution is explicit in returned
commands rather than hidden recursion.

The scheduler owns scan mode and monotonic tick. It computes due actions from a
monotonic clock, records why each action is due, and schedules exactly one next
wake-up. Wall-clock time is retained only for observations and serialization.
Missed-tick handling, coalescing, and continuation after error are explicit
policies verified against the behavioral contract.

## State and transaction model

`state::RadarState` is the only mutable domain aggregate. It owns:

- canonical devices and observations;
- observer track;
- group/correlation state;
- threat assessments;
- aliases;
- active session metadata;
- operation replay/idempotency records;
- analysis input/output versions.

An ingest transaction normalizes once, derives one canonical identity, applies
one merge, increments the state version once, and emits one snapshot. Threat
and correlation workers receive immutable `(StateVersion, Snapshot)` values.
Their result is applied only if its declared version is current or a
domain-specific rebase validates every dependency.

The normal owner is one Rust task/thread, so the aggregate does not require
`Arc<Mutex<_>>`. Immutable snapshots may use shared ownership only when
profiling shows copying is material and the sharing lifetime is bounded.
Platform handles remain adapter-owned opaque IDs; they never enter persisted
state.

## Concurrency, cancellation, and resources

- One event processor serializes mutations. Worker concurrency is limited by
  typed pools and per-capability budgets.
- Every spawned operation belongs to a cancellation scope rooted in the
  lifecycle. Child scopes cannot outlive their parent.
- Deadlines use monotonic time. Timeout, caller cancellation, dependency
  cancellation, and shutdown remain distinguishable outcomes.
- Retry policy is data: maximum attempts, elapsed budget, backoff, jitter,
  retryable error set, and idempotency requirement.
- Resource commands are leases. Each acquired wake lock, scan registration,
  receiver, location subscription, foreground notification, GATT connection,
  and network request has one owner and one release path.
- Cleanup is idempotent and records outstanding leases. Termination completes
  only after release acknowledgements or an explicit bounded forced-close
  result.
- Panic does not cross FFI. The facade translates it to a terminal internal
  error, requests cleanup, and preserves diagnostics without exposing memory
  contents.

Unsafe Rust is prohibited by the workspace lint. A future unavoidable unsafe
platform bridge requires a separately audited crate, a stated invariant per
block, and a proof that generated safe bindings cannot provide the boundary.

## Persistence ownership

`persistence` owns serialization, validation, schema migration, filenames,
retention, and atomic commit. Android supplies an app-private directory or
user-selected stream but cannot construct domain JSON/CSV.

File-backed commits use same-directory staging, flush, atomic replacement where
the platform guarantees it, directory synchronization where supported, and
typed degradation where not supported. Readers impose byte/count/depth limits,
validate before state mutation, reject unsupported future versions, and retain
the original after a failed upgrade.

The alias migration is one explicit transaction:

1. read native JSON and legacy SharedPreferences through adapters;
2. normalize keys and validate values in Rust;
3. produce deterministic conflicts rather than last-writer-wins;
4. commit the canonical Rust document;
5. mark migration complete only after durable verification;
6. make the legacy source read-only, then remove it after the support window.

Session v2 and export formats retain golden readers/writers. Schema changes add
new versions and migration tests; they never reinterpret existing bytes in
place.

## Networking and OSINT

`network::osint` owns the frontier, recursion depth, entity budget, module
eligibility, deduplication, request planning, cache policy, rate limits, retries,
partial-result aggregation, and termination reason.

Transport ports accept normalized requests and return response bytes plus
transport metadata. They do not choose URLs, follow-up entities, retry rules,
or success policy. DNS, TLS/certificate, HTTP, and archive integrations may use
Rust libraries or platform transports, but selection is a Rust decision.

Recursive expansion is iterative and bounded. Every frontier node records
parent, depth, generating module, operation, and deduplication key. Completion
requires an empty frontier or an explicit reason: entity limit, depth limit,
deadline, cancellation, rate limit, dependency exhaustion, or fatal policy
error.

Cache keys include module contract version and normalized request. Entries
record acquisition time, source, validation metadata, and expiry. Offline mode
cannot accidentally consume an online-only stale entry.

## Presentation ownership

Rust receives a typed presentation request and returns an immutable view model
with filtered/sorted entities, selected state, map/radar geometry, labels,
status, action availability, and explicit degraded/error states. Rendering
layout, typography, colors, animations, accessibility nodes, and Android
navigation remain Compose responsibilities.

This removes policy from UI helpers without attempting to implement Android
graphics in Rust. Stable item IDs are domain IDs; list position is never
identity.

## External boundary exceptions

| Non-Rust remainder | Hard blocker | Thinnest permitted boundary |
|---|---|---|
| Android `Application`, `Activity`, `Service`, receivers/listeners | Android framework instantiates and calls JVM component classes by manifest/class-loader contract | component methods translate callbacks to FFI events and execute returned platform commands |
| BLE/classic/Wi-Fi/location/GATT APIs | public Android SDK objects and callbacks are JVM interfaces | copy primitive/byte fields, retain opaque handle IDs, execute Rust-decided operations |
| permissions, intents, content URIs, FileProvider, foreground notification/wake lock | Android framework tokens, lifecycles, and APIs | capability/event reporting and command execution only |
| Compose rendering/accessibility/navigation | Android UI toolkit and generated resources execute on the JVM | render Rust view models; no domain filtering/scoring/validation |
| manifest/resources/Gradle/signing metadata | declarative/platform packaging and external toolchain syntax | declarations and build glue only |
| generated UniFFI/JNI/JNA binding code | ABI marshalling must match platform calling conventions/tooling | generated wrappers with no handwritten policy or state |

These exceptions do not justify JVM scheduling, parsing, normalization,
eligibility, filtering, persistence policy, retries, cache policy, state, or
network orchestration. A platform transport may be non-Rust only when a
specific required Android/vendor API has no viable Rust invocation; even then,
request and response policy remains Rust.

## FFI contract

The facade exposes a small versioned API around runtime creation, event
submission, command polling/completion, immutable snapshots, cancellation, and
shutdown. It does not reproduce the 124 free-function/store-method ABI as the
target internal architecture.

All FFI records have explicit ownership, schema version, size limits, and error
variants. Byte buffers are copied or leased under generated binding rules.
Opaque handles have generation counters to reject stale callbacks. Compatibility
exports may temporarily delegate to the one `Runtime`; they may not own parallel
state.

ABI drift is checked from generated interface metadata and contract census.
Each boundary test covers invalid handles, oversized data, unknown enum/schema,
panic containment, concurrent callback, cancellation, and use after shutdown.

## Observability and termination

Structured events include operation, state version, lifecycle state, action,
reason, attempt, deadline, dependency, duration, and result class. Sensitive
observation/network payloads are excluded or redacted by Rust policy.

Every loop/job terminates with a typed reason. Runtime shutdown rejects new
work, cancels children, drains bounded completions, commits or aborts pending
persistence, releases leases, publishes the final snapshot, and marks itself
terminated. Repeated shutdown returns the existing terminal result.

Health and circuit decisions are Rust-owned. A dependency circuit tracks
closed/open/probing states with monotonic deadlines; the adapter reports
transport facts only. Health degradation is visible in snapshots and cannot be
converted to an empty-success result.

## Configuration and feature control

Rust parses and validates scan modes, limits, endpoints, module enablement,
budgets, and compatibility switches into an immutable configuration snapshot.
Changes are versioned commands. Environment variables, Android preferences, or
build flags are input sources, never alternate policy owners.

Compile-time features may include or omit platform capabilities. They cannot
silently change persisted schemas or core semantics. Every supported feature
combination has a topology entry and gate; untested combinations are
unsupported rather than conditionally live.

## Consolidation sequence and gates

1. Complete real-Android characterization for lifecycle, callbacks, state,
   persistence, GATT, concurrency, and network failure behavior.
2. Freeze versioned interchange schemas and generate the narrow FFI facade.
3. Introduce the Rust runtime/state owner without changing analytical behavior;
   route one input type at a time and remove its JVM mutation immediately.
4. Move scheduler and lifecycle policy, leaving Android as command executor.
5. Move radio/location normalization and eliminate the JVM state mirror.
6. Migrate alias/session/export persistence with tested rollback and explicit
   one-time data conversion.
7. Move threat/correlation candidate generation and failure handling.
8. Move OSINT transport planning, retries, rate limits, cache, and recursive
   frontier behind deterministic ports.
9. Move presentation policy and GATT state while retaining Android rendering
   and framework handles.
10. Remove compatibility exports only after each census item has
    differential, state-transition, fault, restart, and termination evidence.

At each step there is exactly one writer and one scheduler for the migrated
responsibility. A switch is not complete while both old and new paths can
mutate state. Removal requires the corresponding ledger row in
`BEHAVIORAL_CONTRACT.md` to be satisfied and the topology census to report no
unclassified reachable path.

