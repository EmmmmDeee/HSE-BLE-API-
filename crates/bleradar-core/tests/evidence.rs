//! Regression tests for immutable evidence provenance and calibrated fusion.

use bleradar_core::{
    Action, ActionOutcome, Artifact, Claim, ConfidenceUpdate, EdgeType, Event, Evidence,
    EvidenceQuality, Feature, Hypothesis, Observation, Relationship, Representation, Source, Test,
    TraceError, Transformation, fuse_evidence, trace_claim,
};

fn quality(value: u8) -> EvidenceQuality {
    EvidenceQuality {
        reliability: value,
        specificity: value,
        rarity: value,
        discriminative_power: value,
        source_independence: value,
        temporal_compatibility: value,
        transformation_resistance: value,
        provenance_quality: value,
        reproducibility: value,
    }
}

#[test]
fn normalization_preserves_raw_value_and_chronology() {
    let source = Source {
        id: "scanner-1".into(),
        source_type: "ble_advertisement".into(),
        retrieval_method: "passive_scan".into(),
    };
    let observation = Observation::new("obs-1", " 36-32-62-36-31-33 ", source, 20)
        .with_normalization("36:32:62:36:31:33", "canonical-mac-v1")
        .with_seen_at(10)
        .with_seen_at(30);

    assert_eq!(observation.raw_value(), " 36-32-62-36-31-33 ");
    assert_eq!(observation.normalized_value(), Some("36:32:62:36:31:33"));
    assert_eq!(observation.derivation_history(), ["canonical-mac-v1"]);
    assert_eq!(
        (observation.first_seen_ms(), observation.last_seen_ms()),
        (10, 30)
    );
    assert_eq!(observation.source().id, "scanner-1");
}

#[test]
fn fusion_collapses_duplicate_dependencies_and_tests_strongest_support() {
    let evidence = [
        Evidence {
            id: "e1".into(),
            observation_id: "o1".into(),
            source_id: "s1".into(),
            supports: true,
            dependency_key: "original-report".into(),
            quality: quality(90),
        },
        Evidence {
            id: "e2".into(),
            observation_id: "o2".into(),
            source_id: "s2".into(),
            supports: true,
            dependency_key: "original-report".into(),
            quality: quality(30),
        },
        Evidence {
            id: "e3".into(),
            observation_id: "o3".into(),
            source_id: "s3".into(),
            supports: false,
            dependency_key: "independent-contradiction".into(),
            quality: quality(40),
        },
    ];

    let result = fuse_evidence(&evidence);
    assert_eq!(result.supporting_score, 90);
    assert_eq!(result.contradictory_score, 40);
    assert_eq!(result.collapsed_dependency_keys, ["original-report"]);
    assert_eq!(result.without_strongest_support_score, -40);
}

#[test]
fn fusion_removes_high_base_rate_support() {
    // Common feature: high overall score but low rarity, so it should not be
    // able to carry the conclusion on its own.
    let common_quality = EvidenceQuality {
        rarity: 10,
        ..quality(90)
    };
    // Rare feature: genuinely discriminating support.
    let rare_quality = EvidenceQuality {
        rarity: 95,
        ..quality(60)
    };
    let contradiction_quality = quality(20);

    let evidence = [
        Evidence {
            id: "common".into(),
            observation_id: "o1".into(),
            source_id: "s1".into(),
            supports: true,
            dependency_key: "common-dep".into(),
            quality: common_quality,
        },
        Evidence {
            id: "rare".into(),
            observation_id: "o2".into(),
            source_id: "s2".into(),
            supports: true,
            dependency_key: "rare-dep".into(),
            quality: rare_quality,
        },
        Evidence {
            id: "contradiction".into(),
            observation_id: "o3".into(),
            source_id: "s3".into(),
            supports: false,
            dependency_key: "contradiction-dep".into(),
            quality: contradiction_quality,
        },
    ];

    let result = fuse_evidence(&evidence);
    assert_eq!(
        result.without_high_base_rate_support_score,
        i32::from(rare_quality.score()) - i32::from(contradiction_quality.score())
    );
    assert!(result.without_high_base_rate_support_score < result.supporting_score as i32);
}

#[test]
fn trace_claim_succeeds_when_every_link_resolves() {
    let source = Source {
        id: "s1".into(),
        source_type: "public_web".into(),
        retrieval_method: "http_get".into(),
    };
    let observation = Observation::new("o1", "raw", source.clone(), 0);
    let hypothesis = Hypothesis {
        id: "h1".into(),
        statement: "same operator".into(),
        is_null: false,
    };
    let evidence = Evidence {
        id: "e1".into(),
        observation_id: "o1".into(),
        source_id: "s1".into(),
        supports: true,
        dependency_key: "d1".into(),
        quality: quality(80),
    };
    let claim = Claim {
        id: "c1".into(),
        statement: "entities are related".into(),
        hypothesis_id: "h1".into(),
        evidence_ids: vec!["e1".into()],
    };

    assert_eq!(
        trace_claim(
            &claim,
            &[hypothesis],
            &[evidence],
            &[observation],
            &[source]
        ),
        Ok(())
    );
}

#[test]
fn trace_claim_reports_the_first_broken_link() {
    let claim = Claim {
        id: "c1".into(),
        statement: "unsupported".into(),
        hypothesis_id: "missing-hypothesis".into(),
        evidence_ids: vec!["e1".into()],
    };

    assert_eq!(
        trace_claim(&claim, &[], &[], &[], &[]),
        Err(TraceError::MissingHypothesis("missing-hypothesis".into()))
    );

    let hypothesis = Hypothesis {
        id: "h1".into(),
        statement: "explanation".into(),
        is_null: true,
    };
    let claim_with_hypothesis = Claim {
        hypothesis_id: "h1".into(),
        ..claim
    };
    assert_eq!(
        trace_claim(&claim_with_hypothesis, &[hypothesis], &[], &[], &[]),
        Err(TraceError::MissingEvidence("e1".into()))
    );
}

#[test]
fn canonical_entity_kinds_compose_into_one_evidence_graph() {
    let artifact = Artifact {
        id: "art-1".into(),
        artifact_type: "web_page".into(),
        source_id: "s1".into(),
        collected_at_ms: 100,
    };
    let representation = Representation {
        id: "rep-1".into(),
        subject_id: artifact.id.clone(),
        format: "text".into(),
        feature_ids: vec!["feat-1".into()],
    };
    let feature = Feature {
        id: "feat-1".into(),
        representation_id: representation.id.clone(),
        name: "distinctive_phrase".into(),
        value: "unique marketing slogan".into(),
    };
    let transformation = Transformation {
        id: "trans-1".into(),
        input_representation_id: "rep-0".into(),
        output_representation_id: representation.id.clone(),
        preserved_feature_ids: vec![feature.id.clone()],
        changed_feature_ids: vec![],
        verification_ids: vec!["test-1".into()],
    };
    let test = Test {
        id: "test-1".into(),
        subject_id: transformation.id.clone(),
        method: "normalization_equivalence".into(),
        passed: true,
        executed_at_ms: 110,
    };
    let event = Event {
        id: "event-1".into(),
        description: "page republished".into(),
        source_id: "s1".into(),
        started_at_ms: 100,
        ended_at_ms: Some(100),
    };
    let relationship = Relationship {
        id: "rel-1".into(),
        subject_id: "site-a".into(),
        object_id: "site-b".into(),
        relationship_type: "content_reuse".into(),
        edge_type: EdgeType::Derived,
        source_id: "s1".into(),
        method: "asset_hash_match".into(),
        observed_at_ms: 100,
        supporting_evidence_ids: vec!["e1".into()],
        contradicting_evidence_ids: vec![],
        confidence: 70,
    };
    let action = Action {
        id: "action-1".into(),
        description: "verify_transformation".into(),
        target_id: transformation.id.clone(),
        initiated_at_ms: 105,
        outcome: Some(ActionOutcome::Succeeded),
    };
    let confidence_update = ConfidenceUpdate {
        id: "conf-1".into(),
        subject_id: relationship.id.clone(),
        previous_confidence: 55,
        updated_confidence: relationship.confidence,
        evidence_ids: vec!["e1".into()],
        reason: "independent asset-hash corroboration".into(),
        updated_at_ms: 100,
    };

    assert_eq!(representation.subject_id, artifact.id);
    assert_eq!(feature.representation_id, representation.id);
    assert_eq!(test.subject_id, transformation.id);
    assert_eq!(action.target_id, transformation.id);
    assert_eq!(confidence_update.subject_id, relationship.id);
    assert!(confidence_update.updated_confidence > confidence_update.previous_confidence);
    assert_eq!(event.ended_at_ms, Some(event.started_at_ms));
}
