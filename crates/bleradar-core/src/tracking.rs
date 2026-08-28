//! Device-centric observation and map-tracking state.

use crate::{
    LatLon, ProximityBand, RssiEma, SignalTrend, haversine_m, proximity_label, signal_trend,
};

/// Normalized confidence score in the inclusive range 0..=100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Confidence(u8);

impl Confidence {
    /// Creates a confidence value, clamped to 100.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(if value > 100 { 100 } else { value })
    }

    /// Returns the numeric score.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// One directly observed BLE measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceObservation {
    /// Monotonic/session timestamp in milliseconds supplied by the caller.
    pub timestamp_ms: u64,
    /// Receiver position when available.
    pub observer_position: Option<LatLon>,
    /// Reported GNSS horizontal accuracy in metres.
    pub gps_accuracy_m: Option<f64>,
    /// Raw BLE RSSI in dBm.
    pub rssi_dbm: f64,
    /// Optional transmitter power from advertisement metadata.
    pub tx_power_dbm: Option<i16>,
}

/// Distinguishes fact from inference in the visual layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateKind {
    /// Direct measurement tied to the receiver position.
    Observed,
    /// Derived from multiple observations.
    Inferred,
    /// Forward-looking extrapolation; weakest evidence class.
    Predicted,
}

/// Map-ready point that preserves evidence class and uncertainty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPoint {
    /// Coordinate rendered on the map.
    pub position: LatLon,
    /// Evidence class.
    pub kind: EstimateKind,
    /// Horizontal uncertainty radius in metres.
    pub uncertainty_m: f64,
    /// Confidence score.
    pub confidence: Confidence,
    /// Observation time.
    pub timestamp_ms: u64,
}

/// Spatial estimate derived from the observation history.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialEstimate {
    /// Estimated center.
    pub center: LatLon,
    /// Approximate uncertainty radius in metres.
    pub uncertainty_m: f64,
    /// Number of positioned observations supporting the estimate.
    pub supporting_observations: usize,
    /// Confidence in the estimate.
    pub confidence: Confidence,
}

/// Track validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackError {
    /// RSSI is NaN or infinite.
    NonFiniteRssi,
    /// GPS accuracy is negative, zero, NaN, or infinite.
    InvalidGpsAccuracy,
    /// Timestamps must not move backwards inside one track.
    NonMonotonicTime,
}

/// Persistent track state for one selected device.
#[derive(Debug, Clone)]
pub struct DeviceTrack {
    observations: Vec<DeviceObservation>,
    filter: RssiEma,
    filtered_rssi: Option<f64>,
    trend: SignalTrend,
}

impl DeviceTrack {
    /// Creates an empty track using the supplied EMA alpha.
    pub fn new(rssi_alpha: f64) -> Result<Self, crate::FilterError> {
        Ok(Self {
            observations: Vec::new(),
            filter: RssiEma::new(rssi_alpha)?,
            filtered_rssi: None,
            trend: SignalTrend::Stable,
        })
    }

    /// Adds one observation and updates deterministic signal state.
    pub fn push(&mut self, observation: DeviceObservation) -> Result<(), TrackError> {
        if !observation.rssi_dbm.is_finite() {
            return Err(TrackError::NonFiniteRssi);
        }
        if let Some(accuracy) = observation.gps_accuracy_m
            && (!accuracy.is_finite() || accuracy <= 0.0)
        {
            return Err(TrackError::InvalidGpsAccuracy);
        }
        if self
            .observations
            .last()
            .is_some_and(|last| observation.timestamp_ms < last.timestamp_ms)
        {
            return Err(TrackError::NonMonotonicTime);
        }

        let next = self
            .filter
            .push(observation.rssi_dbm)
            .map_err(|_| TrackError::NonFiniteRssi)?;
        if let Some(previous) = self.filtered_rssi {
            self.trend = signal_trend(previous, next, 2.0);
        }
        self.filtered_rssi = Some(next);
        self.observations.push(observation);
        Ok(())
    }

    /// Read-only observation history.
    #[must_use]
    pub fn observations(&self) -> &[DeviceObservation] {
        &self.observations
    }

    /// Current filtered RSSI.
    #[must_use]
    pub const fn filtered_rssi(&self) -> Option<f64> {
        self.filtered_rssi
    }

    /// Current hot/cold trend.
    #[must_use]
    pub const fn trend(&self) -> SignalTrend {
        self.trend
    }

    /// Current coarse proximity band.
    #[must_use]
    pub fn proximity(&self) -> Option<ProximityBand> {
        self.filtered_rssi.map(proximity_label)
    }

    /// Returns directly observed map points for measurements that had a location fix.
    #[must_use]
    pub fn observed_map_points(&self) -> Vec<MapPoint> {
        self.observations
            .iter()
            .filter_map(|obs| {
                let position = obs.observer_position?;
                let uncertainty = obs.gps_accuracy_m.unwrap_or(50.0);
                let confidence = confidence_from_accuracy(uncertainty);
                Some(MapPoint {
                    position,
                    kind: EstimateKind::Observed,
                    uncertainty_m: uncertainty,
                    confidence,
                    timestamp_ms: obs.timestamp_ms,
                })
            })
            .collect()
    }

    /// Produces a conservative weighted centroid using GPS accuracy and relative signal strength.
    ///
    /// This estimates the strongest observed region, not the transmitter's exact coordinate.
    #[must_use]
    pub fn spatial_estimate(&self) -> Option<SpatialEstimate> {
        let positioned: Vec<_> = self
            .observations
            .iter()
            .filter_map(|obs| {
                Some((
                    obs.observer_position?,
                    obs.gps_accuracy_m.unwrap_or(50.0),
                    obs.rssi_dbm,
                ))
            })
            .collect();
        if positioned.len() < 2 {
            return None;
        }

        let max_rssi = positioned
            .iter()
            .map(|(_, _, rssi)| *rssi)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut weight_sum = 0.0;
        let mut lat_sum = 0.0;
        let mut lon_sum = 0.0;
        for (pos, accuracy, rssi) in &positioned {
            let accuracy_weight = 1.0 / accuracy.max(1.0).powi(2);
            let signal_weight = 10_f64.powf((rssi - max_rssi) / 20.0).clamp(0.05, 1.0);
            let weight = accuracy_weight * signal_weight;
            weight_sum += weight;
            lat_sum += pos.lat * weight;
            lon_sum += pos.lon * weight;
        }
        if weight_sum <= 0.0 || !weight_sum.is_finite() {
            return None;
        }
        let center = LatLon::new(lat_sum / weight_sum, lon_sum / weight_sum).ok()?;
        let weighted_radius = positioned
            .iter()
            .map(|(pos, accuracy, _)| haversine_m(center, *pos) + *accuracy)
            .sum::<f64>()
            / positioned.len() as f64;
        let count_score = (positioned.len().min(20) * 3) as u8;
        let accuracy_score = confidence_from_accuracy(weighted_radius).value();
        let confidence = Confidence::new(count_score.saturating_add(accuracy_score / 2));

        Some(SpatialEstimate {
            center,
            uncertainty_m: weighted_radius.max(1.0),
            supporting_observations: positioned.len(),
            confidence,
        })
    }
}

fn confidence_from_accuracy(accuracy_m: f64) -> Confidence {
    let score = if accuracy_m <= 3.0 {
        95
    } else if accuracy_m <= 5.0 {
        90
    } else if accuracy_m <= 10.0 {
        80
    } else if accuracy_m <= 20.0 {
        65
    } else if accuracy_m <= 50.0 {
        45
    } else {
        25
    };
    Confidence::new(score)
}

/// UI selection state for the map/device interaction layer.
#[derive(Debug, Clone)]
pub struct SelectedDevice {
    /// Canonical device identifier chosen by the UI layer.
    pub id: String,
    /// Whether tracking is actively locked to the device.
    pub tracking: bool,
    /// Track data for the selected device.
    pub track: DeviceTrack,
}

impl SelectedDevice {
    /// Creates a selected-device state with tracking disabled.
    pub fn new(id: impl Into<String>, rssi_alpha: f64) -> Result<Self, crate::FilterError> {
        Ok(Self {
            id: id.into(),
            tracking: false,
            track: DeviceTrack::new(rssi_alpha)?,
        })
    }

    /// Locks the selection for active tracking.
    pub const fn start_tracking(&mut self) {
        self.tracking = true;
    }

    /// Releases active tracking while retaining history.
    pub const fn stop_tracking(&mut self) {
        self.tracking = false;
    }
}
