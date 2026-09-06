//! Reconstructed Rust domain core for BLE Radar.
//!
//! The crate intentionally separates observed measurements from inferred state.
//! Functions that cannot be proven from the supplied APK remain compatibility
//! gaps rather than guessed legacy behavior.

mod advancement;
mod evidence;
mod fusion;
mod geo;
mod identity;
mod infrastructure;
mod osint;
mod signal;
mod tracking;
mod validation;
mod verification;
mod website;

/// Huntsman Search Engine (HSE) universal coordinate parser.
pub mod coords;
/// Huntsman Search Engine (HSE) entity model.
pub mod entity;
/// Huntsman Search Engine (HSE) canonical entity tag vocabulary.
pub mod tags;

pub use advancement::{
    AdvancementDecision, AdvancementError, AdvancementExecution, AdvancementFactors,
    AdvancementPhase, AdvancementPriority, AdvancementProposal, AdvancementRanking,
    AdvancementRejection, AdvancementRun, AdvancementState, BenchmarkMetric, BenchmarkReport,
    FalsificationCheck, FalsificationFinding, FalsificationResult, FalsificationStatus,
    MetamorphicSoftwareAdvancementEngine, MetricDirection, SoftwareAdvancementEngine,
};
pub use entity::{
    CANDIDATE_CONF, CONSENSUS_SOURCE, CORROBORATION_COEFF, CORROBORATION_DOUBT_DECAY,
    CROSS_SCAN_CORROBORATION_SOURCE, CROSS_SCAN_SOURCE, ENRICHMENT_ONLY_SOURCES, GAMMA_PER_HOUR,
    GEO_CORROBORATION_SOURCE, HseClassification, HseEntity, HseEntityBuilder, HseEntityKind,
    HseEntityRef, HseEvidence, HseVerificationMethod, MULTIPATH_CORROBORATION_SOURCE,
    RECALL_SOURCE, Sha256, canonical_handle, derive_uid, evidence_sources, expansion_timeline,
    hex_encode, is_engine_corroboration_source, is_enrichment_source, is_non_corroborating_source,
    is_promotion_source, is_tracking_param_key, normalise, scan_id, uid_for,
};
pub use evidence::{
    Action, ActionId, ActionStatus, ActionType, Artifact, ArtifactId, ArtifactType,
    CanonicalEvidence, Claim, ClaimId, ClaimTrace, ConfidenceTarget, ConfidenceUpdate,
    ConfidenceUpdateId, EdgeType, Entity, EntityId, EntityKind, EntityType, Event, EventId,
    EventType, Evidence, EvidenceId, EvidenceRole, EvidenceStore, EvidenceTrace, EvidenceValue,
    Feature, FeatureId, Hypothesis, HypothesisId, HypothesisKind, Observation, ObservationId,
    ObservationTimeline, ProvenanceCore, ProvenanceError, RecordId, Relationship, RelationshipId,
    RelationshipProvenance, Representation, RepresentationId, RepresentationType, RetrievalMethod,
    Source, SourceId, SourceType, Test, TestId, TestStatus, TestType, Timestamp, Transformation,
    TransformationId, TransformationTrace, Value, Verification, VerificationStatus,
};
pub use fusion::{
    CalibratedEvidenceFusion, DependencyKind, EvidenceAssessment, EvidenceQuality,
    ExpectedEvidence, FalsificationReport, FusionError, FusionResult, HypothesisScore,
};
pub use geo::{GeoError, LatLon, bearing_deg, haversine_m};
pub use identity::{
    AddressKind, DeviceIdentity, IdentityEvidence, canonical_mac, is_locally_administered,
};
pub use infrastructure::{
    CompetingExplanation, ControlAssessment, CorrelationEdge, CorrelationFactors,
    CorrelationFalsification, CorrelationRanking, CorrelationReport, InfrastructureCorrelationEdge,
    InfrastructureCorrelationEngine, InfrastructureCorrelationReport, InfrastructureError,
    InfrastructureExplanation, InfrastructureFactors, InfrastructureFalsificationReport,
    InfrastructureKind, InfrastructureLimits, InfrastructureObservation, InfrastructurePhase,
    InfrastructureRecord, TemporalInfrastructureCorrelationEngine, TemporalInterval,
    TemporalMetamorphicInfrastructureCorrelationEngine, TemporalRelation,
};
pub use osint::{
    AdaptiveOsintSearchEngine, AdaptiveSearchFactors, ExecutionFeedbackAdaptiveOsintSearchEngine,
    ExecutionFeedbackAdaptiveSearchEngine, OsintSearchError, SearchError, SearchExecution,
    SearchFamilyStatistics, SearchFeedback, SearchFinding, SearchLimits, SearchOutcome,
    SearchPhase, SearchPivot, SearchPivotSeed, SearchPivotState, SearchPriority,
    SearchPriorityFactors, SearchRanking, SearchRepresentation,
};
pub use signal::{
    FilterError, ProximityBand, RssiEma, SignalTrend, ble_distance_m, proximity_label, signal_trend,
};
pub use tracking::{
    Confidence, DeviceObservation, DeviceTrack, EstimateKind, MapPoint, SelectedDevice,
    SpatialEstimate, TrackError,
};
pub use verification::{
    DifferentialCase, DifferentialReport, DifferentialViolation, ExecutionOutcome, FailureCause,
    FamilyStatistics, MetamorphicRelation, MetamorphicTest, RegressionLock, RepairRecord,
    RequiredSemantics, VerificationEngine, VerificationError, VerificationReport,
    VerificationSurface, VerificationViolation,
};
pub use website::{
    OperatorAssessment, WebsiteCorrelationEdge, WebsiteCorrelationFactors,
    WebsiteCorrelationFalsification, WebsiteCorrelationRanking, WebsiteCorrelationReport,
    WebsiteEcosystemAnalysisEngine, WebsiteError, WebsiteExplanation, WebsiteFactors,
    WebsiteFalsificationReport, WebsiteFeatureKind, WebsiteLimits,
    WebsiteLineageEcosystemAnalysisEngine, WebsiteLineageEdge, WebsiteLineageEngine,
    WebsiteLineageReport, WebsiteObservation, WebsiteObservationKind, WebsiteObservationPair,
    WebsitePhase, WebsiteRecord, WebsiteSnapshot, WebsiteTimeline,
};

/// 2.4/5 GHz Wi-Fi channel to center frequency in MHz where defined.
///
/// # Examples
/// ```
/// use bleradar_core::wifi_channel_to_frequency;
/// assert_eq!(wifi_channel_to_frequency(1), Some(2412));
/// assert_eq!(wifi_channel_to_frequency(14), Some(2484));
/// assert_eq!(wifi_channel_to_frequency(15), None);
/// ```
#[must_use]
pub fn wifi_channel_to_frequency(channel: u16) -> Option<u16> {
    match channel {
        1..=13 => Some(2407 + channel * 5),
        14 => Some(2484),
        32..=177 => Some(5000 + channel * 5),
        _ => None,
    }
}

/// Wi-Fi center frequency in MHz to channel where unambiguous for 2.4/5/6 GHz.
///
/// Channel numbers are recovered by floor division within each inclusive
/// range, not only at exact 5 MHz grid points: this matches the immutable
/// native oracle's verified behavior (`docs/BEHAVIORAL_CONTRACT.md` BF-004),
/// including the 6 GHz band (IEEE 802.11ax-2021).
///
/// # Examples
/// ```
/// use bleradar_core::wifi_frequency_to_channel;
/// assert_eq!(wifi_frequency_to_channel(2412), Some(1));
/// assert_eq!(wifi_frequency_to_channel(2484), Some(14));
/// assert_eq!(wifi_frequency_to_channel(5955), Some(1));
/// assert_eq!(wifi_frequency_to_channel(7115), Some(233));
/// ```
#[must_use]
pub fn wifi_frequency_to_channel(mhz: u16) -> Option<u16> {
    match mhz {
        2412..=2472 => Some((mhz - 2407) / 5),
        2484 => Some(14),
        5160..=5885 => Some((mhz - 5000) / 5),
        5955..=7115 => Some((mhz - 5950) / 5),
        _ => None,
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

impl std::fmt::Display for CompatibilityGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unsupported contract `{}`: {}",
            self.contract, self.reason
        )
    }
}

impl std::error::Error for CompatibilityGap {}
