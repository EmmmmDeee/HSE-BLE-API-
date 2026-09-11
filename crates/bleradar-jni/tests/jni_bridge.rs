//! Regression tests for the JNI-facing ordinal/sentinel encodings that
//! `android/app/src/main/java/com/hse/bleradar/NativeRadar.java` depends on.

use bleradar_jni::{
    TrackingSnapshotJniInput, ble_distance_m_or_nan, calibration_profile_path_loss_exponent_or_nan,
    calibration_profile_rssi_at_1m_dbm_or_nan, default_calibration_profile_ordinal,
    default_tracking_profile_ordinal, distance_lower_bound_m_or_nan, distance_upper_bound_m_or_nan,
    download_readiness_ordinal, filtered_rssi_or_nan, proximity_label_ordinal,
    retry_backoff_delay_secs, should_check_for_update_flag, signal_confidence_percent_or_negative,
    signal_trend_ordinal, tracking_confidence_percent_or_negative,
    tracking_distance_lower_bound_m_or_nan, tracking_distance_m_or_nan,
    tracking_distance_proximity_ordinal, tracking_distance_upper_bound_m_or_nan,
    tracking_filtered_rssi_or_nan, tracking_freshness_ordinal, tracking_proximity_ordinal,
    tracking_trend_ordinal, update_decision_ordinal,
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
    assert_eq!(default_tracking_profile_ordinal(), 0);
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
        rssi_spread_db: 4.0,
        sample_count: 6,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 1,
        age_ms: 250,
        tx_power_dbm: f64::NAN,
    };
    let filtered = tracking_filtered_rssi_or_nan(input);
    let distance = tracking_distance_m_or_nan(input);
    let lower = tracking_distance_lower_bound_m_or_nan(input);
    let upper = tracking_distance_upper_bound_m_or_nan(input);
    assert!((filtered - (-69.0)).abs() < 1e-9);
    assert!(distance.is_finite());
    assert!(lower < distance);
    assert!(upper > distance);
    assert_eq!(tracking_trend_ordinal(input), 0);
    assert_eq!(tracking_proximity_ordinal(input), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input), 2);
    assert!(tracking_confidence_percent_or_negative(input) > 0);
    assert_eq!(tracking_freshness_ordinal(input), 0);
}

#[test]
fn tracking_distance_prefers_plausible_device_tx_power_over_profile_calibration() {
    let base = TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -70.0,
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    let without_tx_power = tracking_distance_m_or_nan(base);
    let with_tx_power = tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        tx_power_dbm: -70.0,
        ..base
    });
    assert!((with_tx_power - 1.0).abs() < 1e-9);
    assert!(with_tx_power < without_tx_power);
    // Android's `ScanResult.TX_POWER_NOT_PRESENT` sentinel must fall back to
    // the profile calibration, not corrupt the estimate.
    let not_present = tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        tx_power_dbm: 127.0,
        ..base
    });
    assert_eq!(not_present, without_tx_power);
}

#[test]
fn distance_proximity_ordinal_uses_calibrated_boundaries() {
    let input = |distance_m: f64| TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -59.0 - 20.0 * distance_m.log10(),
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    assert_eq!(tracking_distance_proximity_ordinal(input(0.999)), 0);
    assert_eq!(tracking_distance_proximity_ordinal(input(1.001)), 1);
    assert_eq!(tracking_distance_proximity_ordinal(input(1.999)), 1);
    assert_eq!(tracking_distance_proximity_ordinal(input(2.001)), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input(4.999)), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input(5.001)), 3);
    assert_eq!(tracking_distance_proximity_ordinal(input(24.999)), 3);
}

#[test]
fn tracking_snapshot_uses_invalid_sentinels() {
    let invalid_profile = TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -70.0,
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 99,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    let invalid_count = TrackingSnapshotJniInput {
        sample_count: -1,
        tracking_profile_ordinal: 0,
        ..invalid_profile
    };
    assert!(tracking_filtered_rssi_or_nan(invalid_profile).is_nan());
    assert!(tracking_distance_m_or_nan(invalid_count).is_nan());
    assert_eq!(tracking_distance_proximity_ordinal(invalid_count), 3);
    assert_eq!(tracking_confidence_percent_or_negative(invalid_count), -1);
    assert_eq!(tracking_freshness_ordinal(invalid_count), 2);
}

#[test]
fn tracking_snapshot_survives_invalid_spread() {
    // Live adverse-condition proof: invalid spread must not erase filtered RSSI.
    let input = TrackingSnapshotJniInput {
        previous_filtered_dbm: -70.0,
        current_rssi_dbm: -68.0,
        rssi_spread_db: f64::NAN,
        sample_count: 5,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 1_000,
        tx_power_dbm: f64::NAN,
    };
    let filtered = tracking_filtered_rssi_or_nan(input);
    assert!(
        filtered.is_finite(),
        "filtered RSSI must survive invalid spread"
    );
    assert!(tracking_distance_m_or_nan(input).is_finite());
    assert!(tracking_distance_lower_bound_m_or_nan(input).is_nan());
    assert!(tracking_distance_upper_bound_m_or_nan(input).is_nan());
    assert_eq!(tracking_confidence_percent_or_negative(input), -1);
    assert_eq!(tracking_freshness_ordinal(input), 0);
}

#[test]
fn proximity_and_trend_ordinals_use_conservative_invalid_sentinels() {
    assert_eq!(proximity_label_ordinal(f64::NAN), 3);
    assert_eq!(proximity_label_ordinal(f64::INFINITY), 3);
    assert_eq!(signal_trend_ordinal(f64::NAN, -70.0, 2.0), 2);
    assert_eq!(signal_trend_ordinal(-80.0, f64::INFINITY, 2.0), 2);
}

// ===== Automatic-update decision bridges =====
//
// These lock the JNI encodings that `NativeRadar.java`'s `UPDATE_*`,
// `NETWORK_*`, and `DOWNLOAD_*` constants depend on, and cross-check each
// bridge against the authoritative `bleradar_core::update` core so the two can
// never silently diverge.

#[test]
fn update_decision_ordinal_matches_the_four_documented_outcomes() {
    // Strictly newer and OS-compatible -> Available (1).
    assert_eq!(update_decision_ordinal(41, 42, 34, 26), 1);
    // Same versionCode -> UpToDate (0).
    assert_eq!(update_decision_ordinal(42, 42, 34, 26), 0);
    // Older build -> DowngradeRefused (2).
    assert_eq!(update_decision_ordinal(42, 41, 34, 26), 2);
    // Newer but device SDK below the build minimum -> IncompatibleOs (3).
    assert_eq!(update_decision_ordinal(41, 42, 24, 26), 3);
    // A downgrade is refused before the OS check even runs.
    assert_eq!(update_decision_ordinal(42, 41, 10, 26), 2);
}

#[test]
fn update_decision_ordinal_clamps_negative_inputs_to_zero() {
    // Negative "installed" clamps to 0, so a non-negative available > 0 is newer.
    assert_eq!(update_decision_ordinal(-5, 1, 34, 0), 1);
    // Negative device SDK clamps to 0; a positive minimum then blocks it.
    assert_eq!(update_decision_ordinal(1, 2, -1, 1), 3);
    // All-zero (the live harness's default-argument invocation) is UpToDate.
    assert_eq!(update_decision_ordinal(0, 0, 0, 0), 0);
}

#[test]
fn update_decision_ordinal_agrees_with_the_core_across_a_grid() {
    for installed in 0..8u64 {
        for available in 0..8u64 {
            for device_sdk in [0u32, 24, 26, 34] {
                for min_sdk in [0u32, 24, 26, 34] {
                    let expected =
                        bleradar_core::update_decision(installed, available, device_sdk, min_sdk)
                            .ordinal();
                    let actual = update_decision_ordinal(
                        installed as i64,
                        available as i64,
                        device_sdk as i32,
                        min_sdk as i32,
                    );
                    assert_eq!(actual, expected, "grid mismatch at {installed}/{available}");
                }
            }
        }
    }
}

#[test]
fn should_check_for_update_flag_respects_interval_and_backwards_clock() {
    assert!(should_check_for_update_flag(1_000, 0, 900)); // 1000s elapsed >= 900s
    assert!(!should_check_for_update_flag(1_000, 500, 900)); // only 500s elapsed
    assert!(!should_check_for_update_flag(400, 1_000, 900)); // clock went backwards
    assert!(should_check_for_update_flag(900, 0, 900)); // exactly the interval
    // Negative inputs clamp to 0; (0, 0, 0) is the default-argument invocation.
    assert!(should_check_for_update_flag(0, 0, 0));
    assert!(should_check_for_update_flag(-1, -1, 0));
}

#[test]
fn download_readiness_ordinal_follows_precedence_and_mapping() {
    let big = 100_000_000i64;
    // Ready: Wi-Fi (unmetered = 2), enough battery, ample storage.
    assert_eq!(
        download_readiness_ordinal(2, 80, false, big, false, 20, 0, 40_000_000),
        0
    );
    // No network (0) takes precedence over everything else.
    assert_eq!(
        download_readiness_ordinal(0, 80, true, big, true, 20, 0, 40_000_000),
        1
    );
    // Metered (1) is blocked when the policy forbids it.
    assert_eq!(
        download_readiness_ordinal(1, 80, false, big, false, 20, 0, 40_000_000),
        2
    );
    // Metered allowed proceeds (subject to the later gates).
    assert_eq!(
        download_readiness_ordinal(1, 80, false, big, true, 20, 0, 40_000_000),
        0
    );
    // Low battery when below the minimum and not charging.
    assert_eq!(
        download_readiness_ordinal(2, 10, false, big, false, 20, 0, 40_000_000),
        3
    );
    // Charging overrides low battery.
    assert_eq!(
        download_readiness_ordinal(2, 10, true, big, false, 20, 0, 40_000_000),
        0
    );
    // Insufficient storage: the artifact plus the required headroom exceeds free.
    assert_eq!(
        download_readiness_ordinal(2, 80, false, 30_000_000, false, 20, 20_000_000, 40_000_000),
        4
    );
    // An unknown network ordinal is treated as "no network" (conservative).
    assert_eq!(
        download_readiness_ordinal(99, 80, false, big, true, 20, 0, 40_000_000),
        1
    );
    // All-zero default-argument invocation: network 0 -> NoNetwork (1), never a panic.
    assert_eq!(
        download_readiness_ordinal(0, 0, false, 0, false, 0, 0, 0),
        1
    );
}

#[test]
fn retry_backoff_delay_secs_is_exponential_and_capped() {
    // base = 10, cap = 60: 10, 20, 40, 60 (clamped), 60 (clamped, no overflow).
    assert_eq!(retry_backoff_delay_secs(1, 10, 60), 10);
    assert_eq!(retry_backoff_delay_secs(2, 10, 60), 20);
    assert_eq!(retry_backoff_delay_secs(3, 10, 60), 40);
    assert_eq!(retry_backoff_delay_secs(4, 10, 60), 60);
    assert_eq!(retry_backoff_delay_secs(9, 10, 60), 60);
    // A huge attempt cannot overflow; it saturates to the cap.
    assert_eq!(retry_backoff_delay_secs(1_000_000, 30, 3600), 3600);
    // Negative inputs clamp to 0; (0, 0, 0) is the default-argument invocation.
    assert_eq!(retry_backoff_delay_secs(0, 0, 0), 0);
    assert_eq!(retry_backoff_delay_secs(-5, -5, -5), 0);
}

#[test]
fn retry_backoff_delay_secs_agrees_with_the_core_policy() {
    let policy = bleradar_core::RetryPolicy {
        max_attempts: 1,
        base_delay_secs: 30,
        max_delay_secs: 3600,
    };
    for attempt in 1..=12u32 {
        let expected = policy.backoff_delay_secs(attempt) as i64;
        let actual = retry_backoff_delay_secs(attempt as i32, 30, 3600);
        assert_eq!(actual, expected, "attempt {attempt}");
    }
}
