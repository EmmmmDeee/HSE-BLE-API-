//! Compatibility inventory for the shipped APK's native Rust surface.

/// Exported native contracts observed in `libbleradar_core.so`.
pub const OBSERVED_NATIVE_CONTRACTS: &[&str] = &[
    "assess_threat", "bearing_deg", "ble_distance", "bleadv_appearance",
    "bleadv_fingerprints", "bleadv_flags", "bt_category_from_class", "bt_describe",
    "bt_major", "bt_major_label", "bt_minor", "bt_services", "core_version", "correlate",
    "device_category", "export_csv_field", "export_device_json", "export_session_json",
    "export_wigle_csv", "gatt_characteristic_name", "gatt_decode", "gatt_service_name",
    "haversine_m", "import_parse", "import_parse_json", "import_parse_wigle", "mac_info",
    "multilaterate", "osint_scan", "oui_vendor", "proximity_label", "session_to_track",
    "ui_geo_sketch", "ui_radar_points", "wifi_channel_to_frequency", "wifi_distance",
    "wifi_frequency_to_channel", "wifi_security", "RadarStore"
];

/// Returns whether a contract name was observed in the binary ABI census.
#[must_use]
pub fn is_observed_contract(name: &str) -> bool {
    OBSERVED_NATIVE_CONTRACTS.contains(&name)
}
