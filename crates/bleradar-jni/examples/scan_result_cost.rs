//! Reproducible host microbenchmark of the per-scan-result cost of the
//! Rust-owned tracking math behind `BleScanEngine.recordResult`.
//!
//! `BleScanEngine` calls eight `NativeRadar.tracking*` natives per BLE scan
//! result, and each of them recomputes the full [`bleradar_core::tracking_snapshot`]
//! from the same inputs. This example measures (a) one `tracking_snapshot`,
//! (b) the eight-call pattern through the exact `bleradar-jni` wrapper
//! functions the exports delegate to, and (c) the bare EMA, so the
//! "consolidate into one packed JNI call" candidate can be decided on
//! measured numbers rather than intuition. It excludes the JVM→native
//! transition itself, which is host-dependent and roughly 0.1 µs per call.
//!
//! Run with `cargo run --release -p bleradar-jni --example scan_result_cost`;
//! the recorded baseline lives in `benchmarks/README.md`.

use std::hint::black_box;
use std::time::Instant;

use bleradar_core::{
    CalibrationProfile, TrackingProfile, TrackingSnapshotInput, filtered_rssi, tracking_snapshot,
};
use bleradar_jni::{
    TrackingSnapshotJniInput, tracking_confidence_percent_or_negative,
    tracking_distance_lower_bound_m_or_nan, tracking_distance_m_or_nan,
    tracking_distance_proximity_ordinal, tracking_distance_upper_bound_m_or_nan,
    tracking_filtered_rssi_or_nan, tracking_freshness_ordinal, tracking_trend_ordinal,
};

const INPUTS: usize = 1024;
const ITERATIONS: u64 = 10_000_000;
const ROUNDS: usize = 5;

fn nanos_per_op(elapsed_ns: u128, ops: u64) -> f64 {
    elapsed_ns as f64 / ops as f64
}

fn main() {
    let inputs: Vec<TrackingSnapshotInput> = (0..INPUTS)
        .map(|i| TrackingSnapshotInput {
            previous_filtered_rssi_dbm: -60.0 - (i % 40) as f64,
            current_rssi_dbm: -55.0 - (i % 37) as f64,
            rssi_spread_db: (i % 9) as f64,
            sample_count: i % 16,
            calibration_profile: CalibrationProfile::Baseline,
            tracking_profile: TrackingProfile::Standard,
            age_ms: (i % 7000) as u64,
            tx_power_dbm: if i % 3 == 0 { Some(-63.0) } else { None },
        })
        .collect();
    let jni_inputs: Vec<TrackingSnapshotJniInput> = inputs
        .iter()
        .map(|input| TrackingSnapshotJniInput {
            previous_filtered_dbm: input.previous_filtered_rssi_dbm,
            current_rssi_dbm: input.current_rssi_dbm,
            rssi_spread_db: input.rssi_spread_db,
            sample_count: i32::try_from(input.sample_count).unwrap_or(i32::MAX),
            calibration_profile_ordinal: 0,
            tracking_profile_ordinal: 0,
            age_ms: i64::try_from(input.age_ms).unwrap_or(i64::MAX),
            tx_power_dbm: input.tx_power_dbm.unwrap_or(f64::NAN),
        })
        .collect();

    let mut best_snapshot = f64::MAX;
    let mut best_eight_calls = f64::MAX;
    let mut best_ema = f64::MAX;
    for _ in 0..ROUNDS {
        let started = Instant::now();
        let mut accumulator = 0.0;
        for k in 0..ITERATIONS {
            let input = black_box(inputs[(k as usize) % INPUTS]);
            if let Some(snapshot) = tracking_snapshot(input) {
                accumulator += snapshot.filtered_rssi_dbm;
            }
        }
        black_box(accumulator);
        best_snapshot = best_snapshot.min(nanos_per_op(started.elapsed().as_nanos(), ITERATIONS));

        let scan_results = ITERATIONS / 8;
        let started = Instant::now();
        let mut doubles = 0.0;
        let mut ordinals = 0i64;
        for k in 0..scan_results {
            let input = black_box(jni_inputs[(k as usize) % INPUTS]);
            doubles += tracking_filtered_rssi_or_nan(input);
            ordinals += i64::from(tracking_trend_ordinal(input));
            doubles += tracking_distance_m_or_nan(input);
            doubles += tracking_distance_lower_bound_m_or_nan(input);
            doubles += tracking_distance_upper_bound_m_or_nan(input);
            ordinals += i64::from(tracking_distance_proximity_ordinal(input));
            ordinals += i64::from(tracking_confidence_percent_or_negative(input));
            ordinals += i64::from(tracking_freshness_ordinal(input));
        }
        black_box((doubles, ordinals));
        best_eight_calls =
            best_eight_calls.min(nanos_per_op(started.elapsed().as_nanos(), scan_results));

        let started = Instant::now();
        let mut ema_sum = 0.0;
        for k in 0..ITERATIONS {
            let input = black_box(inputs[(k as usize) % INPUTS]);
            ema_sum += filtered_rssi(
                input.previous_filtered_rssi_dbm,
                input.current_rssi_dbm,
                0.35,
            )
            .unwrap_or(0.0);
        }
        black_box(ema_sum);
        best_ema = best_ema.min(nanos_per_op(started.elapsed().as_nanos(), ITERATIONS));
    }

    println!("best of {ROUNDS} rounds, {ITERATIONS} iterations each, {INPUTS} rotating inputs");
    println!("tracking_snapshot                         {best_snapshot:8.1} ns/op");
    println!("eight tracking* wrapper calls per result  {best_eight_calls:8.1} ns/result");
    println!("filtered_rssi (EMA only)                  {best_ema:8.1} ns/op");
}
