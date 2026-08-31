# Rust Conversion Analysis

This project is already a Rust workspace: the migration question here is not
"should this move to Rust" but "what remains outside safe Rust, and in what
order should it be converted." This document analyzes that frontier. It is
grounded in the machine-readable parity registry
(`crates/bleradar-compat/src/lib.rs`) and the generated
`docs/PARITY_COVERAGE.md`, so its claims stay auditable.

## Current architecture

- **Safe Rust domain core** (`bleradar-core`): geometry, identity, RSSI
  filtering, proximity, device tracking, and spatial estimation. Zero
  third-party crates (CI-enforced by `cargo xtask check-dependency-policy`),
  `unsafe_code = "forbid"`, clippy-clean at `-D warnings`.
- **Semantic parity registry** (`bleradar-compat`): every high-value native
  contract is classified `Reconstructed`, `OracleOnly`, or `Blocked`.
- **Immutable oracles**: the original APK and its `libbleradar_core.so` /
  `classes.dex` (inside the retained migration archive) are the behavioral
  reference, integrity-locked in CI by `cargo xtask check-oracle-integrity`.
- **Android layer** (not in this repo as source): Compose UI and BLE service
  code exists only as R8-obfuscated DEX in the oracle APK.

Current registry state: **124** observed UniFFI symbols; **18** registered
with explicit status — 6 `Reconstructed`, 5 `OracleOnly`, 7 `Blocked` —
leaving **106** symbols awaiting semantic registration.

## Conversion prerequisite (highest leverage single investment)

**Build an oracle execution harness.** Almost every remaining conversion is
gated on characterizing the aarch64 Android `libbleradar_core.so`
(ISSUE_LEDGER MIG-003/MIG-005). Options, in order of fidelity:

1. On-device/emulator harness driving the UniFFI surface with generated
   inputs and recording inputs → outputs → errors as fixtures.
2. `qemu-aarch64` user-mode with an Android (Bionic) linker and sysroot —
   lower setup cost, adequate for pure functions.

The recorded fixtures become differential tests: a contract is promoted to
`Reconstructed` only when the Rust implementation matches the oracle on
inputs, outputs, errors, and side effects. This is the promotion rule the
repository already documents; the harness is what makes it executable.

## Recommended conversions, ranked

1. **`oui_vendor` (OracleOnly).** Pure embedded-database lookup. Extract the
   vendor table from the oracle, embed it as static data, differential-test
   exhaustively over the 24-bit OUI space. Low risk, and it unlocks
   `mac_info`. Benefit: removes a native dependency for a hot identity path.
2. **`export_device_json` / `export_session_json` / `import_parse`
   (Blocked).** Pure serialization; schema is recoverable by running the
   oracle over generated sessions. Benefit: round-trip property tests plus
   memory-safe parsing of untrusted import data — parser code is exactly
   where Rust's safety pays. Constraint: the zero-dependency policy means
   either a deliberate, recorded exception for `serde`/`serde_json` or a
   small hand-rolled JSON layer; recommend the recorded `serde_json`
   exception, since a bespoke parser re-creates the risk Rust is meant to
   remove.
3. **`multilaterate` (OracleOnly).** CPU-bound numeric geometry — the
   classic Rust target. Differential-test against oracle fixtures with
   generated geometries; property-test against the existing conservative
   estimator as a sanity floor. Benefit: performance and a testable
   replacement for the least-transparent piece of positioning logic.
4. **`assess_threat` / `correlate` (OracleOnly).** Policy thresholds are
   private; characterize decision boundaries empirically before porting.
   Medium effort, medium risk of silent semantic drift — keep `OracleOnly`
   until fixtures cover the boundary regions densely.
5. **`RadarStore` + `session_to_track` (OracleOnly/Blocked).** The stateful
   store: convert last among the native contracts, once the pure functions
   around it are fixture-locked. Benefit: ownership discipline over shared
   mutable scan state, the historical source of race bugs in scanner apps.
6. **Android BLE scan-ingest path (Kotlin, out-of-repo).** Keep Compose UI
   and OS lifecycle in Kotlin; move per-advertisement parsing, dedup,
   filtering, and history updates behind the existing UniFFI boundary into
   `bleradar-core`. Benefit: the highest-frequency code path (thousands of
   advertisements/minute) gains Rust throughput and drops JVM allocation
   churn; the UI keeps platform idioms.

**Superseded anti-recommendation:** an earlier revision of this document
recommended keeping `tools/*.py` (parity report, dependency and oracle gates)
in Python, reasoning that I/O-bound scripts with no safety-critical surface
were negative-value conversions. A subsequent migration mandate requires
zero non-Rust host dependencies for tooling as well as for the shipped
crates, and the pinned-toolchain "one command from a clean extraction"
invariant does not actually hold while a gate depends on a Python
interpreter being present. `tools/*.py` and `tools/native_abi.sh` were
converted to `cargo xtask` (`xtask/src/main.rs`) after proving every
subcommand byte-identical to the script it replaces, then removed; see
`docs/AUTONOMOUS_DECISIONS.md` #25. The risk profile assessment (low
safety-critical surface) was correct and is exactly why the conversion was
low-risk, not why it was skippable.

**Anti-recommendation (current):** `xtask`'s ZIP reader does not implement
DEFLATE decompression purely to auto-extract `classes.dex` from the APK for
one-time documentation regeneration (`docs/AUTONOMOUS_DECISIONS.md` #26).
That capability is on no gate's hot path; the new parsing surface a
hand-rolled inflate implementation would add is not justified until a gated
consumer actually needs it.

## Expected benefits

- Every promotion shrinks the unauditable native surface and grows the
  clippy-clean, `forbid(unsafe_code)` domain.
- Parsing/serialization in safe Rust removes the memory-safety risk of
  handling untrusted import files.
- The scan-ingest move reduces battery and GC pressure on the device's
  hottest loop.
- Each conversion lands with differential fixtures, so parity becomes a
  measured number (`docs/PARITY_COVERAGE.md`), not a claim.

## Challenges and mitigations

- **Oracle is aarch64 Android-only** → the harness prerequisite above;
  until it exists, promotions stay honestly `Blocked` (MIG-003).
- **R8 obfuscation hides intent** → characterize behavior, never decompile
  names into guessed semantics (AUTONOMOUS_DECISIONS #1).
- **Zero-dependency policy vs. serde** → the CI dependency gate forces the
  trade-off to be an explicit, recorded decision rather than drift.
- **Signing identity cannot transfer** (MIG-002) → a rebuilt APK is a new
  application identity; plan distribution accordingly.
- **Floating-point parity** → match the oracle within documented ULP
  tolerances per contract; exact bit-parity across compilers/targets is not
  a realistic acceptance bar and should not be claimed.
