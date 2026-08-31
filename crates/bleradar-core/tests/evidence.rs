//! Regression tests for the canonical evidence and provenance core.

use bleradar_core::{
    Artifact, ArtifactType, Claim, Confidence, Evidence, EvidenceRole, EvidenceStore,
    EvidenceValue, Hypothesis, HypothesisKind, Observation, ObservationTimeline, ProvenanceError,
    Representation, RepresentationType, RetrievalMethod, Source, SourceType, Test, TestType,
    Transformation, Verification,
};

fn source() -> Source {
    Source::new("source-1", SourceType::Sensor, RetrievalMethod::Direct).unwrap()
}

fn observation(source: &Source) -> Observation {
    Observation::from_source(
        "observation-1",
        EvidenceValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
        Some(EvidenceValue::Text("deadbeef".to_owned())),
        source,
        100,
    )
    .unwrap()
}

#[test]
fn observation_keeps_raw_value_when_normalized() {
    let source = source();
    let original = observation(&source);
    let normalized = original
        .with_normalization("DE:AD:BE:EF", "normalization-1")
        .unwrap();

    assert_eq!(
        original.raw_value(),
        &EvidenceValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])
    );
    assert_eq!(normalized.raw_value(), original.raw_value());
    assert_eq!(
        normalized.normalized_value(),
        Some(&EvidenceValue::Text("DE:AD:BE:EF".to_owned()))
    );
    assert_eq!(
        normalized.derivation_history(),
        &["normalization-1".to_owned()]
    );
}

#[test]
fn observation_timeline_is_ordered_and_extendable() {
    let source = source();
    let timeline = ObservationTimeline::new(10, 20, 30).unwrap();
    let observation = Observation::with_timeline(
        "observation-1",
        "raw",
        None,
        source.id(),
        SourceType::Sensor,
        RetrievalMethod::Direct,
        timeline,
    )
    .unwrap();
    let extended = observation.seen_at(40).unwrap();

    assert_eq!(observation.first_seen(), 10);
    assert_eq!(observation.observed_at(), 20);
    assert_eq!(observation.last_seen(), 30);
    assert_eq!(extended.last_seen(), 40);
    assert!(ObservationTimeline::new(30, 20, 40).is_err());
    assert_eq!(
        observation.seen_at(29),
        Err(ProvenanceError::InvalidTimeline {
            first_seen: 10,
            observed_at: 20,
            last_seen: 29,
        })
    );
}

#[test]
fn claim_trace_reaches_the_authoritative_source() {
    let source = source();
    let observation = observation(&source);
    let hypothesis =
        Hypothesis::new("hypothesis-1", "ordinary explanation", HypothesisKind::Null).unwrap();
    let claim = Claim::new(
        "claim-1",
        "the observed value has the ordinary explanation",
        hypothesis.id(),
    )
    .unwrap();
    let evidence = Evidence::new(
        "evidence-1",
        hypothesis.id(),
        observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();

    let mut store = EvidenceStore::new();
    store.add_source(source).unwrap();
    store.add_observation(observation).unwrap();
    store.add_hypothesis(hypothesis).unwrap();
    store.add_claim(claim).unwrap();
    store.add_evidence(evidence).unwrap();

    let trace = store.trace_claim("claim-1").unwrap();
    assert_eq!(
        trace.claim.statement(),
        "the observed value has the ordinary explanation"
    );
    assert_eq!(trace.hypothesis.id(), "hypothesis-1");
    assert_eq!(trace.evidence.len(), 1);
    assert_eq!(trace.evidence[0].observation.id(), "observation-1");
    assert_eq!(trace.evidence[0].source.id(), "source-1");
    assert_eq!(store.validate(), Ok(()));
}

#[test]
fn transformation_trace_contains_features_and_verification() {
    let source = source();
    let observation = observation(&source);
    let feature = bleradar_core::Feature::new("feature-1", "digest", "deadbeef")
        .unwrap()
        .from_observation(observation.id())
        .with_confidence(Confidence::new(90));
    let artifact = Artifact::new("artifact-1", ArtifactType::Digital).unwrap();
    let input = Representation::new("representation-raw", artifact.id(), RepresentationType::Raw)
        .unwrap()
        .with_feature(feature.id());
    let output = Representation::new(
        "representation-normalized",
        artifact.id(),
        RepresentationType::Normalized,
    )
    .unwrap()
    .with_feature(feature.id());
    let test = Test::new(
        "test-1",
        "normalization preserves digest",
        TestType::Provenance,
    )
    .unwrap()
    .completed(bleradar_core::TestStatus::Passed, 200);
    let verification = Verification::passed(vec!["test-1".to_owned()]).unwrap();
    let transformation = Transformation::new(
        "transformation-1",
        input.id(),
        output.id(),
        vec![feature.id().to_owned()],
        Vec::<String>::new(),
        verification,
    )
    .unwrap();

    let mut store = EvidenceStore::new();
    store.add_source(source).unwrap();
    store.add_observation(observation).unwrap();
    store.add_artifact(artifact).unwrap();
    store.add_feature(feature).unwrap();
    store.add_representation(input).unwrap();
    store.add_representation(output).unwrap();
    store.add_test(test).unwrap();
    store.add_transformation(transformation).unwrap();

    let trace = store.trace_transformation("transformation-1").unwrap();
    assert_eq!(trace.input_representation.id(), "representation-raw");
    assert_eq!(
        trace.output_representation.id(),
        "representation-normalized"
    );
    assert_eq!(trace.preserved_features[0].id(), "feature-1");
    assert_eq!(trace.verification_tests[0].id(), "test-1");
    assert_eq!(
        store
            .artifact("artifact-1")
            .unwrap()
            .representation_ids()
            .len(),
        2
    );
}

#[test]
fn overlapping_transformation_features_are_rejected() {
    let error = Transformation::new(
        "transformation-1",
        "input",
        "output",
        vec!["feature-1".to_owned()],
        vec!["feature-1".to_owned()],
        Verification::unverified(),
    )
    .unwrap_err();

    assert!(matches!(error, ProvenanceError::FeatureInBothSets { .. }));
}

#[test]
fn source_metadata_is_not_silently_changed() {
    let source = source();
    let observation = Observation::new(
        "observation-1",
        "raw",
        None,
        source.id(),
        SourceType::Document,
        RetrievalMethod::Direct,
        100,
    )
    .unwrap();
    let mut store = EvidenceStore::new();
    store.add_source(source).unwrap();

    assert!(matches!(
        store.add_observation(observation),
        Err(ProvenanceError::SourceMetadataMismatch { .. })
    ));
}
