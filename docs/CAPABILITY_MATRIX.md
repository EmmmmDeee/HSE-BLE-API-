# Capability supersession matrix

Generated from `bleradar_core::registry` by its snapshot test — edit the registry, not this file.

Statuses are conservative: without a head-to-head competitor benchmark or physical-device validation, a capability that works, is live and is tested is `PARTIAL` with its limit named, not `SUPERIOR`. `SUPERIOR`/`PARITY` require test evidence (no evidence = `UNVERIFIED`). An `ABSENT`, `UNVERIFIED` or unexplained `PARTIAL` capability blocks final competitive closure.

| Capability | Objective | Strongest reference | Rust owner | Live entry point | Status | Residual gap |
|---|---|---|---|---|---|---|
| address-trackability | Distinguish a rotating/randomized BLE address from trackable hardware (identifier is not a device) | BlueHydra | bleradar_core::sweep::address_trackability | NativeRadar.deviceAddressTrackability | PARTIAL | Classification uses the 802 U/L bit, an approximation of BLE address types |
| adv-decoding | Decode the BLE advertising payload (AD structures, service UUIDs, manufacturer/service data, iBeacon/Eddystone) | nRF Connect / Beacon Scanner | bleradar_core::adv | NativeRadar.advertisementCompanyId/advertisementBeacon | PARTIAL | No GATT-level detail, not every beacon format, no competitor head-to-head benchmark |
| auto-update | Decide and safely apply an app self-update (version, integrity, lifecycle, recovery) | Play Store in-app updates | bleradar_core::update | NativeRadar.updateDecision/downloadReadiness/artifactVerifyFile | PARITY | The network fetch and OS installer are the documented platform boundary |
| gatt-inspection | Connect to a selected device and browse its GATT services, characteristics and descriptors | nRF Connect | — | — | ABSENT | Not implemented; must stay user-initiated and separate from passive scanning |
| history-persistence | A per-device timeline (first/last seen, recurrence) persisted across sessions | BlueHydra / WiGLE | bleradar_core::history | NativeRadar.historyMerge / NativeRadar.historyLookup | PARTIAL | Public addresses only (rotating addresses are not remembered); first seen + visit count, not a full sighting timeline or location trail; no history view or export; no competitor benchmark |
| identity-correlation | Correlate one physical device across BLE address rotation with explicit uncertainty | BlueHydra | bleradar_core::identity | NativeRadar.deviceGroupKey | PARTIAL | Cross-session history covers public addresses only; U/L-bit address-type approximation; app does not yet visually merge rows |
| manufacturer-name | Name the Bluetooth SIG assignee behind a company identifier | nRF Connect | bleradar_core::adv::company_name | NativeRadar.advertisementManufacturerName | PARTIAL | A curated ~40-entry subset, not the full SIG registry (a data-only follow-up) |
| rssi-signal | Filter RSSI and report proximity/distance with uncertainty (never exact distance without calibration) | BLE Radar-class trackers | bleradar_core::signal | NativeRadar.trackingFilteredRssi/trackingProximity | PARITY | No empirical comparison of alternative filters (median/Hampel/Kalman) under real load |
| sensor-rules | One authority for the multi-sensor radar's reading-interpretation rules (Wi-Fi/BT/cell) | Huntsman Search Engine signal_radar | bleradar_core::sweep | NativeRadar.deviceAddressTrackability (BLE path only) | PARTIAL | The app scans BLE only, so the Wi-Fi and cell rules have no in-app caller yet |
| wifi-survey | A wireless observation map/history (SSID/BSSID/channel/security/location) — WiGLE-class collection | WiGLE | — | — | BLOCKED_BY_PLATFORM | The Android app captures BLE only; Wi-Fi scanning would need a separate platform surface |

Totals: 0 SUPERIOR, 2 PARITY, 6 PARTIAL, 1 ABSENT, 1 BLOCKED_BY_PLATFORM, 0 UNVERIFIED.

No capability is marked `SUPERIOR`: none has a head-to-head competitive benchmark yet, so the project is not at final competitive closure (§52/§53). Each `PARTIAL`/`ABSENT`/`BLOCKED` row names what remains.
