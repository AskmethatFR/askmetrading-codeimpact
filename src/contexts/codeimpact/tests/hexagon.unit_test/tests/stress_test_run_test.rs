use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::ReactiveAnalyzer;
use codeimpact_hexagon::analysis::RunStressTest;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;
use codeimpact_secondaries::gateways::test_runners::test_runner_stub::TestRunnerStub;

// Test List:
// 1. stress_test_run_constructs
// 2. stress_test_run_passed_never_exceeds_total
// 3. stress_test_run_duration_can_be_zero (a genuinely instant run is not "unmeasured")
// 4. stress_test_run_reports_unmeasurable_cpu_and_memory (#36)
// 5. measurement_available_returns_none_for_unmeasurable (#36 retry N1)
// 6. reactive_analyzer_converts_run_to_economic_impact
// 7. reactive_analyzer_high_cpu_time_gives_moderate_level
// 8. reactive_analyzer_zero_cpu_time_gives_low_level
// 9. reactive_analyzer_high_memory_gives_high_level
// 10. reactive_analyzer_extreme_memory_gives_critical_level
// 11. run_stress_test_invokes_runner_and_writer
// 12. run_stress_test_with_filter
// 13. run_stress_test_propagates_runner_error

fn available(value: u64) -> Measurement<u64> {
    Measurement::Available(value)
}

fn make_run() -> StressTestRun {
    StressTestRun::new(1500, available(1200), available(8192), 42, 50, None)
}

fn make_run_with_filter() -> StressTestRun {
    StressTestRun::new(
        500,
        available(400),
        available(2048),
        10,
        10,
        Some("test_foo".into()),
    )
}

#[test]
fn stress_test_run_constructs() {
    let run = make_run();
    assert_eq!(run.duration_ms(), 1500);
    assert_eq!(run.cpu_time_ms(), available(1200));
    assert_eq!(run.memory_kb(), available(8192));
    assert_eq!(run.tests_passed(), 42);
    assert_eq!(run.tests_total(), 50);
    assert_eq!(run.filter(), None);
}

#[test]
fn stress_test_run_passed_never_exceeds_total() {
    let run = StressTestRun::new(100, available(50), available(1024), 55, 50, None);
    assert_eq!(run.tests_passed(), 50);
}

#[test]
fn stress_test_run_duration_can_be_zero() {
    // A run that legitimately completes in under a millisecond is not the
    // same thing as an unmeasured run — it must report the real 0, not a
    // fudged 1 (#36 bonus smell: silently inflating a measured value is the
    // same disease as defaulting an unmeasured one to 0).
    let run = StressTestRun::new(0, available(0), available(0), 0, 0, None);
    assert_eq!(run.duration_ms(), 0);
}

#[test]
fn stress_test_run_reports_unmeasurable_cpu_and_memory() {
    let run = StressTestRun::new(
        10,
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        1,
        1,
        None,
    );
    assert_eq!(
        run.cpu_time_ms(),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
    );
    assert_eq!(
        run.memory_kb(),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
    );
}

#[test]
fn measurement_available_returns_none_for_unmeasurable() {
    assert_eq!(
        Measurement::<u64>::Unmeasurable(UnmeasurableReason::NoSampler).available(),
        None
    );
}

#[test]
fn reactive_analyzer_converts_run_to_economic_impact() {
    let run = make_run();
    let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    // 1200 ms CPU = 1.2 CPU-seconds
    // ~$0.10/CPU-heure = $0.10/3600 CPU-seconds = 27.78 μ$/CPU-second
    // 1.2 * 27.78 ≈ 33.33 μ$
    let expected_cpu = 1200.0 / 1000.0 * ReactiveAnalyzer::MICRODOLLARS_PER_CPU_SECOND;
    assert!((impact.cpu_cost_microdollars() - expected_cpu).abs() < 0.1);
    // memory: 8192 KB = 8 MB
    assert_eq!(impact.memory_bytes(), 8192 * 1024);
    assert!(impact.total_cost_microdollars() > 0.0);
}

#[test]
fn reactive_analyzer_high_cpu_time_gives_moderate_level() {
    let run = StressTestRun::new(60000, available(50000), available(1024), 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    // 50s CPU = 1389 μ$ + 1 MB mem = 105 μ$ → total ≈ 1494 μ$ → moderate
    assert_eq!(impact.level(), "moderate");
}

#[test]
fn reactive_analyzer_zero_cpu_time_gives_low_level() {
    let run = StressTestRun::new(10, available(0), available(0), 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    assert_eq!(impact.level(), "low");
}

#[test]
fn reactive_analyzer_high_memory_gives_high_level() {
    // 500 MB memory → 500*1024*1024*0.0001 = 52428 μ$ → high
    let run = StressTestRun::new(1000, available(1000), available(512_000), 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    assert_eq!(impact.level(), "high");
}

#[test]
fn reactive_analyzer_extreme_memory_gives_critical_level() {
    // 10 GB memory → 10*1024*1024*1024*0.0001 = 1_073_741 μ$ → critical
    let run = StressTestRun::new(1000, available(1000), available(10_485_760), 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
    assert_eq!(impact.level(), "critical");
}

#[test]
fn run_stress_test_invokes_runner_and_writer() {
    let runner = TestRunnerStub::new(Ok(make_run()));
    let writer = SharedReportWriterStub::new();
    let use_case = RunStressTest::new(Box::new(runner), Box::new(writer.clone()));
    use_case.handle(None).expect("stress test should succeed");
    let captured = writer.last_stress_run.lock().unwrap();
    assert!(captured.is_some());
    let run = captured.as_ref().unwrap();
    assert_eq!(run.duration_ms(), 1500);
}

#[test]
fn run_stress_test_with_filter() {
    let runner = TestRunnerStub::new(Ok(make_run_with_filter()));
    let writer = SharedReportWriterStub::new();
    let use_case = RunStressTest::new(Box::new(runner), Box::new(writer.clone()));
    use_case
        .handle(Some("test_foo"))
        .expect("stress test should succeed");
    let captured = writer.last_stress_run.lock().unwrap();
    let run = captured.as_ref().unwrap();
    assert_eq!(run.filter(), Some("test_foo".to_string()));
}

// Test List (StressTestRun::aggregate — #39 aggregation law, folds N
// per-binary runs from a --workspace build into one):
// 5. sums tests_passed and tests_total
// 6. sums durations (binaries run sequentially -> wall clock actually spent)
// 7. sums cpu_time_ms when every run is Available
// 8. takes peak memory_kb, not the sum (processes never coexist)
// 9. cpu_time_ms is Unmeasurable if ANY run is Unmeasurable (ADR-0010)
// 10. memory_kb is Unmeasurable if ANY run is Unmeasurable (ADR-0010)
// 11. preserves the filter
// 12. aggregate of no runs is an error, not a synthesized all-zero run

#[test]
fn aggregate_sums_tests_passed_and_total() {
    let run_a = StressTestRun::new(100, available(10), available(1024), 2, 2, None);
    let run_b = StressTestRun::new(100, available(10), available(1024), 1, 3, None);

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(aggregated.tests_passed(), 3);
    assert_eq!(aggregated.tests_total(), 5);
}

#[test]
fn aggregate_sums_durations() {
    let run_a = StressTestRun::new(100, available(10), available(1024), 1, 1, None);
    let run_b = StressTestRun::new(250, available(10), available(1024), 1, 1, None);

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(aggregated.duration_ms(), 350);
}

#[test]
fn aggregate_sums_cpu_time_when_every_run_is_available() {
    let run_a = StressTestRun::new(100, available(100), available(1024), 1, 1, None);
    let run_b = StressTestRun::new(100, available(200), available(1024), 1, 1, None);

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(aggregated.cpu_time_ms(), available(300));
}

#[test]
fn aggregate_takes_peak_memory_not_the_sum() {
    let run_a = StressTestRun::new(100, available(10), available(8192), 1, 1, None);
    let run_b = StressTestRun::new(100, available(10), available(2048), 1, 1, None);

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(aggregated.memory_kb(), available(8192));
}

#[test]
fn aggregate_cpu_time_is_unmeasurable_when_any_run_is_unmeasurable() {
    let run_a = StressTestRun::new(100, available(100), available(1024), 1, 1, None);
    let run_b = StressTestRun::new(
        100,
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        available(1024),
        1,
        1,
        None,
    );

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(
        aggregated.cpu_time_ms(),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
    );
}

#[test]
fn aggregate_memory_is_unmeasurable_when_any_run_is_unmeasurable() {
    let run_a = StressTestRun::new(100, available(100), available(1024), 1, 1, None);
    let run_b = StressTestRun::new(
        100,
        available(100),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        1,
        1,
        None,
    );

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(
        aggregated.memory_kb(),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
    );
}

#[test]
fn aggregate_preserves_the_filter() {
    let run_a = StressTestRun::new(
        100,
        available(10),
        available(1024),
        1,
        1,
        Some("test_foo".into()),
    );
    let run_b = StressTestRun::new(
        100,
        available(10),
        available(1024),
        1,
        1,
        Some("test_foo".into()),
    );

    let aggregated = StressTestRun::aggregate(&[run_a, run_b]).expect("aggregate should succeed");

    assert_eq!(aggregated.filter(), Some("test_foo".to_string()));
}

#[test]
fn aggregate_of_no_runs_is_an_error() {
    let result = StressTestRun::aggregate(&[]);

    match result {
        Err(AnalysisError::TestRunnerError(_)) => {}
        other => panic!("expected TestRunnerError, got {:?}", other),
    }
}

#[test]
fn run_stress_test_propagates_runner_error() {
    let runner = TestRunnerStub::new(Err(AnalysisError::TestRunnerError(
        "cargo test failed".into(),
    )));
    let writer = SharedReportWriterStub::new();
    let use_case = RunStressTest::new(Box::new(runner), Box::new(writer.clone()));
    let result = use_case.handle(None);
    assert!(result.is_err());
    match result.unwrap_err() {
        AnalysisError::TestRunnerError(msg) => assert_eq!(msg, "cargo test failed"),
        _ => panic!("expected TestRunnerError"),
    }
}
