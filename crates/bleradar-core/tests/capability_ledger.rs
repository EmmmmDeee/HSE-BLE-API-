//! Integration tests for the defensive ATT&CK capability ledger.
//!
//! These tests assert that Verified status cannot be set by static data alone,
//! that evidence failure auto-downgrades derived status, that corroboration and
//! regression health is derived from the linked records (never a hand-set
//! flag), that blank or whitespace ids count as missing, and that Navigator
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
        failed_test_ids: vec![], // no failed test, so Partial can surface
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

#[test]
fn hand_set_flags_without_ids_are_not_verified() {
    // The flags no longer exist as fields (a struct literal setting them does
    // not compile: see the compile_fail doctests on CapabilityEvidenceLinks).
    // Every non-id part of the chain is healthy and every test passed, but
    // no corroboration or regression-lock record is linked.
    let unlinked = CapabilityEvidenceLinks {
        corroboration_ids: vec![],
        regression_lock_ids: vec![],
        ..complete_links()
    };
    assert!(!unlinked.corroboration_ok());
    assert!(!unlinked.regression_ok());
    assert!(!unlinked.mandatory_complete());
    assert_eq!(
        derive_status(ClaimScope::InScope, &unlinked),
        CapabilityStatus::Partial
    );

    // With no ids at all nothing is linked: Unverified, never Verified.
    let empty = CapabilityEvidenceLinks::empty();
    assert!(!empty.corroboration_ok());
    assert!(!empty.regression_ok());
    assert_eq!(
        derive_status(ClaimScope::InScope, &empty),
        CapabilityStatus::Unverified
    );

    let mut ledger = CapabilityLedger::seed_v0();
    ledger.set_links("T1040", unlinked).expect("T1040 present");
    assert_ne!(ledger.status_of("T1040"), Some(CapabilityStatus::Verified));
    assert_eq!(ledger.verified_count(), 0);
}

#[test]
fn whitespace_ids_count_as_missing() {
    let blank = || vec![String::new(), " ".to_string(), "\t\n".to_string()];

    // Each mandatory list replaced by blank ids alone breaks the chain.
    type SetIds = fn(&mut CapabilityEvidenceLinks, Vec<String>);
    let cases: [(&str, SetIds); 6] = [
        ("source_ids", |l, v| l.source_ids = v),
        ("input_ids", |l, v| l.input_ids = v),
        ("output_ids", |l, v| l.output_ids = v),
        ("corroboration_ids", |l, v| l.corroboration_ids = v),
        ("test_ids", |l, v| l.test_ids = v),
        ("regression_lock_ids", |l, v| l.regression_lock_ids = v),
    ];
    for (field, set) in cases {
        let mut links = complete_links();
        set(&mut links, blank());
        assert!(!links.mandatory_complete(), "blank {field} must be missing");
        assert_ne!(
            derive_status(ClaimScope::InScope, &links),
            CapabilityStatus::Verified,
            "blank {field} must not be Verified"
        );
        let mut ledger = CapabilityLedger::seed_v0();
        ledger.set_links("T1040", links).expect("T1040 present");
        assert_ne!(ledger.status_of("T1040"), Some(CapabilityStatus::Verified));
        assert_eq!(ledger.verified_count(), 0, "blank {field}");
    }

    // Blank single-record ids are missing too.
    for links in [
        CapabilityEvidenceLinks {
            execution_record_id: Some("  ".into()),
            ..complete_links()
        },
        CapabilityEvidenceLinks {
            provenance_claim_id: Some(String::new()),
            ..complete_links()
        },
    ] {
        assert!(!links.mandatory_complete());
        assert_eq!(
            derive_status(ClaimScope::InScope, &links),
            CapabilityStatus::Partial
        );
    }

    // Only-blank ids anywhere are no evidence at all: Unverified, not Partial.
    let all_blank = CapabilityEvidenceLinks {
        source_ids: blank(),
        input_ids: blank(),
        execution_record_id: Some(" ".into()),
        output_ids: blank(),
        provenance_claim_id: Some(" ".into()),
        corroboration_ids: blank(),
        test_ids: blank(),
        benchmark_ids: blank(),
        regression_lock_ids: blank(),
        passed_test_ids: blank(),
        failed_test_ids: blank(),
    };
    assert!(!all_blank.any_mandatory_partial());
    assert!(!all_blank.corroboration_ok());
    assert!(!all_blank.regression_ok());
    assert_eq!(
        derive_status(ClaimScope::InScope, &all_blank),
        CapabilityStatus::Unverified
    );

    // A blank failed-test id is no failure: the chain stays Verified.
    let blank_failure = CapabilityEvidenceLinks {
        failed_test_ids: vec![" ".into()],
        ..complete_links()
    };
    assert_eq!(
        derive_status(ClaimScope::InScope, &blank_failure),
        CapabilityStatus::Verified
    );

    // set_links trims ids and drops blank ones; padded ids still match.
    let padded = CapabilityEvidenceLinks {
        source_ids: vec![" src-oracle-1 ".into(), "  ".into()],
        execution_record_id: Some("  ".into()),
        test_ids: vec!["test-capability-1".into()],
        passed_test_ids: vec!["\ttest-capability-1 ".into()],
        failed_test_ids: vec![String::new()],
        ..complete_links()
    };
    let mut ledger = CapabilityLedger::seed_v0();
    ledger.set_links("T1040", padded).expect("T1040 present");
    let stored = &ledger.get("T1040").expect("T1040 row").links;
    assert_eq!(stored.source_ids, vec!["src-oracle-1".to_string()]);
    assert_eq!(stored.execution_record_id, None);
    assert_eq!(
        stored.passed_test_ids,
        vec!["test-capability-1".to_string()]
    );
    assert!(stored.failed_test_ids.is_empty());
    // The blank execution record left the chain incomplete.
    assert_eq!(ledger.status_of("T1040"), Some(CapabilityStatus::Partial));

    // A blank test id passed to invalidate_test names no test.
    ledger.set_links("T1040", complete_links()).expect("T1040");
    ledger.invalidate_test("T1040", "   ").expect("invalidate");
    assert!(
        ledger
            .get("T1040")
            .expect("row")
            .links
            .failed_test_ids
            .is_empty()
    );
    assert_eq!(ledger.status_of("T1040"), Some(CapabilityStatus::Verified));
    // A padded one is trimmed to the linked id it names.
    ledger
        .invalidate_test("T1040", " test-capability-1 ")
        .expect("invalidate");
    let links = &ledger.get("T1040").expect("row").links;
    assert_eq!(links.failed_test_ids, vec!["test-capability-1".to_string()]);
    assert!(links.passed_test_ids.is_empty());
    assert_eq!(
        ledger.status_of("T1040"),
        Some(CapabilityStatus::Unverified)
    );
}

#[test]
fn derived_flags_match_linked_records() {
    // corroboration_ok follows corroboration_ids alone.
    for (ids, expected) in [
        (vec![], false),
        (vec![" ".to_string()], false),
        (vec!["corr-1".to_string()], true),
        (vec![" ".to_string(), "corr-1".to_string()], true),
    ] {
        let links = CapabilityEvidenceLinks {
            corroboration_ids: ids.clone(),
            ..CapabilityEvidenceLinks::empty()
        };
        assert_eq!(links.corroboration_ok(), expected, "{ids:?}");
    }

    // regression_ok needs a linked lock and no failed linked test.
    for (locks, failed, expected) in [
        (vec![], vec![], false),
        (vec![" ".to_string()], vec![], false),
        (vec!["lock-1".to_string()], vec![], true),
        (vec!["lock-1".to_string()], vec!["  ".to_string()], true),
        (vec!["lock-1".to_string()], vec!["t-1".to_string()], false),
        (vec![], vec!["t-1".to_string()], false),
    ] {
        let links = CapabilityEvidenceLinks {
            regression_lock_ids: locks.clone(),
            failed_test_ids: failed.clone(),
            ..CapabilityEvidenceLinks::empty()
        };
        assert_eq!(links.regression_ok(), expected, "{locks:?} / {failed:?}");
    }

    // On the full chain both hold; each flips with its own records.
    let full = complete_links();
    assert!(full.corroboration_ok() && full.regression_ok());

    let mut ledger = CapabilityLedger::seed_v0();
    ledger.set_links("T1040", complete_links()).expect("T1040");
    ledger
        .invalidate_test("T1040", "test-capability-1")
        .expect("fail");
    let failed = &ledger.get("T1040").expect("row").links;
    assert!(failed.corroboration_ok());
    assert!(
        !failed.regression_ok(),
        "a failed linked test breaks the regression"
    );
    assert_eq!(
        ledger.status_of("T1040"),
        Some(CapabilityStatus::Unverified)
    );

    // Status agrees with the derived flags on every seeded row, and the
    // export carries status only — no stored flag reaches it.
    for (tid, row) in ledger.iter() {
        let status = ledger.status_of(tid).expect("row status");
        if status == CapabilityStatus::Verified {
            assert!(row.links.corroboration_ok() && row.links.regression_ok());
        }
    }
    let layer = ledger.navigator_layer("flags", "enterprise-attack", "14.1");
    assert!(!layer.contains("corroboration_ok") && !layer.contains("regression_ok"));
}
