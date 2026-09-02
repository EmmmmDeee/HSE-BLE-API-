//! Runtime-topology and source-parity registry for the shipped APK's native
//! Rust surface.
//!
//! This crate classifies every exported contract independently from the
//! smaller source-replacement registry so native implementation, reachability,
//! and source parity cannot be confused.

/// Current migration status for an observed native contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityStatus {
    /// A source analogue exists, but oracle equivalence has not been proved.
    SourceAnalog,
    /// Required behavior is captured and the source implementation matches it.
    DifferentiallyVerified,
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

/// Kind of application contract exported through UniFFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    /// Free function.
    Function,
    /// `RadarStore` constructor.
    Constructor,
    /// `RadarStore` method.
    Method,
}

/// Language-boundary classification of the implementation that actually runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationClass {
    /// Implemented by the retained Rust native library.
    RustNative,
    /// Core behavior currently outside Rust and requiring migration.
    RustMigrationRequired,
    /// The smallest boundary imposed by an external platform.
    NonRustJustifiedBoundary,
    /// Available evidence does not identify the implementation owner.
    Unknown,
}

/// Reachability classification for an observed runtime contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    /// Executed by an instrumented oracle probe.
    VerifiedRuntime,
    /// A concrete call site reaches the contract.
    StaticallyReachable,
    /// Reachable only under an identified configuration or event.
    ConditionallyReachable,
    /// Proved not to be reachable from supported entry points.
    Unreachable,
    /// Exported, but no non-generated caller or runtime observation was found.
    Unknown,
}

/// Strongest evidence used for a reachability classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachabilityEvidence {
    /// Direct observation on the supported runtime.
    ObservedExecution,
    /// Instrumented execution trace.
    InstrumentedRuntimeTrace,
    /// Integration test.
    IntegrationTest,
    /// Concrete call-site analysis.
    CallSiteAnalysis,
    /// Static export or control-flow analysis only.
    StaticReachability,
}

/// One exported application contract in the authoritative APK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeContract {
    /// Canonical lower-snake-case contract name.
    pub name: &'static str,
    /// Export kind.
    pub kind: ContractKind,
    /// Owner of the implementation that actually executes.
    pub implementation: ImplementationClass,
    /// Best available reachability result.
    pub reachability: Reachability,
    /// Strongest supporting evidence.
    pub evidence: ReachabilityEvidence,
}

/// High-value exported contracts observed in `libbleradar_core.so`.
///
/// The complete symbol census remains in `docs/NATIVE_ABI.txt`; this registry
/// records the migration frontier for semantic contracts.
pub const CONTRACTS: &[ContractStatus] = &[
    ContractStatus {
        name: "bearing_deg",
        status: ParityStatus::SourceAnalog,
        evidence: "source analogue and sampled oracle outputs; exhaustive differential parity is pending",
    },
    ContractStatus {
        name: "haversine_m",
        status: ParityStatus::SourceAnalog,
        evidence: "oracle probe proves the source uses a different Earth-radius constant",
    },
    ContractStatus {
        name: "wifi_channel_to_frequency",
        status: ParityStatus::SourceAnalog,
        evidence: "exact oracle ranges are captured; the source matches only over its narrower u16 input contract",
    },
    ContractStatus {
        name: "wifi_frequency_to_channel",
        status: ParityStatus::SourceAnalog,
        evidence: "the source rejects 1,789 off-center or 6 GHz frequencies accepted by the oracle",
    },
    ContractStatus {
        name: "ble_distance",
        status: ParityStatus::SourceAnalog,
        evidence: "oracle fixes calibration at -59/2.4, ignores tx power, and clamps at 100 m",
    },
    ContractStatus {
        name: "proximity_label",
        status: ParityStatus::SourceAnalog,
        evidence: "oracle accepts distance while the source analogue accepts RSSI",
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

macro_rules! runtime_contract {
    ($name:literal, $kind:ident, $reachability:ident, $evidence:ident) => {
        RuntimeContract {
            name: $name,
            kind: ContractKind::$kind,
            implementation: ImplementationClass::RustNative,
            reachability: Reachability::$reachability,
            evidence: ReachabilityEvidence::$evidence,
        }
    };
}

/// Complete application-contract census from the retained UniFFI metadata.
///
/// `VerifiedRuntime` means the pure native function was exercised by the
/// compatibility trace described in `docs/BEHAVIORAL_CONTRACT.md`. It does not
/// promote a source analogue to differential parity. `Unknown` entries are
/// exports with no direct call from non-generated DEX and no runtime trace.
pub const RUNTIME_CONTRACTS: &[RuntimeContract] = &[
    runtime_contract!(
        "radarstore_new",
        Constructor,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "assess_threat",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "bearing_deg",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bleadv_appearance",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "bleadv_fingerprints",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "bleadv_flags",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ble_distance",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bt_category_from_class",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bt_describe",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "bt_major",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bt_major_label",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bt_minor",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "bt_services",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "core_version",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!("correlate", Function, StaticallyReachable, CallSiteAnalysis),
    runtime_contract!(
        "device_category",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "export_csv_field",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "export_device_json",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "export_session_json",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "export_wigle_csv",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "fmt_category_label",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "fmt_channel",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "fmt_company",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "fmt_coord",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "fmt_distance",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!("fmt_hex", Function, StaticallyReachable, CallSiteAnalysis),
    runtime_contract!(
        "fmt_rssi",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "fmt_upper_invariant",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "gatt_characteristic_name",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "gatt_decode",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "gatt_service_name",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "gatt_service_name_for_short",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "gatt_short",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "haversine_m",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "import_parse",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "import_parse_json",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "import_parse_wigle",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "import_records",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "import_split_csv",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "mac_info",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "multilaterate",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "osint_default_options",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "osint_module_names",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "osint_scan",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "osint_seed_kinds",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "oui_vendor",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "perms_can_connect_bt",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "perms_can_scan_ble",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "perms_can_scan_wifi",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "perms_has_location",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "perms_optional",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "perms_required",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "proximity_label",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "scan_mode_params",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "scan_tick_plan",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "sessions_delete",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "sessions_list",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "sessions_load",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "sessions_save",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "session_display_name",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "session_filter_sort",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "session_fingerprint",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "session_parse_transport",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "session_report_html",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "session_summaries",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "session_to_track",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("times_ago", Function, StaticallyReachable, CallSiteAnalysis),
    runtime_contract!(
        "times_clock",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "times_duration",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "times_file_stamp",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "times_iso",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "times_parse_iso",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "times_parse_wigle",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "times_wigle",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_address_type_label",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_appearance_from_bytes",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_bond_label",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_channel_width_mhz",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("ui_fixed", Function, StaticallyReachable, CallSiteAnalysis),
    runtime_contract!(
        "ui_gatt_props",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_geo_sketch",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_overlay_signature",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_permission_error_message",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_phy_label",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_point_alpha",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "ui_radar_points",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_radius_fraction",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_ring_dist",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_ring_labels",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_signal_bars",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "ui_sparkline",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_stable_angle",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "ui_status_line",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "ui_wifi_result_ts",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("wifi_band", Function, StaticallyReachable, CallSiteAnalysis),
    runtime_contract!(
        "wifi_channel_to_frequency",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "wifi_distance",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "wifi_frequency_to_channel",
        Function,
        VerifiedRuntime,
        InstrumentedRuntimeTrace
    ),
    runtime_contract!(
        "wifi_is_enterprise",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "wifi_security",
        Function,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_aliases",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("radarstore_alias_get", Method, Unknown, StaticReachability),
    runtime_contract!(
        "radarstore_apply_threats",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_clear",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_filter_sort",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("radarstore_get", Method, Unknown, StaticReachability),
    runtime_contract!(
        "radarstore_groups",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_import_aliases",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_ingest_ble",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_ingest_classic",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_ingest_wifi",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("radarstore_is_empty", Method, Unknown, StaticReachability),
    runtime_contract!("radarstore_len", Method, Unknown, StaticReachability),
    runtime_contract!(
        "radarstore_load",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_observer",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_observer_track",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_prune",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_session_start_ms",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_set_alias",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_set_alias_file",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_set_groups",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_snapshot",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!(
        "radarstore_update_observer",
        Method,
        StaticallyReachable,
        CallSiteAnalysis
    ),
    runtime_contract!("radarstore_version", Method, Unknown, StaticReachability),
];

/// Returns whether a contract name is in the semantic registry.
///
/// # Examples
/// ```
/// use bleradar_compat::is_observed_contract;
/// assert!(is_observed_contract("haversine_m"));
/// assert!(!is_observed_contract("nonexistent"));
/// ```
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

/// Counts contracts by migration state as
/// `(source_analog, verified, oracle_only, blocked)`.
///
/// # Examples
/// ```
/// use bleradar_compat::{CONTRACTS, coverage_counts};
/// let (source_analog, verified, oracle_only, blocked) = coverage_counts();
/// assert_eq!(source_analog + verified + oracle_only + blocked, CONTRACTS.len());
/// ```
#[must_use]
pub fn coverage_counts() -> (usize, usize, usize, usize) {
    CONTRACTS.iter().fold((0, 0, 0, 0), |mut acc, contract| {
        match contract.status {
            ParityStatus::SourceAnalog => acc.0 += 1,
            ParityStatus::DifferentiallyVerified => acc.1 += 1,
            ParityStatus::OracleOnly => acc.2 += 1,
            ParityStatus::Blocked => acc.3 += 1,
        }
        acc
    })
}

/// Returns the complete runtime-map entry for a native contract.
#[must_use]
pub fn runtime_contract(name: &str) -> Option<&'static RuntimeContract> {
    RUNTIME_CONTRACTS
        .iter()
        .find(|contract| contract.name == name)
}

/// Counts runtime contracts as `(verified, static, conditional, unreachable, unknown)`.
#[must_use]
pub fn reachability_counts() -> (usize, usize, usize, usize, usize) {
    RUNTIME_CONTRACTS
        .iter()
        .fold((0, 0, 0, 0, 0), |mut acc, contract| {
            match contract.reachability {
                Reachability::VerifiedRuntime => acc.0 += 1,
                Reachability::StaticallyReachable => acc.1 += 1,
                Reachability::ConditionallyReachable => acc.2 += 1,
                Reachability::Unreachable => acc.3 += 1,
                Reachability::Unknown => acc.4 += 1,
            }
            acc
        })
}
