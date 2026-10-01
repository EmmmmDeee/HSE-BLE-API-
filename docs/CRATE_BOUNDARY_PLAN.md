# Crate boundary plan: separating the non-radar engines from the BLE radar surface

Status: **step 0 done** (this document plus a check-only gate). Steps 1 to 4 are
proposals, each with its own risks and verification gate. Nothing in this plan
has changed the shipped native library.

Evidence below was taken on `main` at `d0b1e8d` (#42) with rustc 1.98.0. The
module graph is printed by `cargo xtask check-crate-boundary`, so you can
reproduce it.

## 1. Inventory

### Workspace crates (`cargo tree --workspace -e normal,build,dev`)

```
bleradar-compat v0.5.0 ── bleradar-core v0.6.10
bleradar-core   v0.6.10   (no dependencies)
bleradar-jni    v0.2.0  ── bleradar-core v0.6.10
```

| Crate | Kind | Role | Depends on |
|---|---|---|---|
| `bleradar-core` | `rlib` + `cdylib` | every rule and engine, 25,180 lines in 24 source files, flat re-exports at the crate root (18 `pub use` blocks) | nothing |
| `bleradar-jni` | `cdylib` + `lib` | the JNI façade loaded by the app as `libbleradar_jni.so` | `bleradar-core` |
| `bleradar-compat` | `lib` | native-ABI census, parity registry, executed-oracle differential tests | `bleradar-core` (its tests only) |
| `xtask` | bin, **its own workspace** | dev tooling and gates; depends on no workspace crate | nothing |
| `migration/critically-enhanced-v0.3.0` | excluded from the workspace | frozen v0.3.0 snapshot | n/a |

Outside this repository, HSE depends on `bleradar-core` through a pinned git
`rev` and uses only `sweep` (`docs/REPOSITORY_BOUNDARY.md`).

### `bleradar-core` modules, by layer

The layer is recorded in `xtask/src/boundary.rs` (`LAYERS`).

| Layer | Modules (lines) | Total |
|---|---|---|
| **Radar**: what the app and HSE use, plus the radar's own scheduling and capability records | crate root `lib.rs` (433: Wi-Fi channel/band/distance/security), `adv` (1,129), `update` (1,410), `identity` (725), `tracking` (585), `sweep` (555), `history` (435), `signal` (384), `registry` (384), `scan` (274), `runtime` (217), `geo` (122) | 6,653 (26%) |
| **HseModel**: the HSE entity model | `entity` (2,013), `coords` (691), `tags` (198) | 2,902 (12%) |
| **Engine**: the non-radar engines from the v0.3.0 reconstruction | `evidence` (3,824), `infrastructure` (2,713), `website` (2,581), `osint` (1,786), `verification` (1,694), `advancement` (1,107), `fusion` (1,058), `pipeline` (834), `validation` (28, shared helper) | 15,625 (62%) |

### Who depends on whom (non-test code; comments and intra-doc links excluded)

This is the output of `cargo xtask check-crate-boundary` at `d0b1e8d`:

```
(crate root) [Radar]  -> (none)
adv          [Radar]  -> (none)
geo          [Radar]  -> (none)
history      [Radar]  -> (none)
identity     [Radar]  -> adv, sweep
registry     [Radar]  -> (none)
runtime      [Radar]  -> (none)
scan         [Radar]  -> (none)
signal       [Radar]  -> (none)
sweep        [Radar]  -> (crate root), identity, signal
tracking     [Radar]  -> geo, signal
update       [Radar]  -> entity                      <- the only radar -> HSE-model edge
coords       [HseModel] -> geo
entity       [HseModel] -> coords, geo, tags
tags         [HseModel] -> (none)
validation     [Engine] -> (none)
evidence       [Engine] -> tracking, validation        (tracking: Confidence)
fusion         [Engine] -> evidence, tracking, validation
infrastructure [Engine] -> evidence, tracking, validation
osint          [Engine] -> evidence, tracking, validation
verification   [Engine] -> evidence, validation
advancement    [Engine] -> tracking, validation, verification
website        [Engine] -> evidence, infrastructure, tracking, validation
pipeline       [Engine] -> evidence, geo, tracking, validation  (Confidence, LatLon, Timestamp)

bleradar-jni reaches:    (crate root), adv, coords, entity, geo, history, identity, scan, signal, sweep, tags, tracking, update
bleradar-compat reaches: (crate root), geo, signal
```

What this shows:

1. **The direction is already clean.** No Radar or HseModel module reaches an
   engine. The engines depend on the radar only for small value types:
   `tracking::Confidence`, `geo::LatLon`, and `evidence::Timestamp`, which is
   engine-internal.
2. **`bleradar-jni` reaches no engine.** Its imports are `adv`, `update`,
   `signal`, `tracking`, `scan`, `history`, `identity`, `sweep` and
   `entity::hex_encode`. It pulls in the HSE entity model through
   `update -> entity` (`Sha256`, `hex_encode`) and through its own direct
   `hex_encode` import, and `entity` in turn pulls in `coords` and `tags`.
   This is the one tangle on the radar side.
3. **The shipped library already excludes the engines.** The release profile
   is `lto = true, opt-level = "z", strip = true`, so unreachable code is
   dropped. Measured against `libbleradar_jni.so` from the committed
   `HSE-BLE-Radar-arm64-v1.0.0.apk` (523,728 bytes, SHA-256 `220af46f…e7d7d2`):
   none of the engines' string literals of 12 characters or more are present
   (osint 0/15, website 0/46, infrastructure 0/23, evidence 0/42, pipeline 0/17,
   verification 0/8, advancement 0/6, fusion 0/4). The radar's are present
   (adv 28/35, update 11/37), and the HSE model has 1/36 (`entity`'s hex
   table, via `hex_encode`).
   **So a split will not shrink the APK.** What it buys is a smaller compile
   and audit surface for the app (62% of `bleradar-core`'s lines are engines),
   an API that says what the radar is, and a clean unit to relocate later.
4. **Tests.** Of the workspace's 571 tests, 142 integration tests are
   engine-only: `advancement*` 12, `evidence*` 20, `fusion*` 14,
   `infrastructure*` 16, `osint*` 14, `pipeline` 27, `verification*` 13,
   `website*` 17, `properties_engines` 6 and `composition` 3. Another 127 are
   `entity`, plus unit tests inside `infrastructure` and `pipeline`. The rest
   are radar, JNI and compat tests.

## 2. Proposed staged split

Each step is one PR. It lands only when its gate is green and CI (gates,
android-apk, web-dashboard, android-emulator) passes. Steps 1 to 3 keep
`bleradar-core`'s public API source-compatible for HSE, so HSE's pinned `rev`
keeps building without changes.

### Step 0: record the boundary (this PR, done)

- **Change:** this document, plus `cargo xtask check-crate-boundary`
  (`xtask/src/boundary.rs`), added to `cargo xtask gates` and therefore to CI.
  It reads the sources and fails when:
  - a Radar or HseModel module reaches an Engine module;
  - a Radar module gains an HseModel edge that isn't in
    `ALLOWED_RADAR_TO_HSE_MODEL` (today only `update -> entity`);
  - a recorded allowance no longer exists. The list works as a ratchet and
    can only shrink;
  - `bleradar-jni` or `bleradar-compat` transitively reaches an engine;
  - a module is unclassified, or a classification names a module that is gone;
  - a `crate::`/`super::`/`bleradar_core::` path can't be resolved. The check
    fails loudly rather than miss an edge.
- **Why it is safe:** `xtask` is its own workspace and no shipped crate
  depends on it. No file under `crates/` or `android/` changes, so
  `libbleradar_jni.so` and the committed APK stay byte-identical. CI's
  android-apk reproduce check confirms that.
- **Gate:** 10 unit tests in `boundary.rs`. Nine use fixtures for the lexer,
  the `use`-group parser, `lib.rs` re-export resolution, and each rule. One
  holds the committed tree to the recorded boundary. The check was falsified
  by hand: each of the following fails it with a message naming the edge,
  and restoring the file passes it again:
  - an engine import added to `signal`;
  - an engine type added to `bleradar-jni`'s imports;
  - a new `history -> tags` edge;
  - a `super::validation` path in `coords`.
  An engine path that appears only in a doc comment and a string literal
  passes, as it should.

### Step 1: cut the radar's only edge into the HSE model

- **Change:** move `Sha256` and `hex_encode` from `entity` into a small Radar
  module (for example `digest`). Keep `entity` re-exporting them, so
  `bleradar_core::{Sha256, hex_encode}` and `entity::…` paths still resolve
  for HSE. Point `update` and `bleradar-jni` at the new home. Empty
  `ALLOWED_RADAR_TO_HSE_MODEL`, after which the ratchet forbids any radar ->
  HSE-model edge.
- **Risks:** the code is identical, but it moves within the crate, so the
  stripped LTO `.so` may still change bytes through panic-location strings or
  layout. That means a **committed-APK rebuild**. SHA-256 behaviour must stay
  bit-identical.
- **Gate:** `check-crate-boundary` with an empty allowance, and bleradar-jni
  reaching neither `entity`, `coords` nor `tags`. Then `cargo test --workspace`
  (571 or more), `verify-jni-live`, `verify-jni-target` (aarch64/Bionic under
  qemu), `check-jni-contract` (42 ↔ 42 exports), and `verify-android-live`
  reproducing the rebuilt APK. `tests/update.rs` and
  `tests/update_campaign.rs` must pass unchanged.

### Step 2: feature-gate the engines inside `bleradar-core`

- **Change:** add `[features] default = ["engines", "hse-model"]` with empty
  `engines` and `hse-model` features. Put `#[cfg(feature = "engines")]` on the
  engine `mod` declarations and their `pub use` blocks, and the same for the
  HSE model. Engine integration tests get `required-features`.
  `bleradar-jni` and `bleradar-compat` depend on `bleradar-core` with
  `default-features = false`.
- **Risks:**
  - Cargo unifies features across the packages in one build. Under
    `cargo build --workspace` the engines are still compiled for
    `bleradar-jni`. They are off only in `-p bleradar-jni` builds, which is
    what `build-apk` does.
  - HSE pins a `rev` and gets the default features, so nothing changes for it.
  - `docs/CAPABILITY_MATRIX.md` and `registry` name engine modules and must
    keep rendering.
  - Expect another APK rebuild, because Cargo metadata can change.
- **Gate:** `cargo check -p bleradar-core --no-default-features`,
  `cargo test -p bleradar-core --all-features`, and
  `cargo check -p bleradar-jni` with the engines off. Add these to `gates`.
  Also `check-crate-boundary`, the 571-test total conserved,
  `verify-android-live` reproducing, and `cargo deny`/`cargo audit` green.

### Step 3: extract the engines into their own crate

- **Change:** a new workspace member, `crates/bleradar-engines`, depending on
  `bleradar-core` for `Confidence` and `LatLon`. Move `evidence`, `fusion`,
  `infrastructure`, `osint`, `verification`, `advancement`, `website`,
  `pipeline` and `validation` there, along with their 142 integration tests
  and their unit tests. `bleradar-core` keeps a deprecated
  `engines` feature that re-exports `bleradar_engines::*` for one release, so
  HSE can move on its own schedule. Decide separately whether the HSE model
  (`entity`, `coords`, `tags`) goes with the engines or into its own crate.
  The gate already proves no radar code needs it after step 1.
- **Risks:**
  - Intra-doc links in `pipeline.rs` (`[`crate::osint`]` and similar) break
    under `RUSTDOCFLAGS=-D warnings` and must be rewritten.
  - `check-dependency-policy` keeps an exact workspace-crate allowlist and
    `deny.toml` covers crate licences; both need the new crate (it is
    `publish = false`).
  - `Cargo.lock` changes.
  - The `registry` capability rows name module paths.
  - The re-export would create a dependency cycle (core -> engines -> core).
    Avoid it by having HSE depend on `bleradar-engines` directly if it ever
    needs the engines. It doesn't today.
- **Gate:** `cargo tree -p bleradar-jni` shows no `bleradar-engines`. Also
  `check-crate-boundary`, extended to classify crates, the per-file test
  counts conserved (571 in total), `check-dependency-policy`, `cargo deny`,
  `cargo doc -D warnings`, and `verify-android-live` reproducing.

### Step 4 (separate decision): relocate the engines

`docs/REPOSITORY_BOUNDARY.md` records the engines as "not reached by the
Android app and not consumed by HSE … relocating it is a separate decision".
After step 3 that decision is a crate move, either into HSE (which owns OSINT)
or into an archive repository. It needs its own record and HSE's agreement,
and it is outside this plan's scope.

## 3. What the first step deliberately does not do

- It moves no code and changes no crate's manifest, so the native library
  doesn't change and the committed APK doesn't need a rebuild.
- It doesn't use `cargo metadata` or a `syn` parse. `xtask` is dependency-free
  by policy (`check-dependency-policy`), so the check is a small lexer:
  comments and literals are blanked, `#[cfg(test)] mod` bodies are dropped,
  and `use` groups are read with aliases. Unit tests cover each of those, and
  any path it can't resolve fails the gate instead of being skipped.
