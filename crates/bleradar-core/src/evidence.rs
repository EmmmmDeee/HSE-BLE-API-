//! Immutable evidence, provenance, transformation, and calibrated-fusion primitives.

/// The role of a record in the canonical evidence graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// A collected object.
    Artifact,
    /// A directly recorded value.
    Observation,
    /// A stable characteristic extracted from a representation.
    Feature,
    /// A concrete encoding of an object.
    Representation,
    /// A conversion between representations.
    Transformation,
    /// An origin from which material was obtained.
    Source,
    /// A time-bounded occurrence.
    Event,
    /// A supported or contested connection.
    Relationship,
    /// A competing explanation.
    Hypothesis,
    /// A support or contradiction item.
    Evidence,
    /// A verification execution.
    Test,
    /// An attempted operation.
    Action,
    /// A recorded confidence change.
    ConfidenceUpdate,
}

/// A source of evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Source category, such as `ble_advertisement` or `public_web`.
    pub source_type: String,
    /// How the source was retrieved.
    pub retrieval_method: String,
}

/// A collected object, distinct from any single observation made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Artifact category, such as `binary`, `document`, or `web_page`.
    pub artifact_type: String,
    /// Source the artifact was collected from.
    pub source_id: String,
    /// Time the artifact was collected, in caller-defined monotonic milliseconds.
    pub collected_at_ms: u64,
}

/// An immutable direct observation retaining raw and normalized forms separately.
///
/// Fields are private so raw evidence can never be overwritten after
/// construction; only additive builder methods may extend an observation, and
/// accessors expose the immutable state. This closes the same class of gap as
/// [`crate::LatLon`]: a claimed invariant that can be bypassed by direct field
/// mutation is not enforced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    id: String,
    raw_value: String,
    normalized_value: Option<String>,
    source: Source,
    observed_at_ms: u64,
    first_seen_ms: u64,
    last_seen_ms: u64,
    derivation_history: Vec<String>,
}

impl Observation {
    /// Creates a direct observation whose first and last seen times equal its observation time.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        raw_value: impl Into<String>,
        source: Source,
        observed_at_ms: u64,
    ) -> Self {
        Self {
            id: id.into(),
            raw_value: raw_value.into(),
            normalized_value: None,
            source,
            observed_at_ms,
            first_seen_ms: observed_at_ms,
            last_seen_ms: observed_at_ms,
            derivation_history: Vec::new(),
        }
    }

    /// Returns a new observation with an additive normalization record.
    #[must_use]
    pub fn with_normalization(
        mut self,
        normalized_value: impl Into<String>,
        transformation_id: impl Into<String>,
    ) -> Self {
        self.normalized_value = Some(normalized_value.into());
        self.derivation_history.push(transformation_id.into());
        self
    }

    /// Returns a new observation whose known time span includes `seen_at_ms`.
    #[must_use]
    pub fn with_seen_at(mut self, seen_at_ms: u64) -> Self {
        self.first_seen_ms = self.first_seen_ms.min(seen_at_ms);
        self.last_seen_ms = self.last_seen_ms.max(seen_at_ms);
        self
    }

    /// Stable caller-assigned identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Original value exactly as collected; never overwritten after construction.
    #[must_use]
    pub fn raw_value(&self) -> &str {
        &self.raw_value
    }

    /// Optional normalized representation; never replaces `raw_value`.
    #[must_use]
    pub fn normalized_value(&self) -> Option<&str> {
        self.normalized_value.as_deref()
    }

    /// Origin of this observation.
    #[must_use]
    pub const fn source(&self) -> &Source {
        &self.source
    }

    /// Time the value was observed, in caller-defined monotonic milliseconds.
    #[must_use]
    pub const fn observed_at_ms(&self) -> u64 {
        self.observed_at_ms
    }

    /// First known observation time.
    #[must_use]
    pub const fn first_seen_ms(&self) -> u64 {
        self.first_seen_ms
    }

    /// Last known observation time.
    #[must_use]
    pub const fn last_seen_ms(&self) -> u64 {
        self.last_seen_ms
    }

    /// Ordered identifiers describing derivations applied to this observation.
    #[must_use]
    pub fn derivation_history(&self) -> &[String] {
        &self.derivation_history
    }
}

/// A stable characteristic extracted from a representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Representation this feature was extracted from.
    pub representation_id: String,
    /// Feature name, such as `distinctive_phrase` or `public_asset_hash`.
    pub name: String,
    /// Extracted feature value.
    pub value: String,
}

/// A representation of an artifact or observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Representation {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// The represented artifact or observation identifier.
    pub subject_id: String,
    /// Representation family, such as `raw`, `normalized`, or `text`.
    pub format: String,
    /// Stable extracted feature identifiers.
    pub feature_ids: Vec<String>,
}

/// A verified conversion between two representations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformation {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Input representation identifier.
    pub input_representation_id: String,
    /// Output representation identifier.
    pub output_representation_id: String,
    /// Feature identifiers retained by the conversion.
    pub preserved_feature_ids: Vec<String>,
    /// Feature identifiers altered by the conversion.
    pub changed_feature_ids: Vec<String>,
    /// Test or verification identifiers supporting this conversion.
    pub verification_ids: Vec<String>,
}

/// A verification execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Identifier of the transformation, claim, or relationship under test.
    pub subject_id: String,
    /// Verification method or metamorphic relation name.
    pub method: String,
    /// Whether the subject survived this verification.
    pub passed: bool,
    /// Time this test executed, in caller-defined monotonic milliseconds.
    pub executed_at_ms: u64,
}

/// A time-bounded occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// What occurred.
    pub description: String,
    /// Source that recorded this event.
    pub source_id: String,
    /// Start time, in caller-defined monotonic milliseconds.
    pub started_at_ms: u64,
    /// End time, or `None` if the event is ongoing or instantaneous.
    pub ended_at_ms: Option<u64>,
}

/// How a relationship was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    /// Directly observed rather than derived.
    Observed,
    /// Computed from other observed material.
    Derived,
    /// Asserted from indirect indicators.
    Inferred,
    /// Currently disputed by contradicting evidence.
    Contested,
    /// Investigated and rejected.
    Rejected,
}

/// A supported or contested connection between two entities, with full provenance.
///
/// Every material edge stores its source, time, method, support,
/// contradiction and confidence so graph density can never substitute for
/// proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Identifier of the entity the relationship originates from.
    pub subject_id: String,
    /// Identifier of the entity the relationship points to.
    pub object_id: String,
    /// Relationship category, such as `same_public_entity` or `shared_infrastructure`.
    pub relationship_type: String,
    /// How this relationship was established.
    pub edge_type: EdgeType,
    /// Source that produced this relationship.
    pub source_id: String,
    /// Method used to establish the relationship.
    pub method: String,
    /// Time this relationship was recorded, in caller-defined monotonic milliseconds.
    pub observed_at_ms: u64,
    /// Evidence identifiers supporting this relationship.
    pub supporting_evidence_ids: Vec<String>,
    /// Evidence identifiers contradicting this relationship.
    pub contradicting_evidence_ids: Vec<String>,
    /// Calibrated confidence in the inclusive range 0..=100, not a raw probability.
    pub confidence: u8,
}

/// The resolved outcome of an [`Action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionOutcome {
    /// The action completed and had the intended effect.
    Succeeded,
    /// The action completed without the intended effect.
    Failed,
    /// The action could not be completed.
    Aborted,
}

/// An attempted operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Operation attempted, such as `verify_transformation` or `query_source`.
    pub description: String,
    /// Identifier of the entity this action targets.
    pub target_id: String,
    /// Time this action was initiated, in caller-defined monotonic milliseconds.
    pub initiated_at_ms: u64,
    /// Outcome once resolved; `None` while the action is still pending.
    pub outcome: Option<ActionOutcome>,
}

/// A hypothesis competing to explain a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hypothesis {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Explanation under consideration.
    pub statement: String,
    /// Whether this is the ordinary or null explanation.
    pub is_null: bool,
}

/// Evidence that supports or contradicts a hypothesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Directly observed input.
    pub observation_id: String,
    /// Source that produced the observed input.
    pub source_id: String,
    /// `true` for support and `false` for contradiction.
    pub supports: bool,
    /// Caller-defined dependency key; equal keys collapse to one independent contribution.
    pub dependency_key: String,
    /// Calibrated quality dimensions in the inclusive range 0..=100.
    pub quality: EvidenceQuality,
}

/// Explicit quality dimensions used for calibrated weighting, not probability claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceQuality {
    /// Trustworthiness of collection and source.
    pub reliability: u8,
    /// How uniquely the evidence identifies an explanation.
    pub specificity: u8,
    /// How uncommon the feature is in the relevant population.
    pub rarity: u8,
    /// Ability to distinguish competing hypotheses.
    pub discriminative_power: u8,
    /// Independence from other evidence.
    pub source_independence: u8,
    /// Consistency with chronology.
    pub temporal_compatibility: u8,
    /// Survival under relevant transformations.
    pub transformation_resistance: u8,
    /// Completeness and traceability of provenance.
    pub provenance_quality: u8,
    /// Ability for an independent party to reproduce the result.
    pub reproducibility: u8,
}

impl EvidenceQuality {
    /// Returns the conservative mean quality score, clamping malformed inputs to 100.
    #[must_use]
    pub fn score(self) -> u8 {
        let total = self.reliability.min(100) as u16
            + self.specificity.min(100) as u16
            + self.rarity.min(100) as u16
            + self.discriminative_power.min(100) as u16
            + self.source_independence.min(100) as u16
            + self.temporal_compatibility.min(100) as u16
            + self.transformation_resistance.min(100) as u16
            + self.provenance_quality.min(100) as u16
            + self.reproducibility.min(100) as u16;
        (total / 9) as u8
    }
}

/// A claim with an auditable path through hypothesis, evidence, observation, and source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Assertion made by this claim.
    pub statement: String,
    /// The hypothesis this claim evaluates.
    pub hypothesis_id: String,
    /// Evidence identifiers used to evaluate it.
    pub evidence_ids: Vec<String>,
}

/// A missing or broken link in a claim's `CLAIM -> HYPOTHESIS -> EVIDENCE ->
/// OBSERVATION -> SOURCE` provenance chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError {
    /// The claim's `hypothesis_id` has no matching hypothesis.
    MissingHypothesis(String),
    /// The claim cites no evidence, so its statement is unsupported.
    NoEvidence,
    /// An `evidence_ids` entry has no matching evidence record.
    MissingEvidence(String),
    /// An evidence record's `observation_id` has no matching observation.
    MissingObservation(String),
    /// An evidence record's `source_id` has no matching source record.
    MissingSource(String),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHypothesis(id) => write!(f, "no hypothesis found for id `{id}`"),
            Self::NoEvidence => f.write_str("claim cites no evidence"),
            Self::MissingEvidence(id) => write!(f, "no evidence found for id `{id}`"),
            Self::MissingObservation(id) => write!(f, "no observation found for id `{id}`"),
            Self::MissingSource(id) => write!(f, "no source found for id `{id}`"),
        }
    }
}

impl std::error::Error for TraceError {}

/// Verifies the full `CLAIM -> HYPOTHESIS -> EVIDENCE -> OBSERVATION -> SOURCE`
/// chain a claim depends on, so a claim can never be mistaken for evidence.
///
/// # Errors
/// Returns the first broken or missing link encountered.
pub fn trace_claim(
    claim: &Claim,
    hypotheses: &[Hypothesis],
    evidence: &[Evidence],
    observations: &[Observation],
    sources: &[Source],
) -> Result<(), TraceError> {
    if !hypotheses.iter().any(|h| h.id == claim.hypothesis_id) {
        return Err(TraceError::MissingHypothesis(claim.hypothesis_id.clone()));
    }
    if claim.evidence_ids.is_empty() {
        return Err(TraceError::NoEvidence);
    }
    for evidence_id in &claim.evidence_ids {
        let item = evidence
            .iter()
            .find(|item| &item.id == evidence_id)
            .ok_or_else(|| TraceError::MissingEvidence(evidence_id.clone()))?;
        if !observations.iter().any(|o| o.id() == item.observation_id) {
            return Err(TraceError::MissingObservation(item.observation_id.clone()));
        }
        if !sources.iter().any(|s| s.id == item.source_id) {
            return Err(TraceError::MissingSource(item.source_id.clone()));
        }
    }
    Ok(())
}

/// An auditable record of a confidence change, so calibration can never
/// silently inflate into an unearned conclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfidenceUpdate {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Identifier of the claim or relationship whose confidence changed.
    pub subject_id: String,
    /// Confidence before this update, in the inclusive range 0..=100.
    pub previous_confidence: u8,
    /// Confidence after this update, in the inclusive range 0..=100.
    pub updated_confidence: u8,
    /// Evidence identifiers that justify this change.
    pub evidence_ids: Vec<String>,
    /// Why the confidence changed.
    pub reason: String,
    /// Time this update was recorded, in caller-defined monotonic milliseconds.
    pub updated_at_ms: u64,
}

/// Rarity at or below this value is treated as high-base-rate (common)
/// support: a feature this unremarkable in the relevant population should not
/// carry the leading hypothesis on its own.
pub const HIGH_BASE_RATE_RARITY_THRESHOLD: u8 = 50;

/// Result of calibrated evidence fusion and adversarial falsification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionResult {
    /// Sum of unique supporting evidence scores.
    pub supporting_score: u32,
    /// Sum of unique contradictory evidence scores.
    pub contradictory_score: u32,
    /// Dependency keys whose duplicate items were collapsed.
    pub collapsed_dependency_keys: Vec<String>,
    /// Score after removing the strongest supporting item.
    pub without_strongest_support_score: i32,
    /// Score after removing support at or below [`HIGH_BASE_RATE_RARITY_THRESHOLD`].
    pub without_high_base_rate_support_score: i32,
}

/// Fuses evidence without treating repeated reporting as independent confirmation.
#[must_use]
pub fn fuse_evidence(evidence: &[Evidence]) -> FusionResult {
    let mut unique: Vec<&Evidence> = Vec::new();
    let mut collapsed_dependency_keys = Vec::new();
    for item in evidence {
        if let Some(existing) = unique
            .iter_mut()
            .find(|known| known.dependency_key == item.dependency_key)
        {
            if item.quality.score() > existing.quality.score() {
                *existing = item;
            }
            if !collapsed_dependency_keys.contains(&item.dependency_key) {
                collapsed_dependency_keys.push(item.dependency_key.clone());
            }
        } else {
            unique.push(item);
        }
    }

    let mut supporting_score = 0_u32;
    let mut contradictory_score = 0_u32;
    let mut strongest_support = 0_u32;
    let mut rare_supporting_score = 0_u32;
    for item in &unique {
        let score = u32::from(item.quality.score());
        if item.supports {
            supporting_score += score;
            strongest_support = strongest_support.max(score);
            if item.quality.rarity > HIGH_BASE_RATE_RARITY_THRESHOLD {
                rare_supporting_score += score;
            }
        } else {
            contradictory_score += score;
        }
    }
    FusionResult {
        supporting_score,
        contradictory_score,
        collapsed_dependency_keys,
        without_strongest_support_score: supporting_score as i32
            - strongest_support as i32
            - contradictory_score as i32,
        without_high_base_rate_support_score: rare_supporting_score as i32
            - contradictory_score as i32,
    }
}
