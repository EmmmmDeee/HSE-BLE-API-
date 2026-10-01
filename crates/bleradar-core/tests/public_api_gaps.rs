//! Focused tests for public `bleradar-core` functions that no test, example,
//! or in-repo caller exercised before (found by a reference census of every
//! `pub fn` against `crates/`, `xtask/` and the Android sources). Each test
//! pins the behaviour the function's documentation promises, so a future
//! change to it is caught instead of passing silently.

use bleradar_core::entity::{
    CONSENSUS_SOURCE, HseEntity, HseEntityKind, HseEvidence, HseVerificationMethod, RECALL_SOURCE,
};
use bleradar_core::{
    Confidence, InfrastructureExplanation, RequiredSemantics, TemporalGeoGraph, VerificationSurface,
};

// ── HseEvidence::with_optional_attrs ─────────────────────────────────────

#[test]
fn with_optional_attrs_skips_absent_and_blank_values_and_trims_the_rest() {
    let ev = HseEvidence::new("breach", "row", 0).with_optional_attrs([
        ("name", Some("  Ada Lovelace  ")),
        ("phone", None),
        ("city", Some("   ")),
        ("country", Some("")),
        ("email", Some("ada@example.com")),
    ]);
    assert_eq!(ev.attributes.len(), 2, "{:?}", ev.attributes);
    assert_eq!(ev.attributes["name"], "Ada Lovelace");
    assert_eq!(ev.attributes["email"], "ada@example.com");
    assert!(!ev.attributes.contains_key("phone"));
    assert!(!ev.attributes.contains_key("city"));
    assert!(!ev.attributes.contains_key("country"));
}

#[test]
fn with_optional_attrs_keeps_with_attrs_accumulate_and_dedup_semantics() {
    let ev = HseEvidence::new("breach", "rows", 0)
        .with_attr("gender", "f")
        .with_optional_attrs([
            ("gender", Some("m")),
            ("gender", Some(" f ")), // trims to an existing value: idempotent
            ("gender", None),
        ]);
    assert_eq!(ev.attributes["gender"], "f; m");
    assert_eq!(ev.attr_values("gender").collect::<Vec<_>>(), ["f", "m"]);
}

#[test]
fn with_optional_attrs_of_nothing_is_the_identity() {
    let ev = HseEvidence::new("s", "x", 7).with_optional_attrs(std::iter::empty());
    assert!(ev.attributes.is_empty());
    assert_eq!(ev.recorded_at, 7);
}

// ── HseEvidence::with_verification / with_inferred ───────────────────────

#[test]
fn evidence_defaults_to_observed_and_unverified() {
    let ev = HseEvidence::new("s", "x", 0);
    assert_eq!(ev.verification, None);
    assert!(!ev.is_inferred);
}

#[test]
fn with_verification_and_with_inferred_set_only_their_own_field() {
    let ev = HseEvidence::new("s", "x", 3)
        .with_attr("k", "v")
        .with_verification(HseVerificationMethod::SelfDisclosed)
        .with_inferred(true);
    assert_eq!(ev.verification, Some(HseVerificationMethod::SelfDisclosed));
    assert!(ev.is_inferred);
    assert_eq!(ev.source, "s");
    assert_eq!(ev.summary, "x");
    assert_eq!(ev.recorded_at, 3);
    assert_eq!(ev.attributes["k"], "v");

    // Later calls overwrite, and inference can be cleared again.
    let ev = ev
        .with_verification(HseVerificationMethod::Unverified)
        .with_inferred(false);
    assert_eq!(ev.verification, Some(HseVerificationMethod::Unverified));
    assert!(!ev.is_inferred);
}

// ── HseEntity::corroborating_records ─────────────────────────────────────

#[test]
fn corroborating_records_keep_distinct_findings_and_drop_derived_sources() {
    let mut e = HseEntity::new(HseEntityKind::Email, "ada@example.com", 0.6, "scan", 0);
    e.add_evidence(HseEvidence::new("hibp", "breach A", 0));
    e.add_evidence(HseEvidence::new("hibp", "breach B", 0)); // same module, other finding
    e.add_evidence(HseEvidence::new("hibp", "breach A", 9)); // same identity again
    e.add_evidence(HseEvidence::new("github", "profile", 0));
    // Non-corroborating (derived / recalled) evidence never counts.
    e.add_evidence(HseEvidence::new(RECALL_SOURCE, "seen before", 0));
    e.add_evidence(HseEvidence::new(CONSENSUS_SOURCE, "consensus", 0));

    let records = e.corroborating_records();
    let mut sorted: Vec<(&str, &str)> = records.into_iter().collect();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            ("github", "profile"),
            ("hibp", "breach A"),
            ("hibp", "breach B")
        ]
    );
}

#[test]
fn corroborating_records_of_an_entity_without_evidence_is_empty() {
    let e = HseEntity::new(HseEntityKind::Domain, "example.com", 0.5, "scan", 0);
    assert!(e.corroborating_records().is_empty());
}

// ── InfrastructureExplanation::is_shared_infrastructure ──────────────────

#[test]
fn shared_infrastructure_is_exactly_the_common_service_explanations() {
    let shared: Vec<InfrastructureExplanation> = InfrastructureExplanation::ALL
        .into_iter()
        .filter(|explanation| explanation.is_shared_infrastructure())
        .collect();
    assert_eq!(
        shared,
        [
            InfrastructureExplanation::CommonCdn,
            InfrastructureExplanation::CommonHost,
            InfrastructureExplanation::CommonCms,
            InfrastructureExplanation::CommonRegistrar,
            InfrastructureExplanation::CommonTemplate,
            InfrastructureExplanation::SharedThirdPartyService,
        ]
    );
    // Control-style explanations, and the non-discriminating one, are not.
    for explanation in [
        InfrastructureExplanation::DirectTechnicalRelationship,
        InfrastructureExplanation::PossibleCommonAdministration,
        InfrastructureExplanation::Unknown,
    ] {
        assert!(!explanation.is_shared_infrastructure(), "{explanation:?}");
    }
}

// ── RequiredSemantics::checks_surface ────────────────────────────────────

#[test]
fn checks_surface_reports_exactly_the_required_surfaces() {
    let empty = RequiredSemantics::new("nothing").unwrap();
    assert!(!empty.checks_surface(VerificationSurface::Outputs));
    assert!(!empty.checks_surface(VerificationSurface::Inputs));

    let outputs = RequiredSemantics::new("outputs")
        .unwrap()
        .requires_surface(VerificationSurface::Outputs);
    assert!(outputs.checks_surface(VerificationSurface::Outputs));
    assert!(!outputs.checks_surface(VerificationSurface::State));
    assert!(!outputs.checks_surface(VerificationSurface::Inputs));
}

#[test]
fn checks_surface_agrees_with_surfaces_and_excludes_inputs_from_all_observables() {
    let all = RequiredSemantics::new("observables")
        .unwrap()
        .requires_all_observables();
    let listed: Vec<VerificationSurface> = all.surfaces().copied().collect();
    assert!(!listed.is_empty());
    for surface in &listed {
        assert!(all.checks_surface(*surface), "{surface:?}");
    }
    // "every observable surface except the input representation"
    assert!(!all.checks_surface(VerificationSurface::Inputs));
    assert!(!listed.contains(&VerificationSurface::Inputs));
}

// ── TemporalGeoGraph::edge_count ─────────────────────────────────────────

#[test]
fn edge_count_counts_every_accepted_edge_and_no_rejected_one() {
    let mut graph = TemporalGeoGraph::new();
    assert_eq!(graph.edge_count(), 0);
    graph.add_node("a", None).unwrap();
    graph.add_node("b", None).unwrap();
    graph
        .add_edge("a", "b", "corroborates", 1, Confidence::new(50))
        .unwrap();
    // Parallel edges are distinct observations, not deduplicated.
    graph
        .add_edge("a", "b", "corroborates", 2, Confidence::new(60))
        .unwrap();
    // Rejected edges (unknown endpoint, empty label) are not counted.
    assert!(
        graph
            .add_edge("a", "zz", "corroborates", 3, Confidence::new(50))
            .is_err()
    );
    assert!(
        graph
            .add_edge("a", "b", " ", 3, Confidence::new(50))
            .is_err()
    );
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(graph.edge_count(), graph.edges().len());
    assert_eq!(graph.node_count(), 2);
}
