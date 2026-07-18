use codeimpact_hexagon::analysis::economic_impact::compute_level;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::ReactiveAnalyzer;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::{
    CodeLocation, CodeMetrics, ComplexityWarning, FunctionDetail, ParsedFunction, WarningPattern,
    WarningSeverity,
};

// Test List:
// 1. trivial_complexity → economic impact near zero (AC5)
// 2. moderate_complexity → cpu cost > 0, memory > 0, total > 0
// 3. high_complexity → level is "high"
// 4. critical_complexity → level is "critical"
// 5. warnings_increase_cpu_cost → each warning adds 2 μ$
// 6. loops_increase_memory → functions with loops increase memory
// 7. hidden_complexity_increases_memory → transitive_hidden * 200 bytes
// 8. economic_impact_included_in_metrics → economic_impact field is Some after analyze
// 9. economic_impact_none_by_default → new CodeMetrics has economic_impact = None
// 10. level_thresholds_match_total_cost → levels match total cost ranges
// 11. known_defect_static_and_measured_level_scales_diverge_36_bug3 →
//     characterization test pinning the 100x scale mismatch (#36 bug 3,
//     tracked fix: #36/S3) — do not "fix" by editing this test

fn make_warning(function: &str, severity: WarningSeverity) -> ComplexityWarning {
    ComplexityWarning {
        pattern: WarningPattern::DeepConditional,
        severity,
        function: function.to_string(),
        location: CodeLocation::new(String::new(), 1, 1),
        message: "test warning".to_string(),
        suggestion: "test suggestion".to_string(),
    }
}

fn make_fn(name: &str, decision_points: u32, has_loop: bool, calls: Vec<&str>) -> ParsedFunction {
    ParsedFunction {
        name: name.to_string(),
        start_line: 1,
        calls: calls.into_iter().map(String::from).collect(),
        has_loop,
        has_nested_loop: false,
        decision_points,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }
}

// === 1. Trivial complexity → near-zero cost (AC5) ===
#[test]
fn trivial_complexity_near_zero_cost() {
    let metrics = CodeMetrics::new(1);
    let functions = vec![make_fn("main", 1, false, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let impact = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics, &functions, &graph,
    );
    assert!(impact.cpu_cost_microdollars() < 1.0);
    assert!(impact.memory_bytes() < 200);
    assert!(impact.total_cost_microdollars() < 1.0);
    assert_eq!(impact.level(), "low");
}

// === 2. Moderate complexity → positive costs ===
#[test]
fn moderate_complexity_positive_costs() {
    let metrics = {
        let mut m = CodeMetrics::with_call_graph(
            15,     // direct
            25,     // transitive
            3,      // max depth
            vec![], // no cycles
            vec![], // empty details
        );
        m = m.with_warnings(vec![make_warning("foo", WarningSeverity::Warning)]);
        m
    };
    let functions = vec![make_fn("foo", 15, true, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let impact = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics, &functions, &graph,
    );
    assert!(impact.cpu_cost_microdollars() > 0.0);
    assert!(impact.memory_bytes() > 0);
    assert!(impact.total_cost_microdollars() > 0.0);
}

// === 3. High complexity → level "high" ===
#[test]
fn high_complexity_high_level() {
    let metrics = CodeMetrics::new(30);
    let functions = vec![make_fn("main", 30, false, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let impact = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics, &functions, &graph,
    );
    assert_eq!(impact.level(), "high");
}

// === 4. Critical complexity → level "critical" ===
#[test]
fn critical_complexity_critical_level() {
    let metrics = CodeMetrics::new(50);
    let functions = vec![make_fn("main", 50, false, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let impact = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics, &functions, &graph,
    );
    assert_eq!(impact.level(), "critical");
}

// === 5. Warnings increase CPU cost ===
#[test]
fn warnings_increase_cpu_cost() {
    let metrics_no_warnings = CodeMetrics::new(5);
    let metrics_with_warnings = CodeMetrics::new(5).with_warnings(vec![
        make_warning("foo", WarningSeverity::Warning),
        make_warning("bar", WarningSeverity::Critical),
    ]);

    let functions = vec![make_fn("main", 5, false, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let graph2 = codeimpact_hexagon::analysis::CallGraph::build(&functions);

    let impact_no = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics_no_warnings,
        &functions,
        &graph,
    );
    let impact_with = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics_with_warnings,
        &functions,
        &graph2,
    );
    // 2 warnings × 2.0 μ$ = 4.0 μ$ more
    let diff = impact_with.cpu_cost_microdollars() - impact_no.cpu_cost_microdollars();
    assert!((diff - 4.0).abs() < 1e-9);
}

// === 6. Loops increase memory ===
#[test]
fn loops_increase_memory() {
    let no_loop_fns = vec![
        make_fn("a", 5, false, vec![]),
        make_fn("b", 3, false, vec![]),
    ];
    let with_loop_fns = vec![
        make_fn("a", 5, true, vec![]),
        make_fn("b", 3, false, vec![]),
    ];

    let metrics = CodeMetrics::new(8);
    let graph_no = codeimpact_hexagon::analysis::CallGraph::build(&no_loop_fns);
    let graph_with = codeimpact_hexagon::analysis::CallGraph::build(&with_loop_fns);

    let impact_no = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics,
        &no_loop_fns,
        &graph_no,
    );
    let impact_with = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics,
        &with_loop_fns,
        &graph_with,
    );
    // 1 loop function × 1024 bytes = 1024 more
    assert_eq!(impact_with.memory_bytes() - impact_no.memory_bytes(), 1024);
}

// === 7. Hidden complexity increases memory ===
#[test]
fn hidden_complexity_increases_memory() {
    // direct=5, transitive=20 → hidden=15 → 15*200 = 3000 bytes
    // hidden_complexity() sums per-function hidden (#46/#49, ADR-0012), so
    // the fixture carries the one function it represents instead of an
    // empty function_details (which would now correctly report hidden=0,
    // per ADR-0010, since nothing was measured).
    let metrics = {
        let mut m = CodeMetrics::with_call_graph(
            5,  // direct
            20, // transitive
            1,  // max depth
            vec![],
            vec![FunctionDetail::new(
                "main".to_string(),
                CodeLocation::new(String::new(), 1, 1),
                5,
                15,
                1,
                false,
            )],
        );
        m = m.with_warnings(vec![]);
        m
    };
    let functions = vec![make_fn("main", 5, false, vec![])];
    let graph = codeimpact_hexagon::analysis::CallGraph::build(&functions);
    let impact = codeimpact_hexagon::analysis::EconomicImpactEstimator::estimate(
        &metrics, &functions, &graph,
    );
    // hidden = 20 - 5 = 15 → 15 * 200 = 3000
    // direct * 100 = 5 * 100 = 500
    // loops = 0
    // total = 500 + 3000 + 0 = 3500
    assert_eq!(impact.memory_bytes(), 3500);
}

// === 8. Economic impact can be attached to CodeMetrics ===
#[test]
fn economic_impact_attached_to_metrics() {
    let impact = codeimpact_hexagon::analysis::EconomicImpact::new(5.0, 1000, 5.1, "low");
    let metrics = CodeMetrics::new(5).with_economic_impact(impact);
    assert!(metrics.economic_impact().is_some());
    let attached = metrics.economic_impact().unwrap();
    assert!((attached.cpu_cost_microdollars() - 5.0).abs() < 1e-9);
    assert_eq!(attached.memory_bytes(), 1000);
}

// === 9. Economic impact is None by default ===
#[test]
fn economic_impact_none_by_default() {
    let metrics = CodeMetrics::new(5);
    assert!(metrics.economic_impact().is_none());
}

// === 10. Level thresholds match total cost ranges ===
#[test]
fn level_thresholds_match_total_cost() {
    let low = codeimpact_hexagon::analysis::EconomicImpact::new(0.5, 100, 0.51, "low");
    assert_eq!(low.level(), "low");

    let moderate = codeimpact_hexagon::analysis::EconomicImpact::new(15.0, 1000, 15.1, "moderate");
    assert_eq!(moderate.level(), "moderate");

    let high = codeimpact_hexagon::analysis::EconomicImpact::new(30.0, 5000, 30.5, "high");
    assert_eq!(high.level(), "high");

    let critical = codeimpact_hexagon::analysis::EconomicImpact::new(50.0, 10000, 51.0, "critical");
    assert_eq!(critical.level(), "critical");
}

// === KNOWN DEFECT (characterization test, do NOT "fix" by editing this
// test — see #36 bug 3 / tracked for a real fix in #36/S3) ===
//
// `EconomicImpactEstimator` (static heuristic) and `ReactiveAnalyzer`
// (measured stress test) both produce an `EconomicImpact` with the same
// `level` field, but on two DIFFERENT threshold scales, 100x apart:
// - static:   low <= 10 μ$,   moderate <= 20 μ$,   high <= 40 μ$
// - measured: low <= 1000 μ$, moderate <= 10000 μ$, high <= 100000 μ$
// The exact same `total_cost_microdollars` value (15 μ$) is "moderate" on
// one scale and "low" on the other. This test pins that divergence, and
// pins that `impl Add` still lets you sum a static estimate with a
// measured run as if they were commensurable — arithmetic nonsense that
// compiles and ships today.
#[test]
fn known_defect_static_and_measured_level_scales_diverge_36_bug3() {
    // 15 μ$ is "moderate" on the static (complexity-heuristic) scale.
    let static_impact = EconomicImpact::new(15.0, 0, 15.0, compute_level(15.0));
    assert_eq!(static_impact.level(), "moderate");

    // The same 15 μ$ total, reached via a measured stress-test run, is
    // "low" on ReactiveAnalyzer's scale (its "low" ceiling is 1_000 μ$).
    // cpu_time_ms=540 -> 0.54 CPU-s * 27.7778 μ$/s ≈ 15 μ$, memory=0.
    let run = StressTestRun::new(
        1000,
        Measurement::Available(540),
        Measurement::Available(0),
        1,
        1,
        None,
    );
    let measured_impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    assert!((measured_impact.total_cost_microdollars() - 15.0).abs() < 0.1);
    assert_eq!(measured_impact.level(), "low");

    // Known defect: `Add` does not know the two impacts came from
    // different scales — it happily sums them into a value on neither scale.
    let mixed = static_impact.clone() + measured_impact.clone();
    assert!(
        (mixed.total_cost_microdollars()
            - (static_impact.total_cost_microdollars()
                + measured_impact.total_cost_microdollars()))
        .abs()
            < 1e-9
    );
}
