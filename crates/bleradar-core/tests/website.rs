//! Regression tests for website lineage and ecosystem analysis.

use bleradar_core::{
    EdgeType, EntityType, EvidenceStore, EvidenceValue, Observation, OperatorAssessment,
    RetrievalMethod, Source, SourceType, TemporalInterval, TemporalRelation, WebsiteError,
    WebsiteExplanation, WebsiteFactors, WebsiteFeatureKind, WebsiteLimits,
    WebsiteLineageEcosystemAnalysisEngine, WebsiteObservation, WebsitePhase, WebsiteSnapshot,
};

fn source(id: &str, provider: Option<&str>) -> Source {
    let source = Source::new(id, SourceType::Website, RetrievalMethod::Direct).unwrap();
    if let Some(provider) = provider {
        source.with_metadata("provider", provider)
    } else {
        source
    }
}

fn factors(score: u8) -> WebsiteFactors {
    WebsiteFactors::new(
        score, score, score, score, score, score, score, score, score,
    )
}

fn observation(
    id: &str,
    website_id: &str,
    kind: WebsiteFeatureKind,
    value: &str,
    source: Source,
    observed_at: u64,
) -> WebsiteObservation {
    WebsiteObservation::new(id, website_id, kind, value, source, observed_at)
        .unwrap()
        .with_normalized_value(value.to_ascii_lowercase())
        .with_factors(factors(100))
}

#[test]
fn snapshot_extracts_feature_families_and_preserves_raw_html() {
    let snapshot = WebsiteSnapshot::new(
        "snapshot-a",
        "site-a",
        "<html><head><script src='/app.js'></script></head><body><main>Rare public phrase for lineage analysis.</main></body></html>",
        source("snapshot-source", None),
        100,
    )
    .unwrap()
    .with_timeline(TemporalInterval::new(80, 100, 120).unwrap())
    .with_public_asset("assets/rare-logo.svg")
    .unwrap()
    .with_script_reference("/app.js")
    .unwrap()
    .with_style_reference("/app.css")
    .unwrap()
    .with_public_analytics_identifier("analytics-rare")
    .unwrap()
    .with_public_contact_information("public@example.test")
    .unwrap()
    .with_certificate("sha256:certificate")
    .unwrap()
    .with_outbound_link("https://partner.example")
    .unwrap()
    .with_application_characteristic("framework-x")
    .unwrap()
    .with_archived_state("archive:2026")
    .unwrap();

    let observations = snapshot.extract_observations().unwrap();
    assert!(
        observations
            .iter()
            .any(|observation| observation.kind() == WebsiteFeatureKind::NormalizedText)
    );
    assert!(
        observations
            .iter()
            .any(|observation| observation.kind() == WebsiteFeatureKind::DistinctivePhrase)
    );
    assert!(
        observations
            .iter()
            .any(|observation| observation.kind() == WebsiteFeatureKind::HtmlStructure)
    );
    for kind in WebsiteFeatureKind::ALL {
        if kind == WebsiteFeatureKind::NormalizedText
            || kind == WebsiteFeatureKind::DistinctivePhrase
            || kind == WebsiteFeatureKind::HtmlStructure
        {
            continue;
        }
        assert!(
            observations
                .iter()
                .any(|observation| observation.kind() == kind),
            "missing extracted {kind}"
        );
    }

    let text = observations
        .iter()
        .find(|observation| observation.kind() == WebsiteFeatureKind::NormalizedText)
        .unwrap();
    assert_eq!(
        text.raw_value(),
        &EvidenceValue::Text(
            "<html><head><script src='/app.js'></script></head><body><main>Rare public phrase for lineage analysis.</main></body></html>"
                .to_owned()
        )
    );
    assert_eq!(
        text.normalized_value(),
        Some(&EvidenceValue::Text(
            "rare public phrase for lineage analysis.".to_owned()
        ))
    );
    assert_eq!(text.first_seen(), 80);
    assert_eq!(text.last_seen(), 120);
}

#[test]
fn snapshot_observation_is_transactionally_persisted_as_website_evidence() {
    let snapshot = WebsiteSnapshot::new(
        "snapshot-a",
        "site-a",
        "<html><body>Public site content with enough words to normalize.</body></html>",
        source("snapshot-source", None),
        100,
    )
    .unwrap()
    .with_public_asset("assets/rare.svg")
    .unwrap();
    let mut engine = WebsiteLineageEcosystemAnalysisEngine::new(EvidenceStore::new());
    let ids = engine.observe_snapshot(&snapshot).unwrap();
    assert_eq!(ids.len(), engine.observation_count());
    assert_eq!(
        engine.evidence().entity("site-a").unwrap().kind(),
        &EntityType::Website
    );
    assert!(engine.evidence().validate().is_ok());
}

#[test]
fn rare_assets_and_independent_sources_survive_falsification() {
    let mut engine = WebsiteLineageEcosystemAnalysisEngine::new(EvidenceStore::new());
    for (id, site, source_id, value) in [
        ("asset-a", "site-a", "asset-a-source", "rare-logo.svg"),
        ("asset-b", "site-b", "asset-b-source", "rare-logo.svg"),
        (
            "asset-a-two",
            "site-a",
            "asset-a-two-source",
            "rare-font.woff2",
        ),
        (
            "asset-b-two",
            "site-b",
            "asset-b-two-source",
            "rare-font.woff2",
        ),
    ] {
        engine
            .observe(
                observation(
                    id,
                    site,
                    WebsiteFeatureKind::PublicAsset,
                    value,
                    source(source_id, None),
                    100,
                )
                .with_feature(format!("asset:{value}"))
                .unwrap(),
            )
            .unwrap();
    }

    let report = engine.correlate("site-a", "site-b").unwrap();
    assert_eq!(
        report.phases(),
        &[
            WebsitePhase::Capture,
            WebsitePhase::Normalize,
            WebsitePhase::Extract,
            WebsitePhase::TemporalAlign,
            WebsitePhase::Compare,
            WebsitePhase::Score,
            WebsitePhase::Falsify,
            WebsitePhase::Persist,
            WebsitePhase::Recompute,
        ]
    );
    assert_eq!(report.leading_explanation(), WebsiteExplanation::AssetReuse);
    assert!(report.falsification().survives());
    assert!(report.falsification().without_strongest_support().score() > 0);
    assert_eq!(report.temporal_relation(), TemporalRelation::Overlapping);
    assert_eq!(
        report.operator_assessment(),
        OperatorAssessment::NotEstablished
    );
    assert!(!report.common_operator_proven());
    assert_eq!(report.edge().edge_type(), EdgeType::Inferred);
    assert_eq!(
        report.edge().observation_ids(),
        &[
            "asset-a".to_owned(),
            "asset-a-two".to_owned(),
            "asset-b".to_owned(),
            "asset-b-two".to_owned()
        ]
    );
    let relationship = engine
        .evidence()
        .relationship(report.edge().relationship_id())
        .unwrap();
    assert_eq!(
        relationship.provenance().observations(),
        report.edge().observation_ids()
    );
}

#[test]
fn common_platform_support_is_downweighted_and_not_common_operator_proof() {
    let mut engine = WebsiteLineageEcosystemAnalysisEngine::new(EvidenceStore::new());
    for (id, site, source_id) in [
        ("script-a", "site-a", "script-a-source"),
        ("script-b", "site-b", "script-b-source"),
        ("script-a-copy", "site-a", "script-a-copy-source"),
        ("script-b-copy", "site-b", "script-b-copy-source"),
    ] {
        engine
            .observe(
                observation(
                    id,
                    site,
                    WebsiteFeatureKind::ScriptReference,
                    "/common/framework.js",
                    source(source_id, Some("shared-cdn")),
                    100,
                )
                .in_dependency_group("shared-cdn")
                .unwrap(),
            )
            .unwrap();
    }

    let report = engine.correlate("site-a", "site-b").unwrap();
    let platform = report.ranking(WebsiteExplanation::CommonPlatform).unwrap();
    assert_eq!(platform.independent_support(), 1);
    assert!(!platform.collapsed_pairs().is_empty());
    assert!(platform.has_high_base_rate_support());
    assert_eq!(report.falsification().without_high_base_rate().score(), 0);
    assert!(!report.falsification().survives());
    assert_eq!(report.edge().edge_type(), EdgeType::Contested);
    assert_eq!(
        report.operator_assessment(),
        OperatorAssessment::NotEstablished
    );
    assert!(!report.common_operator_proven());
}

#[test]
fn operational_relationship_remains_a_possible_operator_alternative() {
    let mut engine = WebsiteLineageEcosystemAnalysisEngine::new(EvidenceStore::new());
    for (id, site, source_id, value) in [
        ("analytics-a", "site-a", "analytics-a-source", "G-RARE"),
        ("analytics-b", "site-b", "analytics-b-source", "G-RARE"),
        (
            "contact-a",
            "site-a",
            "contact-a-source",
            "public@example.test",
        ),
        (
            "contact-b",
            "site-b",
            "contact-b-source",
            "public@example.test",
        ),
    ] {
        let kind = if id.starts_with("analytics") {
            WebsiteFeatureKind::PublicAnalyticsIdentifier
        } else {
            WebsiteFeatureKind::PublicContactInformation
        };
        engine
            .observe(
                observation(id, site, kind, value, source(source_id, None), 100)
                    .with_feature(format!("{}:{value}", kind.as_str()))
                    .unwrap(),
            )
            .unwrap();
    }

    let report = engine.correlate("site-a", "site-b").unwrap();
    assert_eq!(
        report.leading_explanation(),
        WebsiteExplanation::OperationalRelationship,
        "{:?}",
        report.rankings()
    );
    assert_eq!(
        report.operator_assessment(),
        OperatorAssessment::PossibleCommonOperator
    );
    assert!(!report.common_operator_proven());
}

#[test]
fn snapshot_conflict_rolls_back_all_extracted_features() {
    let source = source("snapshot-source", None);
    let canonical = Observation::new(
        "snapshot-a:normalized-text",
        "different content",
        Some(EvidenceValue::Text("different content".to_owned())),
        source.id(),
        source.source_type().clone(),
        source.retrieval_method().clone(),
        100,
    )
    .unwrap();
    let mut evidence = EvidenceStore::new();
    evidence.add_source(source.clone()).unwrap();
    evidence.add_observation(canonical).unwrap();

    let snapshot = WebsiteSnapshot::new(
        "snapshot-a",
        "site-a",
        "<html><body>New content with enough words to normalize.</body></html>",
        source,
        100,
    )
    .unwrap()
    .with_public_asset("asset.svg")
    .unwrap();
    let mut engine = WebsiteLineageEcosystemAnalysisEngine::new(evidence);
    let error = engine.observe_snapshot(&snapshot).unwrap_err();
    assert!(matches!(
        error,
        WebsiteError::ObservationConflict {
            observation_id
        } if observation_id == "snapshot-a:normalized-text"
    ));
    assert_eq!(engine.observation_count(), 0);
    assert!(
        engine
            .evidence()
            .observation("snapshot-a:normalized-text")
            .is_some()
    );
    assert!(
        engine
            .evidence()
            .observation("snapshot-a:public-asset-0")
            .is_none()
    );
}

#[test]
fn temporal_gap_limits_and_ranking_are_deterministic() {
    let limits = WebsiteLimits::new(10, 2)
        .unwrap()
        .with_maximum_temporal_gap(5);
    let mut first =
        WebsiteLineageEcosystemAnalysisEngine::with_limits(EvidenceStore::new(), limits);
    let mut second =
        WebsiteLineageEcosystemAnalysisEngine::with_limits(EvidenceStore::new(), limits);
    let observations = [
        observation(
            "a",
            "site-a",
            WebsiteFeatureKind::DistinctivePhrase,
            "same rare phrase",
            source("source-a", None),
            10,
        )
        .with_timeline(TemporalInterval::new(10, 10, 10).unwrap()),
        observation(
            "b",
            "site-b",
            WebsiteFeatureKind::DistinctivePhrase,
            "same rare phrase",
            source("source-b", None),
            13,
        )
        .with_timeline(TemporalInterval::new(13, 13, 13).unwrap()),
    ];
    first.observe(observations[0].clone()).unwrap();
    first.observe(observations[1].clone()).unwrap();
    second.observe(observations[1].clone()).unwrap();
    second.observe(observations[0].clone()).unwrap();

    let first_report = first.correlate("site-a", "site-b").unwrap();
    let second_report = second.correlate("site-a", "site-b").unwrap();
    assert_eq!(first_report, second_report);
    assert_eq!(
        first_report.temporal_relation(),
        TemporalRelation::Contiguous
    );
    assert!(matches!(
        first.correlate("site-a", "site-b"),
        Err(WebsiteError::DuplicateCorrelation { .. })
    ));
}
