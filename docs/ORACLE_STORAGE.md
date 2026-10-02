# Oracle storage

Where the immutable v0.3.0 inputs live, how they are verified, and what is
recommended next. Nothing here changes an oracle's bytes: every file below has
the same SHA-256 it had when it was first committed.

## One canonical directory: `oracle/`

| File | SHA-256 | What it is |
|---|---|---|
| `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` | `dc1c5129…d467e255` | the original release APK, the behavioural oracle (also pinned by `docs/INPUT_SHA256.txt`) |
| `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0.zip` | `07d2d80c…28cf3f76` | the byte-pinned migration archive |
| `libbleradar_core.so` | `d14022cd…b980b12d` | the native oracle, the same bytes as the archive's `oracle/libbleradar_core.so` (what `cargo xtask oracle-differential` executes under qemu) |
| `classes.dex` | `a1311463…d3feeccc` | the original DEX, the same bytes as inside the archive |
| `git-history.bundle` | `e93a359b…e94abc6c` | the recovery history (`archive/migration-v0.3.0`, tags `migration/*`, `recovery/*`) |

Full digests: `oracle/SHA256SUMS` (`sha256sum` format; `cd oracle && sha256sum -c SHA256SUMS`
checks it by hand).

### What `cargo xtask check-oracle-integrity` (a `gates` step) enforces

1. The APK matches `docs/INPUT_SHA256.txt` and the archive matches its pinned
   baseline (unchanged from before this layout).
2. Every file listed in `oracle/SHA256SUMS` exists as a regular file (a
   symlink is never followed) with exactly that digest.
3. Every entry in `oracle/` is listed: a file, subdirectory or symlink that is
   not pinned fails, so nothing unpinned can sit beside the oracles.
4. The manifest agrees with the gate's own independent pins for all five
   files (the APK via `docs/INPUT_SHA256.txt`; the archive, native oracle, DEX
   and history bundle via constants in `xtask/src/main.rs`), so editing the
   manifest cannot bless a changed oracle.
5. `migration/critically-enhanced-v0.3.0/SHA256SUMS` (the checked-in
   reconstruction's own manifest) verifies, and each of its copies of the
   canonical files is byte-identical to its own counterpart. The gate maps
   every copy explicitly (`SNAPSHOT_ORACLE_COPIES` in `xtask/src/main.rs`):

   | Snapshot copy | Canonical counterpart |
   |---|---|
   | `./oracle/BLE-Radar-v0.3.0-original.apk` | `oracle/BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` |
   | `./oracle/classes.dex` | `oracle/classes.dex` |
   | `./oracle/libbleradar_core.so` | `oracle/libbleradar_core.so` |
   | `./git-history.bundle` | `oracle/git-history.bundle` |

   Each copy must be listed, its file must match its line, and its line must
   carry exactly its counterpart's digest from `oracle/SHA256SUMS`. Another
   canonical file's digest does not count. Any other entry under `./oracle/`,
   or with a canonical file's name elsewhere in the snapshot, is an unmapped
   copy and fails. Those are copies, never a second, divergent oracle.
6. Every file in the snapshot is listed in its `SHA256SUMS`: a file, symlink
   or anything else that is not listed fails, at any depth. Only the manifest
   itself and the gitignored local build directory `target/` at the
   snapshot's top level are skipped.

Each branch was falsified before merging: a byte appended to `classes.dex`, a
stray unpinned file in `oracle/`, an unpinned subdirectory, an unpinned
symlink, a byte appended to the snapshot's oracle copy, the snapshot's history
bundle changed together with its own manifest line, the snapshot's APK copy
replaced by the bytes of `classes.dex` together with its own manifest line,
`classes.dex` and its snapshot copy changed together with both manifest lines,
a stray unlisted `oracle/evil.so` in the snapshot, the
APK removed, and the archive's manifest line edited to a different digest each
fail the gate with a message naming the file; restoring the tree passes it
again.

## What moved, and why it was safe

| Before (repo root) | After |
|---|---|
| `BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` | `oracle/BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk` |
| `BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip` | `oracle/BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0.zip` (no space, no ` (1)`) |
| `git-history.bundle` | `oracle/git-history.bundle` |

- Pure renames (`git mv`): the blobs, and so every SHA-256, are unchanged; git
  stores them once, so the move adds nothing to the repository's size. History
  is untouched; the old paths remain reachable in every earlier commit.
- Every reference was updated in the same change: `xtask/src/main.rs` (the
  integrity gate and `oracle-differential`'s archive extraction, which reads
  the archive's internal entry `oracle/libbleradar_core.so`, unchanged),
  `README.md`, `docs/ORACLE_DIFFERENTIAL.md`, `docs/REQUIREMENTS_LEDGER.md`.
  Mentions of the APK's *file name* as provenance (`docs/ANDROID_APP.md`,
  `docs/PHASE0_AUDIT.md`, the comment in `AndroidManifest.xml`) still hold,
  since the name did not change; the manifest comment was deliberately left
  alone because it is packaged into the shipped APK. Historical entries in
  `docs/AUTONOMOUS_DECISIONS.md` record the paths of their time and are not
  rewritten.
- No CI workflow names these files, and the shipped
  `HSE-BLE-Radar-arm64-v1.0.0.apk` stays at the root untouched.

## Duplicates that remain, deliberately

The original APK exists in three places with one digest: `oracle/` (canonical),
inside the archive, and as `migration/critically-enhanced-v0.3.0/oracle/BLE-Radar-v0.3.0-original.apk`.
The native oracle, DEX and history bundle likewise have a snapshot copy. The
archive must stay byte-for-byte (it is the baseline), and the snapshot is a
verbatim extraction whose own `SHA256SUMS` lists those copies, so deleting
either would break a recorded manifest for no storage gain: identical blobs
are stored once by git. The gate now proves the copies are identical instead.

## Recommended follow-up (not done here)

About 49.6 MB of immutable binaries are committed (the APK 12.8 MB, the archive
16.5 MB, the native oracle 10.8 MB, the bundle 6.6 MB, the DEX 2.9 MB), and every
clone carries them. Two options, either a deliberate decision of its own:

- **GitHub release assets.** Publish the five files once on a dedicated,
  immutable release (for example `oracle-v0.3.0`), keep `oracle/SHA256SUMS`
  in the tree as the authority, and have `check-oracle-integrity` (and
  `oracle-differential`) fetch and verify by digest when a file is absent.
  Clones get small; the gate needs network or a cache.
- **Git LFS.** Not used by this repository today. Adopting it changes clone
  behaviour (`git lfs install`, LFS bandwidth quotas) and CI
  (`actions/checkout` with `lfs: true`), and it does not shrink history that
  already holds the blobs without a history rewrite, which this repository
  does not do.

Either way the digests in `oracle/SHA256SUMS` stay the contract, so the move
can be verified byte-for-byte.
