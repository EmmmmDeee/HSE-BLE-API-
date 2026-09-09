//! Regression tests for the JNI-facing ordinal/sentinel encodings that
//! `android/app/src/main/java/com/hse/bleradar/NativeRadar.java` depends on.

use bleradar_jni::{
    ble_distance_m_or_nan, distance_lower_bound_m_or_nan, distance_upper_bound_m_or_nan,
    filtered_rssi_or_nan, proximity_label_ordinal, signal_confidence_percent_or_negative,
    signal_trend_ordinal, tracking_confidence_percent_or_negative,
    tracking_distance_lower_bound_m_or_nan, tracking_distance_m_or_nan,
    tracking_distance_upper_bound_m_or_nan, tracking_filtered_rssi_or_nan,
    tracking_freshness_ordinal, tracking_proximity_ordinal, tracking_trend_ordinal,
};

#[test]
fn distance_matches_reference_at_one_metre() {
    let distance = ble_distance_m_or_nan(-59.0, -59.0, 2.0);
    assert!((distance - 1.0).abs() < 1e-9);
}

#[test]
fn filtered_rssi_matches_ema_and_bootstraps_from_nan() {
    assert_eq!(filtered_rssi_or_nan(f64::NAN, -80.0, 0.35), -80.0);
    let filtered = filtered_rssi_or_nan(-80.0, -60.0, 0.5);
    assert!((filtered + 70.0).abs() < 1e-9);
}

#[test]
fn distance_is_nan_sentinel_when_core_returns_none() {
    // Non-positive path-loss exponent: `bleradar_core::ble_distance_m` returns `None`.
    assert!(ble_distance_m_or_nan(-70.0, -59.0, 0.0).is_nan());
    // Non-finite RSSI input.
    assert!(ble_distance_m_or_nan(f64::NAN, -59.0, 2.0).is_nan());
}

#[test]
fn distance_is_never_negative_or_infinite_for_finite_valid_input() {
    for rssi in [-100.0, -80.0, -59.0, -40.0, -10.0] {
        let distance = ble_distance_m_or_nan(rssi, -59.0, 2.0);
        assert!(distance.is_finite());
        assert!(distance > 0.0);
    }
}

#[test]
fn distance_bounds_expand_around_the_central_estimate() {
    let lower = distance_lower_bound_m_or_nan(-59.0, 6.0, -59.0, 2.0);
    let upper = distance_upper_bound_m_or_nan(-59.0, 6.0, -59.0, 2.0);
    assert!(lower.is_finite());
    assert!(upper.is_finite());
    assert!(lower < 1.0);
    assert!(upper > 1.0);
    assert!(lower < upper);
}

#[test]
fn proximity_ordinals_match_documented_mapping() {
    assert_eq!(proximity_label_ordinal(-40.0), 0); // Immediate
    assert_eq!(proximity_label_ordinal(-60.0), 1); // Near
    assert_eq!(proximity_label_ordinal(-75.0), 2); // Mid
    assert_eq!(proximity_label_ordinal(-95.0), 3); // Far
}

#[test]
fn signal_trend_ordinals_match_documented_mapping() {
    assert_eq!(signal_trend_ordinal(-80.0, -60.0, 3.0), 0); // Stronger
    assert_eq!(signal_trend_ordinal(-60.0, -80.0, 3.0), 1); // Weaker
    assert_eq!(signal_trend_ordinal(-70.0, -71.0, 3.0), 2); // Stable
}

#[test]
fn confidence_percent_uses_negative_one_as_invalid_sentinel() {
    assert_eq!(signal_confidence_percent_or_negative(-1, 2.0), -1);
    assert_eq!(signal_confidence_percent_or_negative(8, f64::NAN), -1);
    assert!(signal_confidence_percent_or_negative(8, 1.0) > 0);
}

#[test]
fn tracking_snapshot_exports_a_coherent_bundle() {
    let filtered = tracking_filtered_rssi_or_nan(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000);
    let distance = tracking_distance_m_or_nan(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000);
    let lower = tracking_distance_lower_bound_m_or_nan(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000);
    let upper = tracking_distance_upper_bound_m_or_nan(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000);
    assert!((filtered - (-70.0)).abs() < 1e-9);
    assert!(distance.is_finite());
    assert!(lower < distance);
    assert!(upper > distance);
    assert_eq!(tracking_trend_ordinal(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000), 0);
    assert_eq!(tracking_proximity_ordinal(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000), 2);
    assert!(tracking_confidence_percent_or_negative(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000) > 0);
    assert_eq!(tracking_freshness_ordinal(-80.0, -60.0, 0.5, 3.0, 4.0, 6, -59.0, 2.0, 250, 1_000, 30_000), 0);
}

#[test]
fn tracking_snapshot_uses_invalid_sentinels() {
    assert!(tracking_filtered_rssi_or_nan(f64::NAN, -70.0, 0.0, 3.0, 0.0, 1, -59.0, 2.0, 0, 1_000, 30_000).is_nan());
    assert!(tracking_distance_m_or_nan(f64::NAN, -70.0, 0.35, 3.0, 0.0, -1, -59.0, 2.0, 0, 1_000, 30_000).is_nan());
    assert_eq!(tracking_confidence_percent_or_negative(f64::NAN, -70.0, 0.35, 3.0, 0.0, -1, -59.0, 2.0, 0, 1_000, 30_000), -1);
    assert_eq!(tracking_freshness_ordinal(f64::NAN, -70.0, 0.35, 3.0, 0.0, -1, -59.0, 2.0, 0, 1_000, 30_000), 2);
}
