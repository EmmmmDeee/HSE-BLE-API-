//! JNI bridge exposing `bleradar-core`'s verified pure signal math to the
//! hand-built Android radar app in `android/`.
//!
//! # Why this crate exists
//!
//! The Android app renders each detected device as a polar "blip": an angle
//! (stable per-device, since raw RSSI carries no bearing information) and a
//! ring radius derived from [`bleradar_core::ble_distance_m`]. Recomputing
//! that math in Java would silently fork it from the workspace's single,
//! tested implementation (`crates/bleradar-core/tests/core.rs`,
//! `crates/bleradar-core/tests/properties.rs`); this crate instead compiles
//! `bleradar-core` itself for `aarch64-linux-android` and re-exports three of
//! its pure functions through the JNI calling convention, so the on-device
//! calculation is the exact code already covered by the workspace's gates.
//!
//! # Why `unsafe_code` is allowed here (and nowhere else in the workspace)
//!
//! Every exported function below ignores its `env`/`class` parameters
//! entirely — it never dereferences them — so no pointer is ever read
//! through unchecked Rust code. The only reason this crate cannot keep the
//! workspace-wide `unsafe_code = "forbid"` lint is that Rust's 2024-edition
//! "unsafe attributes" lint classifies the `#[unsafe(no_mangle)]` attribute
//! itself (needed to export a stable, unmangled C-ABI symbol name that the
//! JVM's dynamic linker can resolve by its `Java_...` convention) as unsafe
//! code, regardless of what the function body does. `crates/bleradar-jni/Cargo.toml`
//! documents this narrowing; see `docs/AUTONOMOUS_DECISIONS.md` for the
//! decision record.
//!
//! # ABI stability
//!
//! The exported symbol names, argument order, and sentinel encodings below
//! are a contract with `android/app/src/main/java/com/hse/bleradar/NativeRadar.java`.
//! Changing either side without the other will fail to link (missing symbol)
//! or silently misinterpret return values (sentinel/ordinal drift), so keep
//! them in lockstep.

use bleradar_core::{
    CalibrationProfile, FreshnessClass, ProximityBand, SignalTrend, TrackingProfile,
    TrackingSnapshot, TrackingSnapshotInput, ble_distance_m, ble_distance_range_m,
    calibration_profile, calibration_profile_from_ordinal, filtered_rssi, proximity_label,
    signal_confidence_percent, signal_trend, tracking_profile_from_ordinal, tracking_snapshot,
};

/// Opaque, never-dereferenced pointer type standing in for the JNI `JNIEnv*`
/// and `jclass`/`jobject` parameters every native method receives.
///
/// Declaring a raw-pointer *parameter* is not `unsafe` in Rust; only
/// dereferencing one is. Every function below only ever ignores this type.
type JniOpaquePtr = *mut core::ffi::c_void;

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_bleDistanceM`].
///
/// Returns [`f64::NAN`] where [`bleradar_core::ble_distance_m`] would return
/// [`None`] (non-finite input, non-positive path-loss exponent, or an
/// estimate that would overflow/underflow) since JNI's `jdouble` has no
/// native `Option` encoding; the Java side must check `Double.isNaN(...)`.
#[must_use]
pub fn ble_distance_m_or_nan(rssi_dbm: f64, rssi_at_1m_dbm: f64, path_loss_exponent: f64) -> f64 {
    ble_distance_m(rssi_dbm, rssi_at_1m_dbm, path_loss_exponent).unwrap_or(f64::NAN)
}

/// Pure, unit-testable core of [`Java_com_hse_bleradar_NativeRadar_filteredRssi`].
///
/// Returns [`f64::NAN`] when the current sample or alpha is invalid.
#[must_use]
pub fn filtered_rssi_or_nan(previous_filtered_dbm: f64, current_rssi_dbm: f64, alpha: f64) -> f64 {
    filtered_rssi(previous_filtered_dbm, current_rssi_dbm, alpha).unwrap_or(f64::NAN)
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_proximityLabel`].
///
/// Encodes [`ProximityBand`] as a small ordinal (`0` = [`ProximityBand::Immediate`],
/// `1` = [`ProximityBand::Near`], `2` = [`ProximityBand::Mid`], `3` = [`ProximityBand::Far`])
/// since JNI has no shared enum type; `NativeRadar.java` mirrors this mapping
/// in matching `int` constants.
#[must_use]
pub fn proximity_label_ordinal(rssi_dbm: f64) -> i32 {
    match proximity_label(rssi_dbm) {
        ProximityBand::Immediate => 0,
        ProximityBand::Near => 1,
        ProximityBand::Mid => 2,
        ProximityBand::Far => 3,
    }
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_signalTrend`].
///
/// Encodes [`SignalTrend`] as a small ordinal (`0` = [`SignalTrend::Stronger`],
/// `1` = [`SignalTrend::Weaker`], `2` = [`SignalTrend::Stable`]); see
/// [`proximity_label_ordinal`] for why an ordinal rather than a shared enum.
#[must_use]
pub fn signal_trend_ordinal(previous_dbm: f64, current_dbm: f64, deadband_db: f64) -> i32 {
    match signal_trend(previous_dbm, current_dbm, deadband_db) {
        SignalTrend::Stronger => 0,
        SignalTrend::Weaker => 1,
        SignalTrend::Stable => 2,
    }
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_distanceLowerBoundM`].
#[must_use]
pub fn distance_lower_bound_m_or_nan(
    rssi_dbm: f64,
    rssi_spread_db: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> f64 {
    ble_distance_range_m(rssi_dbm, rssi_spread_db, rssi_at_1m_dbm, path_loss_exponent)
        .map_or(f64::NAN, |(lower, _)| lower)
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_distanceUpperBoundM`].
#[must_use]
pub fn distance_upper_bound_m_or_nan(
    rssi_dbm: f64,
    rssi_spread_db: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> f64 {
    ble_distance_range_m(rssi_dbm, rssi_spread_db, rssi_at_1m_dbm, path_loss_exponent)
        .map_or(f64::NAN, |(_, upper)| upper)
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_signalConfidencePercent`].
///
/// Returns `-1` when the inputs are invalid.
#[must_use]
pub fn signal_confidence_percent_or_negative(sample_count: i32, rssi_spread_db: f64) -> i32 {
    if sample_count < 0 {
        return -1;
    }
    signal_confidence_percent(sample_count as usize, rssi_spread_db).map_or(-1, i32::from)
}

/// Pure, unit-testable core of `NativeRadar.calibrationProfileRssiAt1mDbm(...)`.
#[must_use]
pub fn calibration_profile_rssi_at_1m_dbm_or_nan(profile_ordinal: i32) -> f64 {
    calibration_profile_from_ordinal(profile_ordinal)
        .map(calibration_profile)
        .map_or(f64::NAN, |profile| profile.rssi_at_1m_dbm)
}

/// Pure, unit-testable core of `NativeRadar.calibrationProfilePathLossExponent(...)`.
#[must_use]
pub fn calibration_profile_path_loss_exponent_or_nan(profile_ordinal: i32) -> f64 {
    calibration_profile_from_ordinal(profile_ordinal)
        .map(calibration_profile)
        .map_or(f64::NAN, |profile| profile.path_loss_exponent)
}

/// Pure, unit-testable core of `NativeRadar.defaultCalibrationProfile()`.
#[must_use]
pub const fn default_calibration_profile_ordinal() -> i32 {
    CalibrationProfile::Baseline.ordinal()
}

/// Pure, unit-testable core of `NativeRadar.defaultTrackingProfile()`.
#[must_use]
pub const fn default_tracking_profile_ordinal() -> i32 {
    TrackingProfile::Standard.ordinal()
}

/// JNI-friendly input bundle for the canonical `NativeRadar.tracking*` surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackingSnapshotJniInput {
    /// Previously filtered RSSI, or NaN when bootstrapping.
    pub previous_filtered_dbm: f64,
    /// Latest raw RSSI sample.
    pub current_rssi_dbm: f64,
    /// Recent filtered-RSSI spread.
    pub rssi_spread_db: f64,
    /// Number of filtered samples in the current history window.
    pub sample_count: i32,
    /// Rust-owned calibration profile ordinal.
    pub calibration_profile_ordinal: i32,
    /// Rust-owned tracking profile ordinal.
    pub tracking_profile_ordinal: i32,
    /// Observation age relative to "now".
    pub age_ms: i64,
}

fn tracking_snapshot_from_input(input: TrackingSnapshotJniInput) -> Option<TrackingSnapshot> {
    let sample_count = usize::try_from(input.sample_count).ok()?;
    let age_ms = u64::try_from(input.age_ms).ok()?;
    let calibration_profile = calibration_profile_from_ordinal(input.calibration_profile_ordinal)?;
    let tracking_profile = tracking_profile_from_ordinal(input.tracking_profile_ordinal)?;
    tracking_snapshot(TrackingSnapshotInput {
        previous_filtered_rssi_dbm: input.previous_filtered_dbm,
        current_rssi_dbm: input.current_rssi_dbm,
        rssi_spread_db: input.rssi_spread_db,
        sample_count,
        calibration_profile,
        tracking_profile,
        age_ms,
    })
}

/// Pure, unit-testable core of `NativeRadar.trackingFilteredRssi(...)`.
#[must_use]
pub fn tracking_filtered_rssi_or_nan(input: TrackingSnapshotJniInput) -> f64 {
    tracking_snapshot_from_input(input).map_or(f64::NAN, |snapshot| snapshot.filtered_rssi_dbm)
}

/// Pure, unit-testable core of `NativeRadar.trackingDistanceM(...)`.
#[must_use]
pub fn tracking_distance_m_or_nan(input: TrackingSnapshotJniInput) -> f64 {
    tracking_snapshot_from_input(input)
        .and_then(|snapshot| snapshot.distance_m)
        .unwrap_or(f64::NAN)
}

/// Pure, unit-testable core of `NativeRadar.trackingDistanceLowerBoundM(...)`.
#[must_use]
pub fn tracking_distance_lower_bound_m_or_nan(input: TrackingSnapshotJniInput) -> f64 {
    tracking_snapshot_from_input(input)
        .and_then(|snapshot| snapshot.distance_lower_bound_m)
        .unwrap_or(f64::NAN)
}

/// Pure, unit-testable core of `NativeRadar.trackingDistanceUpperBoundM(...)`.
#[must_use]
pub fn tracking_distance_upper_bound_m_or_nan(input: TrackingSnapshotJniInput) -> f64 {
    tracking_snapshot_from_input(input)
        .and_then(|snapshot| snapshot.distance_upper_bound_m)
        .unwrap_or(f64::NAN)
}

/// Pure, unit-testable core of `NativeRadar.trackingTrend(...)`.
#[must_use]
pub fn tracking_trend_ordinal(input: TrackingSnapshotJniInput) -> i32 {
    tracking_snapshot_from_input(input).map_or(2, |snapshot| match snapshot.trend {
        SignalTrend::Stronger => 0,
        SignalTrend::Weaker => 1,
        SignalTrend::Stable => 2,
    })
}

/// Pure, unit-testable core of `NativeRadar.trackingProximity(...)`.
#[must_use]
pub fn tracking_proximity_ordinal(input: TrackingSnapshotJniInput) -> i32 {
    tracking_snapshot_from_input(input).map_or(3, |snapshot| match snapshot.proximity {
        ProximityBand::Immediate => 0,
        ProximityBand::Near => 1,
        ProximityBand::Mid => 2,
        ProximityBand::Far => 3,
    })
}

/// Pure, unit-testable core of `NativeRadar.trackingConfidencePercent(...)`.
#[must_use]
pub fn tracking_confidence_percent_or_negative(input: TrackingSnapshotJniInput) -> i32 {
    tracking_snapshot_from_input(input)
        .map_or(-1, |snapshot| i32::from(snapshot.confidence_percent))
}

/// Pure, unit-testable core of `NativeRadar.trackingFreshness(...)`.
#[must_use]
pub fn tracking_freshness_ordinal(input: TrackingSnapshotJniInput) -> i32 {
    tracking_snapshot_from_input(input).map_or(2, |snapshot| match snapshot.freshness {
        FreshnessClass::Live => 0,
        FreshnessClass::Recent => 1,
        FreshnessClass::Stale => 2,
    })
}

/// `NativeRadar.filteredRssi(double, double, double): double` — see
/// [`filtered_rssi_or_nan`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_filteredRssi(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    alpha: f64,
) -> f64 {
    filtered_rssi_or_nan(previous_filtered_dbm, current_rssi_dbm, alpha)
}

/// `NativeRadar.bleDistanceM(double, double, double): double` — see
/// [`ble_distance_m_or_nan`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them. See the module docs for
/// why `#[unsafe(no_mangle)]` is nonetheless required.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_bleDistanceM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    rssi_dbm: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> f64 {
    ble_distance_m_or_nan(rssi_dbm, rssi_at_1m_dbm, path_loss_exponent)
}

/// `NativeRadar.proximityLabel(double): int` — see [`proximity_label_ordinal`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them. See the module docs for
/// why `#[unsafe(no_mangle)]` is nonetheless required.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_proximityLabel(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    rssi_dbm: f64,
) -> i32 {
    proximity_label_ordinal(rssi_dbm)
}

/// `NativeRadar.signalTrend(double, double, double): int` — see
/// [`signal_trend_ordinal`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them. See the module docs for
/// why `#[unsafe(no_mangle)]` is nonetheless required.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_signalTrend(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_dbm: f64,
    current_dbm: f64,
    deadband_db: f64,
) -> i32 {
    signal_trend_ordinal(previous_dbm, current_dbm, deadband_db)
}

/// `NativeRadar.distanceLowerBoundM(double, double, double, double): double`
/// — see [`distance_lower_bound_m_or_nan`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_distanceLowerBoundM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    rssi_dbm: f64,
    rssi_spread_db: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> f64 {
    distance_lower_bound_m_or_nan(rssi_dbm, rssi_spread_db, rssi_at_1m_dbm, path_loss_exponent)
}

/// `NativeRadar.distanceUpperBoundM(double, double, double, double): double`
/// — see [`distance_upper_bound_m_or_nan`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_distanceUpperBoundM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    rssi_dbm: f64,
    rssi_spread_db: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> f64 {
    distance_upper_bound_m_or_nan(rssi_dbm, rssi_spread_db, rssi_at_1m_dbm, path_loss_exponent)
}

/// `NativeRadar.signalConfidencePercent(int, double): int` — see
/// [`signal_confidence_percent_or_negative`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_signalConfidencePercent(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    sample_count: i32,
    rssi_spread_db: f64,
) -> i32 {
    signal_confidence_percent_or_negative(sample_count, rssi_spread_db)
}

/// `NativeRadar.calibrationProfileRssiAt1mDbm(int): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_calibrationProfileRssiAt1mDbm(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    profile_ordinal: i32,
) -> f64 {
    calibration_profile_rssi_at_1m_dbm_or_nan(profile_ordinal)
}

/// `NativeRadar.calibrationProfilePathLossExponent(int): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_calibrationProfilePathLossExponent(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    profile_ordinal: i32,
) -> f64 {
    calibration_profile_path_loss_exponent_or_nan(profile_ordinal)
}

/// `NativeRadar.defaultCalibrationProfile(): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_defaultCalibrationProfile(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
) -> i32 {
    default_calibration_profile_ordinal()
}

/// `NativeRadar.defaultTrackingProfile(): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_defaultTrackingProfile(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
) -> i32 {
    default_tracking_profile_ordinal()
}

/// `NativeRadar.trackingFilteredRssi(...): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingFilteredRssi(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> f64 {
    tracking_filtered_rssi_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingDistanceM(...): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingDistanceM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> f64 {
    tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingDistanceLowerBoundM(...): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingDistanceLowerBoundM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> f64 {
    tracking_distance_lower_bound_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingDistanceUpperBoundM(...): double`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingDistanceUpperBoundM(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> f64 {
    tracking_distance_upper_bound_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingTrend(...): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingTrend(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> i32 {
    tracking_trend_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingProximity(...): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingProximity(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> i32 {
    tracking_proximity_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingConfidencePercent(...): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingConfidencePercent(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> i32 {
    tracking_confidence_percent_or_negative(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.trackingFreshness(...): int`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingFreshness(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
) -> i32 {
    tracking_freshness_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
    })
}

/// `NativeRadar.abiVersion(): int` — a constant sanity check the Java side
/// calls once at startup to confirm the loaded `.so` matches the ABI this
/// file documents, independent of the app's own version number.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_abiVersion(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
) -> i32 {
    6
}
