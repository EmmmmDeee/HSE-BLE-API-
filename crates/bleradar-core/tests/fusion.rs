//! Regression tests for calibrated evidence fusion and falsification.

use bleradar_core::{
    CalibratedEvidenceFusion, DependencyKind, Evidence, EvidenceAssessment, EvidenceQuality,
    EvidenceRole, EvidenceStore, ExpectedEvidence, FusionError, Hypothesis, HypothesisKind,
    Observation, RetrievalMethod, Source, SourceType,
};

fn source(id: &str) -> Source {
    Source::new(id, SourceType::Document, RetrievalMethod::Direct).unwrap()
}

fn observation(id: &str, source: &Source, timestamp: u64) -> Observation {
    Observation::from_source(id, "raw", None, source, timestamp).unwrap()
}

fn uniform_assessment(id: &str, score: u8) -> EvidenceAssessment {
    EvidenceAssessment::new(id, EvidenceQuality::uniform(score)).unwrap()
}

#[test]
fn quality_keeps_all_calibration_dimensions_visible() {
    let quality = EvidenceQuality::new(100, 90, 80, 70, 60, 50, 40, 30, 20);

    assert_eq!(quality.reliability().value(), 100);
    assert_eq!(quality.specificity().value(), 90);
    assert_eq!(quality.rarity().value(), 80);
    assert_eq!(quality.discriminative_power().value(), 70);
    assert_eq!(quality.source_independence().value(), 60);
    assert_eq!(quality.temporal_compatibility().value(), 50);
    assert_eq!(quality.transformation_resistance().value(), 40);
    assert_eq!(quality.provenance_quality().value(), 30);
    assert_eq!(quality.reproducibility().value(), 20);
    assert_eq!(quality.calibrated_weight(), 60);
}

#[test]
fn dependent_reporting_is_counted_once() {
    let first_source = source("source-a");
    let second_source = source("source-b");
    let hypothesis =
        Hypothesis::new("hypothesis-1", "shared origin", HypothesisKind::Leading).unwrap();
    let first_observation = observation("observation-a", &first_source, 10);
    let second_observation = observation("observation-b", &second_source, 20);
    let first_evidence = Evidence::new(
        "evidence-a",
        hypothesis.id(),
        first_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();
    let second_evidence = Evidence::new(
        "evidence-b",
        hypothesis.id(),
        second_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();

    let mut store = EvidenceStore::new();
    store.add_source(first_source).unwrap();
    store.add_source(second_source).unwrap();
    store.add_hypothesis(hypothesis).unwrap();
    store.add_observation(first_observation).unwrap();
    store.add_observation(second_observation).unwrap();
    store.add_evidence(first_evidence).unwrap();
    store.add_evidence(second_evidence).unwrap();

    let mut fusion = CalibratedEvidenceFusion::new();
    fusion
        .add_assessment(
            uniform_assessment("evidence-a", 80)
                .in_group("original-report")
                .with_dependency(DependencyKind::CopiedReporting),
        )
        .unwrap();
    fusion
        .add_assessment(
            uniform_assessment("evidence-b", 70)
                .in_group("original-report")
                .with_dependency(DependencyKind::CopiedReporting),
        )
        .unwrap();

    let result = fusion.fuse(&store).unwrap();
    let score = result.score("hypothesis-1").unwrap();
    assert_eq!(score.support_score(), 80);
    assert_eq!(score.supporting_evidence(), &["evidence-a".to_owned()]);
    assert_eq!(score.collapsed_evidence(), &["evidence-b".to_owned()]);
}

#[test]
fn same_source_is_conservative_without_explicit_group() {
    let source = source("source-a");
    let hypothesis =
        Hypothesis::new("hypothesis-1", "same source", HypothesisKind::Leading).unwrap();
    let first_observation = observation("observation-a", &source, 10);
    let second_observation = observation("observation-b", &source, 20);
    let first_evidence = Evidence::new(
        "evidence-a",
        hypothesis.id(),
        first_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();
    let second_evidence = Evidence::new(
        "evidence-b",
        hypothesis.id(),
        second_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();
    let mut store = EvidenceStore::new();
    store.add_source(source).unwrap();
    store.add_hypothesis(hypothesis).unwrap();
    store.add_observation(first_observation).unwrap();
    store.add_observation(second_observation).unwrap();
    store.add_evidence(first_evidence).unwrap();
    store.add_evidence(second_evidence).unwrap();

    let mut fusion = CalibratedEvidenceFusion::new();
    fusion
        .add_assessment(uniform_assessment("evidence-a", 60))
        .unwrap();
    fusion
        .add_assessment(uniform_assessment("evidence-b", 90))
        .unwrap();

    let score = fusion
        .fuse(&store)
        .unwrap()
        .score("hypothesis-1")
        .unwrap()
        .clone();
    assert_eq!(score.support_score(), 90);
    assert_eq!(score.supporting_evidence(), &["evidence-b".to_owned()]);
    assert_eq!(score.collapsed_evidence(), &["evidence-a".to_owned()]);
}

#[test]
fn falsification_removes_base_rate_support_and_reports_gaps() {
    let leading = Hypothesis::new(
        "hypothesis-leading",
        "specific explanation",
        HypothesisKind::Leading,
    )
    .unwrap();
    let alternative = Hypothesis::new(
        "hypothesis-null",
        "ordinary explanation",
        HypothesisKind::Null,
    )
    .unwrap();
    let leading_source = source("source-leading");
    let base_rate_source = source("source-base-rate");
    let contradiction_source = source("source-contradiction");
    let alternative_source = source("source-alternative");
    let leading_observation = observation("observation-leading", &leading_source, 10);
    let base_rate_observation = observation("observation-base-rate", &base_rate_source, 20);
    let contradiction_observation =
        observation("observation-contradiction", &contradiction_source, 30);
    let alternative_observation = observation("observation-alternative", &alternative_source, 40);
    let leading_evidence = Evidence::new(
        "evidence-leading",
        leading.id(),
        leading_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();
    let base_rate_evidence = Evidence::new(
        "evidence-base-rate",
        leading.id(),
        base_rate_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();
    let contradiction_evidence = Evidence::new(
        "evidence-contradiction",
        leading.id(),
        contradiction_observation.id(),
        EvidenceRole::Contradicting,
    )
    .unwrap();
    let alternative_evidence = Evidence::new(
        "evidence-alternative",
        alternative.id(),
        alternative_observation.id(),
        EvidenceRole::Supporting,
    )
    .unwrap();

    let mut store = EvidenceStore::new();
    for item in [
        leading_source,
        base_rate_source,
        contradiction_source,
        alternative_source,
    ] {
        store.add_source(item).unwrap();
    }
    store.add_hypothesis(leading).unwrap();
    store.add_hypothesis(alternative).unwrap();
    for item in [
        leading_observation,
        base_rate_observation,
        contradiction_observation,
        alternative_observation,
    ] {
        store.add_observation(item).unwrap();
    }
    for item in [
        leading_evidence,
        base_rate_evidence,
        contradiction_evidence,
        alternative_evidence,
    ] {
        store.add_evidence(item).unwrap();
    }

    let mut fusion = CalibratedEvidenceFusion::new();
    fusion
        .add_assessment(uniform_assessment("evidence-leading", 90))
        .unwrap();
    fusion
        .add_assessment(
            uniform_assessment("evidence-base-rate", 80)
                .high_base_rate()
                .with_dependency(DependencyKind::CommonDataset),
        )
        .unwrap();
    fusion
        .add_assessment(uniform_assessment("evidence-contradiction", 20))
        .unwrap();
    fusion
        .add_assessment(uniform_assessment("evidence-alternative", 75))
        .unwrap();
    fusion
        .add_expected_evidence(
            ExpectedEvidence::new(
                "expected-leading-confirmation",
                "hypothesis-leading",
                "an independent confirmation",
            )
            .unwrap(),
        )
        .unwrap();

    let result = fusion.fuse(&store).unwrap();
    assert_eq!(result.leading_hypothesis(), "hypothesis-leading");
    let report = result.falsify(&fusion, &store).unwrap();
    assert_eq!(report.strongest_alternative(), Some("hypothesis-null"));
    assert_eq!(report.baseline().net_score(), 150);
    assert_eq!(report.without_high_base_rate().net_score(), 70);
    assert_eq!(report.without_strongest_support().net_score(), 60);
    assert_eq!(
        report.contradictory_evidence(),
        &["evidence-contradiction".to_owned()]
    );
    assert_eq!(report.removed_support(), Some("evidence-leading"));
    assert_eq!(report.missing_expected_evidence().len(), 1);
    assert!(!report.survives());
}

#[test]
fn fusion_rejects_unregistered_evidence() {
    let mut fusion = CalibratedEvidenceFusion::new();
    fusion
        .add_assessment(uniform_assessment("missing", 50))
        .unwrap();

    assert_eq!(
        fusion.fuse(&EvidenceStore::new()),
        Err(FusionError::MissingEvidence {
            evidence_id: "missing".to_owned()
        })
    );
}
