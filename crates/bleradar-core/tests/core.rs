use bleradar_core::{bearing_deg, canonical_mac, haversine_m, is_locally_administered, signal_trend, wifi_channel_to_frequency, wifi_frequency_to_channel, LatLon, RssiEma, SignalTrend};

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
    assert_eq!(canonical_mac("36-32-62-36-31-33").as_deref(), Some("36:32:62:36:31:33"));
    assert_eq!(is_locally_administered("36:32:62:36:31:33"), Some(true));
    assert_eq!(is_locally_administered("00:11:22:33:44:55"), Some(false));
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
