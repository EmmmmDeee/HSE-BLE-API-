//! Executable fixtures from the immutable v0.3.0 native oracle.

use bleradar_compat::{ParityStatus, parity_status};
use bleradar_core::{
    LatLon, ProximityBand, ble_distance_m, haversine_m, proximity_label,
    wifi_channel_to_frequency, wifi_frequency_to_channel,
};

const ORACLE_EQUATORIAL_DEGREE_M: f64 = 111_195.080_233_532_9;
const ORACLE_BLE_MINUS_70_DBM_M: f64 = 2.872_984_833_353_664_5;

fn oracle_wifi_channel_to_frequency(channel: i32) -> Option<i32> {
    match channel {
        1..=13 => Some(2407 + channel * 5),
        14 => Some(2484),
        32..=177 => Some(5000 + channel * 5),
        _ => None,
    }
}

fn oracle_wifi_frequency_to_channel(mhz: Option<i32>) -> Option<i32> {
    let mhz = mhz?;
    match mhz {
        2412..=2472 => Some((mhz - 2407) / 5),
        2484 => Some(14),
        5160..=5885 => Some((mhz - 5000) / 5),
        5955..=7115 => Some((mhz - 5950) / 5),
        _ => None,
    }
}

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
fn oracle_wifi_boundary_trace_locks_ranges_and_flooring() {
    for (channel, expected) in [
        (i32::MIN, None),
        (-1, None),
        (0, None),
        (1, Some(2412)),
        (13, Some(2472)),
        (14, Some(2484)),
        (15, None),
        (31, None),
        (32, Some(5160)),
        (177, Some(5885)),
        (178, None),
        (i32::MAX, None),
    ] {
        assert_eq!(oracle_wifi_channel_to_frequency(channel), expected);
    }

    for (mhz, expected) in [
        (None, None),
        (Some(i32::MIN), None),
        (Some(2411), None),
        (Some(2412), Some(1)),
        (Some(2413), Some(1)),
        (Some(2416), Some(1)),
        (Some(2417), Some(2)),
        (Some(2472), Some(13)),
        (Some(2473), None),
        (Some(2484), Some(14)),
        (Some(5160), Some(32)),
        (Some(5164), Some(32)),
        (Some(5165), Some(33)),
        (Some(5885), Some(177)),
        (Some(5886), None),
        (Some(5954), None),
        (Some(5955), Some(1)),
        (Some(5959), Some(1)),
        (Some(5960), Some(2)),
        (Some(6000), Some(10)),
        (Some(7115), Some(233)),
        (Some(7116), None),
        (Some(i32::MAX), None),
    ] {
        assert_eq!(oracle_wifi_frequency_to_channel(mhz), expected);
    }
}

#[test]
fn source_wifi_channel_mapping_matches_captured_model_over_its_domain() {
    for channel in u16::MIN..=u16::MAX {
        assert_eq!(
            wifi_channel_to_frequency(channel).map(i32::from),
            oracle_wifi_channel_to_frequency(i32::from(channel)),
            "channel {channel}"
        );
    }
}

#[test]
fn oracle_wifi_frequency_gap_is_exhaustively_classified_over_source_domain() {
    let mismatches = (u16::MIN..=u16::MAX)
        .filter(|&mhz| {
            wifi_frequency_to_channel(mhz).map(i32::from)
                != oracle_wifi_frequency_to_channel(Some(i32::from(mhz)))
        })
        .collect::<Vec<_>>();

    assert_eq!(mismatches.len(), 1_789);
    assert_eq!(
        mismatches
            .iter()
            .filter(|&&mhz| (2412..=2472).contains(&mhz))
            .count(),
        48
    );
    assert_eq!(
        mismatches
            .iter()
            .filter(|&&mhz| (5160..=5885).contains(&mhz))
            .count(),
        580
    );
    assert_eq!(
        mismatches
            .iter()
            .filter(|&&mhz| (5955..=7115).contains(&mhz))
            .count(),
        1_161
    );
    assert!(mismatches.iter().all(|&mhz| {
        wifi_frequency_to_channel(mhz).is_none()
            && oracle_wifi_frequency_to_channel(Some(i32::from(mhz))).is_some()
    }));
    assert_eq!(
        parity_status("wifi_frequency_to_channel"),
        Some(ParityStatus::SourceAnalog)
    );
}

#[test]
#[ignore = "the source replacement does not yet implement the captured oracle contract"]
fn wifi_frequency_oracle_parity_removal_gate() {
    for mhz in u16::MIN..=u16::MAX {
        assert_eq!(
            wifi_frequency_to_channel(mhz).map(i32::from),
            oracle_wifi_frequency_to_channel(Some(i32::from(mhz))),
            "frequency {mhz} MHz"
        );
    }
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
