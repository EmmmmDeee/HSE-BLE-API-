//! Regression tests for temporal metamorphic infrastructure correlation.

use bleradar_core::{
    ControlAssessment, CorrelationFactors, EdgeType, EvidenceStore, EvidenceValue,
    InfrastructureError, InfrastructureExplanation, InfrastructureKind, InfrastructureLimits,
    InfrastructureObservation, InfrastructurePhase, RetrievalMethod, Source, SourceType,
    TemporalInterval, TemporalMetamorphicInfrastructureCorrelationEngine, TemporalRelation,
};

fn source(id: &str, provider: Option<&str>) -> Source {
    let source = Source::new(id, SourceType::Website, RetrievalMethod::Direct).unwrap();
    if let Some(provider) = provider {
        source.with_metadata("provider", provider)
    } else {
        source
    }
}

fn factors(score: u8) -> CorrelationFactors {
    CorrelationFactors::new(
        score, score, score, score, score, score, score, score, score,
    )
}

fn observation(
    id: &str,
    node_id: &str,
    kind: InfrastructureKind,
    value: &str,
    source: Source,
    observed_at: u64,
) -> InfrastructureObservation {
    InfrastructureObservation::new(id, node_id, kind, value, source, observed_at)
        .unwrap()
        .with_factors(factors(90))
}

#[test]
fn all_infrastructure_kinds_and_temporal_intervals_are_explicit() {
    assert_eq!(InfrastructureKind::ALL.len(), 11);
    assert_eq!(
        InfrastructureKind::ALL
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "domain",
            "dns",
            "ip_address",
            "asn",
            "certificate",
            "hosting_provider",
            "http_characteristic",
            "public_asset",
            "public_identifier",
            "application_structure",
            "archived_state",
        ]
    );

    let left = TemporalInterval::new(10, 20, 30).unwrap();
    let right = TemporalInterval::new(25, 30, 40).unwrap();
    assert!(left.overlaps(right));
    assert_eq!(left.intersection(right).unwrap().first_seen(), 25);
    assert_eq!(left.gap(right), 0);
    assert!(left.is_contiguous_with(TemporalInterval::at(100), 70));
    assert_eq!(
        TemporalInterval::new(30, 20, 40),
        Err(InfrastructureError::Provenance {
            error: bleradar_core::ProvenanceError::InvalidTimeline {
                first_seen: 30,
                observed_at: 20,
                last_seen: 40,
            }
        })
    );
}

#[test]
fn observations_preserve_raw_normalized_values_and_canonical_provenance() {
    let source = source("source-a", None)
        .with_locator("https://example.test/dns")
        .captured_at(10);
    let observation = observation(
        "dns-a",
        "site-a",
        InfrastructureKind::Dns,
        " 203.0.113.7 ",
        source.clone(),
        20,
    )
    .with_normalized_value("203.0.113.7")
    .with_interval(TemporalInterval::new(10, 20, 30).unwrap());

    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::new(EvidenceStore::new());
    engine.observe(observation.clone()).unwrap();
    let canonical = engine.evidence().observation("dns-a").unwrap();
    assert_eq!(
        canonical.raw_value(),
        &EvidenceValue::Text(" 203.0.113.7 ".to_owned())
    );
    assert_eq!(
        canonical.normalized_value(),
        Some(&EvidenceValue::Text("203.0.113.7".to_owned()))
    );
    assert_eq!(canonical.source(), "source-a");
    assert_eq!(canonical.first_seen(), 10);
    assert_eq!(canonical.last_seen(), 30);
    assert_eq!(
        engine.evidence().entity("site-a").unwrap().kind(),
        &bleradar_core::EntityType::Infrastructure
    );
}

#[test]
fn correlation_is_temporal_provenance_linked_and_deterministic() {
    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::new(EvidenceStore::new());
    engine
        .observe(
            observation(
                "cert-a",
                "site-a",
                InfrastructureKind::Certificate,
                "sha256:rare",
                source("cert-source-a", None),
                20,
            )
            .with_normalized_value("sha256:rare")
            .with_interval(TemporalInterval::new(10, 20, 30).unwrap()),
        )
        .unwrap();
    engine
        .observe(
            observation(
                "cert-b",
                "site-b",
                InfrastructureKind::Certificate,
                "sha256:rare",
                source("cert-source-b", None),
                25,
            )
            .with_normalized_value("sha256:rare")
            .with_interval(TemporalInterval::new(15, 25, 35).unwrap()),
        )
        .unwrap();
    engine
        .observe(
            observation(
                "identifier-a",
                "site-a",
                InfrastructureKind::PublicIdentifier,
                "operator-id",
                source("identifier-source-a", None),
                22,
            )
            .with_normalized_value("operator-id"),
        )
        .unwrap();
    engine
        .observe(
            observation(
                "identifier-b",
                "site-b",
                InfrastructureKind::PublicIdentifier,
                "operator-id",
                source("identifier-source-b", None),
                27,
            )
            .with_normalized_value("operator-id"),
        )
        .unwrap();

    let report = engine.correlate("site-a", "site-b").unwrap();
    assert_eq!(
        report.phases(),
        &[
            InfrastructurePhase::Capture,
            InfrastructurePhase::Normalize,
            InfrastructurePhase::TemporalAlign,
            InfrastructurePhase::Compare,
            InfrastructurePhase::Score,
            InfrastructurePhase::Falsify,
            InfrastructurePhase::Persist,
            InfrastructurePhase::Recompute,
        ]
    );
    assert_eq!(
        report.leading_explanation(),
        InfrastructureExplanation::DirectTechnicalRelationship
    );
    assert_eq!(report.temporal_relation(), TemporalRelation::Overlapping);
    assert_eq!(
        report.control_assessment(),
        ControlAssessment::DirectTechnicalRelationship
    );
    assert!(!report.common_control_proven());
    assert!(report.falsification().survives());
    assert_eq!(report.edge().edge_type(), EdgeType::Inferred);

    let relationship = engine
        .evidence()
        .relationship(report.edge().relationship_id())
        .unwrap();
    assert_eq!(relationship.subject(), "site-a");
    assert_eq!(relationship.object(), "site-b");
    assert_eq!(
        relationship.provenance().observations(),
        &[
            "cert-a".to_owned(),
            "cert-b".to_owned(),
            "identifier-a".to_owned(),
            "identifier-b".to_owned()
        ]
    );
    assert_eq!(
        engine
            .evidence()
            .source("temporal-infrastructure-correlation")
            .unwrap()
            .source_type(),
        &SourceType::Derived
    );
}

#[test]
fn common_host_is_downweighted_and_dependent_sources_collapse() {
    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::new(EvidenceStore::new());
    for (id, node, source_id) in [
        ("host-a-1", "site-a", "provider-a"),
        ("host-a-2", "site-a", "provider-a-copy"),
        ("host-b-1", "site-b", "provider-b"),
        ("host-b-2", "site-b", "provider-b-copy"),
    ] {
        let source = source(source_id, Some("same-provider"));
        let observation = observation(
            id,
            node,
            InfrastructureKind::HostingProvider,
            "shared-host",
            source,
            100,
        )
        .with_factors(factors(100))
        .in_dependency_group("hosting-provider")
        .unwrap();
        engine.observe(observation).unwrap();
    }

    let report = engine.correlate("site-a", "site-b").unwrap();
    let host = report
        .ranking(InfrastructureExplanation::CommonHost)
        .unwrap();
    assert_eq!(host.independent_support(), 1);
    assert!(!host.collapsed_pairs().is_empty());
    assert!(host.has_high_base_rate_support());
    assert!(host.score() < 100);
    assert_eq!(report.falsification().without_high_base_rate().score(), 0);
    assert!(!report.falsification().survives());
    assert_eq!(report.edge().edge_type(), EdgeType::Contested);
    assert_eq!(
        report.control_assessment(),
        ControlAssessment::SharedInfrastructure
    );
}

#[test]
fn rare_features_and_independent_sources_raise_possible_administration_alternative() {
    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::new(EvidenceStore::new());
    for (id, node, source_id, value) in [
        ("asset-a", "site-a", "asset-a-source", "asset-hash"),
        ("asset-b", "site-b", "asset-b-source", "asset-hash"),
        (
            "identifier-a",
            "site-a",
            "identifier-a-source",
            "operator-id",
        ),
        (
            "identifier-b",
            "site-b",
            "identifier-b-source",
            "operator-id",
        ),
    ] {
        let observation = observation(
            id,
            node,
            if id.starts_with("asset") {
                InfrastructureKind::PublicAsset
            } else {
                InfrastructureKind::PublicIdentifier
            },
            value,
            source(source_id, None),
            200,
        )
        .with_factors(factors(100));
        engine.observe(observation).unwrap();
    }

    let report = engine.correlate("site-a", "site-b").unwrap();
    let administration = report
        .ranking(InfrastructureExplanation::PossibleCommonAdministration)
        .unwrap();
    assert!(administration.independent_support() >= 2);
    assert_ne!(
        report.control_assessment(),
        ControlAssessment::PossibleCommonAdministration
    );
    assert!(!report.common_control_proven());
}

#[test]
fn correlation_persistence_is_transactional_on_source_conflict() {
    let mut evidence = EvidenceStore::new();
    evidence
        .add_source(
            Source::new(
                "temporal-infrastructure-correlation",
                SourceType::Website,
                RetrievalMethod::Direct,
            )
            .unwrap(),
        )
        .unwrap();
    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::new(evidence);
    engine
        .observe(observation(
            "ip-a",
            "site-a",
            InfrastructureKind::IpAddress,
            "203.0.113.8",
            source("ip-source-a", None),
            1,
        ))
        .unwrap();
    engine
        .observe(observation(
            "ip-b",
            "site-b",
            InfrastructureKind::IpAddress,
            "203.0.113.8",
            source("ip-source-b", None),
            1,
        ))
        .unwrap();

    let error = engine.correlate("site-a", "site-b").unwrap_err();
    assert_eq!(
        error,
        InfrastructureError::SourceConflict {
            source_id: "temporal-infrastructure-correlation".to_owned()
        }
    );
    assert_eq!(engine.correlation_count(), 0);
    assert!(
        engine
            .evidence()
            .relationship("infrastructure-correlation:site-a:site-b:relationship")
            .is_none()
    );
}

#[test]
fn limits_and_repeated_correlations_are_enforced() {
    let limits = InfrastructureLimits::new(2, 1).unwrap();
    let mut engine = TemporalMetamorphicInfrastructureCorrelationEngine::with_limits(
        EvidenceStore::new(),
        limits,
    );
    engine
        .observe(observation(
            "one",
            "site-a",
            InfrastructureKind::Domain,
            "a.example",
            source("one-source", None),
            1,
        ))
        .unwrap();
    engine
        .observe(observation(
            "two",
            "site-b",
            InfrastructureKind::Domain,
            "b.example",
            source("two-source", None),
            1,
        ))
        .unwrap();
    assert!(matches!(
        engine.observe(observation(
            "three",
            "site-c",
            InfrastructureKind::Domain,
            "c.example",
            source("three-source", None),
            1,
        )),
        Err(InfrastructureError::ResourceLimit {
            resource: "observations",
            limit: 2
        })
    ));
    assert!(matches!(
        engine.correlate("site-a", "site-b"),
        Err(InfrastructureError::NoComparableObservations { .. })
    ));
}
