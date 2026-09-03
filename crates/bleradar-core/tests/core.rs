//! Behavioral regression tests for the reconstructed BLE Radar domain core.

use bleradar_core::{
    AddressKind, DeviceIdentity, DeviceObservation, DeviceTrack, EstimateKind, GeoError,
    IdentityEvidence, LatLon, ProximityBand, RssiEma, SelectedDevice, SignalTrend, TrackError,
    bearing_deg, ble_distance_m, canonical_mac, haversine_m, is_locally_administered, signal_trend,
    wifi_channel_to_frequency, wifi_frequency_to_channel,
};

/// Builds a `DeviceObservation` from its varying fields; `tx_power_dbm` is
/// always `None` across these tests.
fn observation(
    timestamp_ms: u64,
    observer_position: Option<LatLon>,
    gps_accuracy_m: Option<f64>,
    rssi_dbm: f64,
) -> DeviceObservation {
    DeviceObservation {
        timestamp_ms,
        observer_position,
        gps_accuracy_m,
        rssi_dbm,
        tx_power_dbm: None,
    }
}

#[test]
fn zero_distance_is_zero() {
    let p = LatLon::new(-26.8, 152.8).unwrap();
    assert_eq!(haversine_m(p, p), 0.0);
}

#[test]
fn haversine_is_finite_at_near_antipodal_points() {
    // Reproducer found by randomized falsification (3,525 NaN results in 60M
    // near-antipodal samples of the unclamped formula): floating error pushes
    // the haversine term above 1.0 and asin leaves its domain.
    let a = LatLon::new(58.533_453_260_712_69, -79.146_585_029_992_61).unwrap();
    let b = LatLon::new(-58.533_453_260_712_285, 100.853_414_970_007_24).unwrap();
    let d = haversine_m(a, b);
    assert!(d.is_finite());
    // Near-antipodal separation is close to half the great circle (BF-002
    // fixed the radius to 6,371,008.8 m; the 5 km tolerance dwarfs that
    // ~27 m shift, but the literal is kept in sync for clarity).
    assert!((d - std::f64::consts::PI * 6_371_008.8).abs() < 5_000.0);
}

#[test]
fn bearing_stays_in_documented_range() {
    // rem_euclid rounds a tiny negative angle up to exactly 360.0, violating
    // the documented [0, 360) contract without the fold-back.
    let a = LatLon::new(0.0, 0.0).unwrap();
    let b = LatLon::new(1.0e-9, -1.0e-300).unwrap();
    let bearing = bearing_deg(a, b);
    assert!((0.0..360.0).contains(&bearing));
}

#[test]
fn latlon_rejects_invalid_input() {
    assert_eq!(LatLon::new(f64::NAN, 0.0), Err(GeoError::NonFinite));
    assert_eq!(LatLon::new(0.0, f64::INFINITY), Err(GeoError::NonFinite));
    assert_eq!(LatLon::new(90.1, 0.0), Err(GeoError::OutOfRange));
    assert_eq!(LatLon::new(0.0, -180.5), Err(GeoError::OutOfRange));
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
        .push(observation(10, Some(p), Some(5.0), -70.0))
        .unwrap();
    let err = track
        .push(observation(9, Some(p), Some(5.0), -69.0))
        .unwrap_err();
    assert_eq!(err, TrackError::NonMonotonicTime);
}

#[test]
fn map_points_remain_observed_not_inferred() {
    let mut track = DeviceTrack::new(0.5).unwrap();
    let p = LatLon::new(-26.8, 152.8).unwrap();
    track
        .push(observation(1, Some(p), Some(4.0), -55.0))
        .unwrap();
    let points = track.observed_map_points();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].kind, EstimateKind::Observed);
}

#[test]
fn stronger_samples_produce_hotter_state() {
    let mut track = DeviceTrack::new(0.5).unwrap();
    track.push(observation(1, None, None, -80.0)).unwrap();
    track.push(observation(2, None, None, -50.0)).unwrap();
    assert_eq!(track.trend(), SignalTrend::Stronger);
    assert_eq!(track.proximity(), Some(ProximityBand::Near));
}

#[test]
fn spatial_estimate_requires_multiple_positioned_observations() {
    let mut track = DeviceTrack::new(0.4).unwrap();
    let a = LatLon::new(-26.8000, 152.8000).unwrap();
    let b = LatLon::new(-26.8001, 152.8001).unwrap();
    track
        .push(observation(1, Some(a), Some(5.0), -65.0))
        .unwrap();
    assert!(track.spatial_estimate().is_none());
    track
        .push(observation(2, Some(b), Some(5.0), -55.0))
        .unwrap();
    let estimate = track.spatial_estimate().unwrap();
    assert_eq!(estimate.supporting_observations, 2);
    assert!(estimate.uncertainty_m > 0.0);
    // The center of two ~15 m-apart observations must stay local to them.
    assert!(haversine_m(estimate.center, a) < 1_000.0);
}

#[test]
fn spatial_estimate_handles_antimeridian_straddling() {
    // Two observations ~111 m apart across the ±180° meridian. A linear
    // longitude mean places the center near lon 0 — the far side of the
    // planet — instead of near ±180.
    let mut track = DeviceTrack::new(0.5).unwrap();
    let east = LatLon::new(0.0, 179.9995).unwrap();
    let west = LatLon::new(0.0, -179.9995).unwrap();
    track
        .push(observation(1, Some(east), Some(5.0), -60.0))
        .unwrap();
    track
        .push(observation(2, Some(west), Some(5.0), -60.0))
        .unwrap();
    let estimate = track.spatial_estimate().unwrap();
    let true_midpoint = LatLon::new(0.0, 180.0).unwrap();
    assert!(haversine_m(estimate.center, true_midpoint) < 1_000.0);
    assert!(estimate.uncertainty_m < 10_000.0);
}

#[test]
fn selection_lock_retains_history() {
    let mut selected = SelectedDevice::new("device-1", 0.5).unwrap();
    selected.start_tracking();
    assert!(selected.tracking);
    selected
        .track
        .push(observation(1, None, None, -70.0))
        .unwrap();
    selected.stop_tracking();
    assert!(!selected.tracking);
    // "Releases active tracking while retaining history" (tracking.rs docs)
    // is a claim about `track`, not just the `tracking` flag: unlocking must
    // not discard previously observed samples.
    assert_eq!(selected.track.observations().len(), 1);
    // History must keep accumulating even while unlocked.
    selected
        .track
        .push(observation(2, None, None, -65.0))
        .unwrap();
    assert_eq!(selected.track.observations().len(), 2);
}
