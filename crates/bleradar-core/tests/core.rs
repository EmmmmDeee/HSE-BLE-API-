//! Behavioral regression tests for the reconstructed BLE Radar domain core.

use bleradar_core::{
    AddressKind, DeviceIdentity, DeviceObservation, DeviceTrack, EstimateKind, IdentityEvidence,
    LatLon, ProximityBand, RssiEma, SelectedDevice, SignalTrend, TrackError, bearing_deg,
    ble_distance_m, canonical_mac, haversine_m, is_locally_administered, signal_trend,
    wifi_channel_to_frequency, wifi_frequency_to_channel,
};

#[test]
fn zero_distance_is_zero() {
    let p = LatLon::new(-26.8, 152.8).unwrap();
    assert_eq!(haversine_m(p, p), 0.0);
}

#[test]
fn bearing_north_is_zero() {
    let a = LatLon::new(0.0, 0.0).unwrap();
    let b = LatLon::new(1.0, 0.0).unwrap();
    assert!(bearing_deg(a, b).abs() < 1e-9);
}

#[test]
fn mac_canonicalization_and_local_bit() {
    assert_eq!(
        canonical_mac("36-32-62-36-31-33").as_deref(),
        Some("36:32:62:36:31:33")
    );
    assert_eq!(is_locally_administered("36:32:62:36:31:33"), Some(true));
    assert_eq!(is_locally_administered("00:11:22:33:44:55"), Some(false));
}

#[test]
fn randomized_address_is_not_stable_identity() {
    let identity = DeviceIdentity::new("36:32:62:36:31:33", IdentityEvidence::default()).unwrap();
    assert_eq!(identity.address_kind, AddressKind::Randomized);
}

#[test]
fn wifi_channel_round_trip() {
    assert_eq!(wifi_channel_to_frequency(1), Some(2412));
    assert_eq!(wifi_channel_to_frequency(14), Some(2484));
    assert_eq!(wifi_frequency_to_channel(2412), Some(1));
    assert_eq!(wifi_frequency_to_channel(2484), Some(14));
}

#[test]
fn ema_and_trend_are_deterministic() {
    let mut f = RssiEma::new(0.5).unwrap();
    assert_eq!(f.push(-80.0).unwrap(), -80.0);
    assert_eq!(f.push(-60.0).unwrap(), -70.0);
    assert_eq!(signal_trend(-80.0, -70.0, 2.0), SignalTrend::Stronger);
}

#[test]
fn distance_model_is_calibrated_not_absolute() {
    assert!((ble_distance_m(-59.0, -59.0, 2.0).unwrap() - 1.0).abs() < 1e-12);
    assert!(ble_distance_m(-70.0, -59.0, 0.0).is_none());
}

#[test]
fn track_rejects_time_reversal() {
    let mut track = DeviceTrack::new(0.5).unwrap();
    let p = LatLon::new(-26.8, 152.8).unwrap();
    track
        .push(DeviceObservation {
            timestamp_ms: 10,
            observer_position: Some(p),
            gps_accuracy_m: Some(5.0),
            rssi_dbm: -70.0,
            tx_power_dbm: None,
        })
        .unwrap();
    let err = track
        .push(DeviceObservation {
            timestamp_ms: 9,
            observer_position: Some(p),
            gps_accuracy_m: Some(5.0),
            rssi_dbm: -69.0,
            tx_power_dbm: None,
        })
        .unwrap_err();
    assert_eq!(err, TrackError::NonMonotonicTime);
}

#[test]
fn map_points_remain_observed_not_inferred() {
    let mut track = DeviceTrack::new(0.5).unwrap();
    let p = LatLon::new(-26.8, 152.8).unwrap();
    track
        .push(DeviceObservation {
            timestamp_ms: 1,
            observer_position: Some(p),
            gps_accuracy_m: Some(4.0),
            rssi_dbm: -55.0,
            tx_power_dbm: None,
        })
        .unwrap();
    let points = track.observed_map_points();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].kind, EstimateKind::Observed);
}

#[test]
fn stronger_samples_produce_hotter_state() {
    let mut track = DeviceTrack::new(0.5).unwrap();
    track
        .push(DeviceObservation {
            timestamp_ms: 1,
            observer_position: None,
            gps_accuracy_m: None,
            rssi_dbm: -80.0,
            tx_power_dbm: None,
        })
        .unwrap();
    track
        .push(DeviceObservation {
            timestamp_ms: 2,
            observer_position: None,
            gps_accuracy_m: None,
            rssi_dbm: -50.0,
            tx_power_dbm: None,
        })
        .unwrap();
    assert_eq!(track.trend(), SignalTrend::Stronger);
    assert_eq!(track.proximity(), Some(ProximityBand::Near));
}

#[test]
fn spatial_estimate_requires_multiple_positioned_observations() {
    let mut track = DeviceTrack::new(0.4).unwrap();
    let a = LatLon::new(-26.8000, 152.8000).unwrap();
    let b = LatLon::new(-26.8001, 152.8001).unwrap();
    track
        .push(DeviceObservation {
            timestamp_ms: 1,
            observer_position: Some(a),
            gps_accuracy_m: Some(5.0),
            rssi_dbm: -65.0,
            tx_power_dbm: None,
        })
        .unwrap();
    assert!(track.spatial_estimate().is_none());
    track
        .push(DeviceObservation {
            timestamp_ms: 2,
            observer_position: Some(b),
            gps_accuracy_m: Some(5.0),
            rssi_dbm: -55.0,
            tx_power_dbm: None,
        })
        .unwrap();
    let estimate = track.spatial_estimate().unwrap();
    assert_eq!(estimate.supporting_observations, 2);
    assert!(estimate.uncertainty_m > 0.0);
}

#[test]
fn selection_lock_retains_history() {
    let mut selected = SelectedDevice::new("device-1", 0.5).unwrap();
    selected.start_tracking();
    assert!(selected.tracking);
    selected.stop_tracking();
    assert!(!selected.tracking);
}
