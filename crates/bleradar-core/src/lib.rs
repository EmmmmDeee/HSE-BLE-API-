//! Reconstructed Rust domain core for BLE Radar.
//!
//! The crate intentionally separates observed measurements from inferred state.
//! Functions that cannot be proven from the supplied APK remain compatibility
//! gaps rather than guessed legacy behavior.

mod evidence;
mod fusion;
mod geo;
mod identity;
mod signal;
mod tracking;
mod verification;

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

/// Wi-Fi center frequency in MHz to channel where unambiguous for 2.4/5 GHz.
///
/// # Examples
/// ```
/// use bleradar_core::wifi_frequency_to_channel;
/// assert_eq!(wifi_frequency_to_channel(2412), Some(1));
/// assert_eq!(wifi_frequency_to_channel(2484), Some(14));
/// ```
#[must_use]
pub fn wifi_frequency_to_channel(mhz: u16) -> Option<u16> {
    match mhz {
        2412..=2472 if (mhz - 2407).is_multiple_of(5) => Some((mhz - 2407) / 5),
        2484 => Some(14),
        5160..=5885 if (mhz - 5000).is_multiple_of(5) => Some((mhz - 5000) / 5),
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
