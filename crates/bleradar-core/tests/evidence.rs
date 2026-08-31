//! Regression tests for immutable evidence provenance and calibrated fusion.

use bleradar_core::{Evidence, EvidenceQuality, Observation, Source, fuse_evidence};

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

    assert_eq!(observation.raw_value, " 36-32-62-36-31-33 ");
    assert_eq!(
        observation.normalized_value.as_deref(),
        Some("36:32:62:36:31:33")
    );
    assert_eq!(observation.derivation_history, ["canonical-mac-v1"]);
    assert_eq!(
        (observation.first_seen_ms, observation.last_seen_ms),
        (10, 30)
    );
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
