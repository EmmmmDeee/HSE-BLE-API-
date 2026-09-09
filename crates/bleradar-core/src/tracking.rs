//! Device-centric observation and map-tracking state.

use crate::{
    LatLon, ProximityBand, RssiEma, SignalTrend, ble_distance_m, ble_distance_range_m,
    filtered_rssi, haversine_m, proximity_label, signal_confidence_percent, signal_trend,
};

/// Assumed horizontal accuracy, in metres, for an observation carrying no GPS fix.
const DEFAULT_GPS_ACCURACY_M: f64 = 50.0;

/// Deadband, in dB, below which a filtered-RSSI change is treated as stable.
const TREND_DEADBAND_DB: f64 = 2.0;

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

/// Coarse recency class for UI/device ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FreshnessClass {
    /// Observed within the tight live window.
    Live,
    /// Not live, but still recent enough to keep visible.
    Recent,
    /// Old enough to be treated as stale.
    Stale,
}

impl FreshnessClass {
    /// Derives a freshness class from age and configured windows.
    #[must_use]
    pub const fn from_age(age_ms: u64, live_window_ms: u64, recent_window_ms: u64) -> Self {
        let effective_recent_window_ms = if recent_window_ms > live_window_ms {
            recent_window_ms
        } else {
            live_window_ms
        };
        if age_ms <= live_window_ms {
            Self::Live
        } else if age_ms <= effective_recent_window_ms {
            Self::Recent
        } else {
            Self::Stale
        }
    }
}

/// One canonical, Rust-owned per-device signal snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackingSnapshot {
    /// Filtered RSSI after ingesting the latest sample.
    pub filtered_rssi_dbm: f64,
    /// Current hot/cold guidance.
    pub trend: SignalTrend,
    /// Coarse proximity band.
    pub proximity: ProximityBand,
    /// Central distance estimate in metres when representable.
    pub distance_m: Option<f64>,
    /// Conservative lower distance bound in metres when representable.
    pub distance_lower_bound_m: Option<f64>,
    /// Conservative upper distance bound in metres when representable.
    pub distance_upper_bound_m: Option<f64>,
    /// Deterministic confidence score.
    pub confidence_percent: u8,
    /// Recency class for ranking/pruning.
    pub freshness: FreshnessClass,
}

/// Raw inputs required to derive a [`TrackingSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackingSnapshotInput {
    /// Previously filtered RSSI, or NaN when bootstrapping.
    pub previous_filtered_rssi_dbm: f64,
    /// Latest raw RSSI sample.
    pub current_rssi_dbm: f64,
    /// EMA alpha.
    pub rssi_alpha: f64,
    /// Trend deadband.
    pub trend_deadband_db: f64,
    /// Recent filtered-RSSI spread for uncertainty/confidence.
    pub rssi_spread_db: f64,
    /// Number of filtered samples represented by the spread/history.
    pub sample_count: usize,
    /// Calibration reference RSSI at 1 metre.
    pub rssi_at_1m_dbm: f64,
    /// Calibration path-loss exponent.
    pub path_loss_exponent: f64,
    /// Age of the latest observation relative to "now".
    pub age_ms: u64,
    /// Tight live freshness window.
    pub live_window_ms: u64,
    /// Broader recent freshness window.
    pub recent_window_ms: u64,
}

/// Derives one coherent per-device signal snapshot from raw tracking inputs.
#[must_use]
pub fn tracking_snapshot(input: TrackingSnapshotInput) -> Option<TrackingSnapshot> {
    let filtered_rssi_dbm = filtered_rssi(
        input.previous_filtered_rssi_dbm,
        input.current_rssi_dbm,
        input.rssi_alpha,
    )?;
    let trend = if input.previous_filtered_rssi_dbm.is_finite() {
        signal_trend(
            input.previous_filtered_rssi_dbm,
            filtered_rssi_dbm,
            input.trend_deadband_db,
        )
    } else {
        SignalTrend::Stable
    };
    let proximity = proximity_label(filtered_rssi_dbm);
    let distance_m = ble_distance_m(
        filtered_rssi_dbm,
        input.rssi_at_1m_dbm,
        input.path_loss_exponent,
    )?;
    let (distance_lower_bound_m, distance_upper_bound_m) = ble_distance_range_m(
        filtered_rssi_dbm,
        input.rssi_spread_db,
        input.rssi_at_1m_dbm,
        input.path_loss_exponent,
    )
    .map(|(lower, upper)| (Some(lower), Some(upper)))?;
    let confidence_percent = signal_confidence_percent(input.sample_count, input.rssi_spread_db)?;
    let freshness =
        FreshnessClass::from_age(input.age_ms, input.live_window_ms, input.recent_window_ms);
    Some(TrackingSnapshot {
        filtered_rssi_dbm,
        trend,
        proximity,
        distance_m: Some(distance_m),
        distance_lower_bound_m,
        distance_upper_bound_m,
        confidence_percent,
        freshness,
    })
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

impl std::fmt::Display for TrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteRssi => "RSSI sample was NaN or infinite",
            Self::InvalidGpsAccuracy => "GPS accuracy must be finite and positive",
            Self::NonMonotonicTime => "observation timestamps must not move backwards",
        })
    }
}

impl std::error::Error for TrackError {}

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
    ///
    /// # Errors
    /// Returns [`FilterError`](crate::FilterError) if `rssi_alpha` is not within `(0, 1]`.
    ///
    /// # Examples
    /// ```
    /// use bleradar_core::{DeviceObservation, DeviceTrack, LatLon, ProximityBand};
    /// let mut track = DeviceTrack::new(0.5).unwrap();
    /// track
    ///     .push(DeviceObservation {
    ///         timestamp_ms: 0,
    ///         observer_position: Some(LatLon::new(0.0, 0.0).unwrap()),
    ///         gps_accuracy_m: Some(5.0),
    ///         rssi_dbm: -45.0,
    ///         tx_power_dbm: None,
    ///     })
    ///     .unwrap();
    /// assert_eq!(track.proximity(), Some(ProximityBand::Immediate));
    /// ```
    pub fn new(rssi_alpha: f64) -> Result<Self, crate::FilterError> {
        Ok(Self {
            observations: Vec::new(),
            filter: RssiEma::new(rssi_alpha)?,
            filtered_rssi: None,
            trend: SignalTrend::Stable,
        })
    }

    /// Adds one observation and updates deterministic signal state.
    ///
    /// # Errors
    /// Returns [`TrackError`] if the RSSI is non-finite, the GPS accuracy is
    /// present but not finite and positive, or the timestamp precedes the
    /// previous observation.
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
            self.trend = signal_trend(previous, next, TREND_DEADBAND_DB);
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
                let uncertainty = obs.gps_accuracy_m.unwrap_or(DEFAULT_GPS_ACCURACY_M);
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
                    obs.gps_accuracy_m.unwrap_or(DEFAULT_GPS_ACCURACY_M),
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
        // Longitude wraps at ±180°, so positions are averaged as weighted 3-D
        // unit vectors on the sphere; a linear mean of straddling longitudes
        // would place the center on the far side of the planet.
        let mut weight_sum = 0.0;
        let mut x_sum = 0.0;
        let mut y_sum = 0.0;
        let mut z_sum = 0.0;
        for (pos, accuracy, rssi) in &positioned {
            let weight = observation_weight(*accuracy, *rssi, max_rssi);
            let lat_rad = pos.lat().to_radians();
            let lon_rad = pos.lon().to_radians();
            weight_sum += weight;
            x_sum += weight * lat_rad.cos() * lon_rad.cos();
            y_sum += weight * lat_rad.cos() * lon_rad.sin();
            z_sum += weight * lat_rad.sin();
        }
        if weight_sum <= 0.0 || !weight_sum.is_finite() {
            return None;
        }
        let norm = (x_sum * x_sum + y_sum * y_sum + z_sum * z_sum).sqrt();
        // A vanishing mean vector means the observations surround the sphere
        // with no meaningful center; local BLE clusters never trigger this.
        if norm < weight_sum * 1e-9 {
            return None;
        }
        // Clamp for the same reason as haversine_m: float error must not push
        // the asin argument outside its domain.
        let center_lat = (z_sum / norm).clamp(-1.0, 1.0).asin().to_degrees();
        let center_lon = y_sum.atan2(x_sum).to_degrees();
        let center = LatLon::new(center_lat, center_lon).ok()?;
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

/// Weight for one positioned observation: nearer-fix (smaller accuracy) and
/// stronger-relative-signal observations contribute more to the centroid.
fn observation_weight(accuracy_m: f64, rssi_dbm: f64, max_rssi_dbm: f64) -> f64 {
    let accuracy_weight = 1.0 / accuracy_m.max(1.0).powi(2);
    let signal_weight = 10_f64
        .powf((rssi_dbm - max_rssi_dbm) / 20.0)
        .clamp(0.05, 1.0);
    accuracy_weight * signal_weight
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
