# Repository boundary: the HSE BLE Radar and HSE

Two repositories, two purposes. **They are currently independent: neither depends on the other.** This file was last checked on 2026-10-04 against HSE `main` at `bd0ccad8` and this repository's `main` at `5aa9b94`.

| | **HSE BLE Radar** — this repository | **Huntsman Search Engine (HSE)** — [`EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-`](https://github.com/EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-) |
|---|---|---|
| Purpose | A standalone Android ARM64 wireless-intelligence app (BLE radar today) and the safe-Rust engine library behind it (`bleradar-core`) | The single `huntsman-recon` crate: a CLI with local search, a hash-chained ledger, GEOINT/identity utilities and a guarded fetch layer, for Termux on Android aarch64 |
| Ships | An installable `HSE-BLE-Radar-arm64-<version>.apk`, attached to a GitHub release (release policy: pre-releases only; see the README's "Releasing") | A source build (`cargo build --release`), or the `huntsman-recon-aarch64-linux-android` CI artifact |
| Owns | BLE/Wi-Fi reading rules, advertisement decoding, identity across address rotation, device history, signal/proximity math, the self-updater | Its own recon core. It keeps its own copies of any wireless rules it has (see "Duplicated rules") |
| Dependencies on the other | None | None. HSE's `Cargo.toml` depends on `serde`, `serde_json`, `thiserror` and `ureq` only |

## Dependency direction

```
HSE        ──────────── nothing from the radar (currently) ───────▶ (none)
BLE Radar  ──────────── nothing from HSE, ever ─────────────▶ (none)
```

- The radar never depends on HSE. `cargo xtask check-dependency-policy` (a `gates` step) fails when `Cargo.lock` holds any crate outside the audited workspace set, and `xtask`'s `no_workspace_manifest_depends_on_hse` fails on any manifest that names an HSE crate or a git source.
- HSE does not currently depend on `bleradar-core`: no HSE manifest that is built names it, and no HSE code outside `legacy/` refers to it. Current HSE has no `src/modules/` and no `install.sh`.
- Earlier HSE trees did depend on it, through a git dependency pinned to a commit. They are history, not a current dependency, and the two snapshots used different rules:
  - **`legacy/hse-monolith-v1.41.0/`** (a read-only reference copy in HSE, not built) pins `bleradar-core` rev `1e47bca`. That rev has no `sweep` module. The copy calls `bleradar_core::wifi_frequency_to_channel`, `proximity_label` and `ProximityBand` (`src/modules/signal_radar/wifi.rs`, `mod.rs`).
  - **The last tree before HSE's reconstruction** (HSE `f0a1c64c^`, after HSE #657) pinned rev `98c4b08` and called the `sweep` rules listed below, `sighting_key` included, from `src/modules/signal_radar/wifi.rs` and `bluetooth.rs`. HSE `f0a1c64c` ("Make huntsman-recon the only current tree") removed that tree.

## Rules the radar could offer HSE (`bleradar_core`, module `sweep`)

These are exported from `bleradar-core` with tests. **No HSE code consumes them today.** The column records what HSE's pre-reconstruction tree (`f0a1c64c^`, rev `98c4b08`) used each one for; the `legacy/` v1.41.0 copy used none of them.

| Rule | What HSE `f0a1c64c^` used it for |
|---|---|
| `is_real_device_address` | dropping placeholder / malformed BSSIDs and Bluetooth addresses |
| `wifi_rssi_reliability` | the Wi-Fi observation confidence tier |
| `wifi_channel` | the `channel:<n>` tag |
| `wifi_proximity` | the `proximity:<band>` tag |
| `sighting_key` | the key each Wi-Fi and Bluetooth sighting was stored under, instead of the scan's raw address |

## Duplicated rules

Because the repositories are independent, a rule can exist in both. Known case:

- **`tower_id`** (the `mcc-mnc-lac-cid` cell tower id) is implemented twice: in HSE as `src/rf.rs:195` (`tower_id(mcc, mnc, lac, cid)` over `Display` values), and here as `crates/bleradar-core/src/sweep.rs:296` (`tower_id(mcc: &str, mnc: &str, area_code: i64, cid: i64)`). Both produce `{mcc}-{mnc}-{lac}-{cid}`. Neither repository depends on the other's copy, and neither copy is called from a shipped path: the app scans BLE and Wi-Fi only, and no HSE command calls it.

## Changing the boundary

- **To re-establish a seam:** add `bleradar-core` to HSE's `Cargo.toml` as a git dependency pinned to a full commit `rev` (never a branch or tag), call the `sweep` rules from a wired HSE command, and delete HSE's duplicate (for example `src/rf.rs` `tower_id`). Then update this file.
- **A rule the radar needs from HSE:** copy nothing. If it is a reading rule, it belongs here already; if it is OSINT, it does not belong in the app.
- **The radar's cell rules** (`sweep`: cell identity, `tower_id`, `usable_dbm`) have no caller today; the capability registry (`sensor-rules`) records that. They are kept as tested reading rules for a future cell surface. Removing them, or making one `tower_id` the single authority, is a separate decision.
- **The engine library** (`bleradar-core`'s evidence, OSINT-frontier, infrastructure, website, fusion, verification and pipeline engines, from the v0.3.0 reconstruction) is not reached by the Android app and is not consumed by HSE. It is retained as tested library code; relocating it is a separate decision, not implied by this boundary.
