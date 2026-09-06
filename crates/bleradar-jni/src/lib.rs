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

use bleradar_core::{ProximityBand, SignalTrend, ble_distance_m, proximity_label, signal_trend};

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

/// `NativeRadar.abiVersion(): int` — a constant sanity check the Java side
/// calls once at startup to confirm the loaded `.so` matches the ABI this
/// file documents, independent of the app's own version number.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_hse_bleradar_NativeRadar_abiVersion(
    _env: JniOpaquePtr,
    _class: JniOpaquePtr,
) -> i32 {
    1
}
