//! Regression tests for the JNI-facing ordinal/sentinel encodings that
//! `android/app/src/main/java/com/hse/bleradar/NativeRadar.java` depends on.

use bleradar_jni::{
    TrackingSnapshotJniInput, ble_distance_m_or_nan, calibration_profile_path_loss_exponent_or_nan,
    calibration_profile_rssi_at_1m_dbm_or_nan, default_calibration_profile_ordinal,
    distance_lower_bound_m_or_nan, distance_upper_bound_m_or_nan, filtered_rssi_or_nan,
    proximity_label_ordinal, signal_confidence_percent_or_negative, signal_trend_ordinal,
    tracking_confidence_percent_or_negative, tracking_distance_lower_bound_m_or_nan,
    tracking_distance_m_or_nan, tracking_distance_upper_bound_m_or_nan,
    tracking_filtered_rssi_or_nan, tracking_freshness_ordinal, tracking_proximity_ordinal,
    tracking_trend_ordinal,
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
fn calibration_profiles_expose_rust_owned_defaults() {
    assert_eq!(default_calibration_profile_ordinal(), 0);
    assert_eq!(calibration_profile_rssi_at_1m_dbm_or_nan(0), -59.0);
    assert_eq!(calibration_profile_path_loss_exponent_or_nan(0), 2.0);
    assert!(calibration_profile_path_loss_exponent_or_nan(1) > 2.0);
    assert!(calibration_profile_path_loss_exponent_or_nan(2) < 2.0);
    assert!(calibration_profile_rssi_at_1m_dbm_or_nan(99).is_nan());
}

#[test]
fn tracking_snapshot_exports_a_coherent_bundle() {
    let input = TrackingSnapshotJniInput {
        previous_filtered_dbm: -80.0,
        current_rssi_dbm: -60.0,
        alpha: 0.5,
        trend_deadband_db: 3.0,
        rssi_spread_db: 4.0,
        sample_count: 6,
        calibration_profile_ordinal: 0,
        age_ms: 250,
        live_window_ms: 1_000,
        recent_window_ms: 30_000,
    };
    let filtered = tracking_filtered_rssi_or_nan(input);
    let distance = tracking_distance_m_or_nan(input);
    let lower = tracking_distance_lower_bound_m_or_nan(input);
    let upper = tracking_distance_upper_bound_m_or_nan(input);
    assert!((filtered - (-70.0)).abs() < 1e-9);
    assert!(distance.is_finite());
    assert!(lower < distance);
    assert!(upper > distance);
    assert_eq!(tracking_trend_ordinal(input), 0);
    assert_eq!(tracking_proximity_ordinal(input), 2);
    assert!(tracking_confidence_percent_or_negative(input) > 0);
    assert_eq!(tracking_freshness_ordinal(input), 0);
}

#[test]
fn tracking_snapshot_uses_invalid_sentinels() {
    let invalid_alpha = TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -70.0,
        alpha: 0.0,
        trend_deadband_db: 3.0,
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        age_ms: 0,
        live_window_ms: 1_000,
        recent_window_ms: 30_000,
    };
    let invalid_count = TrackingSnapshotJniInput {
        sample_count: -1,
        alpha: 0.35,
        ..invalid_alpha
    };
    assert!(tracking_filtered_rssi_or_nan(invalid_alpha).is_nan());
    assert!(tracking_distance_m_or_nan(invalid_count).is_nan());
    assert_eq!(tracking_confidence_percent_or_negative(invalid_count), -1);
    assert_eq!(tracking_freshness_ordinal(invalid_count), 2);
}
