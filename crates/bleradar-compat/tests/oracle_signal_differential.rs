//! Executed-oracle differential for the core BLE signal contracts `ble_distance`
//! and `proximity_label` (`docs/ORACLE_DIFFERENTIAL.md`,
//! `docs/AUTONOMOUS_DECISIONS.md` decision 74).
//!
//! `cargo xtask oracle-differential` executes the immutable oracle under
//! `qemu-aarch64` against a real Bionic runtime and records
//! `oracle/signal_executed_vectors.tsv`. This test replays that ground truth to
//! do two things (decision #74 is "document & lock the gaps", not a behavior
//! change, so both contracts stay `SourceAnalog`):
//!
//! 1. **Verify** the reconstruction's core BLE-distance *calibration formula*
//!    matches the executed oracle wherever the oracle neither clamps nor
//!    sentinels — the reconstruction's raw `ble_distance_m(rssi, -59, 2.4)`
//!    reproduces the oracle to machine precision (measured max 2e-16 relative).
//! 2. **Document and lock** the divergences the reconstruction deliberately does
//!    not replicate: the oracle clamps distance to `[0.1, 100]` m and treats
//!    `rssi >= 0` as an invalid/too-strong sentinel returning `100` m (far),
//!    while the reconstruction's raw model returns the unclamped estimate; and
//!    the oracle's `proximity_label` bands (`<1.5` immediate, `<5` near, `<15`
//!    mid, else far) are wider than the reconstruction's
//!    `proximity_label_from_distance_m` bands (`<=1` / `<=2` / `<=5`). Pinning
//!    both makes the gap explicit and catches any future drift on either side.

use bleradar_compat::{ParityStatus, parity_status};
use bleradar_core::{ProximityBand, ble_distance_m, proximity_label_from_distance_m};

const VECTORS: &str = include_str!("oracle/signal_executed_vectors.tsv");
const ORACLE_SO_SHA256: &str = "d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d";

/// The oracle's fixed BLE calibration (confirmed by the executed-oracle sweep):
/// RSSI at 1 m = -59 dBm, path-loss exponent 2.4, tx_power ignored.
const ORACLE_RSSI_AT_1M: f64 = -59.0;
const ORACLE_PATH_LOSS: f64 = 2.4;
const ORACLE_LOW_CLAMP_M: f64 = 0.1;
const ORACLE_HIGH_CLAMP_M: f64 = 100.0;

fn hex_bits(field: &str) -> u64 {
    u64::from_str_radix(field, 16).unwrap_or_else(|_| panic!("expected hex f64, got {field:?}"))
}

fn band_str(band: ProximityBand) -> &'static str {
    match band {
        ProximityBand::Immediate => "immediate",
        ProximityBand::Near => "near",
        ProximityBand::Mid => "mid",
        ProximityBand::Far => "far",
    }
}

/// The oracle's proximity banding, recovered from the executed-oracle sweep:
/// `< 1.5` immediate, `< 5` near, `< 15` mid, else far.
fn oracle_band_model(distance_m: f64) -> &'static str {
    if distance_m < 1.5 {
        "immediate"
    } else if distance_m < 5.0 {
        "near"
    } else if distance_m < 15.0 {
        "mid"
    } else {
        "far"
    }
}

#[derive(Default)]
struct BleCounts {
    linear_verified: usize,
    sentinel: usize,
    low_clamp: usize,
    high_clamp: usize,
    max_rel: f64,
}

#[test]
fn ble_distance_calibration_matches_oracle_in_the_valid_region_clamp_sentinel_locked() {
    let mut c = BleCounts::default();
    for (index, raw) in VECTORS.lines().enumerate() {
        let line = raw.trim_end();
        if !line.starts_with("BLE\t") {
            continue;
        }
        let lineno = index + 1;
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 4, "line {lineno}: BLE expects 4 columns");
        assert_eq!(
            f[2], "0",
            "line {lineno}: oracle ble_distance status not ok"
        );
        let rssi: i32 = f[1].parse().unwrap();
        let oracle = f64::from_bits(hex_bits(f[3]));

        // The reconstruction's raw calibrated estimate (unclamped, no sentinel).
        let source = ble_distance_m(f64::from(rssi), ORACLE_RSSI_AT_1M, ORACLE_PATH_LOSS)
            .unwrap_or_else(|| {
                panic!("line {lineno}: reconstruction returned None for rssi {rssi}")
            });

        if rssi >= 0 {
            // Documented divergence: the oracle treats rssi >= 0 as an invalid /
            // implausibly-strong signal and returns the far clamp; the raw model
            // would instead report a tiny "immediate" distance.
            assert_eq!(
                oracle, ORACLE_HIGH_CLAMP_M,
                "line {lineno}: oracle ble_distance({rssi}) sentinel expected {ORACLE_HIGH_CLAMP_M}"
            );
            assert!(
                source < 1.0,
                "line {lineno}: the raw reconstruction is expected to diverge below the sentinel"
            );
            c.sentinel += 1;
        } else if oracle == ORACLE_HIGH_CLAMP_M {
            // Documented high clamp: raw estimate exceeds 100 m.
            assert!(
                source >= ORACLE_HIGH_CLAMP_M,
                "line {lineno}: raw {source} should exceed the high clamp at rssi {rssi}"
            );
            c.high_clamp += 1;
        } else if oracle == ORACLE_LOW_CLAMP_M {
            // Documented low clamp: raw estimate is below 0.1 m.
            assert!(
                source <= ORACLE_LOW_CLAMP_M,
                "line {lineno}: raw {source} should be below the low clamp at rssi {rssi}"
            );
            c.low_clamp += 1;
        } else {
            // Valid region: the reconstruction's calibration formula must match
            // the executed oracle to machine precision.
            let rel = (source - oracle).abs() / oracle;
            c.max_rel = c.max_rel.max(rel);
            assert!(
                rel <= 1.0e-12,
                "line {lineno}: ble_distance({rssi}) source {source} vs executed oracle {oracle} relative {rel} (> 1e-12)"
            );
            c.linear_verified += 1;
        }
    }
    println!(
        "ble_distance: linear-verified={} (max rel {:e}), sentinel={}, low_clamp={}, high_clamp={}",
        c.linear_verified, c.max_rel, c.sentinel, c.low_clamp, c.high_clamp
    );
    assert!(c.linear_verified >= 50, "too few linear-region checks");
    assert!(c.sentinel > 0, "rssi>=0 sentinel region not exercised");
    assert!(c.low_clamp > 0, "low-clamp region not exercised");
    assert!(c.high_clamp > 0, "high-clamp region not exercised");
}

#[test]
fn proximity_banding_locks_the_oracle_thresholds_and_documents_the_reconstruction_gap() {
    let mut rows = 0usize;
    let mut agree = 0usize;
    let mut recon_narrower_immediate = 0usize; // oracle immediate, recon already near
    let mut recon_narrower_near = 0usize; // oracle near, recon already mid
    let mut recon_narrower_mid = 0usize; // oracle mid, recon already far

    for (index, raw) in VECTORS.lines().enumerate() {
        let line = raw.trim_end();
        if !line.starts_with("PRX\t") {
            continue;
        }
        let lineno = index + 1;
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 4, "line {lineno}: PRX expects 4 columns");
        assert_eq!(
            f[2], "0",
            "line {lineno}: oracle proximity_label status not ok"
        );
        let distance = f64::from_bits(hex_bits(f[1]));
        let oracle = f[3];

        // Lock the executed-oracle thresholds (1.5 / 5 / 15).
        assert_eq!(
            oracle,
            oracle_band_model(distance),
            "line {lineno}: executed oracle proximity_label({distance}) drifted from the recorded 1.5/5/15 thresholds"
        );

        let recon = band_str(
            proximity_label_from_distance_m(distance).unwrap_or_else(|| {
                panic!("line {lineno}: reconstruction returned None for {distance}")
            }),
        );
        rows += 1;
        if recon == oracle {
            agree += 1;
        } else {
            match (oracle, recon) {
                ("immediate", "near") => recon_narrower_immediate += 1,
                ("near", "mid") => recon_narrower_near += 1,
                ("mid", "far") => recon_narrower_mid += 1,
                other => panic!(
                    "line {lineno}: unexpected proximity divergence at {distance} m: {other:?}"
                ),
            }
        }
    }

    println!(
        "proximity: rows={rows} agree={agree} recon-narrower immediate->near={recon_narrower_immediate} near->mid={recon_narrower_near} mid->far={recon_narrower_mid}"
    );
    assert!(rows >= 2000, "too few proximity rows: {rows}");
    // The reconstruction's bands are strictly narrower than the oracle's, so each
    // documented divergence direction must actually occur; every divergence is
    // the reconstruction promoting a still-in-band distance to the next-farther
    // band (never the reverse), which the exhaustive match arm above enforces.
    assert!(
        recon_narrower_immediate > 0,
        "no immediate->near divergence found"
    );
    assert!(recon_narrower_near > 0, "no near->mid divergence found");
    assert!(recon_narrower_mid > 0, "no mid->far divergence found");
    assert!(agree > 0, "the bands should still agree on shared regions");
}

#[test]
fn signal_contracts_stay_source_analog_documented_divergences() {
    for name in ["ble_distance", "proximity_label"] {
        assert_eq!(
            parity_status(name),
            Some(ParityStatus::SourceAnalog),
            "{name} is a documented divergence, not a promotion"
        );
    }
}

#[test]
fn executed_signal_vectors_are_pinned_to_the_immutable_oracle() {
    assert!(
        VECTORS.contains(ORACLE_SO_SHA256),
        "the committed signal vectors must record the immutable oracle .so SHA-256"
    );
    assert!(VECTORS.contains("qemu-aarch64"));
}
