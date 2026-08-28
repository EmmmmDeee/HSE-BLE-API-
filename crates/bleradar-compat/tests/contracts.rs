//! Contract-registry regression tests for the semantic parity frontier.

use bleradar_compat::{ParityStatus, coverage_counts, is_observed_contract, parity_status};

#[test]
fn high_value_contracts_are_in_inventory() {
    assert!(is_observed_contract("RadarStore"));
    assert!(is_observed_contract("ui_radar_points"));
    assert!(is_observed_contract("multilaterate"));
}

#[test]
fn registry_does_not_confuse_observed_with_reconstructed() {
    assert_eq!(
        parity_status("bearing_deg"),
        Some(ParityStatus::Reconstructed)
    );
    assert_eq!(parity_status("RadarStore"), Some(ParityStatus::OracleOnly));
    assert_eq!(
        parity_status("ui_radar_points"),
        Some(ParityStatus::Blocked)
    );
}

#[test]
fn all_status_buckets_are_exercised() {
    let (reconstructed, oracle_only, blocked) = coverage_counts();
    assert!(reconstructed > 0);
    assert!(oracle_only > 0);
    assert!(blocked > 0);
}
