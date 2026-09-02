# Parity Coverage

Generated from `docs/NATIVE_ABI.txt` and the semantic compatibility/runtime registries.

- Observed UniFFI function/method/constructor symbols: **124**
- Contracts with runtime implementation/reachability classification: **124**
- Remaining observed symbols requiring runtime classification: **0**

## Shipped implementation

- `RUST_NATIVE`: **124**
- `RUST_MIGRATION_REQUIRED`: **0**
- `NON_RUST_JUSTIFIED_BOUNDARY`: **0**
- `UNKNOWN`: **0**

## Reachability

- `VERIFIED_RUNTIME`: **41**
- `STATICALLY_REACHABLE`: **78**
- `CONDITIONALLY_REACHABLE`: **0**
- `UNREACHABLE`: **0**
- `UNKNOWN`: **5**

## Source-replacement parity frontier

- Differentially verified: **0**
- Source analogue only: **6**
- Oracle only: **5**
- Blocked: **7**

Registered source-replacement contracts:

- `bearing_deg`
- `haversine_m`
- `wifi_channel_to_frequency`
- `wifi_frequency_to_channel`
- `ble_distance`
- `proximity_label`
- `ui_radar_points`
- `ui_geo_sketch`
- `multilaterate`
- `assess_threat`
- `correlate`
- `export_device_json`
- `export_session_json`
- `import_parse`
- `mac_info`
- `oui_vendor`
- `RadarStore`
- `session_to_track`

## Interpretation

The shipped implementations behind all 124 ABI contracts are Rust-native; this does not establish parity for similarly named functions in the reconstructed source workspace. Exact source-replacement parity requires characterization of inputs, outputs, side effects, state, termination, and errors against the immutable oracle. Reachability and evidence details are recorded in `docs/VERIFIED_RUNTIME_TOPOLOGY.md`.
