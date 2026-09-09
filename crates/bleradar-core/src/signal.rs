//! RSSI filtering and conservative proximity helpers.

/// RSSI filter configuration/sample error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterError {
    /// Alpha is outside (0, 1].
    InvalidAlpha,
    /// RSSI sample was NaN or infinite.
    NonFiniteSample,
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidAlpha => "EMA alpha must be within (0, 1]",
            Self::NonFiniteSample => "RSSI sample was NaN or infinite",
        })
    }
}

impl std::error::Error for FilterError {}

/// Simple exponential moving average used as a stable, deterministic RSSI filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RssiEma {
    alpha: f64,
    value: Option<f64>,
}

impl RssiEma {
    /// Creates a filter. Alpha must be in (0, 1].
    ///
    /// # Errors
    /// Returns [`FilterError::InvalidAlpha`] if `alpha` is not within `(0, 1]`.
    ///
    /// # Examples
    /// ```
    /// use bleradar_core::RssiEma;
    /// let mut ema = RssiEma::new(0.5).unwrap();
    /// assert_eq!(ema.push(-80.0).unwrap(), -80.0);
    /// assert_eq!(ema.push(-60.0).unwrap(), -70.0);
    /// assert!(RssiEma::new(0.0).is_err());
    /// ```
    pub fn new(alpha: f64) -> Result<Self, FilterError> {
        if !alpha.is_finite() || !(0.0 < alpha && alpha <= 1.0) {
            return Err(FilterError::InvalidAlpha);
        }
        Ok(Self { alpha, value: None })
    }

    /// Adds a sample and returns the filtered value.
    ///
    /// # Errors
    /// Returns [`FilterError::NonFiniteSample`] if `rssi_dbm` is NaN or infinite.
    pub fn push(&mut self, rssi_dbm: f64) -> Result<f64, FilterError> {
        if !rssi_dbm.is_finite() {
            return Err(FilterError::NonFiniteSample);
        }
        let next = match self.value {
            Some(old) => self.alpha.mul_add(rssi_dbm, (1.0 - self.alpha) * old),
            None => rssi_dbm,
        };
        self.value = Some(next);
        Ok(next)
    }

    /// Current filtered value.
    #[must_use]
    pub const fn value(self) -> Option<f64> {
        self.value
    }
}

/// Stateless EMA helper for callers that persist only the previous filtered
/// value rather than the whole [`RssiEma`] object.
///
/// Treats a non-finite `previous_filtered_dbm` as "no prior sample yet" and
/// returns `current_rssi_dbm` unchanged in that case.
#[must_use]
pub fn filtered_rssi(previous_filtered_dbm: f64, current_rssi_dbm: f64, alpha: f64) -> Option<f64> {
    if !current_rssi_dbm.is_finite() {
        return None;
    }
    let mut ema = RssiEma::new(alpha).ok()?;
    if previous_filtered_dbm.is_finite() {
        ema.value = Some(previous_filtered_dbm);
    }
    ema.push(current_rssi_dbm).ok()
}

/// Trend classification for deterministic hot/cold guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalTrend {
    /// Signal improved by more than the deadband.
    Stronger,
    /// Signal weakened by more than the deadband.
    Weaker,
    /// Change falls within the deadband.
    Stable,
}

/// Compares filtered RSSI samples. Less-negative RSSI is stronger.
#[must_use]
pub fn signal_trend(previous_dbm: f64, current_dbm: f64, deadband_db: f64) -> SignalTrend {
    let deadband = deadband_db.abs();
    let delta = current_dbm - previous_dbm;
    if delta > deadband {
        SignalTrend::Stronger
    } else if delta < -deadband {
        SignalTrend::Weaker
    } else {
        SignalTrend::Stable
    }
}

/// Coarse proximity band. This intentionally avoids false precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProximityBand {
    /// Typically very near the observer.
    Immediate,
    /// Nearby.
    Near,
    /// Moderate separation.
    Mid,
    /// Weak/far signal or uncertain environment.
    Far,
}

// Minimum filtered RSSI (dBm) admitted to each coarse proximity band.
const IMMEDIATE_MIN_DBM: f64 = -50.0;
const NEAR_MIN_DBM: f64 = -65.0;
const MID_MIN_DBM: f64 = -80.0;

/// Maps RSSI to a coarse proximity label without pretending to know exact distance.
///
/// # Examples
/// ```
/// use bleradar_core::{proximity_label, ProximityBand};
/// assert_eq!(proximity_label(-40.0), ProximityBand::Immediate);
/// assert_eq!(proximity_label(-95.0), ProximityBand::Far);
/// ```
#[must_use]
pub fn proximity_label(rssi_dbm: f64) -> ProximityBand {
    if rssi_dbm >= IMMEDIATE_MIN_DBM {
        ProximityBand::Immediate
    } else if rssi_dbm >= NEAR_MIN_DBM {
        ProximityBand::Near
    } else if rssi_dbm >= MID_MIN_DBM {
        ProximityBand::Mid
    } else {
        ProximityBand::Far
    }
}

/// Log-distance estimate in metres from RSSI, calibrated RSSI at 1 m, and path-loss exponent.
///
/// Returns `None` for non-finite input, a non-positive path-loss exponent, or
/// an estimate that overflows to infinity or underflows to zero. The result is
/// an estimate only and should be displayed with an uncertainty band rather
/// than as exact range.
///
/// # Examples
/// ```
/// use bleradar_core::ble_distance_m;
/// // At the reference RSSI the estimate is 1 metre.
/// assert!((ble_distance_m(-59.0, -59.0, 2.0).unwrap() - 1.0).abs() < 1e-9);
/// assert!(ble_distance_m(-70.0, -59.0, 0.0).is_none());
/// ```
#[must_use]
pub fn ble_distance_m(rssi_dbm: f64, rssi_at_1m_dbm: f64, path_loss_exponent: f64) -> Option<f64> {
    if !rssi_dbm.is_finite()
        || !rssi_at_1m_dbm.is_finite()
        || !path_loss_exponent.is_finite()
        || path_loss_exponent <= 0.0
    {
        return None;
    }
    let distance = 10_f64.powf((rssi_at_1m_dbm - rssi_dbm) / (10.0 * path_loss_exponent));
    (distance.is_finite() && distance > 0.0).then_some(distance)
}

/// Conservative near/far distance band in metres given a plausible RSSI spread
/// around the filtered reading.
///
/// `rssi_spread_db` is treated as a symmetric ± tolerance around `rssi_dbm`.
/// A stronger plausible RSSI yields the lower bound; a weaker plausible RSSI
/// yields the upper bound.
#[must_use]
pub fn ble_distance_range_m(
    rssi_dbm: f64,
    rssi_spread_db: f64,
    rssi_at_1m_dbm: f64,
    path_loss_exponent: f64,
) -> Option<(f64, f64)> {
    if !rssi_spread_db.is_finite() || rssi_spread_db < 0.0 {
        return None;
    }
    let spread = rssi_spread_db.abs();
    let lower = ble_distance_m(rssi_dbm + spread, rssi_at_1m_dbm, path_loss_exponent)?;
    let upper = ble_distance_m(rssi_dbm - spread, rssi_at_1m_dbm, path_loss_exponent)?;
    Some((lower.min(upper), lower.max(upper)))
}

/// Deterministic confidence score for a tracked signal from sample support and
/// recent RSSI spread.
///
/// More samples increase confidence; higher spread reduces it. Returns `None`
/// for a negative, NaN, or infinite spread input.
#[must_use]
pub fn signal_confidence_percent(sample_count: usize, rssi_spread_db: f64) -> Option<u8> {
    if !rssi_spread_db.is_finite() || rssi_spread_db < 0.0 {
        return None;
    }
    let support_score = sample_count.min(12) as u8 * 5;
    let stability_score = if rssi_spread_db <= 2.0 {
        40
    } else if rssi_spread_db <= 4.0 {
        32
    } else if rssi_spread_db <= 6.0 {
        24
    } else if rssi_spread_db <= 8.0 {
        16
    } else if rssi_spread_db <= 12.0 {
        8
    } else {
        0
    };
    Some((support_score + stability_score).min(100))
}
