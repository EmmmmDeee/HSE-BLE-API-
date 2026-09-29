# Repository boundary: the HSE BLE Radar and HSE

Two repositories, two purposes. Each stays distinct; each uses the strongest part of the other through one narrow, tested seam.

| | **HSE BLE Radar** — this repository | **Huntsman Search Engine (HSE)** — [`EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-`](https://github.com/EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-) |
|---|---|---|
| Purpose | A standalone Android ARM64 wireless-intelligence app (BLE radar today) and the safe-Rust engine library behind it (`bleradar-core`) | All-source OSINT / GEOINT / NETINT reconnaissance: 194 modules, one CLI/Web-UI binary, run in Termux on Android aarch64 |
| Ships | An installable `HSE-BLE-Radar-arm64-<version>.apk`, published as a GitHub release | A Termux binary (`install.sh`, source build) |
| Owns | BLE/Wi-Fi reading rules, advertisement decoding, identity across address rotation, device history, signal/proximity math, the self-updater | Modules, scan engine, correlator rules, storage, the HTTP API and UI, the installer |
| Does not own | Any OSINT module, scan engine, or Termux tooling | Any radio-reading rule the radar owns |

## Dependency direction

```
HSE  ──(git dependency, pinned to a commit)──▶  bleradar-core
BLE Radar  ──────────── nothing from HSE, ever ─────────────▶ (none)
```

- The radar never depends on HSE. `cargo xtask check-dependency-policy` (a `gates` step) fails when `Cargo.lock` holds any crate outside the audited workspace set, and `xtask`'s `no_workspace_manifest_depends_on_hse` fails on any manifest that names an HSE crate or a git source.
- HSE depends on `bleradar-core` alone, pinned in HSE's `Cargo.toml` to a full commit `rev` (never a branch or tag).
- A rule has one authority. The radar owns every reading-interpretation rule — real-vs-placeholder address, RSSI reliability tiers, 802.11 channel, proximity band, BLE address type, advertisement decoding, identity across rotation. HSE owns everything else, and calls the radar for those rules rather than copying them.

## What the radar offers HSE (`bleradar_core`, module `sweep`)

| Rule | HSE uses it for |
|---|---|
| `is_real_device_address` | dropping placeholder / malformed BSSIDs and Bluetooth addresses |
| `wifi_rssi_reliability` | the Wi-Fi observation confidence tier |
| `wifi_channel` | the `channel:<n>` tag |
| `wifi_proximity` | the `proximity:<band>` tag |

HSE consumes these only in its `src/modules/signal_radar/`; its own architecture test enforces that. See HSE's `docs/REPOSITORY_BOUNDARY.md` for its side.

## Changing the seam

- **A rule HSE needs from the radar:** add it to `bleradar-core` here with its tests, merge it to `main` with CI green, then bump the pinned `rev` in HSE.
- **A rule the radar needs from HSE:** copy nothing. If it is a reading rule, it belongs here already; if it is OSINT, it does not belong in the app.
- **The radar's Wi-Fi and cell rules** (`sweep`) have no in-app caller yet — the app scans BLE only — and exist because HSE consumes them; the capability registry says so. They stay until the app gains a Wi-Fi surface or HSE stops consuming them.
- **The engine library** (`bleradar-core`'s evidence, OSINT-frontier, infrastructure, website, fusion, verification and pipeline engines, from the v0.3.0 reconstruction) is not reached by the Android app and is not consumed by HSE. It is retained as tested library code; relocating it is a separate decision, not implied by this boundary.
