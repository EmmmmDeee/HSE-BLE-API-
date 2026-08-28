//! Geographic primitives used by the radar and map layers.

use std::f64::consts::PI;

/// Validated geographic coordinate in decimal degrees.
///
/// Fields are private so a value can only exist through [`LatLon::new`]; every
/// instance is therefore finite and inside the geographic range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    lat: f64,
    lon: f64,
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

    /// Latitude in decimal degrees, -90..=90.
    #[must_use]
    pub const fn lat(self) -> f64 {
        self.lat
    }

    /// Longitude in decimal degrees, -180..=180.
    #[must_use]
    pub const fn lon(self) -> f64 {
        self.lon
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
    // Floating error can push h a few ULP above 1.0 near antipodal points,
    // where asin would leave its domain and return NaN; mathematically h <= 1.
    2.0 * EARTH_RADIUS_M * h.min(1.0).sqrt().asin()
}

/// Initial bearing from `from` to `to`, normalized to [0, 360).
#[must_use]
pub fn bearing_deg(from: LatLon, to: LatLon) -> f64 {
    let phi1 = from.lat.to_radians();
    let phi2 = to.lat.to_radians();
    let dlambda = (to.lon - from.lon).to_radians();
    let y = dlambda.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlambda.cos();
    // rem_euclid can round a tiny negative angle up to exactly 360.0; the
    // second reduction folds that back so the result stays inside [0, 360).
    (y.atan2(x) * 180.0 / PI).rem_euclid(360.0) % 360.0
}
