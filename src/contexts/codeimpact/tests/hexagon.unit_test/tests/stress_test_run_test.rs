use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::ReactiveAnalyzer;
use codeimpact_hexagon::analysis::RunStressTest;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;
use codeimpact_secondaries::gateways::test_runners::test_runner_stub::TestRunnerStub;

// Test List:
// 1. stress_test_run_constructs
// 2. stress_test_run_passed_never_exceeds_total
// 3. stress_test_run_duration_must_be_positive
// 4. reactive_analyzer_converts_run_to_economic_impact
// 5. reactive_analyzer_high_cpu_time_gives_moderate_level
// 6. reactive_analyzer_zero_cpu_time_gives_low_level
// 7. reactive_analyzer_high_memory_gives_high_level
// 8. reactive_analyzer_extreme_memory_gives_critical_level
// 9. run_stress_test_invokes_runner_and_writer
// 10. run_stress_test_with_filter
// 11. run_stress_test_propagates_runner_error

fn make_run() -> StressTestRun {
    StressTestRun::new(1500, 1200, 8192, 42, 50, None)
}

fn make_run_with_filter() -> StressTestRun {
    StressTestRun::new(500, 400, 2048, 10, 10, Some("test_foo".into()))
}

#[test]
fn stress_test_run_constructs() {
    let run = make_run();
    assert_eq!(run.duration_ms(), 1500);
    assert_eq!(run.cpu_time_ms(), 1200);
    assert_eq!(run.memory_kb(), 8192);
    assert_eq!(run.tests_passed(), 42);
    assert_eq!(run.tests_total(), 50);
    assert_eq!(run.filter(), None);
}

#[test]
fn stress_test_run_passed_never_exceeds_total() {
    let run = StressTestRun::new(100, 50, 1024, 55, 50, None);
    assert_eq!(run.tests_passed(), 50);
}

#[test]
fn stress_test_run_duration_must_be_positive() {
    let run = StressTestRun::new(0, 0, 0, 0, 0, None);
    assert_eq!(run.duration_ms(), 1);
}

#[test]
fn reactive_analyzer_converts_run_to_economic_impact() {
    let run = make_run();
    let impact = ReactiveAnalyzer::analyze(&run);
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
    let run = StressTestRun::new(60000, 50000, 1024, 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run);
    // 50s CPU = 1389 μ$ + 1 MB mem = 105 μ$ → total ≈ 1494 μ$ → moderate
    assert_eq!(impact.level(), "moderate");
}

#[test]
fn reactive_analyzer_zero_cpu_time_gives_low_level() {
    let run = StressTestRun::new(10, 0, 0, 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run);
    assert_eq!(impact.level(), "low");
}

#[test]
fn reactive_analyzer_high_memory_gives_high_level() {
    // 500 MB memory → 500*1024*1024*0.0001 = 52428 μ$ → high
    let run = StressTestRun::new(1000, 1000, 512_000, 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run);
    assert_eq!(impact.level(), "high");
}

#[test]
fn reactive_analyzer_extreme_memory_gives_critical_level() {
    // 10 GB memory → 10*1024*1024*1024*0.0001 = 1_073_741 μ$ → critical
    let run = StressTestRun::new(1000, 1000, 10_485_760, 1, 1, None);
    let impact = ReactiveAnalyzer::analyze(&run);
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
    use_case.handle(Some("test_foo")).expect("stress test should succeed");
    let captured = writer.last_stress_run.lock().unwrap();
    let run = captured.as_ref().unwrap();
    assert_eq!(run.filter(), Some("test_foo".to_string()));
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