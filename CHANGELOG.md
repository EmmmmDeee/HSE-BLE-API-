# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The app version (`APP_VERSION_NAME` in `xtask/src/main.rs`) is the release
version. Release policy: only pre-releases (`main-<sha7>` plus a rolling
`latest`); a stable release needs the owner's explicit approval.

## [Unreleased]

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
- `CHANGELOG.md`.

### Changed

- The emulator proof is pinned to emulator 37.2.12 (checked against its pinned
  archive size and SHA-256) and launched with `-feature -WiFiPacketStream`,
  so guest Wi-Fi stays off netsimd (#48).
- `docs/DEVELOPMENT.md` records the verified toolchain setup: Rust 1.98.0,
  Temurin 21, Android platform 36, build-tools 37.0.0 and NDK 27.3.13750724
  (#44).
- Documentation synced with `main` at `8a0ab6e`: release policy and the
  release workflow's actual behaviour, the emulator and system-image pins,
  the full `cargo xtask` command list, the Java unit-test classes (7 classes,
  115 tests), the committed APK's size and entries, and the current state of
  HSE's side of the repository boundary. Historical records keep the
  emulator 37.1.11 they ran on, with a note on the current 37.2.12 pin.

### Fixed

- Holes found by an independent review in the loopback API and the Wi-Fi
  survey (#40).
- `android-emulator` on `main`, which went red when sdkmanager's emulator
  channel moved to 37.2 (#48).

### Known issues

- `.github/workflows/release.yml` publishes a `v<version>` release with
  `--latest` and force-moves its tag, which does not match the pre-release
  policy. GitHub shows `v1.0.0` as a pre-release, so
  `releases/latest/download/release_manifest.txt` answers `404` and the
  in-app check uses the bundled manifest.

## [1.0.0] - 2026-09-30

Published as a GitHub pre-release. The release workflow has since moved the
`v1.0.0` tag to later commits on `main`, most recently `8a0ab6e`.

### Added

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

- Rust 1.98 / rustup PATH onboarding clarified (#35).
