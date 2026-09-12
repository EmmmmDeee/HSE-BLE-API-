//! Differential campaign over every exported JNI symbol
//! (`docs/AUTONOMOUS_DECISIONS.md` #65).
//!
//! The Android app calls the 31 `Java_com_hse_bleradar_NativeRadar_*` exports
//! (the signal/tracking surface, the four automatic-update decision bridges —
//! `updateDecision`, `shouldCheckForUpdate`, `downloadReadiness`,
//! `retryBackoffDelaySeconds` — the two device-map policy bridges —
//! `deviceShouldPrune`, `deviceRankKey` — and the four string-bridge exports
//! of the release-manifest / artifact surface) on every scan result,
//! snapshot, or update check, and the release profile they ship with aborts
//! the process on any panic. Every primitive export ignores its
//! `JNIEnv`/`jclass` arguments, so this test calls the exported functions
//! themselves with null pointers; the string exports are driven through the
//! mock JNI function table in `common/mod.rs` (and with null pointers, which
//! must answer their sentinels). Over random and adversarial inputs (NaN,
//! infinities, signed zeros, subnormals, `f64::MAX`, arbitrary bit patterns,
//! `i32`/`i64` extremes, invalid ordinals, negative counts and ages, mutated
//! manifests) it checks:
//!
//! - no export panics;
//! - every export returns bit-for-bit what its documented pure core returns,
//!   and the pure core returns what the underlying `bleradar-core` function
//!   returns (argument order and sentinel encoding are therefore proven for
//!   the exported symbols, not only for the cores the other tests call);
//! - every ordinal is inside its documented range, every distance is NaN or
//!   finite and positive, bounds are NaN together or ordered around the
//!   central estimate, confidence is `-1` or `0..=100`;
//! - the domain invariants the UI relies on hold: a stronger signal never
//!   reads farther, trend is antisymmetric in its two samples, confidence is
//!   monotone in sample support and spread, the eight `tracking*` exports
//!   describe one coherent snapshot whose fields agree with the core
//!   classifiers and policy windows.
//!
//! Scale with `BLERADAR_JNI_CAMPAIGN_ITERATIONS`, reseed with
//! `BLERADAR_JNI_CAMPAIGN_SEED`; a failure prints the seed and the inputs.

mod common;

use std::cmp::Reverse;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bleradar_core::{
    CalibrationProfile, DownloadConditions, DownloadPolicy, DownloadReadiness, FreshnessClass,
    NetworkType, ProximityBand, ReleaseManifest, RetryPolicy, SignalTrend, TrackingProfile,
    TrackingSnapshotInput, ble_distance_m, ble_distance_range_m, calibration_profile,
    calibration_profile_from_ordinal, download_readiness, filtered_rssi, proximity_label,
    proximity_label_from_distance_m, should_check_for_update, signal_confidence_percent,
    signal_trend, tracking_profile, tracking_profile_from_ordinal, tracking_snapshot,
    update_decision,
};
use bleradar_jni::{
    ARTIFACT_MANIFEST_INVALID, ARTIFACT_UNREADABLE, Java_com_hse_bleradar_NativeRadar_abiVersion,
    Java_com_hse_bleradar_NativeRadar_artifactVerifyFile,
    Java_com_hse_bleradar_NativeRadar_bleDistanceM,
    Java_com_hse_bleradar_NativeRadar_calibrationProfilePathLossExponent,
    Java_com_hse_bleradar_NativeRadar_calibrationProfileRssiAt1mDbm,
    Java_com_hse_bleradar_NativeRadar_defaultCalibrationProfile,
    Java_com_hse_bleradar_NativeRadar_defaultTrackingProfile,
    Java_com_hse_bleradar_NativeRadar_deviceRankKey,
    Java_com_hse_bleradar_NativeRadar_deviceShouldPrune,
    Java_com_hse_bleradar_NativeRadar_distanceLowerBoundM,
    Java_com_hse_bleradar_NativeRadar_distanceUpperBoundM,
    Java_com_hse_bleradar_NativeRadar_downloadReadiness,
    Java_com_hse_bleradar_NativeRadar_filteredRssi,
    Java_com_hse_bleradar_NativeRadar_proximityLabel,
    Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical,
    Java_com_hse_bleradar_NativeRadar_releaseManifestError,
    Java_com_hse_bleradar_NativeRadar_releaseManifestField,
    Java_com_hse_bleradar_NativeRadar_retryBackoffDelaySeconds,
    Java_com_hse_bleradar_NativeRadar_shouldCheckForUpdate,
    Java_com_hse_bleradar_NativeRadar_signalConfidencePercent,
    Java_com_hse_bleradar_NativeRadar_signalTrend,
    Java_com_hse_bleradar_NativeRadar_trackingConfidencePercent,
    Java_com_hse_bleradar_NativeRadar_trackingDistanceLowerBoundM,
    Java_com_hse_bleradar_NativeRadar_trackingDistanceM,
    Java_com_hse_bleradar_NativeRadar_trackingDistanceProximity,
    Java_com_hse_bleradar_NativeRadar_trackingDistanceUpperBoundM,
    Java_com_hse_bleradar_NativeRadar_trackingFilteredRssi,
    Java_com_hse_bleradar_NativeRadar_trackingFreshness,
    Java_com_hse_bleradar_NativeRadar_trackingProximity,
    Java_com_hse_bleradar_NativeRadar_trackingTrend,
    Java_com_hse_bleradar_NativeRadar_updateDecision, TrackingSnapshotJniInput,
    ble_distance_m_or_nan, calibration_profile_path_loss_exponent_or_nan,
    calibration_profile_rssi_at_1m_dbm_or_nan, default_calibration_profile_ordinal,
    default_tracking_profile_ordinal, device_rank_key, device_should_prune,
    distance_lower_bound_m_or_nan, distance_upper_bound_m_or_nan, download_readiness_ordinal,
    filtered_rssi_or_nan, proximity_label_ordinal, release_manifest_canonical,
    release_manifest_error, release_manifest_field, retry_backoff_delay_secs,
    should_check_for_update_flag, signal_confidence_percent_or_negative, signal_trend_ordinal,
    tracking_confidence_percent_or_negative, tracking_distance_lower_bound_m_or_nan,
    tracking_distance_m_or_nan, tracking_distance_proximity_ordinal,
    tracking_distance_upper_bound_m_or_nan, tracking_filtered_rssi_or_nan,
    tracking_freshness_ordinal, tracking_proximity_ordinal, tracking_trend_ordinal,
    update_decision_ordinal,
};

const DEFAULT_ITERATIONS: u64 = 20_000;
const DEFAULT_SEED: u64 = 0x0517_2026_0910_0ABE;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }
}

const SPECIAL_F64: &[f64] = &[
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    0.0,
    -0.0,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    5e-324,
    -5e-324,
    f64::MAX,
    f64::MIN,
    1e308,
    -1e308,
    1e-308,
    1.0,
    -1.0,
    127.0,
    -127.0,
    -59.0,
    -100.0,
    20.0,
    2.0,
];
const SPECIAL_I32: &[i32] = &[i32::MIN, i32::MAX, -1, 0, 1, 2, 3, 11, 12, 13, 99, -99];
const SPECIAL_I64: &[i64] = &[
    i64::MIN,
    i64::MAX,
    -1,
    0,
    1,
    3_999,
    4_000,
    4_001,
    4_999,
    5_000,
    5_001,
    24_999,
    25_000,
    25_001,
    29_999,
    30_000,
    30_001,
];

/// A signal-like value: mostly plausible dBm/dB/exponent magnitudes, often a
/// special value, sometimes an arbitrary bit pattern (which covers NaN
/// payloads and every exponent range).
fn value(rng: &mut Rng, low: f64, high: f64) -> f64 {
    match rng.below(10) {
        0..=4 => rng.uniform(low, high),
        5..=7 => SPECIAL_F64[rng.below(SPECIAL_F64.len())],
        _ => f64::from_bits(rng.next()),
    }
}

fn int(rng: &mut Rng) -> i32 {
    match rng.below(4) {
        0 => SPECIAL_I32[rng.below(SPECIAL_I32.len())],
        1 => rng.below(20) as i32,
        _ => rng.next() as i32,
    }
}

fn age(rng: &mut Rng) -> i64 {
    match rng.below(4) {
        0 => SPECIAL_I64[rng.below(SPECIAL_I64.len())],
        1 => rng.below(60_000) as i64,
        _ => rng.next() as i64,
    }
}

fn null() -> *mut core::ffi::c_void {
    std::ptr::null_mut()
}

fn same(left: f64, right: f64) -> bool {
    (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
}

fn or_nan(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

fn proximity_ordinal(band: ProximityBand) -> i32 {
    match band {
        ProximityBand::Immediate => 0,
        ProximityBand::Near => 1,
        ProximityBand::Mid => 2,
        ProximityBand::Far => 3,
    }
}

fn trend_ordinal(trend: SignalTrend) -> i32 {
    match trend {
        SignalTrend::Stronger => 0,
        SignalTrend::Weaker => 1,
        SignalTrend::Stable => 2,
    }
}

fn freshness_ordinal(freshness: FreshnessClass) -> i32 {
    match freshness {
        FreshnessClass::Live => 0,
        FreshnessClass::Recent => 1,
        FreshnessClass::Stale => 2,
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn check_distance(label: &str, value: f64) -> Result<(), String> {
    if value.is_nan() || (value.is_finite() && value > 0.0) {
        Ok(())
    } else {
        Err(format!(
            "{label} returned {value}, not NaN or finite positive"
        ))
    }
}

fn check_stateless(rng: &mut Rng) -> Result<(), String> {
    let rssi = value(rng, -120.0, 0.0);
    let rssi_at_1m = value(rng, -90.0, -30.0);
    let exponent = value(rng, -1.0, 6.0);
    let spread = value(rng, -1.0, 20.0);
    let previous = value(rng, -120.0, 0.0);
    let alpha = value(rng, -0.5, 1.5);
    let deadband = value(rng, -5.0, 5.0);
    let count = int(rng);
    let ordinal = int(rng);
    let context = format!(
        "rssi={rssi:?} rssi_at_1m={rssi_at_1m:?} exponent={exponent:?} spread={spread:?} \
         previous={previous:?} alpha={alpha:?} deadband={deadband:?} count={count} ordinal={ordinal}"
    );
    let fail = |message: String| Err(format!("{message} [{context}]"));

    // bleDistanceM
    let distance =
        Java_com_hse_bleradar_NativeRadar_bleDistanceM(null(), null(), rssi, rssi_at_1m, exponent);
    if !same(distance, ble_distance_m_or_nan(rssi, rssi_at_1m, exponent))
        || !same(distance, or_nan(ble_distance_m(rssi, rssi_at_1m, exponent)))
    {
        return fail("bleDistanceM export/core/bleradar-core disagree".into());
    }
    check_distance("bleDistanceM", distance).or_else(&fail)?;
    if distance.is_finite() {
        // A stronger signal never reads farther (within floating-point slack).
        let stronger = ble_distance_m_or_nan(rssi + 1.0, rssi_at_1m, exponent);
        if stronger.is_finite() && stronger > distance * (1.0 + 1e-12) {
            return fail(format!(
                "stronger signal reads farther: {stronger} > {distance}"
            ));
        }
    }

    // filteredRssi
    let filtered =
        Java_com_hse_bleradar_NativeRadar_filteredRssi(null(), null(), previous, rssi, alpha);
    if !same(filtered, filtered_rssi_or_nan(previous, rssi, alpha))
        || !same(filtered, or_nan(filtered_rssi(previous, rssi, alpha)))
    {
        return fail("filteredRssi export/core/bleradar-core disagree".into());
    }
    if !(filtered.is_nan() || filtered.is_finite()) {
        return fail(format!("filteredRssi returned {filtered}"));
    }
    if filtered.is_finite() && !previous.is_finite() && !same(filtered, rssi) {
        return fail("filteredRssi did not bootstrap from the current sample".into());
    }

    // proximityLabel
    let proximity = Java_com_hse_bleradar_NativeRadar_proximityLabel(null(), null(), rssi);
    let expected = proximity_label(rssi).map_or(3, proximity_ordinal);
    if proximity != proximity_label_ordinal(rssi) || proximity != expected {
        return fail("proximityLabel export/core/bleradar-core disagree".into());
    }
    if !(0..=3).contains(&proximity) {
        return fail(format!("proximityLabel ordinal {proximity} out of range"));
    }
    if rssi.is_finite() {
        let weaker = proximity_label_ordinal(rssi - rng.uniform(0.0, 40.0));
        if weaker < proximity {
            return fail(format!(
                "weaker signal reads nearer: {weaker} < {proximity}"
            ));
        }
    }

    // signalTrend
    let trend =
        Java_com_hse_bleradar_NativeRadar_signalTrend(null(), null(), previous, rssi, deadband);
    let expected = signal_trend(previous, rssi, deadband).map_or(2, trend_ordinal);
    if trend != signal_trend_ordinal(previous, rssi, deadband) || trend != expected {
        return fail("signalTrend export/core/bleradar-core disagree".into());
    }
    if !(0..=2).contains(&trend) {
        return fail(format!("signalTrend ordinal {trend} out of range"));
    }
    let reversed = signal_trend_ordinal(rssi, previous, deadband);
    let antisymmetric = match trend {
        0 => reversed == 1,
        1 => reversed == 0,
        _ => reversed == 2,
    };
    if !antisymmetric {
        return fail(format!(
            "signalTrend is not antisymmetric: {trend} vs reversed {reversed}"
        ));
    }

    // distanceLowerBoundM / distanceUpperBoundM
    let lower = Java_com_hse_bleradar_NativeRadar_distanceLowerBoundM(
        null(),
        null(),
        rssi,
        spread,
        rssi_at_1m,
        exponent,
    );
    let upper = Java_com_hse_bleradar_NativeRadar_distanceUpperBoundM(
        null(),
        null(),
        rssi,
        spread,
        rssi_at_1m,
        exponent,
    );
    let range = ble_distance_range_m(rssi, spread, rssi_at_1m, exponent);
    if !same(
        lower,
        distance_lower_bound_m_or_nan(rssi, spread, rssi_at_1m, exponent),
    ) || !same(
        upper,
        distance_upper_bound_m_or_nan(rssi, spread, rssi_at_1m, exponent),
    ) || !same(lower, or_nan(range.map(|(lower, _)| lower)))
        || !same(upper, or_nan(range.map(|(_, upper)| upper)))
    {
        return fail("distance bounds export/core/bleradar-core disagree".into());
    }
    check_distance("distanceLowerBoundM", lower).or_else(&fail)?;
    check_distance("distanceUpperBoundM", upper).or_else(&fail)?;
    if lower.is_nan() != upper.is_nan() {
        return fail(format!("bounds disagree on validity: {lower} / {upper}"));
    }
    if lower.is_finite() && lower > upper {
        return fail(format!("lower bound {lower} above upper bound {upper}"));
    }
    if lower.is_finite() && distance.is_finite() && !(lower <= distance && distance <= upper) {
        return fail(format!(
            "central estimate {distance} outside its bounds {lower}..{upper}"
        ));
    }

    // signalConfidencePercent
    let confidence =
        Java_com_hse_bleradar_NativeRadar_signalConfidencePercent(null(), null(), count, spread);
    let expected = usize::try_from(count)
        .ok()
        .and_then(|count| signal_confidence_percent(count, spread))
        .map_or(-1, i32::from);
    if confidence != signal_confidence_percent_or_negative(count, spread) || confidence != expected
    {
        return fail("signalConfidencePercent export/core/bleradar-core disagree".into());
    }
    if confidence != -1 && !(0..=100).contains(&confidence) {
        return fail(format!("signalConfidencePercent {confidence} out of range"));
    }
    if confidence >= 0 {
        let more_support = signal_confidence_percent_or_negative(count.saturating_add(1), spread);
        if more_support < confidence {
            return fail(format!(
                "more samples lowered confidence: {more_support} < {confidence}"
            ));
        }
        let more_spread =
            signal_confidence_percent_or_negative(count, spread + rng.uniform(0.0, 20.0));
        if more_spread > confidence {
            return fail(format!(
                "more spread raised confidence: {more_spread} > {confidence}"
            ));
        }
    }

    // calibration profiles and defaults
    let rssi_at_1m_profile =
        Java_com_hse_bleradar_NativeRadar_calibrationProfileRssiAt1mDbm(null(), null(), ordinal);
    let exponent_profile = Java_com_hse_bleradar_NativeRadar_calibrationProfilePathLossExponent(
        null(),
        null(),
        ordinal,
    );
    let profile = calibration_profile_from_ordinal(ordinal).map(calibration_profile);
    if !same(
        rssi_at_1m_profile,
        calibration_profile_rssi_at_1m_dbm_or_nan(ordinal),
    ) || !same(
        rssi_at_1m_profile,
        or_nan(profile.map(|p| p.rssi_at_1m_dbm)),
    ) || !same(
        exponent_profile,
        calibration_profile_path_loss_exponent_or_nan(ordinal),
    ) || !same(
        exponent_profile,
        or_nan(profile.map(|p| p.path_loss_exponent)),
    ) {
        return fail("calibration profile exports disagree with bleradar-core".into());
    }
    if rssi_at_1m_profile.is_nan() != exponent_profile.is_nan() {
        return fail("calibration profile exports disagree on validity".into());
    }
    if exponent_profile.is_finite() && exponent_profile <= 0.0 {
        return fail(format!(
            "profile path-loss exponent {exponent_profile} not positive"
        ));
    }
    if rssi_at_1m_profile.is_finite() && !(-100.0..=0.0).contains(&rssi_at_1m_profile) {
        return fail(format!(
            "profile rssi at 1 m {rssi_at_1m_profile} implausible"
        ));
    }
    let default_calibration =
        Java_com_hse_bleradar_NativeRadar_defaultCalibrationProfile(null(), null());
    let default_tracking = Java_com_hse_bleradar_NativeRadar_defaultTrackingProfile(null(), null());
    if default_calibration != default_calibration_profile_ordinal()
        || default_tracking != default_tracking_profile_ordinal()
        || calibration_profile_from_ordinal(default_calibration).is_none()
        || tracking_profile_from_ordinal(default_tracking).is_none()
    {
        return fail("default profile ordinals are not valid ordinals".into());
    }
    if Java_com_hse_bleradar_NativeRadar_abiVersion(null(), null()) <= 0 {
        return fail("abiVersion is not positive".into());
    }
    Ok(())
}

fn expected_snapshot(input: TrackingSnapshotJniInput) -> Option<bleradar_core::TrackingSnapshot> {
    let sample_count = usize::try_from(input.sample_count).ok()?;
    let age_ms = u64::try_from(input.age_ms).ok()?;
    let calibration_profile: CalibrationProfile =
        calibration_profile_from_ordinal(input.calibration_profile_ordinal)?;
    let tracking_profile: TrackingProfile =
        tracking_profile_from_ordinal(input.tracking_profile_ordinal)?;
    tracking_snapshot(TrackingSnapshotInput {
        previous_filtered_rssi_dbm: input.previous_filtered_dbm,
        current_rssi_dbm: input.current_rssi_dbm,
        rssi_spread_db: input.rssi_spread_db,
        sample_count,
        calibration_profile,
        tracking_profile,
        age_ms,
        tx_power_dbm: input.tx_power_dbm.is_finite().then_some(input.tx_power_dbm),
    })
}

fn check_tracking(rng: &mut Rng) -> Result<(), String> {
    let input = TrackingSnapshotJniInput {
        previous_filtered_dbm: value(rng, -120.0, 0.0),
        current_rssi_dbm: value(rng, -120.0, 0.0),
        rssi_spread_db: value(rng, -1.0, 20.0),
        sample_count: int(rng),
        calibration_profile_ordinal: if rng.below(4) == 0 {
            int(rng)
        } else {
            rng.below(3) as i32
        },
        tracking_profile_ordinal: if rng.below(4) == 0 {
            int(rng)
        } else {
            rng.below(2) as i32
        },
        age_ms: age(rng),
        tx_power_dbm: value(rng, -110.0, 30.0),
    };
    let fail = |message: String| Err(format!("{message} [{input:?}]"));
    let call = |name: &str, export: f64, core: f64, expected: f64| -> Result<(), String> {
        if !same(export, core) || !same(export, expected) {
            return fail(format!(
                "{name} disagrees: export {export} core {core} bleradar-core {expected}"
            ));
        }
        Ok(())
    };
    let (p, c, s, n, cp, tp, a, tx) = (
        input.previous_filtered_dbm,
        input.current_rssi_dbm,
        input.rssi_spread_db,
        input.sample_count,
        input.calibration_profile_ordinal,
        input.tracking_profile_ordinal,
        input.age_ms,
        input.tx_power_dbm,
    );
    let snapshot = expected_snapshot(input);

    let filtered = Java_com_hse_bleradar_NativeRadar_trackingFilteredRssi(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    call(
        "trackingFilteredRssi",
        filtered,
        tracking_filtered_rssi_or_nan(input),
        or_nan(snapshot.map(|snapshot| snapshot.filtered_rssi_dbm)),
    )?;
    let distance = Java_com_hse_bleradar_NativeRadar_trackingDistanceM(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    call(
        "trackingDistanceM",
        distance,
        tracking_distance_m_or_nan(input),
        or_nan(snapshot.and_then(|snapshot| snapshot.distance_m)),
    )?;
    let lower = Java_com_hse_bleradar_NativeRadar_trackingDistanceLowerBoundM(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    call(
        "trackingDistanceLowerBoundM",
        lower,
        tracking_distance_lower_bound_m_or_nan(input),
        or_nan(snapshot.and_then(|snapshot| snapshot.distance_lower_bound_m)),
    )?;
    let upper = Java_com_hse_bleradar_NativeRadar_trackingDistanceUpperBoundM(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    call(
        "trackingDistanceUpperBoundM",
        upper,
        tracking_distance_upper_bound_m_or_nan(input),
        or_nan(snapshot.and_then(|snapshot| snapshot.distance_upper_bound_m)),
    )?;
    let trend =
        Java_com_hse_bleradar_NativeRadar_trackingTrend(null(), null(), p, c, s, n, cp, tp, a, tx);
    let proximity = Java_com_hse_bleradar_NativeRadar_trackingProximity(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    let distance_proximity = Java_com_hse_bleradar_NativeRadar_trackingDistanceProximity(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    let confidence = Java_com_hse_bleradar_NativeRadar_trackingConfidencePercent(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    let freshness = Java_com_hse_bleradar_NativeRadar_trackingFreshness(
        null(),
        null(),
        p,
        c,
        s,
        n,
        cp,
        tp,
        a,
        tx,
    );
    let ordinals = [
        (
            "trackingTrend",
            trend,
            tracking_trend_ordinal(input),
            snapshot.map_or(2, |s| trend_ordinal(s.trend)),
            2,
        ),
        (
            "trackingProximity",
            proximity,
            tracking_proximity_ordinal(input),
            snapshot.map_or(3, |s| proximity_ordinal(s.proximity)),
            3,
        ),
        (
            "trackingDistanceProximity",
            distance_proximity,
            tracking_distance_proximity_ordinal(input),
            snapshot
                .and_then(|s| s.distance_proximity)
                .map_or(3, proximity_ordinal),
            3,
        ),
        (
            "trackingConfidencePercent",
            confidence,
            tracking_confidence_percent_or_negative(input),
            snapshot
                .and_then(|s| s.confidence_percent)
                .map_or(-1, i32::from),
            100,
        ),
        (
            "trackingFreshness",
            freshness,
            tracking_freshness_ordinal(input),
            snapshot.map_or(2, |s| freshness_ordinal(s.freshness)),
            2,
        ),
    ];
    for (name, export, core, expected, max) in ordinals {
        if export != core || export != expected {
            return fail(format!(
                "{name} disagrees: export {export} core {core} bleradar-core {expected}"
            ));
        }
        let min = if name == "trackingConfidencePercent" {
            -1
        } else {
            0
        };
        if !(min..=max).contains(&export) {
            return fail(format!("{name} ordinal {export} out of range"));
        }
    }

    // Snapshot coherence, in terms of the exported values.
    let Some(snapshot) = snapshot else {
        // Rejected input: every export must carry its invalid sentinel.
        if !(filtered.is_nan()
            && distance.is_nan()
            && lower.is_nan()
            && upper.is_nan()
            && trend == 2
            && proximity == 3
            && distance_proximity == 3
            && confidence == -1
            && freshness == 2)
        {
            return fail("rejected input did not yield the invalid sentinels everywhere".into());
        }
        let rejected_for_cause = usize::try_from(n).is_err()
            || u64::try_from(a).is_err()
            || calibration_profile_from_ordinal(cp).is_none()
            || tracking_profile_from_ordinal(tp).is_none()
            || !c.is_finite();
        if !rejected_for_cause {
            return fail("input rejected without a documented cause".into());
        }
        return Ok(());
    };
    if !filtered.is_finite() {
        return fail("accepted input has a non-finite filtered RSSI".into());
    }
    if !p.is_finite() && !same(filtered, c) {
        return fail("bootstrap snapshot did not adopt the current sample".into());
    }
    if !p.is_finite() && trend != 2 {
        return fail("bootstrap snapshot reported a trend".into());
    }
    let policy = tracking_profile(tracking_profile_from_ordinal(tp).unwrap());
    if p.is_finite() {
        let expected = signal_trend(p, filtered, policy.trend_deadband_db).map_or(2, trend_ordinal);
        if trend != expected {
            return fail(format!(
                "trend {trend} disagrees with the core classifier {expected}"
            ));
        }
    }
    if proximity != proximity_label(filtered).map_or(3, proximity_ordinal) {
        return fail("proximity disagrees with proximity_label(filtered)".into());
    }
    check_distance("trackingDistanceM", distance).or_else(&fail)?;
    check_distance("trackingDistanceLowerBoundM", lower).or_else(&fail)?;
    check_distance("trackingDistanceUpperBoundM", upper).or_else(&fail)?;
    if lower.is_nan() != upper.is_nan() {
        return fail("tracking bounds disagree on validity".into());
    }
    if lower.is_finite() && lower > upper {
        return fail(format!(
            "tracking lower bound {lower} above upper bound {upper}"
        ));
    }
    if lower.is_finite() && distance.is_finite() && !(lower <= distance && distance <= upper) {
        return fail(format!(
            "tracking estimate {distance} outside {lower}..{upper}"
        ));
    }
    if lower.is_finite() && !distance.is_finite() {
        return fail("tracking bounds present without a central estimate".into());
    }
    if distance_proximity != proximity_label_from_distance_m(distance).map_or(3, proximity_ordinal)
    {
        return fail("distance proximity disagrees with proximity_label_from_distance_m".into());
    }
    if distance.is_nan() && distance_proximity != 3 {
        return fail("distance proximity without a distance".into());
    }
    let expected_confidence = usize::try_from(n)
        .ok()
        .and_then(|count| signal_confidence_percent(count, s))
        .map_or(-1, i32::from);
    if confidence != expected_confidence {
        return fail(format!(
            "confidence {confidence} disagrees with {expected_confidence}"
        ));
    }
    if (s.is_finite() && s >= 0.0) != (confidence >= 0) {
        return fail("confidence availability disagrees with spread validity".into());
    }
    let expected_freshness = freshness_ordinal(FreshnessClass::from_age(
        u64::try_from(a).unwrap(),
        policy.live_window_ms,
        policy.recent_window_ms,
    ));
    if freshness != expected_freshness {
        return fail(format!(
            "freshness {freshness} disagrees with policy windows {expected_freshness}"
        ));
    }
    if snapshot.freshness
        != FreshnessClass::from_age(
            u64::try_from(a).unwrap(),
            policy.live_window_ms,
            policy.recent_window_ms,
        )
    {
        return fail("snapshot freshness disagrees with FreshnessClass::from_age".into());
    }
    Ok(())
}

/// Independent reference for the bridge's `i64 -> u64` clamp (negatives to 0).
fn nonneg_u64(value: i64) -> u64 {
    if value < 0 { 0 } else { value as u64 }
}

/// Independent reference for the bridge's `i32 -> u32` clamp (negatives to 0).
fn nonneg_u32(value: i32) -> u32 {
    if value < 0 { 0 } else { value as u32 }
}

/// Independent reference for the bridge's battery clamp to `0..=100`.
fn clamp_percent(value: i32) -> u8 {
    value.clamp(0, 100) as u8
}

/// A non-negative-by-contract `i64` (version code, timestamp, byte count),
/// reusing the adversarial [`age`] generator so update scalars also hit the
/// `i64` extremes and the storage/threshold special values.
fn scalar(rng: &mut Rng) -> i64 {
    age(rng)
}

/// Independent reference for the bridge's [`DownloadReadiness`] -> ordinal map.
fn readiness_ordinal(readiness: DownloadReadiness) -> i32 {
    match readiness {
        DownloadReadiness::Ready => 0,
        DownloadReadiness::NoNetwork => 1,
        DownloadReadiness::MeteredBlocked => 2,
        DownloadReadiness::LowBattery => 3,
        DownloadReadiness::InsufficientStorage { .. } => 4,
    }
}

/// Independent reference for the bridge's ordinal -> [`NetworkType`] map.
fn net_from_ordinal(network: i32) -> NetworkType {
    match network {
        1 => NetworkType::Metered,
        2 => NetworkType::Unmetered,
        _ => NetworkType::None,
    }
}

/// Differential check over the four automatic-update decision exports: each is
/// called directly (null `JNIEnv`/`jclass`) with random and adversarial inputs
/// and must return bit-for-bit what its pure core returns and what an
/// independent reconstruction of the underlying `bleradar_core::update`
/// contract returns; every ordinal stays in range, no downgrade is ever
/// offered, and the backoff never exceeds its cap.
fn check_update(rng: &mut Rng) -> Result<(), String> {
    // --- Version / OS decision ---
    let installed = scalar(rng);
    let available = scalar(rng);
    let device_sdk = int(rng);
    let min_sdk = int(rng);
    let decision = Java_com_hse_bleradar_NativeRadar_updateDecision(
        null(),
        null(),
        installed,
        available,
        device_sdk,
        min_sdk,
    );
    let expected = update_decision(
        nonneg_u64(installed),
        nonneg_u64(available),
        nonneg_u32(device_sdk),
        nonneg_u32(min_sdk),
    )
    .ordinal();
    if decision != update_decision_ordinal(installed, available, device_sdk, min_sdk)
        || decision != expected
    {
        return Err(format!(
            "updateDecision export/core/bleradar-core disagree \
             [installed={installed} available={available} device_sdk={device_sdk} min_sdk={min_sdk}]"
        ));
    }
    if !(0..=3).contains(&decision) {
        return Err(format!("updateDecision ordinal {decision} out of range"));
    }
    if nonneg_u64(available) < nonneg_u64(installed) && decision == 1 {
        return Err("a downgrade was offered as Available".into());
    }
    if nonneg_u64(available) == nonneg_u64(installed) && decision != 0 {
        return Err("equal versionCode was not reported as UpToDate".into());
    }

    // --- Re-check throttle ---
    let now = scalar(rng);
    let last = scalar(rng);
    let interval = scalar(rng);
    let flag =
        Java_com_hse_bleradar_NativeRadar_shouldCheckForUpdate(null(), null(), now, last, interval)
            != 0;
    let expected_flag =
        should_check_for_update(nonneg_u64(now), nonneg_u64(last), nonneg_u64(interval));
    if flag != should_check_for_update_flag(now, last, interval) || flag != expected_flag {
        return Err(format!(
            "shouldCheckForUpdate export/core/bleradar-core disagree \
             [now={now} last={last} interval={interval}]"
        ));
    }

    // --- Pre-download gating ---
    let network = int(rng);
    let battery = int(rng);
    let charging = rng.below(2) == 0;
    let free = scalar(rng);
    let allow_metered = rng.below(2) == 0;
    let min_battery = int(rng);
    let headroom = scalar(rng);
    let size = scalar(rng);
    let readiness = Java_com_hse_bleradar_NativeRadar_downloadReadiness(
        null(),
        null(),
        network,
        battery,
        charging as u8,
        free,
        allow_metered as u8,
        min_battery,
        headroom,
        size,
    );
    let conditions = DownloadConditions {
        network: net_from_ordinal(network),
        battery_percent: clamp_percent(battery),
        charging,
        free_storage_bytes: nonneg_u64(free),
    };
    let policy = DownloadPolicy {
        allow_metered,
        min_battery_percent: clamp_percent(min_battery),
        storage_headroom_bytes: nonneg_u64(headroom),
    };
    let expected_readiness =
        readiness_ordinal(download_readiness(&conditions, &policy, nonneg_u64(size)));
    let core_readiness = download_readiness_ordinal(
        network,
        battery,
        charging,
        free,
        allow_metered,
        min_battery,
        headroom,
        size,
    );
    if readiness != core_readiness || readiness != expected_readiness {
        return Err(format!(
            "downloadReadiness export/core/bleradar-core disagree \
             [network={network} battery={battery} charging={charging} free={free} \
              allow_metered={allow_metered} min_battery={min_battery} headroom={headroom} size={size}]"
        ));
    }
    if !(0..=4).contains(&readiness) {
        return Err(format!(
            "downloadReadiness ordinal {readiness} out of range"
        ));
    }
    if net_from_ordinal(network) == NetworkType::None && readiness != 1 {
        return Err("no-network condition not reported as NoNetwork".into());
    }

    // --- Retry backoff ---
    let attempt = int(rng);
    let base = scalar(rng);
    let cap = scalar(rng);
    let delay = Java_com_hse_bleradar_NativeRadar_retryBackoffDelaySeconds(
        null(),
        null(),
        attempt,
        base,
        cap,
    );
    let backoff_policy = RetryPolicy {
        max_attempts: 1,
        base_delay_secs: nonneg_u64(base),
        max_delay_secs: nonneg_u64(cap),
    };
    let raw = backoff_policy.backoff_delay_secs(nonneg_u32(attempt));
    let expected_delay = if raw > i64::MAX as u64 {
        i64::MAX
    } else {
        raw as i64
    };
    if delay != retry_backoff_delay_secs(attempt, base, cap) || delay != expected_delay {
        return Err(format!(
            "retryBackoffDelaySeconds export/core/bleradar-core disagree \
             [attempt={attempt} base={base} cap={cap}]"
        ));
    }
    if delay < 0 {
        return Err(format!(
            "retryBackoffDelaySeconds returned negative {delay}"
        ));
    }
    if delay as u64 > nonneg_u64(cap) {
        return Err(format!(
            "retryBackoffDelaySeconds {delay} exceeds the cap {}",
            nonneg_u64(cap)
        ));
    }
    Ok(())
}

/// A freshness ordinal: usually a valid `0..=2`, sometimes any `i32`.
fn freshness(rng: &mut Rng) -> i32 {
    if rng.below(4) == 0 {
        int(rng)
    } else {
        rng.below(3) as i32
    }
}

/// A device as `BleScanEngine.snapshot()` presents it to `deviceRankKey`:
/// freshness ordinal, last-seen uptime, confidence percent, filtered RSSI —
/// each drawn from the adversarial generators so every clamp edge is hit.
fn device(rng: &mut Rng) -> (i32, i64, i32, f64) {
    (freshness(rng), age(rng), int(rng), value(rng, -130.0, 30.0))
}

/// Independent reference for the ranking the bridge packs into one `i64`:
/// freshness ascending, then last-seen descending, then confidence descending,
/// then whole-dBm RSSI descending, each over the key's documented clamped domain
/// (a non-finite RSSI is the weakest signal).
fn reference_rank(device: (i32, i64, i32, f64)) -> (i32, Reverse<i64>, Reverse<i32>, Reverse<i64>) {
    let rssi = if device.3.is_finite() {
        (device.3 as i64).clamp(-127, 20)
    } else {
        -127
    };
    (
        device.0.clamp(0, 2),
        Reverse(device.1.clamp(0, (1 << 40) - 1)),
        Reverse(device.2.clamp(0, 100)),
        Reverse(rssi),
    )
}

/// Differential check over the two device-map policy exports: the prune
/// decision is exactly "ordinal == Stale" (the encoding `trackingFreshness`
/// emits) and a proper `jboolean`; the rank key equals its core, is never
/// negative, and orders two random devices — which often share fields so every
/// tie-break tier is reached — exactly as the independent reference does.
fn check_device_policy(rng: &mut Rng) -> Result<(), String> {
    let ordinal = freshness(rng);
    let prune = Java_com_hse_bleradar_NativeRadar_deviceShouldPrune(null(), null(), ordinal);
    if prune > 1 {
        return Err(format!(
            "deviceShouldPrune returned non-boolean {prune} [freshness={ordinal}]"
        ));
    }
    let prune = prune != 0;
    if prune != device_should_prune(ordinal)
        || prune != (ordinal == freshness_ordinal(FreshnessClass::Stale))
    {
        return Err(format!(
            "deviceShouldPrune export/core/reference disagree [freshness={ordinal}]"
        ));
    }

    let left = device(rng);
    let mut right = device(rng);
    if rng.below(2) == 0 {
        right.0 = left.0;
    }
    if rng.below(2) == 0 {
        right.1 = left.1;
    }
    if rng.below(2) == 0 {
        right.2 = left.2;
    }
    let key = |d: (i32, i64, i32, f64)| {
        Java_com_hse_bleradar_NativeRadar_deviceRankKey(null(), null(), d.0, d.1, d.2, d.3)
    };
    let (left_key, right_key) = (key(left), key(right));
    for (label, d, k) in [("left", left, left_key), ("right", right, right_key)] {
        if k != device_rank_key(d.0, d.1, d.2, d.3) {
            return Err(format!(
                "deviceRankKey export/core disagree [{label}={d:?}]"
            ));
        }
        if k < 0 {
            return Err(format!(
                "deviceRankKey returned negative {k} [{label}={d:?}]"
            ));
        }
    }
    if left_key.cmp(&right_key) != reference_rank(left).cmp(&reference_rank(right)) {
        return Err(format!(
            "deviceRankKey order disagrees with the reference \
             [left={left:?} key={left_key} right={right:?} key={right_key}]"
        ));
    }
    Ok(())
}

const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

/// A release manifest: a valid template with zero to three random mutations
/// (dropped, duplicated, unknown, malformed or blank lines, comments, an http
/// URL, a zero size, a bad hash, a bad boolean, an empty name, whitespace),
/// sometimes with CRLF line endings, always with non-ASCII notes.
fn manifest_text(rng: &mut Rng) -> String {
    let mut lines: Vec<String> = vec![
        format!("version_code = {}", rng.below(1_000)),
        "version_name = 1.2.3".to_string(),
        "url = https://example.invalid/app.apk".to_string(),
        format!("size_bytes = {}", 1 + rng.below(1_000_000)),
        format!("sha256 = {HELLO_SHA256}"),
        format!("min_sdk = {}", rng.below(40)),
        if rng.below(2) == 0 {
            "mandatory = true".to_string()
        } else {
            "mandatory = false".to_string()
        },
        "notes = Ünïcødé ✓ 😀 #123".to_string(),
    ];
    for _ in 0..rng.below(4) {
        if lines.is_empty() {
            break;
        }
        match rng.below(12) {
            0 => {
                let index = rng.below(lines.len());
                lines.remove(index);
            }
            1 => {
                let index = rng.below(lines.len());
                let duplicate = lines[index].clone();
                lines.push(duplicate);
            }
            2 => lines.push("author = someone".to_string()),
            3 => lines.push("garbage without equals".to_string()),
            4 => lines.push("# a comment".to_string()),
            5 => lines.push(String::new()),
            6 => replace_line(&mut lines, "url", "url = http://example.invalid/app.apk"),
            7 => replace_line(&mut lines, "size_bytes", "size_bytes = 0"),
            8 => replace_line(&mut lines, "sha256", "sha256 = not-hex"),
            9 => replace_line(&mut lines, "mandatory", "mandatory = maybe"),
            10 => replace_line(&mut lines, "version_name", "version_name ="),
            _ => {
                let index = rng.below(lines.len());
                lines[index] = format!("  {}  ", lines[index].replace('=', "   =   "));
            }
        }
    }
    let separator = if rng.below(4) == 0 { "\r\n" } else { "\n" };
    lines.join(separator)
}

fn replace_line(lines: &mut [String], key: &str, replacement: &str) {
    for line in lines.iter_mut() {
        if line.trim_start().starts_with(key) {
            *line = replacement.to_string();
        }
    }
}

/// Differential check over the four string-bridge exports, driven through the
/// mock JNI function table: canonical and error are each other's complement
/// and equal the core; every field selector renders exactly the canonical
/// line's value; null `env`/`jstring` arguments answer their sentinels.
fn check_manifest(rng: &mut Rng, mock: &common::MockEnv) -> Result<(), String> {
    let text = manifest_text(rng);
    let env = mock.env();
    let jtext = mock.string(&text);
    let fail = |message: &str| Err(format!("{message} [text={text:?}]"));
    let core = ReleaseManifest::parse(&text);

    let canonical = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(
        env,
        null(),
        jtext,
    ));
    let expected_canonical = core.as_ref().ok().map(ReleaseManifest::serialize);
    if canonical != expected_canonical || canonical != release_manifest_canonical(&text) {
        return fail("releaseManifestCanonical export/core/bleradar-core disagree");
    }
    let error = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestError(
        env,
        null(),
        jtext,
    ));
    let expected_error = core.as_ref().err().map(ToString::to_string);
    if error != expected_error || error != release_manifest_error(&text) {
        return fail("releaseManifestError export/core/bleradar-core disagree");
    }
    if canonical.is_some() == error.is_some() {
        return fail("canonical and error must be each other's complement");
    }
    if let Some(canonical) = &canonical
        && ReleaseManifest::parse(canonical).ok() != core.ok()
    {
        return fail("canonical form does not round-trip through the core");
    }

    let selector = if rng.below(4) == 0 {
        int(rng)
    } else {
        rng.below(9) as i32 - 1
    };
    let field = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestField(
        env,
        null(),
        jtext,
        selector,
    ));
    if field != release_manifest_field(&text, selector) {
        return fail("releaseManifestField export/core disagree");
    }
    match (&field, &canonical) {
        (Some(value), Some(canonical)) => {
            let key = [
                "version_code",
                "version_name",
                "url",
                "size_bytes",
                "sha256",
                "min_sdk",
                "mandatory",
                "notes",
            ][usize::try_from(selector).unwrap()];
            if !canonical.contains(&format!("{key} = {value}\n")) {
                return fail("a rendered field is not the canonical line's value");
            }
        }
        (Some(_), None) => return fail("a field was rendered for a rejected manifest"),
        (None, Some(_)) if (0..=7).contains(&selector) => {
            return fail("a valid selector rendered nothing for an accepted manifest");
        }
        _ => {}
    }

    if !Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(null(), null(), jtext).is_null()
        || !Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(env, null(), null())
            .is_null()
        || !Java_com_hse_bleradar_NativeRadar_releaseManifestError(null(), null(), jtext).is_null()
        || !Java_com_hse_bleradar_NativeRadar_releaseManifestField(env, null(), null(), selector)
            .is_null()
    {
        return fail("a null env or jstring did not answer null");
    }
    if Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(null(), null(), jtext, jtext)
        != ARTIFACT_MANIFEST_INVALID
    {
        return fail("artifactVerifyFile with a null env is not ARTIFACT_MANIFEST_INVALID");
    }
    let expected = if canonical.is_some() {
        ARTIFACT_UNREADABLE
    } else {
        ARTIFACT_MANIFEST_INVALID
    };
    if Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null(), null(), jtext) != expected
    {
        return fail("artifactVerifyFile with a null path did not answer the documented ordinal");
    }
    mock.reset();
    Ok(())
}

#[test]
fn every_export_agrees_with_its_core_and_never_panics() {
    let iterations = env_u64("BLERADAR_JNI_CAMPAIGN_ITERATIONS", DEFAULT_ITERATIONS);
    let seed = env_u64("BLERADAR_JNI_CAMPAIGN_SEED", DEFAULT_SEED);
    let mut rng = Rng(seed | 1);
    let mock = common::MockEnv::new();
    for iteration in 0..iterations {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            check_stateless(&mut rng)?;
            check_tracking(&mut rng)?;
            check_update(&mut rng)?;
            check_device_policy(&mut rng)?;
            check_manifest(&mut rng, &mock)
        }));
        match outcome {
            Err(_) => panic!("seed={seed} iteration={iteration}: a JNI export panicked"),
            Ok(Err(violation)) => panic!("seed={seed} iteration={iteration}: {violation}"),
            Ok(Ok(())) => {}
        }
    }
}
