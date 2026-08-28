//! Compatibility registry for the shipped APK's native Rust surface.
//!
//! This crate distinguishes observed ABI from source-reconstructed behavior so
//! incomplete parity can never be mistaken for implemented parity.

/// Current migration status for an observed native contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityStatus {
    /// Behavior has a source implementation and direct regression coverage.
    Reconstructed,
    /// Exact legacy implementation is retained only in the immutable native oracle.
    OracleOnly,
    /// Symbol is observed but exact behavior cannot yet be established from supplied evidence.
    Blocked,
}

/// One ABI contract and its migration status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractStatus {
    /// Public/native contract name.
    pub name: &'static str,
    /// Current parity state.
    pub status: ParityStatus,
    /// Short evidence note.
    pub evidence: &'static str,
}

/// High-value exported contracts observed in `libbleradar_core.so`.
///
/// The complete symbol census remains in `docs/NATIVE_ABI.txt`; this registry
/// records the migration frontier for semantic contracts.
pub const CONTRACTS: &[ContractStatus] = &[
    ContractStatus {
        name: "bearing_deg",
        status: ParityStatus::Reconstructed,
        evidence: "safe Rust implementation + geometry regression tests",
    },
    ContractStatus {
        name: "haversine_m",
        status: ParityStatus::Reconstructed,
        evidence: "safe Rust implementation + geometry regression tests",
    },
    ContractStatus {
        name: "wifi_channel_to_frequency",
        status: ParityStatus::Reconstructed,
        evidence: "safe Rust implementation + round-trip tests",
    },
    ContractStatus {
        name: "wifi_frequency_to_channel",
        status: ParityStatus::Reconstructed,
        evidence: "safe Rust implementation + round-trip tests",
    },
    ContractStatus {
        name: "ble_distance",
        status: ParityStatus::Reconstructed,
        evidence: "calibrated log-distance implementation; exact legacy coefficients still need differential proof",
    },
    ContractStatus {
        name: "proximity_label",
        status: ParityStatus::Reconstructed,
        evidence: "coarse safe-Rust proximity bands; exact legacy thresholds still need oracle comparison",
    },
    ContractStatus {
        name: "ui_radar_points",
        status: ParityStatus::Blocked,
        evidence: "record layout and visual semantics not fully recoverable from stripped binary",
    },
    ContractStatus {
        name: "ui_geo_sketch",
        status: ParityStatus::Blocked,
        evidence: "record layout and exact weighting semantics unknown",
    },
    ContractStatus {
        name: "multilaterate",
        status: ParityStatus::OracleOnly,
        evidence: "original native implementation preserved in oracle/libbleradar_core.so",
    },
    ContractStatus {
        name: "assess_threat",
        status: ParityStatus::OracleOnly,
        evidence: "original native implementation preserved; policy thresholds are private",
    },
    ContractStatus {
        name: "correlate",
        status: ParityStatus::OracleOnly,
        evidence: "original native implementation preserved; private correlation policy",
    },
    ContractStatus {
        name: "export_device_json",
        status: ParityStatus::Blocked,
        evidence: "exact serialization schema/error behavior requires differential characterization",
    },
    ContractStatus {
        name: "export_session_json",
        status: ParityStatus::Blocked,
        evidence: "exact serialization schema/error behavior requires differential characterization",
    },
    ContractStatus {
        name: "import_parse",
        status: ParityStatus::Blocked,
        evidence: "parser edge/error behavior unavailable from ABI metadata alone",
    },
    ContractStatus {
        name: "mac_info",
        status: ParityStatus::Blocked,
        evidence: "full record schema and OUI policy require oracle characterization",
    },
    ContractStatus {
        name: "oui_vendor",
        status: ParityStatus::OracleOnly,
        evidence: "embedded vendor database exists only in original native oracle",
    },
    ContractStatus {
        name: "RadarStore",
        status: ParityStatus::OracleOnly,
        evidence: "stateful store implementation retained in original native oracle",
    },
    ContractStatus {
        name: "session_to_track",
        status: ParityStatus::Blocked,
        evidence: "exact DeviceTrack record layout and conversion semantics unknown",
    },
];

/// Returns whether a contract name is in the semantic registry.
#[must_use]
pub fn is_observed_contract(name: &str) -> bool {
    CONTRACTS.iter().any(|contract| contract.name == name)
}

/// Returns migration status for a registered contract.
#[must_use]
pub fn parity_status(name: &str) -> Option<ParityStatus> {
    CONTRACTS
        .iter()
        .find(|contract| contract.name == name)
        .map(|contract| contract.status)
}

/// Counts contracts by migration state as `(reconstructed, oracle_only, blocked)`.
#[must_use]
pub fn coverage_counts() -> (usize, usize, usize) {
    CONTRACTS.iter().fold((0, 0, 0), |mut acc, contract| {
        match contract.status {
            ParityStatus::Reconstructed => acc.0 += 1,
            ParityStatus::OracleOnly => acc.1 += 1,
            ParityStatus::Blocked => acc.2 += 1,
        }
        acc
    })
}
