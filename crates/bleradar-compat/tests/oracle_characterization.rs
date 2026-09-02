//! Executable fixtures from the immutable v0.3.0 native oracle.

use bleradar_compat::{ParityStatus, parity_status};
use bleradar_core::{
    LatLon, ProximityBand, ble_distance_m, haversine_m, proximity_label, wifi_frequency_to_channel,
};

const ORACLE_EQUATORIAL_DEGREE_M: f64 = 111_195.080_233_532_91;
const ORACLE_BLE_MINUS_70_DBM_M: f64 = 2.872_984_833_353_664_5;

#[test]
fn oracle_haversine_fixture_exposes_radius_gap() {
    let source = haversine_m(
        LatLon::new(0.0, 0.0).unwrap(),
        LatLon::new(1.0, 0.0).unwrap(),
    );
    assert!((source - ORACLE_EQUATORIAL_DEGREE_M).abs() > 0.1);
    assert_eq!(
        parity_status("haversine_m"),
        Some(ParityStatus::SourceAnalog)
    );
}

#[test]
fn oracle_proximity_fixture_exposes_input_semantics_gap() {
    // The oracle accepts metres and maps 2 m to "near"; the source analogue
    // accepts dBm and therefore interprets the same scalar as "immediate".
    assert_eq!(proximity_label(2.0), ProximityBand::Immediate);
    assert_eq!(
        parity_status("proximity_label"),
        Some(ParityStatus::SourceAnalog)
    );
}

#[test]
fn oracle_ble_fixture_records_fixed_calibration() {
    let fixed_model = ble_distance_m(-70.0, -59.0, 2.4).unwrap();
    assert!((fixed_model - ORACLE_BLE_MINUS_70_DBM_M).abs() < 1.0e-12);
    assert_eq!(
        parity_status("ble_distance"),
        Some(ParityStatus::SourceAnalog)
    );
}

#[test]
fn oracle_wifi_fixture_exposes_six_ghz_gap() {
    // The oracle returns channel 10 for 6000 MHz.
    assert_eq!(wifi_frequency_to_channel(6000), None);
    assert_eq!(
        parity_status("wifi_frequency_to_channel"),
        Some(ParityStatus::SourceAnalog)
    );
}

#[test]
fn no_source_analogue_is_mislabeled_as_differentially_verified() {
    for name in [
        "bearing_deg",
        "haversine_m",
        "wifi_channel_to_frequency",
        "wifi_frequency_to_channel",
        "ble_distance",
        "proximity_label",
    ] {
        assert_ne!(
            parity_status(name),
            Some(ParityStatus::DifferentiallyVerified)
        );
    }
}
