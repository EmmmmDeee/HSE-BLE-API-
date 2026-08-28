//! RSSI filtering and conservative proximity helpers.

/// RSSI filter configuration/sample error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterError {
    /// Alpha is outside (0, 1].
    InvalidAlpha,
    /// RSSI sample was NaN or infinite.
    NonFiniteSample,
}

/// Simple exponential moving average used as a stable, deterministic RSSI filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RssiEma {
    alpha: f64,
    value: Option<f64>,
}

impl RssiEma {
    /// Creates a filter. Alpha must be in (0, 1].
    pub fn new(alpha: f64) -> Result<Self, FilterError> {
        if !alpha.is_finite() || !(0.0 < alpha && alpha <= 1.0) {
            return Err(FilterError::InvalidAlpha);
        }
        Ok(Self { alpha, value: None })
    }

    /// Adds a sample and returns the filtered value.
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

/// Maps RSSI to a coarse proximity label without pretending to know exact distance.
#[must_use]
pub fn proximity_label(rssi_dbm: f64) -> ProximityBand {
    if rssi_dbm >= -50.0 {
        ProximityBand::Immediate
    } else if rssi_dbm >= -65.0 {
        ProximityBand::Near
    } else if rssi_dbm >= -80.0 {
        ProximityBand::Mid
    } else {
        ProximityBand::Far
    }
}

/// Log-distance estimate in metres from RSSI, calibrated RSSI at 1 m, and path-loss exponent.
///
/// Returns `None` for non-finite input or a non-positive path-loss exponent. The result is an
/// estimate only and should be displayed with an uncertainty band rather than as exact range.
#[must_use]
pub fn ble_distance_m(rssi_dbm: f64, rssi_at_1m_dbm: f64, path_loss_exponent: f64) -> Option<f64> {
    if !rssi_dbm.is_finite()
        || !rssi_at_1m_dbm.is_finite()
        || !path_loss_exponent.is_finite()
        || path_loss_exponent <= 0.0
    {
        return None;
    }
    Some(10_f64.powf((rssi_at_1m_dbm - rssi_dbm) / (10.0 * path_loss_exponent)))
}
