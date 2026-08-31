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

/// An immutable direct observation retaining raw and normalized forms separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// Stable caller-assigned identifier.
    pub id: String,
    /// Original value exactly as collected.
    pub raw_value: String,
    /// Optional normalized representation; never replaces `raw_value`.
    pub normalized_value: Option<String>,
    /// Origin of this observation.
    pub source: Source,
    /// Time the value was observed, in caller-defined monotonic milliseconds.
    pub observed_at_ms: u64,
    /// First known observation time.
    pub first_seen_ms: u64,
    /// Last known observation time.
    pub last_seen_ms: u64,
    /// Ordered identifiers describing derivations applied to this observation.
    pub derivation_history: Vec<String>,
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
    for item in unique {
        let score = u32::from(item.quality.score());
        if item.supports {
            supporting_score += score;
            strongest_support = strongest_support.max(score);
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
    }
}
