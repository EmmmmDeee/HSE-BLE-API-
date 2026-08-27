# Parity Coverage

Generated from `docs/NATIVE_ABI.txt` and the semantic compatibility registry.

- Observed UniFFI function/method/constructor symbols: **124**
- Contracts with explicit semantic migration status: **18**
- Remaining observed symbols requiring semantic registration/characterization: **106**

## Registered semantic frontier

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

A symbol appearing in the APK is not automatically considered migrated. Exact parity requires characterization of inputs, outputs, side effects, and errors against the immutable oracle. The registry intentionally records that distinction.
