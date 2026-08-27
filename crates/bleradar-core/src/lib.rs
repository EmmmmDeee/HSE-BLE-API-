//! Reconstructed Rust domain core for BLE Radar.
//! 
//! This crate contains only behavior that can be reconstructed with high confidence
//! from the shipped APK's exported Rust/UniFFI surface and standard mathematical
//! definitions. Unknown legacy behavior is deliberately represented as an explicit
//! compatibility gap rather than guessed.

use std::f64::consts::PI;

/// Geographic coordinate in decimal degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    /// Latitude, -90..=90.
    pub lat: f64,
    /// Longitude, -180..=180.
    pub lon: f64,
}

impl LatLon {
    /// Constructs a validated coordinate.
    pub fn new(lat: f64, lon: f64) -> Result<Self, GeoError> {
        if !lat.is_finite() || !lon.is_finite() {
            return Err(GeoError::NonFinite);
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(GeoError::OutOfRange);
        }
        Ok(Self { lat, lon })
    }
}

/// Coordinate validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoError {
    /// One or more coordinate values were NaN or infinite.
    NonFinite,
    /// Latitude or longitude is outside the geographic range.
    OutOfRange,
}

/// Great-circle distance in metres using the haversine formula.
#[must_use]
pub fn haversine_m(a: LatLon, b: LatLon) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Initial bearing from `from` to `to`, normalized to [0, 360).
#[must_use]
pub fn bearing_deg(from: LatLon, to: LatLon) -> f64 {
    let phi1 = from.lat.to_radians();
    let phi2 = to.lat.to_radians();
    let dlambda = (to.lon - from.lon).to_radians();
    let y = dlambda.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlambda.cos();
    (y.atan2(x) * 180.0 / PI).rem_euclid(360.0)
}

/// Returns true when a canonical MAC address has the locally administered bit set.
#[must_use]
pub fn is_locally_administered(mac: &str) -> Option<bool> {
    let first = mac.split([':', '-']).next()?;
    if first.len() != 2 {
        return None;
    }
    u8::from_str_radix(first, 16).ok().map(|octet| octet & 0x02 != 0)
}

/// Canonicalizes a MAC address to lower-case colon-separated form.
pub fn canonical_mac(input: &str) -> Option<String> {
    let compact: String = input.chars().filter(|c| *c != ':' && *c != '-').collect();
    if compact.len() != 12 || !compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        compact
            .as_bytes()
            .chunks(2)
            .map(|p| std::str::from_utf8(p).ok().map(str::to_ascii_lowercase))
            .collect::<Option<Vec<_>>>()?
            .join(":"),
    )
}

/// 2.4/5/6 GHz Wi-Fi channel to center frequency in MHz where defined.
#[must_use]
pub fn wifi_channel_to_frequency(channel: u16) -> Option<u16> {
    match channel {
        1..=13 => Some(2407 + channel * 5),
        14 => Some(2484),
        32..=177 => Some(5000 + channel * 5),
        1_000..=2_335 => None,
        _ => None,
    }
}

/// Wi-Fi center frequency in MHz to channel where unambiguous for 2.4/5 GHz.
#[must_use]
pub fn wifi_frequency_to_channel(mhz: u16) -> Option<u16> {
    match mhz {
        2412..=2472 if (mhz - 2407) % 5 == 0 => Some((mhz - 2407) / 5),
        2484 => Some(14),
        5160..=5885 if (mhz - 5000) % 5 == 0 => Some((mhz - 5000) / 5),
        _ => None,
    }
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
    pub const fn value(self) -> Option<f64> { self.value }
}

/// RSSI filter configuration/sample error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterError {
    /// Alpha is outside (0, 1].
    InvalidAlpha,
    /// RSSI sample was NaN or infinite.
    NonFiniteSample,
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
    let delta = current_dbm - previous_dbm;
    if delta > deadband_db {
        SignalTrend::Stronger
    } else if delta < -deadband_db {
        SignalTrend::Weaker
    } else {
        SignalTrend::Stable
    }
}

/// Unsupported reconstructed behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityGap {
    /// Public symbol or behavior whose exact semantics cannot be recovered from the APK.
    pub contract: &'static str,
    /// Why reconstruction would require guessing.
    pub reason: &'static str,
}
