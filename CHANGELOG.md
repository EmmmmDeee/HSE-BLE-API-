# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The app version (`APP_VERSION_NAME` in `xtask/src/main.rs`) is the release
version. Release policy: only pre-releases (`main-<sha7>` plus a rolling
`latest`); a stable release needs the owner's explicit approval.

This changelog starts at #30. For earlier history, see
`docs/AUTONOMOUS_DECISIONS.md` and the git log.

## [Unreleased]

Changes merged to `main` after the `v1.0.0` release (see `[1.0.0]` below
for what that release contains).

### Added

- `CHANGELOG.md`.
- `cargo xtask verify-apk-rebuild`, run by CI's `android-apk` job after the
  build: the committed APK must equal the fresh build once the APK Signing
  Block is removed from both. Every entry's bytes, the central directory and
  the end record are compared; entry order is free (`zip` stores the build
  host's directory-read order). The check is signer-agnostic and fails closed
  on a malformed, commented, ZIP64 or unsigned package. `--byte-identical`
  also requires the same entry order. The same CI step first runs
  `apksigner verify --print-certs` on the committed APK, as
  `verify-android-live` does on the fresh build, so a committed package whose
  signature does not verify fails the job; no signer is pinned.

### Changed

- The `release` workflow publishes each `main` build as the `main-<sha7>`
  pre-release and moves the rolling `latest` pre-release to it. It never
  creates, edits or re-tags a stable release, and it no longer touches
  `v1.0.0` (#50).
- Documentation synced with `main`: release policy and the release
  workflow's behaviour, the emulator and system-image pins, the full
  `cargo xtask` command list and its prerequisites, the Java unit-test classes
  (7 classes, 115 tests), the committed APK's size and entries, and the
  current state of the repository boundary with HSE. Historical records keep
  the emulator 37.1.11 they ran on, with a note on the current 37.2.12 pin.
- The immutable v0.3.0 inputs (the original APK, the migration archive,
  renamed without ` (1)` with the same bytes, and `git-history.bundle`)
  moved into `oracle/`, where every file is pinned by `oracle/SHA256SUMS`.
  `check-oracle-integrity` now verifies that manifest, requires the
  snapshot's `SHA256SUMS` to list every snapshot file, and requires each
  snapshot copy of an oracle to be byte-identical to its counterpart. Only
  untracked build output in the snapshot's gitignored `target/` is skipped;
  a file force-added there must be listed (#45).

### Fixed

- The emulator's upgrade proof no longer leaves a path to the real
  `github.com` open before its first check. The global HTTP proxy reaches
  the guest's network stack asynchronously (and on API 34 nothing marks the
  moment), so a launch before it took hold could have gone direct. The proof
  now installs a guest `iptables`/`ip6tables` egress guard for the app's and
  `DownloadManager`'s UIDs right after the proof build is installed: only
  loopback and the host alias `10.0.2.2` (the stand-in's proxy) are allowed,
  everything else is rejected, so such a check fails closed. The acceptance
  signal is the stand-in's traffic, not the proxy-setting line in the
  report: the run fails if anything reached the stand-in before the launch,
  if the stand-in answered any request beyond the three the proof asks for,
  or if the guard rejected any packet.
- `android-sdk-install --emulator` now verifies the emulator that sdkmanager
  installed. It keeps the emulator only when its `package.xml` and
  `source.properties` name 37.2.12 and the SHA-256 of seven of its files
  (`PINNED_EMULATOR_FILES`) matches the pinned archive. Otherwise it fetches
  the pinned archive over HTTPS only, checks the archive's size and SHA-256,
  swaps it in and verifies the result again. If the swap or that check fails,
  the previous emulator is restored, or else left at `.emulator-pin-previous`
  and named in the error (#49).
- The `release` workflow tries each release-asset download in
  `verify_assets` up to 3 times in total (2 retries, 5 s and then 15 s
  apart), after a transient HTTP 500 on an asset download failed run
  37055587714. Verification stays strict: only the download is retried, every
  checksum and content check still fails at once, and the job fails if all
  three attempts fail (#56).
- `scripts/scan-for-keys.sh`, the release key scan, fails closed. It exits 2
  and prints no finding count when it cannot actually scan: a required tool
  (`find`, `grep`, `cat`, `strings`, `python3`, `mktemp`) is missing, no path
  is given or a given path does not exist, a file cannot be read, a `find`,
  `grep`, `cat`, `strings` or `python3` call errors, or zero files were
  scanned. Before, each of these could end in `key scan: 0 finding(s)` and
  exit 0. Rule matches no longer run through a `grep -Eo | grep -Evq`
  pipeline: once the matches passed 64 KiB, the second grep's early exit
  killed the first with SIGPIPE and `pipefail` read that as no finding, so a
  log with 3000 copies of a token passed. Each grep now runs on its own and
  its exit status is checked. Files are listed NUL-delimited, so a path with
  spaces or newlines is no longer skipped, and a clean scan also prints
  `files scanned: N`. `scripts/test-scan-for-keys.sh` checks this behaviour as
  a step of the `gates` job (#55).
- `scripts/scan-for-keys.sh` also scans the UTF-16LE and UTF-16BE strings
  of binaries (`strings -e l` / `-e b`, each failing closed), so a key in
  compiled Android XML such as `AndroidManifest.xml` is no longer missed.
  Symlinks are contained: every root is resolved with `realpath -e` (a root
  given as a symlink is scanned as its target), and every symlink under the
  roots is resolved without being followed before the walk. A link that is
  broken, loops or resolves outside every scanned root (for example to `/`,
  `/proc/self/environ`, a `../..` escape or a `root-evil` sibling of `root`)
  exits 2. Links that stay inside the roots are followed and scanned as their
  targets, and a file reached through several links is scanned and reported
  once. Before, every symlink was skipped silently, so a linked token file
  passed as `0 finding(s)`. `realpath` is now a required tool. A finding
  names the path it was reached by, followed by `(resolved: <path>)` when a
  symlink was crossed (a root given as a symlink keeps its own name), and
  never the matched value. A relative root is passed on as `./<root>`, so a
  root named `-L`, `-H` or `-P` is scanned as a path instead of being parsed
  as a `find` option, which walked `.` instead (#57).
- The emulator proof judges the first launch's update check on that check's
  log lines accumulated from the launch on, so the logcat ring buffer can no
  longer rotate them out before they are read. The guest's logcat buffers are
  raised to 16M (best-effort, outcome reported) (#52).
- `check-oracle-integrity` removes `GIT_DIR`, `GIT_WORK_TREE` and
  `GIT_INDEX_FILE` from the environment of the `git ls-files` call that lists
  the files tracked under the snapshot's `target/`. Set in the caller's
  environment to another repository, another work tree or a missing index,
  they made git report no tracked files with exit 0, so a force-added file
  there passed unlisted. The docs now say that "tracked" means in git's index
  and that the check needs `git` on `PATH` and a git checkout (#59).
- The vendored advisory-db materialization (`cargo deny` setup in `gates`)
  runs its `git init` / `add` / `commit` without the caller's `GIT_DIR`,
  `GIT_WORK_TREE` and `GIT_INDEX_FILE`, which pointed them at another
  repository, work tree or index.

### Known issues

- The in-app update check reads
  `releases/latest/download/release_manifest.txt`, which GitHub resolves only
  to a stable release. While only pre-releases exist it answers `404`, and
  the check uses the bundled manifest.

## [1.0.0] - 2026-09-29

Published on 2026-09-29 (UTC) as a stable release marked Latest, from
`98c4b08`; converted to a pre-release on 2026-10-01 (18:24–18:34 UTC), after
which `releases/latest` answers 404.
`98c4b08` is #36's merge commit. Until #50, the release workflow re-pointed
the `v1.0.0` tag and replaced its assets on later `main` builds, since the app
version stayed 1.0.0. Its last move was to `8a0ab6e` (#44), on 2026-10-01 just
before the conversion, and the release's APK asset (598,755 bytes, SHA-256
`4e97dbd3…07d7`) is byte-identical to the APK committed at `8a0ab6e`. The
published `v1.0.0` therefore contains everything through #44: #30–#44 and #48
below. Since #50 the workflow no longer touches `v1.0.0`. Main builds are
published as `main-<sha7>` pre-releases (the first was `main-49b0c97`) and the
rolling `latest` pre-release.

### Added

- ATT&CK capability ledger v0 (`bleradar_core::capability`,
  `docs/CAPABILITY_LEDGER.md`). Coverage status is derived from an evidence
  chain only, with no manual promotion. `Verified` requires a regression
  lock (#43).
- Every manufacturer the Bluetooth SIG has assigned, loaded from the SIG's
  own company-identifier file, plus `cargo xtask sync-company-ids` (#42).
- The scan follows the Bluetooth adapter: `/api/status` reports `idle`,
  `scanning`, `recovering` or `failed` and why (#41).
- Passive Wi-Fi survey through the radar's own rules (`/api/wifi`, ABI 19),
  remembered across sessions and carried across reinstalls by Android Auto
  Backup (#38, #39).
- `docs/REPOSITORY_BOUNDARY.md`, a written and enforced boundary with the
  Huntsman Search Engine (#37).
- HSE radar incorporated: Rust-owned BLE identity across address rotation,
  advertisement decoding, device history, and automated APK releases
  (ABI 18) (#36).
- v0.3.0 critically-enhanced migration archive incorporated, with its Python
  and shell tools replaced by Rust (`bleradar-tools`) (5370767, 2d5869a).
- The app's own upgrade pathway proven on the emulator against a stand-in
  github.com (#34). The app's unit tests run on the host JVM against the real
  native core (#33). One version authority and a byte-reproducible APK
  (#32). The automatic-update pathway completed (#31). A virtual BLE
  advertiser on netsimd's HCI socket (#30).

### Changed

- The emulator proof is pinned to emulator 37.2.12 and launched with
  `-feature -WiFiPacketStream`, so guest Wi-Fi stays off netsimd. A revision
  other than 37.2.12 is replaced by the pinned archive, checked against its
  size and SHA-256 (#48).
- `docs/DEVELOPMENT.md` records the verified toolchain setup: Rust 1.98.0,
  Temurin 21, Android platform 36, build-tools 37.0.0 and NDK 27.3.13750724
  (#44).
- Rust 1.98 / rustup PATH onboarding clarified (#35).

### Fixed

- Holes found by an independent review in the loopback API and the Wi-Fi
  survey (#40).
- `android-emulator` on `main`, which went red when sdkmanager's emulator
  channel moved to 37.2 (#48).
