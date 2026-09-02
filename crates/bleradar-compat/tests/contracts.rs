//! Contract-registry regression tests for the semantic parity frontier.

use bleradar_compat::{
    ContractKind, ImplementationClass, ParityStatus, RUNTIME_CONTRACTS, Reachability,
    coverage_counts, is_observed_contract, parity_status, reachability_counts, runtime_contract,
};

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
        Some(ParityStatus::SourceAnalog)
    );
    assert_eq!(parity_status("RadarStore"), Some(ParityStatus::OracleOnly));
    assert_eq!(
        parity_status("ui_radar_points"),
        Some(ParityStatus::Blocked)
    );
}

#[test]
fn all_status_buckets_are_exercised() {
    let (source_analog, verified, oracle_only, blocked) = coverage_counts();
    assert!(source_analog > 0);
    assert_eq!(verified, 0);
    assert!(oracle_only > 0);
    assert!(blocked > 0);
}

#[test]
fn complete_runtime_map_matches_native_census() {
    assert_eq!(RUNTIME_CONTRACTS.len(), 124);
    assert_eq!(
        RUNTIME_CONTRACTS
            .iter()
            .filter(|contract| contract.kind == ContractKind::Function)
            .count(),
        99
    );
    assert_eq!(
        RUNTIME_CONTRACTS
            .iter()
            .filter(|contract| contract.kind == ContractKind::Method)
            .count(),
        24
    );
    assert_eq!(
        RUNTIME_CONTRACTS
            .iter()
            .filter(|contract| contract.kind == ContractKind::Constructor)
            .count(),
        1
    );
    let (verified, statically_reachable, conditional, unreachable, unknown) = reachability_counts();
    assert_eq!(
        (
            verified,
            statically_reachable,
            conditional,
            unreachable,
            unknown,
        ),
        (41, 78, 0, 0, 5)
    );
    assert!(
        RUNTIME_CONTRACTS
            .iter()
            .all(|contract| contract.implementation == ImplementationClass::RustNative)
    );
}

#[test]
fn unknown_means_export_without_a_verified_application_caller() {
    assert_eq!(
        runtime_contract("radarstore_len").map(|contract| contract.reachability),
        Some(Reachability::Unknown)
    );
    assert_eq!(
        runtime_contract("radarstore_ingest_ble").map(|contract| contract.reachability),
        Some(Reachability::StaticallyReachable)
    );
    assert_eq!(
        runtime_contract("scan_tick_plan").map(|contract| contract.reachability),
        Some(Reachability::VerifiedRuntime)
    );
}
