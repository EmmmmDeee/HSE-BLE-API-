//! Multi-sensor radar sweep domain, imported from the Huntsman Search Engine
//! (HSE) `signal_radar` module so the radar's sensor rules have one authority.
//!
//! HSE's radar sweeps Wi-Fi APs, Bluetooth devices and cell towers in one pass.
//! Its transport (Termux subprocesses, `serde_json`, async I/O) is platform glue
//! and stays in HSE; the *rules* it applies to each reading are pure domain
//! logic and live here, dependency-free, next to the signal primitives they
//! already used (`wifi_frequency_to_channel`, `proximity_label`).
//!
//! Invariants carried over from HSE (each regression-locked below):
//! - a reading the platform omitted stays `None`; it is never defaulted to `0`;
//! - a positive Wi-Fi RSSI is corrupt input and scores the *worst* tier;
//! - placeholder addresses are not devices;
//! - a locally administered MAC is randomized, never a trackable device;
//! - a cell's identity is read from the key its radio actually emits
//!   (`cid` GSM/WCDMA, `ci` LTE, `nci` NR), with `Integer.MAX_VALUE` and `0`
//!   treated as unavailable.

use crate::{
    ProximityBand, canonical_mac, is_locally_administered, proximity_label,
    wifi_frequency_to_channel,
};

/// Android's `Integer.MAX_VALUE` "value unavailable" sentinel.
pub const ANDROID_UNAVAILABLE: i64 = i32::MAX as i64;

/// Addresses that stand for "no device": the all-zero MAC and the fixed MAC
/// Android reports when the caller lacks permission to see the real one.
const PLACEHOLDER_MACS: [&str; 2] = ["00:00:00:00:00:00", "02:00:00:00:00:00"];

/// Whether `mac` names a real device: a canonicalisable address that is not a
/// placeholder sentinel. Canonicalising first means the two sentinels are
/// rejected regardless of separator formatting (`00-00-…` as well as `00:00:…`),
/// and a string that is not a MAC at all is not a device.
#[must_use]
pub fn is_real_device_address(mac: &str) -> bool {
    canonical_device_mac(mac).is_some()
}

/// The canonical (lowercase, colon-separated) form of a real device MAC, or
/// `None` when `mac` is not a canonicalisable address or is a placeholder
/// sentinel. The single gate both [`is_real_device_address`] and
/// [`sighting_key`] apply, so a placeholder can never slip through one spelling.
fn canonical_device_mac(mac: &str) -> Option<String> {
    canonical_mac(mac).filter(|c| !PLACEHOLDER_MACS.contains(&c.as_str()))
}

/// How a MAC-addressed radio entity may be tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressTrackability {
    /// A globally administered (real hardware) address — a followable device.
    Trackable,
    /// A locally administered (rotating/privacy) address — a throwaway, never
    /// plotted as a followable device.
    Randomized,
    /// Not a canonicalisable MAC, so trackability is unknown.
    Unknown,
}

/// Classify how a MAC may be tracked, from its U/L bit. Mirrors the exact
/// distinction HSE's `signal_radar` and WiGLE paths partition on.
#[must_use]
pub fn address_trackability(mac: &str) -> AddressTrackability {
    match is_locally_administered(mac) {
        Some(false) => AddressTrackability::Trackable,
        Some(true) => AddressTrackability::Randomized,
        None => AddressTrackability::Unknown,
    }
}

/// Coarse Wi-Fi RSSI reliability tiers, from HSE `wifi::rssi_confidence`. A
/// positive dBm reading is unphysical (0 dBm is already a theoretical ceiling),
/// so it is corrupt input and degrades to the worst tier — never the best.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RssiReliability {
    /// dBm ≥ -50 — the strongest, most reliable tier. A corrupt (positive)
    /// reading never reaches this tier; it falls to [`Self::LowMedium`].
    VeryHighPlus,
    /// -71 ≤ dBm < -50.
    VeryHigh,
    /// -86 ≤ dBm < -71.
    MediumPlus,
    /// dBm < -86, absent, or corrupt (positive) — the worst tier.
    LowMedium,
}

/// Reliability tier for a Wi-Fi RSSI reading in dBm. `None` and a positive
/// reading both fall to [`RssiReliability::LowMedium`].
#[must_use]
pub fn wifi_rssi_reliability(rssi_dbm: Option<i64>) -> RssiReliability {
    match rssi_dbm {
        Some(r) if r > 0 => RssiReliability::LowMedium,
        Some(r) if r >= -50 => RssiReliability::VeryHighPlus,
        Some(r) if r >= -71 => RssiReliability::VeryHigh,
        Some(r) if r >= -86 => RssiReliability::MediumPlus,
        _ => RssiReliability::LowMedium,
    }
}

/// The specific 802.11 channel for an AP centre frequency in MHz, via the
/// radar's verified frequency↔channel map. `None` for an out-of-plan frequency.
#[must_use]
pub fn wifi_channel(frequency_mhz: Option<i64>) -> Option<u16> {
    frequency_mhz
        .and_then(|f| u16::try_from(f).ok())
        .and_then(wifi_frequency_to_channel)
}

/// The coarse RSSI proximity band for a Wi-Fi reading — an honest signal-strength
/// bucket, never a fabricated distance. `None` when no RSSI was reported.
#[must_use]
pub fn wifi_proximity(rssi_dbm: Option<i64>) -> Option<ProximityBand> {
    rssi_dbm.and_then(|r| proximity_label(r as f64))
}

/// The radio a cell record was seen on, which decides the key its identity lives
/// under and the name of its area code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellRadio {
    /// GSM — `cid` / `lac`.
    Gsm,
    /// WCDMA/UMTS — `cid` / `lac`.
    Wcdma,
    /// LTE — `ci` / `tac`.
    Lte,
    /// NR / 5G — `nci` / `tac`.
    Nr,
    /// An unrecognised or absent type string.
    Unknown,
}

impl CellRadio {
    /// Classify from `termux-telephony-cellinfo`'s `type` string.
    #[must_use]
    pub fn from_type(cell_type: Option<&str>) -> Self {
        match cell_type.map(str::to_ascii_lowercase).as_deref() {
            Some("lte") => Self::Lte,
            Some("nr" | "5g") => Self::Nr,
            Some("umts" | "wcdma") => Self::Wcdma,
            Some("gsm") => Self::Gsm,
            _ => Self::Unknown,
        }
    }

    /// Stable lowercase technology tag.
    #[must_use]
    pub fn tech_tag(self) -> &'static str {
        match self {
            Self::Gsm => "gsm",
            Self::Wcdma => "umts",
            Self::Lte => "lte",
            Self::Nr => "nr",
            Self::Unknown => "unknown",
        }
    }
}

/// A cell identity component (`cid`/`ci`/`nci` or `lac`/`tac`) that survives the
/// unavailable-sentinel and zero filters, or `None` when the platform omitted it
/// or reported it unavailable.
#[must_use]
pub fn usable_cell_identity(raw: Option<i64>) -> Option<i64> {
    raw.filter(|&v| v != 0 && v != ANDROID_UNAVAILABLE)
}

/// A signal reading in dBm that is a real measurement, or `None` when it is the
/// unconditionally written `Integer.MAX_VALUE` sentinel. HSE `Cell::usable_dbm`.
#[must_use]
pub fn usable_dbm(raw: Option<i64>) -> Option<i64> {
    raw.filter(|&v| v != ANDROID_UNAVAILABLE)
}

/// The canonical `mcc-mnc-lac-cid` tower id — one authority so a coerced-string
/// caller and a typed-int caller yield the same id for the same tower.
#[must_use]
pub fn tower_id(mcc: &str, mnc: &str, area_code: i64, cid: i64) -> String {
    format!("{mcc}-{mnc}-{area_code}-{cid}")
}

/// True when `s` is a non-empty run of ASCII digits — the shape every segment of
/// a cell tower id must have to be a valid, re-feedable device id.
#[must_use]
pub fn is_numeric_segment(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The canonical (lowercase, colon-separated) device MAC for a sighting — the
/// stable key it is tracked under — or `None` when `mac` is not a
/// canonicalisable address or is a placeholder sentinel. A non-MAC string never
/// becomes a key: an unstable, verbatim key would let the same device split
/// across formatting differences.
#[must_use]
pub fn sighting_key(mac: &str) -> Option<String> {
    canonical_device_mac(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_and_empty_addresses_are_not_devices() {
        assert!(!is_real_device_address(""));
        assert!(!is_real_device_address("00:00:00:00:00:00"));
        assert!(!is_real_device_address("02:00:00:00:00:00"));
        // Placeholders are rejected regardless of separator formatting.
        assert!(!is_real_device_address("00-00-00-00-00-00"));
        assert!(!is_real_device_address("02-00-00-00-00-00"));
        // A string that is not a MAC at all is not a real device address.
        assert!(!is_real_device_address("not-a-mac"));
        assert!(is_real_device_address("a4:c1:38:00:11:22"));
    }

    #[test]
    fn locally_administered_mac_is_randomized_never_trackable() {
        // U/L bit set in the first octet (0x02).
        assert_eq!(
            address_trackability("02:11:22:33:44:55"),
            AddressTrackability::Randomized
        );
        // Globally administered.
        assert_eq!(
            address_trackability("a4:c1:38:00:11:22"),
            AddressTrackability::Trackable
        );
        assert_eq!(
            address_trackability("not-a-mac"),
            AddressTrackability::Unknown
        );
    }

    #[test]
    fn positive_wifi_rssi_is_corrupt_and_scores_worst() {
        assert_eq!(wifi_rssi_reliability(Some(10)), RssiReliability::LowMedium);
        assert_eq!(wifi_rssi_reliability(None), RssiReliability::LowMedium);
        assert_eq!(
            wifi_rssi_reliability(Some(-40)),
            RssiReliability::VeryHighPlus
        );
        assert_eq!(wifi_rssi_reliability(Some(-60)), RssiReliability::VeryHigh);
        assert_eq!(
            wifi_rssi_reliability(Some(-80)),
            RssiReliability::MediumPlus
        );
        assert_eq!(wifi_rssi_reliability(Some(-99)), RssiReliability::LowMedium);
    }

    #[test]
    fn wifi_channel_derives_from_centre_frequency() {
        assert_eq!(wifi_channel(Some(2412)), Some(1));
        assert_eq!(wifi_channel(Some(5180)), Some(36));
        assert_eq!(wifi_channel(None), None);
        assert_eq!(wifi_channel(Some(-1)), None);
    }

    #[test]
    fn wifi_proximity_absent_without_rssi() {
        assert!(wifi_proximity(None).is_none());
        assert!(wifi_proximity(Some(-40)).is_some());
    }

    #[test]
    fn cell_radio_maps_type_string() {
        assert_eq!(CellRadio::from_type(Some("lte")), CellRadio::Lte);
        assert_eq!(CellRadio::from_type(Some("5G")), CellRadio::Nr);
        assert_eq!(CellRadio::from_type(Some("WCDMA")), CellRadio::Wcdma);
        assert_eq!(CellRadio::from_type(None), CellRadio::Unknown);
        assert_eq!(CellRadio::Lte.tech_tag(), "lte");
    }

    #[test]
    fn cell_identity_rejects_zero_and_unavailable() {
        assert_eq!(usable_cell_identity(Some(222)), Some(222));
        assert_eq!(usable_cell_identity(Some(0)), None);
        assert_eq!(usable_cell_identity(Some(ANDROID_UNAVAILABLE)), None);
        assert_eq!(usable_cell_identity(None), None);
    }

    #[test]
    fn usable_dbm_rejects_the_written_sentinel() {
        assert_eq!(usable_dbm(Some(-80)), Some(-80));
        assert_eq!(usable_dbm(Some(ANDROID_UNAVAILABLE)), None);
        assert_eq!(usable_dbm(None), None);
    }

    #[test]
    fn tower_id_is_stable_and_segments_validated() {
        assert_eq!(tower_id("505", "1", 12345, 67890), "505-1-12345-67890");
        assert!(is_numeric_segment("505"));
        assert!(!is_numeric_segment(""));
        assert!(!is_numeric_segment("5a"));
        assert!(!is_numeric_segment("-1"));
    }

    #[test]
    fn sighting_key_canonicalises_and_rejects_placeholders() {
        assert_eq!(
            sighting_key("A4-C1-38-00-11-22").as_deref(),
            Some("a4:c1:38:00:11:22")
        );
        assert!(sighting_key("00:00:00:00:00:00").is_none());
        assert!(sighting_key("00-00-00-00-00-00").is_none());
        // A non-canonicalisable string never becomes an (unstable) key.
        assert!(sighting_key("not-a-mac").is_none());
    }
}
