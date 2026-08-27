use bleradar_compat::is_observed_contract;

#[test]
fn high_value_contracts_are_in_inventory() {
    assert!(is_observed_contract("RadarStore"));
    assert!(is_observed_contract("ui_radar_points"));
    assert!(is_observed_contract("multilaterate"));
}
