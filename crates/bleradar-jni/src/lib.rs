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
//! `bleradar-core` itself for `aarch64-linux-android` and re-exports its pure
//! functions through the JNI calling convention, so the on-device calculation
//! is the exact code already covered by the workspace's gates.
//!
//! The same principle extends past the signal/tracking math to the app's
//! automatic-update *decision* core: `updateDecision`, `shouldCheckForUpdate`,
//! `downloadReadiness`, and `retryBackoffDelaySeconds` bridge
//! [`bleradar_core::update`] so the self-update safety decisions (version/OS
//! policy, re-check throttle, pre-download gating, backoff schedule) run the
//! verified Rust rather than a Java re-implementation. The network fetch and
//! the OS `PackageInstaller` remain the platform boundary (docs/AUTO_UPDATE.md).
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
    CalibrationProfile, DownloadConditions, DownloadPolicy, DownloadReadiness, FreshnessClass,
    NetworkType, ProximityBand, RetryPolicy, SignalTrend, TrackingProfile, TrackingSnapshot,
    TrackingSnapshotInput, ble_distance_m, ble_distance_range_m, calibration_profile,
    calibration_profile_from_ordinal, download_readiness, filtered_rssi, proximity_label,
    should_check_for_update, signal_confidence_percent, signal_trend,
    tracking_profile_from_ordinal, tracking_snapshot, update_decision,
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
/// in matching `int` constants. Non-finite RSSI maps to `3` (Far) so the UI
/// never treats an invalid reading as Immediate/Near.
#[must_use]
pub fn proximity_label_ordinal(rssi_dbm: f64) -> i32 {
    match proximity_label(rssi_dbm) {
        Some(ProximityBand::Immediate) => 0,
        Some(ProximityBand::Near) => 1,
        Some(ProximityBand::Mid) => 2,
        Some(ProximityBand::Far) | None => 3,
    }
}

/// Pure, unit-testable core of
/// [`Java_com_hse_bleradar_NativeRadar_signalTrend`].
///
/// Encodes [`SignalTrend`] as a small ordinal (`0` = [`SignalTrend::Stronger`],
/// `1` = [`SignalTrend::Weaker`], `2` = [`SignalTrend::Stable`]); see
/// [`proximity_label_ordinal`] for why an ordinal rather than a shared enum.
/// Non-finite inputs map to `2` (Stable) — the least-committing UI state —
/// matching the prior silent fallback while the Rust core itself returns
/// `None` so typed callers can distinguish "unknown" from "stable".
#[must_use]
pub fn signal_trend_ordinal(previous_dbm: f64, current_dbm: f64, deadband_db: f64) -> i32 {
    match signal_trend(previous_dbm, current_dbm, deadband_db) {
        Some(SignalTrend::Stronger) => 0,
        Some(SignalTrend::Weaker) => 1,
        Some(SignalTrend::Stable) | None => 2,
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
    /// Device-advertised/calibrated TX power in dBm, or NaN when absent.
    pub tx_power_dbm: f64,
}

fn tracking_snapshot_from_input(input: TrackingSnapshotJniInput) -> Option<TrackingSnapshot> {
    let sample_count = usize::try_from(input.sample_count).ok()?;
    let age_ms = u64::try_from(input.age_ms).ok()?;
    let calibration_profile = calibration_profile_from_ordinal(input.calibration_profile_ordinal)?;
    let tracking_profile = tracking_profile_from_ordinal(input.tracking_profile_ordinal)?;
    let tx_power_dbm = input.tx_power_dbm.is_finite().then_some(input.tx_power_dbm);
    tracking_snapshot(TrackingSnapshotInput {
        previous_filtered_rssi_dbm: input.previous_filtered_dbm,
        current_rssi_dbm: input.current_rssi_dbm,
        rssi_spread_db: input.rssi_spread_db,
        sample_count,
        calibration_profile,
        tracking_profile,
        age_ms,
        tx_power_dbm,
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

/// Pure, unit-testable core of `NativeRadar.trackingDistanceProximity(...)`.
///
/// This additive API classifies the calibrated distance while
/// [`tracking_proximity_ordinal`] preserves the legacy RSSI-based field.
#[must_use]
pub fn tracking_distance_proximity_ordinal(input: TrackingSnapshotJniInput) -> i32 {
    tracking_snapshot_from_input(input)
        .and_then(|snapshot| snapshot.distance_proximity)
        .map_or(3, |proximity| match proximity {
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
        .and_then(|snapshot| snapshot.confidence_percent)
        .map_or(-1, i32::from)
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

// ===== Automatic-update decision surface =====
//
// These bridge `bleradar_core`'s pure, exhaustively-tested update *decision*
// core — version/OS policy, the re-check throttle, pre-download gating, and the
// retry backoff schedule — to `NativeRadar.java`, so the Android app makes those
// self-update safety decisions with the exact verified Rust code (`update.rs`,
// `tests/update.rs`, `tests/update_campaign.rs`) instead of re-deriving them in
// Java. The network fetch and the OS `PackageInstaller` remain the documented
// platform boundary (see docs/AUTO_UPDATE.md). The stateful streaming
// `ArtifactVerifier` and `UpdateSession` are deliberately not bridged here: an
// owned-native-state JNI handle is a larger, separate surface, and every
// function below is a total function over primitives — exactly what the live
// JVM proof and the JNI differential campaign can exercise.

/// Saturating `i64 -> u64` for a JNI-supplied quantity that is non-negative by
/// contract (a `versionCode`, a timestamp, a byte count): a negative value is
/// nonsensical, so it is clamped to `0` rather than wrapping to a huge `u64`.
const fn jni_nonneg_u64(value: i64) -> u64 {
    if value < 0 { 0 } else { value as u64 }
}

/// Saturating `i32 -> u32` for a JNI-supplied non-negative field (an SDK level).
const fn jni_nonneg_u32(value: i32) -> u32 {
    if value < 0 { 0 } else { value as u32 }
}

/// Clamps a JNI-supplied battery reading to the documented `0..=100` domain.
const fn jni_percent_u8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > 100 {
        100
    } else {
        value as u8
    }
}

/// Pure, unit-testable core of `NativeRadar.updateDecision(...)`.
///
/// Returns the [`UpdateDecision`](bleradar_core::UpdateDecision) ordinal
/// (`0` = UpToDate, `1` = Available, `2` = DowngradeRefused,
/// `3` = IncompatibleOs); `NativeRadar.java` mirrors these as its `UPDATE_*`
/// constants. Version codes and SDK levels are
/// non-negative by contract; a negative input is clamped to `0`.
#[must_use]
pub fn update_decision_ordinal(
    installed_version_code: i64,
    available_version_code: i64,
    device_sdk_int: i32,
    min_sdk_int: i32,
) -> i32 {
    update_decision(
        jni_nonneg_u64(installed_version_code),
        jni_nonneg_u64(available_version_code),
        jni_nonneg_u32(device_sdk_int),
        jni_nonneg_u32(min_sdk_int),
    )
    .ordinal()
}

/// Pure, unit-testable core of `NativeRadar.shouldCheckForUpdate(...)`.
///
/// Whether enough time has elapsed since the last check to poll again, robust
/// against a clock that went backwards. Timestamps are any monotonic unit
/// (seconds recommended); a negative value is clamped to `0`.
#[must_use]
pub fn should_check_for_update_flag(now: i64, last_check: i64, min_interval: i64) -> bool {
    should_check_for_update(
        jni_nonneg_u64(now),
        jni_nonneg_u64(last_check),
        jni_nonneg_u64(min_interval),
    )
}

/// Maps a `NativeRadar.NETWORK_*` ordinal to a [`NetworkType`]. Anything
/// outside `0..=2` becomes [`NetworkType::None`] — the most conservative choice
/// (it blocks the download), so an unknown encoding can never be misread as "a
/// usable network is present".
const fn network_from_ordinal(network: i32) -> NetworkType {
    match network {
        1 => NetworkType::Metered,
        2 => NetworkType::Unmetered,
        _ => NetworkType::None,
    }
}

/// Maps a [`DownloadReadiness`] to a stable JNI ordinal (`0` = Ready,
/// `1` = NoNetwork, `2` = MeteredBlocked, `3` = LowBattery,
/// `4` = InsufficientStorage); `NativeRadar.java` mirrors these as its
/// `DOWNLOAD_*` constants. The `InsufficientStorage { needed, free }` payload is
/// not carried across JNI — the caller already sampled both quantities.
const fn download_readiness_ordinal_of(readiness: DownloadReadiness) -> i32 {
    match readiness {
        DownloadReadiness::Ready => 0,
        DownloadReadiness::NoNetwork => 1,
        DownloadReadiness::MeteredBlocked => 2,
        DownloadReadiness::LowBattery => 3,
        DownloadReadiness::InsufficientStorage { .. } => 4,
    }
}

/// Pure, unit-testable core of `NativeRadar.downloadReadiness(...)`.
///
/// Returns the [`DownloadReadiness`] ordinal for the first unmet precondition in
/// the engine's fixed precedence (no network → metered blocked → low battery →
/// insufficient storage → ready). Non-negative byte counts are clamped to `0`;
/// battery readings are clamped to `0..=100`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn download_readiness_ordinal(
    network: i32,
    battery_percent: i32,
    charging: bool,
    free_storage_bytes: i64,
    allow_metered: bool,
    min_battery_percent: i32,
    storage_headroom_bytes: i64,
    artifact_size_bytes: i64,
) -> i32 {
    let conditions = DownloadConditions {
        network: network_from_ordinal(network),
        battery_percent: jni_percent_u8(battery_percent),
        charging,
        free_storage_bytes: jni_nonneg_u64(free_storage_bytes),
    };
    let policy = DownloadPolicy {
        allow_metered,
        min_battery_percent: jni_percent_u8(min_battery_percent),
        storage_headroom_bytes: jni_nonneg_u64(storage_headroom_bytes),
    };
    download_readiness_ordinal_of(download_readiness(
        &conditions,
        &policy,
        jni_nonneg_u64(artifact_size_bytes),
    ))
}

/// Pure, unit-testable core of `NativeRadar.retryBackoffDelaySeconds(...)`.
///
/// The backoff delay before the given attempt (`base * 2^(attempt-1)`,
/// saturating and capped at `max_delay_secs`), from the authoritative
/// [`RetryPolicy::backoff_delay_secs`]. `max_attempts` is not a parameter — it
/// bounds *how many* retries happen, not the delay computed here. Negative
/// inputs are clamped to `0`, and the `u64` result is saturated into the
/// `i64`/`jlong` range.
#[must_use]
pub fn retry_backoff_delay_secs(attempt: i32, base_delay_secs: i64, max_delay_secs: i64) -> i64 {
    let policy = RetryPolicy {
        max_attempts: 1,
        base_delay_secs: jni_nonneg_u64(base_delay_secs),
        max_delay_secs: jni_nonneg_u64(max_delay_secs),
    };
    let delay = policy.backoff_delay_secs(jni_nonneg_u32(attempt));
    if delay > i64::MAX as u64 {
        i64::MAX
    } else {
        delay as i64
    }
}

// ===== Device snapshot ranking and pruning policy =====
//
// The Android app keeps its address-keyed device map in Java (JNI cannot read
// Java strings or return object arrays without dereferencing `JNIEnv`, which
// this crate never does), but the *policy* decisions over that map — which
// devices to drop and how to rank the survivors — are pure functions over
// primitives, so they live here and run the same code the workspace tests.

/// Pure, unit-testable core of `NativeRadar.deviceShouldPrune(int)`.
///
/// A device is dropped from the live map exactly when its freshness class is
/// [`FreshnessClass::Stale`] (ordinal `2`, matching
/// [`tracking_freshness_ordinal`]). Any other ordinal — including an unknown
/// one — keeps the device, so an encoding drift can never silently empty the map.
#[must_use]
pub const fn device_should_prune(freshness_ordinal: i32) -> bool {
    freshness_ordinal == 2
}

/// Pure, unit-testable core of `NativeRadar.deviceRankKey(int, long, int, double)`.
///
/// Packs the app's device ranking into one `i64` such that sorting ascending
/// by the key orders devices: live before recent before stale; then most
/// recently seen first; then highest confidence first; then strongest RSSI
/// first. Layout, most significant first:
///
/// | bits  | field                                  |
/// |-------|----------------------------------------|
/// | 62–61 | freshness ordinal, clamped to `0..=2`  |
/// | 60–21 | `2^40 - 1 - last_seen_uptime_ms` (40b) |
/// | 20–14 | `100 - confidence_percent` (7b)        |
/// | 13–0  | `RSSI_KEY_CEILING_DBM - rssi_dbm` (14b)|
///
/// Uptime is clamped to 40 bits (about 34.8 years of `SystemClock.uptimeMillis`),
/// confidence to `0..=100`, and RSSI to `-127..=20` dBm, so every field is
/// non-negative and the packed key is always non-negative. A non-finite RSSI
/// ranks as the weakest signal.
#[must_use]
pub fn device_rank_key(
    freshness_ordinal: i32,
    last_seen_uptime_ms: i64,
    confidence_percent: i32,
    rssi_dbm: f64,
) -> i64 {
    const UPTIME_BITS: u32 = 40;
    const UPTIME_MAX: i64 = (1 << UPTIME_BITS) - 1;
    const CONFIDENCE_BITS: u32 = 7;
    const RSSI_BITS: u32 = 14;
    const RSSI_KEY_CEILING_DBM: i64 = 20;
    const RSSI_KEY_FLOOR_DBM: i64 = -127;

    let freshness = i64::from(freshness_ordinal.clamp(0, 2));
    let recency = UPTIME_MAX - last_seen_uptime_ms.clamp(0, UPTIME_MAX);
    let confidence = i64::from(100 - confidence_percent.clamp(0, 100));
    let rssi = if rssi_dbm.is_finite() {
        // Truncation is intentional: sub-dBm precision is below the sensor's.
        (rssi_dbm as i64).clamp(RSSI_KEY_FLOOR_DBM, RSSI_KEY_CEILING_DBM)
    } else {
        RSSI_KEY_FLOOR_DBM
    };
    let weakness = RSSI_KEY_CEILING_DBM - rssi;

    (freshness << (UPTIME_BITS + CONFIDENCE_BITS + RSSI_BITS))
        | (recency << (CONFIDENCE_BITS + RSSI_BITS))
        | (confidence << RSSI_BITS)
        | weakness
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
    tx_power_dbm: f64,
) -> f64 {
    tracking_filtered_rssi_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> f64 {
    tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> f64 {
    tracking_distance_lower_bound_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> f64 {
    tracking_distance_upper_bound_m_or_nan(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> i32 {
    tracking_trend_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> i32 {
    tracking_proximity_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
    })
}

/// `NativeRadar.trackingDistanceProximity(...): int` — see
/// [`tracking_distance_proximity_ordinal`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_trackingDistanceProximity(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    previous_filtered_dbm: f64,
    current_rssi_dbm: f64,
    rssi_spread_db: f64,
    sample_count: i32,
    calibration_profile_ordinal: i32,
    tracking_profile_ordinal: i32,
    age_ms: i64,
    tx_power_dbm: f64,
) -> i32 {
    tracking_distance_proximity_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> i32 {
    tracking_confidence_percent_or_negative(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
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
    tx_power_dbm: f64,
) -> i32 {
    tracking_freshness_ordinal(TrackingSnapshotJniInput {
        previous_filtered_dbm,
        current_rssi_dbm,
        rssi_spread_db,
        sample_count,
        calibration_profile_ordinal,
        tracking_profile_ordinal,
        age_ms,
        tx_power_dbm,
    })
}

/// `NativeRadar.updateDecision(long, long, int, int): int` — see
/// [`update_decision_ordinal`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them. See the module docs for
/// why `#[unsafe(no_mangle)]` is nonetheless required.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_updateDecision(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    installed_version_code: i64,
    available_version_code: i64,
    device_sdk_int: i32,
    min_sdk_int: i32,
) -> i32 {
    update_decision_ordinal(
        installed_version_code,
        available_version_code,
        device_sdk_int,
        min_sdk_int,
    )
}

/// `NativeRadar.shouldCheckForUpdate(long, long, long): boolean` — see
/// [`should_check_for_update_flag`]. Returns the JNI `jboolean` encoding
/// (`1` = true, `0` = false).
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_shouldCheckForUpdate(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    now: i64,
    last_check: i64,
    min_interval: i64,
) -> u8 {
    should_check_for_update_flag(now, last_check, min_interval) as u8
}

/// `NativeRadar.downloadReadiness(int, int, boolean, long, boolean, int, long, long): int`
/// — see [`download_readiness_ordinal`]. The two `boolean` parameters use the
/// JNI `jboolean` encoding (non-zero = true).
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_downloadReadiness(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    network: i32,
    battery_percent: i32,
    charging: u8,
    free_storage_bytes: i64,
    allow_metered: u8,
    min_battery_percent: i32,
    storage_headroom_bytes: i64,
    artifact_size_bytes: i64,
) -> i32 {
    download_readiness_ordinal(
        network,
        battery_percent,
        charging != 0,
        free_storage_bytes,
        allow_metered != 0,
        min_battery_percent,
        storage_headroom_bytes,
        artifact_size_bytes,
    )
}

/// `NativeRadar.retryBackoffDelaySeconds(int, long, long): long` — see
/// [`retry_backoff_delay_secs`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_retryBackoffDelaySeconds(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    attempt: i32,
    base_delay_secs: i64,
    max_delay_secs: i64,
) -> i64 {
    retry_backoff_delay_secs(attempt, base_delay_secs, max_delay_secs)
}

/// `NativeRadar.deviceShouldPrune(int): boolean` — see [`device_should_prune`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_deviceShouldPrune(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    freshness_ordinal: i32,
) -> u8 {
    u8::from(device_should_prune(freshness_ordinal))
}

/// `NativeRadar.deviceRankKey(int, long, int, double): long` — see
/// [`device_rank_key`].
///
/// # Safety note
/// Ignores `_env`/`_class`; never dereferences them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_deviceRankKey(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
    freshness_ordinal: i32,
    last_seen_uptime_ms: i64,
    confidence_percent: i32,
    rssi_dbm: f64,
) -> i64 {
    device_rank_key(
        freshness_ordinal,
        last_seen_uptime_ms,
        confidence_percent,
        rssi_dbm,
    )
}

/// `NativeRadar.abiVersion(): int` — a constant sanity check the Java side
/// calls once at startup to confirm the loaded `.so` matches the ABI this
/// file documents, independent of the app's own version number.
///
/// Bumped to `8` when the automatic-update decision surface
/// (`updateDecision`, `shouldCheckForUpdate`, `downloadReadiness`,
/// `retryBackoffDelaySeconds`) was added to the ABI, and to `9` when the
/// device-map policy surface (`deviceShouldPrune`, `deviceRankKey`) was added.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_abiVersion(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
) -> i32 {
    9
}
