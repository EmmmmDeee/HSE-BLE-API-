//! Integration tests for the defensive ATT&CK capability ledger.
//!
//! These tests assert that Verified status cannot be set by static data alone,
//! that evidence failure auto-downgrades derived status, and that Navigator
//! export is deterministic and derived-only.

use bleradar_core::{
    CapabilityEvidenceLinks, CapabilityLedger, CapabilityStatus, ClaimScope, derive_status,
};

/// Build a complete evidence-links fixture that satisfies `mandatory_complete`.
fn complete_links() -> CapabilityEvidenceLinks {
    CapabilityEvidenceLinks {
        source_ids: vec!["src-oracle-1".into()],
        input_ids: vec!["in-fixture-1".into()],
        execution_record_id: Some("exec-1".into()),
        output_ids: vec!["out-1".into()],
        provenance_claim_id: Some("claim-1".into()),
        corroboration_ids: vec!["corr-independent-1".into()],
        test_ids: vec!["test-capability-1".into()],
        benchmark_ids: vec![], // optional for v0
        regression_lock_ids: vec!["lock-1".into()],
        passed_test_ids: vec!["test-capability-1".into()],
        failed_test_ids: vec![],
        corroboration_ok: true,
        regression_ok: true,
    }
}

#[test]
fn seed_v0_has_zero_verified() {
    let ledger = CapabilityLedger::seed_v0();
    assert_eq!(ledger.verified_count(), 0);

    let in_scope = ["T1040", "T1016", "T1200", "T1195", "T1565"];
    for tid in in_scope {
        assert_eq!(
            ledger.status_of(tid),
            Some(CapabilityStatus::Unverified),
            "{tid} should start Unverified"
        );
    }

    let na = ["T1566", "T1059", "T1003", "T1021", "T1486"];
    for tid in na {
        assert_eq!(
            ledger.status_of(tid),
            Some(CapabilityStatus::NotApplicable),
            "{tid} should be NotApplicable"
        );
    }

    let counts = ledger.counts_by_status();
    assert_eq!(counts.get(&CapabilityStatus::Verified), Some(&0));
    assert_eq!(counts.get(&CapabilityStatus::Unverified), Some(&5));
    assert_eq!(counts.get(&CapabilityStatus::NotApplicable), Some(&5));
}

#[test]
fn manual_verified_without_evidence_is_impossible() {
    // No API exists to set CapabilityStatus::Verified on a row directly.
    // derive_status with empty links is Unverified.
    assert_eq!(
        derive_status(ClaimScope::InScope, &CapabilityEvidenceLinks::empty()),
        CapabilityStatus::Unverified
    );

    // Seed rows have empty links → never Verified.
    let ledger = CapabilityLedger::seed_v0();
    assert_eq!(ledger.verified_count(), 0);

    // Constructing Verified requires mandatory_complete.
    let incomplete = CapabilityEvidenceLinks {
        source_ids: vec!["s".into()],
        regression_ok: true,
        ..CapabilityEvidenceLinks::empty()
    };
    assert!(!incomplete.mandatory_complete());
    assert_ne!(
        derive_status(ClaimScope::InScope, &incomplete),
        CapabilityStatus::Verified
    );

    let full = complete_links();
    assert!(full.mandatory_complete());
    assert_eq!(
        derive_status(ClaimScope::InScope, &full),
        CapabilityStatus::Verified
    );
}

#[test]
fn full_evidence_chain_derives_verified() {
    let mut ledger = CapabilityLedger::seed_v0();
    ledger
        .set_links("T1040", complete_links())
        .expect("T1040 present");
    assert_eq!(ledger.status_of("T1040"), Some(CapabilityStatus::Verified));
    assert_eq!(ledger.verified_count(), 1);
}

#[test]
fn evidence_failure_auto_downgrades() {
    let mut ledger = CapabilityLedger::seed_v0();
    ledger
        .set_links("T1040", complete_links())
        .expect("T1040 present");
    assert_eq!(ledger.status_of("T1040"), Some(CapabilityStatus::Verified));

    ledger
        .invalidate_test("T1040", "test-capability-1")
        .expect("invalidate");
    // derive_status hard-fails any failed linked test to exactly Unverified.
    assert_eq!(
        ledger.status_of("T1040"),
        Some(CapabilityStatus::Unverified)
    );
    assert_eq!(ledger.verified_count(), 0);

    let layer = ledger.navigator_layer("test-layer", "enterprise-attack", "14.1");
    // Must not still show Verified color for T1040.
    assert!(
        !layer.contains("\"techniqueID\":\"T1040\",\"score\":100,\"color\":\"#31a354\""),
        "navigator must not export Verified color after downgrade"
    );
    assert!(
        layer.contains("\"techniqueID\":\"T1040\""),
        "T1040 still present in layer"
    );
    // Downgraded color must be exactly Unverified (#de2d26) given a failed test.
    assert!(
        layer.contains("\"techniqueID\":\"T1040\",\"score\":10,\"color\":\"#de2d26\""),
        "expected the Unverified navigator score and color for T1040"
    );
    assert!(
        !layer.contains("\"techniqueID\":\"T1040\",\"score\":50,"),
        "a failed test must not leave T1040 Partial"
    );
}

#[test]
fn not_applicable_never_counts_as_verified() {
    let mut ledger = CapabilityLedger::seed_v0();
    let before = ledger.verified_count();
    assert_eq!(before, 0);

    // Even with a full evidence chain, N/A scope stays NotApplicable.
    ledger
        .set_links("T1566", complete_links())
        .expect("T1566 present");
    assert_eq!(
        ledger.status_of("T1566"),
        Some(CapabilityStatus::NotApplicable)
    );
    assert_eq!(ledger.verified_count(), before);

    let layer = ledger.navigator_layer("na-check", "enterprise-attack", "14.1");
    assert!(layer.contains("\"techniqueID\":\"T1566\",\"score\":0,\"color\":\"#bdbdbd\""));
}

#[test]
fn navigator_deterministic_and_derived_only() {
    let ledger = CapabilityLedger::seed_v0();
    let a = ledger.navigator_layer("cap-v0", "enterprise-attack", "14.1");
    let b = ledger.navigator_layer("cap-v0", "enterprise-attack", "14.1");
    assert_eq!(a, b, "navigator export must be deterministic");

    assert!(a.contains("\"name\":\"cap-v0\""));
    assert!(a.contains("\"domain\":\"enterprise-attack\""));
    assert!(a.contains("\"attack\":\"14.1\""));
    assert!(a.contains("\"navigator\":\"4.9.1\""));
    assert!(a.contains("\"layer\":\"4.5\""));
    assert!(a.contains("\"techniques\":["));

    // Seed technique IDs present; scores match derive_status on empty links.
    for tid in [
        "T1003", "T1016", "T1021", "T1040", "T1059", "T1195", "T1200", "T1486", "T1565", "T1566",
    ] {
        assert!(
            a.contains(&format!("\"techniqueID\":\"{tid}\"")),
            "missing {tid}"
        );
    }

    // InScope empty → score 10 / Unverified color
    assert!(a.contains("\"techniqueID\":\"T1040\",\"score\":10,\"color\":\"#de2d26\""));
    // N/A → score 0
    assert!(a.contains("\"techniqueID\":\"T1566\",\"score\":0,\"color\":\"#bdbdbd\""));

    // Stable sort: T1003 before T1016 before T1040 (BTreeMap / techniqueID order)
    let i1003 = a.find("\"techniqueID\":\"T1003\"").expect("T1003");
    let i1016 = a.find("\"techniqueID\":\"T1016\"").expect("T1016");
    let i1040 = a.find("\"techniqueID\":\"T1040\"").expect("T1040");
    assert!(i1003 < i1016 && i1016 < i1040);

    // Write the layer for the change report to a unique, freshly created
    // temp file (never a fixed shared path), then clean it up.
    let path = unique_temp_path("capability-navigator-seed", "json");
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("create a new unique temp file");
        file.write_all(a.as_bytes()).expect("write navigator layer");
    }
    assert_eq!(
        std::fs::read_to_string(&path).expect("read navigator layer"),
        a
    );
    std::fs::remove_file(&path).expect("remove temp file");
}

/// A temp-dir path unique to this process and moment; opened with
/// `create_new`, so an existing file or symlink there is never followed.
fn unique_temp_path(stem: &str, extension: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("{stem}-{}-{nanos}.{extension}", std::process::id()))
}

#[test]
fn verified_requires_a_regression_lock() {
    // Every other link complete and healthy, but no regression lock.
    let unlocked = CapabilityEvidenceLinks {
        regression_lock_ids: vec![],
        ..complete_links()
    };
    assert!(!unlocked.mandatory_complete());
    assert_ne!(
        derive_status(ClaimScope::InScope, &unlocked),
        CapabilityStatus::Verified
    );
    assert_eq!(
        derive_status(ClaimScope::InScope, &unlocked),
        CapabilityStatus::Partial
    );

    let mut ledger = CapabilityLedger::seed_v0();
    ledger.set_links("T1040", unlocked).expect("T1040 present");
    assert_eq!(ledger.status_of("T1040"), Some(CapabilityStatus::Partial));
    assert_eq!(ledger.verified_count(), 0);

    // Adding the lock is what promotes the same chain to Verified.
    assert_eq!(
        derive_status(ClaimScope::InScope, &complete_links()),
        CapabilityStatus::Verified
    );
}

#[test]
fn partial_when_incomplete_links() {
    // Some mandatory fields present, regression healthy, but chain incomplete.
    let partial_links = CapabilityEvidenceLinks {
        source_ids: vec!["src-1".into()],
        input_ids: vec!["in-1".into()],
        execution_record_id: None,
        output_ids: vec![],
        provenance_claim_id: None,
        corroboration_ids: vec![],
        test_ids: vec![],
        benchmark_ids: vec![],
        regression_lock_ids: vec![],
        passed_test_ids: vec![],
        failed_test_ids: vec![],
        corroboration_ok: false,
        regression_ok: true, // avoid hard-failure path so Partial can surface
    };
    assert!(!partial_links.mandatory_complete());
    assert_eq!(
        derive_status(ClaimScope::InScope, &partial_links),
        CapabilityStatus::Partial
    );

    let mut ledger = CapabilityLedger::seed_v0();
    ledger
        .set_links("T1016", partial_links)
        .expect("T1016 present");
    assert_eq!(ledger.status_of("T1016"), Some(CapabilityStatus::Partial));
    assert_eq!(ledger.verified_count(), 0);

    let layer = ledger.navigator_layer("partial", "enterprise-attack", "14.1");
    assert!(layer.contains("\"techniqueID\":\"T1016\",\"score\":50,\"color\":\"#fec44f\""));
}
