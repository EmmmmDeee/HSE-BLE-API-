//! Regression tests for metamorphic software advancement gates.

use bleradar_core::{
    AdvancementExecution, AdvancementFactors, AdvancementProposal, AdvancementRejection,
    AdvancementState, BenchmarkMetric, BenchmarkReport, DifferentialCase, ExecutionOutcome,
    FalsificationResult, MetamorphicRelation, MetamorphicTest, MetricDirection, RequiredSemantics,
    VerificationEngine, VerificationSurface,
};

fn verification_engine() -> VerificationEngine {
    let semantics = RequiredSemantics::new("normalization contract")
        .unwrap()
        .requires_surface(VerificationSurface::Outputs)
        .requires_relation(MetamorphicRelation::NormalizationEquivalence);
    let test = MetamorphicTest::new(
        "normalization-1",
        MetamorphicRelation::NormalizationEquivalence,
        vec![b"AA:BB".to_vec(), b"AABB".to_vec()],
    )
    .unwrap();
    let mut engine = VerificationEngine::new(semantics);
    engine.add_test(test).unwrap();
    engine
}

fn execution() -> AdvancementExecution {
    let benchmark = BenchmarkReport::new([BenchmarkMetric::new(
        "throughput",
        10,
        20,
        MetricDirection::HigherIsBetter,
    )
    .unwrap()])
    .unwrap();
    AdvancementExecution::new(
        [DifferentialCase::new("case-1", b"AA:BB".to_vec()).unwrap()],
        benchmark,
        FalsificationResult::resistant(),
        true,
    )
}

fn normalize(input: &[u8]) -> ExecutionOutcome {
    ExecutionOutcome::success(
        input
            .iter()
            .copied()
            .filter(|byte| *byte != b':')
            .collect::<Vec<_>>(),
    )
}

#[test]
fn priority_formula_orders_low_cost_high_value_changes() {
    let factors = AdvancementFactors::new(100, 90, 80, 70, 10, 20).unwrap();
    assert_eq!(factors.priority().numerator(), 50_400_000);
    assert_eq!(factors.priority().denominator(), 200);

    let lower = AdvancementFactors::new(100, 90, 80, 70, 20, 20).unwrap();
    assert!(factors.priority() > lower.priority());
    assert!(AdvancementFactors::new(100, 100, 100, 100, 0, 1).is_err());
}

#[test]
fn accepted_change_requires_all_gates_and_integrates_explicitly() {
    let factors = AdvancementFactors::new(90, 90, 80, 90, 10, 10).unwrap();
    let proposal = AdvancementProposal::new(
        "normalize-fast",
        "normalize identifiers before comparison",
        factors,
    )
    .unwrap()
    .with_dependencies(["verification"])
    .limited_by("input normalization");
    let mut engine =
        bleradar_core::MetamorphicSoftwareAdvancementEngine::new(verification_engine());
    engine.add_proposal(proposal).unwrap();

    let decision = engine
        .evaluate("normalize-fast", execution(), normalize, normalize)
        .unwrap();

    assert!(decision.accepted());
    assert_eq!(
        engine.state("normalize-fast"),
        Some(AdvancementState::Accepted)
    );
    let remaining = engine.integrate("normalize-fast").unwrap();
    assert!(remaining.is_empty());
    let run = engine.run("normalize-fast").unwrap();
    assert_eq!(
        run.phases(),
        &[
            bleradar_core::AdvancementPhase::Verify,
            bleradar_core::AdvancementPhase::DifferentialExecute,
            bleradar_core::AdvancementPhase::Falsify,
            bleradar_core::AdvancementPhase::Benchmark,
            bleradar_core::AdvancementPhase::AcceptOrReject,
            bleradar_core::AdvancementPhase::Integrate,
            bleradar_core::AdvancementPhase::Recompute,
        ]
    );
}

#[test]
fn semantic_or_differential_failure_rejects_even_with_a_benchmark_gain() {
    let factors = AdvancementFactors::new(90, 90, 80, 90, 10, 10).unwrap();
    let proposal =
        AdvancementProposal::new("unsafe-change", "change output semantics", factors).unwrap();
    let mut engine = bleradar_core::SoftwareAdvancementEngine::new(verification_engine());
    engine.add_proposal(proposal).unwrap();

    let decision = engine
        .evaluate("unsafe-change", execution(), normalize, |input| {
            ExecutionOutcome::success(input)
        })
        .unwrap();

    assert!(!decision.accepted());
    assert_eq!(
        engine.state("unsafe-change"),
        Some(AdvancementState::Rejected)
    );
    assert!(
        decision
            .rejection_reasons()
            .contains(&AdvancementRejection::CandidateSemanticsNotPreserved)
    );
    assert!(
        decision
            .rejection_reasons()
            .contains(&AdvancementRejection::DifferentialMismatch)
    );
}

#[test]
fn unexplained_regression_and_nonreproducibility_are_rejections() {
    let factors = AdvancementFactors::new(90, 90, 80, 90, 10, 10).unwrap();
    let proposal =
        AdvancementProposal::new("regression", "faster but less reliable", factors).unwrap();
    let mut engine = bleradar_core::SoftwareAdvancementEngine::new(verification_engine());
    engine.add_proposal(proposal).unwrap();
    let benchmark = BenchmarkReport::new([
        BenchmarkMetric::new("throughput", 10, 20, MetricDirection::HigherIsBetter).unwrap(),
        BenchmarkMetric::new("errors", 1, 3, MetricDirection::LowerIsBetter).unwrap(),
    ])
    .unwrap();
    let execution = AdvancementExecution::new(
        [DifferentialCase::new("case-1", b"AA:BB".to_vec()).unwrap()],
        benchmark,
        FalsificationResult::resistant(),
        false,
    );

    let decision = engine
        .evaluate("regression", execution, normalize, normalize)
        .unwrap();

    assert!(!decision.accepted());
    assert!(
        decision
            .rejection_reasons()
            .contains(&AdvancementRejection::UnexplainedMaterialRegression)
    );
    assert!(
        decision
            .rejection_reasons()
            .contains(&AdvancementRejection::NotReproducible)
    );
}
