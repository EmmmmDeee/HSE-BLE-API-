//! Regression tests for metamorphic and differential verification.

use bleradar_core::{
    DifferentialCase, ExecutionOutcome, FailureCause, MetamorphicRelation, MetamorphicTest,
    RegressionLock, RepairRecord, RequiredSemantics, VerificationEngine, VerificationSurface,
};

fn semantics(relation: MetamorphicRelation) -> RequiredSemantics {
    RequiredSemantics::new("verification contract")
        .unwrap()
        .requires_all_observables()
        .requires_relation(relation)
}

#[test]
fn generated_normalization_case_passes_and_records_family_feedback() {
    let test = MetamorphicTest::generated(
        "normalize-1",
        MetamorphicRelation::NormalizationEquivalence,
        b"AA:BB".to_vec(),
        |input| Ok(input.iter().copied().filter(|byte| *byte != b':').collect()),
    )
    .unwrap();
    let mut engine =
        VerificationEngine::new(semantics(MetamorphicRelation::NormalizationEquivalence));
    engine.add_test(test).unwrap();

    let report = engine
        .verify(|input| {
            ExecutionOutcome::success(
                input
                    .iter()
                    .copied()
                    .filter(|byte| *byte != b':')
                    .map(|byte| byte.to_ascii_uppercase())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap();

    assert!(report.passed());
    assert_eq!(report.executed_tests(), 1);
    assert_eq!(report.passed_tests(), 1);
    assert_eq!(
        engine.family_pressure(MetamorphicRelation::NormalizationEquivalence),
        1
    );
}

#[test]
fn idempotence_failure_is_minimized_and_classified() {
    let test = MetamorphicTest::new(
        "idempotence-1",
        MetamorphicRelation::Idempotence,
        vec![b"raw".to_vec(), b"once".to_vec(), b"twice".to_vec()],
    )
    .unwrap();
    let mut engine = VerificationEngine::new(semantics(MetamorphicRelation::Idempotence));
    engine.add_test(test).unwrap();

    let report = engine
        .verify(|input| {
            if input == b"twice" {
                ExecutionOutcome::success(b"different".to_vec())
            } else {
                ExecutionOutcome::success(input)
            }
        })
        .unwrap();

    assert!(!report.passed());
    assert_eq!(report.violations().len(), 1);
    let violation = &report.violations()[0];
    assert_eq!(
        violation.root_cause(),
        FailureCause::ObservableDivergence(VerificationSurface::Outputs)
    );
    assert!(violation.minimized_input().is_some());
    assert_eq!(engine.family_pressure(MetamorphicRelation::Idempotence), 2);
}

#[test]
fn differential_report_keeps_required_side_effects_visible() {
    let required = RequiredSemantics::new("side effects")
        .unwrap()
        .requires_surface(VerificationSurface::Outputs)
        .requires_surface(VerificationSurface::SideEffects);
    let engine = VerificationEngine::new(required);
    let cases = vec![DifferentialCase::new("case-1", b"input".to_vec()).unwrap()];

    let report = engine.differential(
        cases,
        |_| ExecutionOutcome::success(b"same".to_vec()).with_side_effects(["write-a"]),
        |_| ExecutionOutcome::success(b"same".to_vec()).with_side_effects(["write-b"]),
    );

    assert!(!report.passed());
    assert_eq!(report.violations()[0].case_id(), "case-1");
    assert_eq!(
        report.violations()[0].root_cause(),
        FailureCause::ObservableDivergence(VerificationSurface::SideEffects)
    );
}

#[test]
fn repairs_locks_and_retirement_are_explicit_state_transitions() {
    let relation = MetamorphicRelation::PermutationEquivalence;
    let test = MetamorphicTest::new(
        "permutation-1",
        relation,
        vec![b"abc".to_vec(), b"cba".to_vec()],
    )
    .unwrap();
    let mut engine = VerificationEngine::new(semantics(relation));
    engine.add_test(test.clone()).unwrap();
    engine
        .record_repair(
            RepairRecord::new("permutation-1", "canonicalize before comparison").unwrap(),
        )
        .unwrap();
    engine
        .add_regression_lock(
            RegressionLock::new(
                "lock-1",
                "permutation-1",
                relation,
                test.inputs().to_owned(),
                "preserve permutation behavior",
            )
            .unwrap(),
        )
        .unwrap();
    engine.retire_test("permutation-1").unwrap();

    let report = engine
        .verify(|input| ExecutionOutcome::success(input))
        .unwrap();

    assert_eq!(report.executed_tests(), 0);
    assert_eq!(report.retired_tests(), &["permutation-1".to_owned()]);
    assert_eq!(report.regression_locks(), &["lock-1".to_owned()]);
    assert_eq!(report.repairs()[0].test_id(), "permutation-1");
}
