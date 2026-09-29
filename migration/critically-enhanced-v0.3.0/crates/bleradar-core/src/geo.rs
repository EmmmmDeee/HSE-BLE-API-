//! Geographic primitives used by the radar and map layers.

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
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
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
