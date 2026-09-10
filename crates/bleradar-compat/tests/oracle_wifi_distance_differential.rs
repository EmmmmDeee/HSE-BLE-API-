//! Executed-oracle differential for the previously-unmapped `wifi_distance`
//! contract (`docs/ORACLE_DIFFERENTIAL.md`, `docs/AUTONOMOUS_DECISIONS.md`
//! decision 76).
//!
//! `wifi_distance` is in the runtime census but had no reconstruction analogue.
//! `cargo xtask oracle-differential` executes the immutable oracle under
//! `qemu-aarch64` against a real Bionic runtime and records
//! `oracle/wifi_distance_executed_vectors.tsv`. This test replays that ground
//! truth and proves the reconstruction reproduces the executed oracle across its
//! **whole `i32` domain** -- unlike `ble_distance` (decision #74), the
//! reconstruction faithfully replicates every guard, so there is no behavioral
//! divergence to document:
//!
//! 1. the `rssi >= 0` far-clamp sentinel (400 m),
//! 2. the plausible-frequency window `2000..=7199` MHz with a 2437 MHz
//!    (channel 6) default substituted for any frequency outside it (including
//!    the negative / `> u16` frequencies the oracle's `i32` domain accepts),
//! 3. the log-distance formula `10^((47.55 - 20·log10(f) - rssi) / 27)`, and
//! 4. the `[0.1, 400]` m output clamps.
//!
//! The formula step uses transcendental `libm` (`log10`/`powf`), which is not
//! guaranteed bit-identical across host `libm` versions (cf. `haversine_m` /
//! `bearing_deg`), so the reconstruction is matched to `< 1e-12` relative rather
//! than bit-for-bit and the contract stays `SourceAnalog`. On the reference host
//! the observed max relative is ~2e-15 (a few ULP over 797 formula-region points).

use bleradar_compat::{ParityStatus, parity_status};
use bleradar_core::wifi_distance;

const VECTORS: &str = include_str!("oracle/wifi_distance_executed_vectors.tsv");
const ORACLE_SO_SHA256: &str = "d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d";

/// The recovered output range (metres) and the far-clamp sentinel value.
const ORACLE_LOW_CLAMP_M: f64 = 0.1;
const ORACLE_HIGH_CLAMP_M: f64 = 400.0;
/// The recovered plausible-frequency window and out-of-window default (MHz).
const ORACLE_FREQ_MIN_MHZ: i32 = 2000;
const ORACLE_FREQ_MAX_MHZ: i32 = 7199;
const ORACLE_DEFAULT_FREQ_MHZ: i32 = 2437;
/// libm rounding tolerance for the transcendental formula region.
const RELATIVE_TOLERANCE: f64 = 1.0e-12;

fn hex_bits(field: &str) -> u64 {
    u64::from_str_radix(field, 16).unwrap_or_else(|_| panic!("expected hex f64, got {field:?}"))
}

#[derive(Default)]
struct Coverage {
    rows: usize,
    sentinel: usize,     // rssi >= 0 -> 400 m
    high_clamp: usize,   // rssi < 0 but estimate >= 400 m
    low_clamp: usize,    // estimate <= 0.1 m
    formula: usize,      // strictly inside (0.1, 400) m
    default_freq: usize, // frequency outside the plausible window
    in_window: usize,    // frequency inside the plausible window
    max_rel: f64,
}

#[test]
fn reconstruction_reproduces_the_executed_oracle_over_the_full_domain() {
    let mut c = Coverage::default();
    for (index, raw) in VECTORS.lines().enumerate() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = index + 1;
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 5, "line {lineno}: WD expects 5 columns");
        assert_eq!(f[0], "WD", "line {lineno}: unexpected tag {:?}", f[0]);
        assert_eq!(
            f[3], "0",
            "line {lineno}: oracle wifi_distance status not ok"
        );

        let rssi: i32 = f[1]
            .parse()
            .unwrap_or_else(|_| panic!("line {lineno}: rssi {:?} not i32", f[1]));
        let freq: i32 = f[2]
            .parse()
            .unwrap_or_else(|_| panic!("line {lineno}: freq {:?} not i32", f[2]));
        let oracle = f64::from_bits(hex_bits(f[4]));
        assert!(
            oracle.is_finite() && oracle > 0.0,
            "line {lineno}: oracle value {oracle} out of range"
        );

        let source = wifi_distance(rssi, freq);
        let rel = (source - oracle).abs() / oracle;
        c.max_rel = c.max_rel.max(rel);
        assert!(
            rel <= RELATIVE_TOLERANCE,
            "line {lineno}: wifi_distance({rssi}, {freq}) source {source} vs executed oracle {oracle} relative {rel} (> {RELATIVE_TOLERANCE:e})"
        );

        c.rows += 1;
        if (ORACLE_FREQ_MIN_MHZ..=ORACLE_FREQ_MAX_MHZ).contains(&freq) {
            c.in_window += 1;
        } else {
            c.default_freq += 1;
        }
        if rssi >= 0 {
            assert_eq!(
                oracle, ORACLE_HIGH_CLAMP_M,
                "line {lineno}: rssi>=0 must saturate to the {ORACLE_HIGH_CLAMP_M} m sentinel"
            );
            c.sentinel += 1;
        } else if oracle == ORACLE_HIGH_CLAMP_M {
            c.high_clamp += 1;
        } else if oracle == ORACLE_LOW_CLAMP_M {
            c.low_clamp += 1;
        } else {
            assert!(
                oracle > ORACLE_LOW_CLAMP_M && oracle < ORACLE_HIGH_CLAMP_M,
                "line {lineno}: formula-region oracle {oracle} must lie strictly inside the clamps"
            );
            c.formula += 1;
        }
    }

    println!(
        "wifi_distance: rows={} formula={} (max rel {:e}) sentinel={} high_clamp={} low_clamp={} default_freq={} in_window={}",
        c.rows,
        c.formula,
        c.max_rel,
        c.sentinel,
        c.high_clamp,
        c.low_clamp,
        c.default_freq,
        c.in_window
    );

    // The differential must be substantial and exercise every region so a
    // regression in any guard is caught.
    assert!(c.rows >= 800, "too few wifi_distance vectors: {}", c.rows);
    assert!(
        c.formula >= 300,
        "too few formula-region checks: {}",
        c.formula
    );
    assert!(c.sentinel > 0, "rssi>=0 sentinel region not exercised");
    assert!(c.high_clamp > 0, "high-clamp region not exercised");
    assert!(c.low_clamp > 0, "low-clamp region not exercised");
    assert!(
        c.default_freq > 0,
        "out-of-window default-frequency region not exercised"
    );
    assert!(c.in_window > 0, "in-window frequency region not exercised");
}

/// Locks the exact, bit-exact structural contract recovered from the executed
/// oracle: the sentinel, the plausible-frequency window boundaries, the default
/// substitution, and both distance clamps. These are integer/exact behaviors
/// (no transcendental rounding), so they are asserted for equality; a mutation
/// to any threshold fails here with a readable message even before the
/// differential's relative check.
#[test]
fn reconstruction_locks_the_recovered_structural_contract() {
    // rssi >= 0 -> far clamp, independent of frequency.
    assert_eq!(wifi_distance(0, 2412), ORACLE_HIGH_CLAMP_M);
    assert_eq!(wifi_distance(7, 5955), ORACLE_HIGH_CLAMP_M);

    // Distance clamps at the extremes of a valid frequency.
    assert_eq!(wifi_distance(-130, 2412), ORACLE_HIGH_CLAMP_M);
    assert_eq!(wifi_distance(-1, ORACLE_FREQ_MAX_MHZ), ORACLE_LOW_CLAMP_M);

    // Plausible-frequency window: frequencies at the inclusive edges use their
    // own value; frequencies just outside fall back to the 2437 MHz default.
    let default = wifi_distance(-60, ORACLE_DEFAULT_FREQ_MHZ);
    assert_eq!(
        wifi_distance(-60, ORACLE_FREQ_MIN_MHZ - 1),
        default,
        "just below the window must use the default"
    );
    assert_eq!(
        wifi_distance(-60, ORACLE_FREQ_MAX_MHZ + 1),
        default,
        "just above the window must use the default"
    );
    assert_ne!(
        wifi_distance(-60, ORACLE_FREQ_MIN_MHZ),
        default,
        "the window's lower edge must use its own frequency"
    );
    assert_ne!(
        wifi_distance(-60, ORACLE_FREQ_MAX_MHZ),
        default,
        "the window's upper edge must use its own frequency"
    );
    // The oracle's i32 domain includes frequencies u16 cannot express; they also
    // default, so the reconstruction has no domain divergence there.
    assert_eq!(wifi_distance(-60, -2412), default);
    assert_eq!(wifi_distance(-60, 100_000), default);

    // Every output is finite and within the clamps.
    for rssi in [-140, -90, -59, -30, -1, 0, 20] {
        for freq in [-5, 0, 2000, 2412, 5180, 5955, 7199, 7200, 65_535] {
            let d = wifi_distance(rssi, freq);
            assert!(
                d.is_finite() && (ORACLE_LOW_CLAMP_M..=ORACLE_HIGH_CLAMP_M).contains(&d),
                "wifi_distance({rssi}, {freq}) = {d} escaped the [0.1, 400] m clamps"
            );
        }
    }
}

#[test]
fn wifi_distance_is_source_analog_documented_transcendental_tolerance() {
    assert_eq!(
        parity_status("wifi_distance"),
        Some(ParityStatus::SourceAnalog),
        "wifi_distance is matched to the oracle within libm tolerance, not bit-for-bit, so it stays SourceAnalog"
    );
}

#[test]
fn executed_wifi_distance_vectors_are_pinned_to_the_immutable_oracle() {
    assert!(
        VECTORS.contains(ORACLE_SO_SHA256),
        "the committed vectors must record the immutable oracle .so SHA-256"
    );
    assert!(
        VECTORS.contains("qemu-aarch64"),
        "the committed vectors must record the executed-oracle provenance"
    );
}
