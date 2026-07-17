use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FileDependency;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::UnmeasurableFile;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_hexagon::analysis::WarningPattern;
use codeimpact_hexagon::analysis::WarningSeverity;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use std::path::PathBuf;

#[test]
fn write_console_does_not_panic() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_zero_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(0);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_high_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(50);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_economic_impact() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(18.5, 12600, 19.7, "moderate");
    let metrics = CodeMetrics::new(27).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_memory_mb() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(50.0, 2_000_000, 50.2, "high");
    let metrics = CodeMetrics::new(30).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_ecological_impact() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(6000.0, 0, 6000.0, "low");
    let ecological = EcologicalImpact::new(2.4, 21600.0, EfficiencyClass::B);
    let metrics = CodeMetrics::new(27)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_ecological_zero_co2() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(0.0, 0, 0.0, "low");
    let ecological = EcologicalImpact::new(0.0, 0.0, EfficiencyClass::A);
    let metrics = CodeMetrics::new(1)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn write_project_report_with_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::new(5)
                .with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low"))
                .with_ecological_impact(EcologicalImpact::new(1.0, 9000.0, EfficiencyClass::B)),
        ),
        (
            path("b.rs"),
            CodeMetrics::new(3)
                .with_economic_impact(EconomicImpact::new(20.0, 200, 21.0, "high"))
                .with_ecological_impact(EcologicalImpact::new(2.0, 18000.0, EfficiencyClass::D)),
        ),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

#[test]
fn write_project_report_without_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.rs"), CodeMetrics::new(5)),
        (path("b.rs"), CodeMetrics::new(3)),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_io_in_loops() {
    let writer = ConsoleReportWriter::new();
    let warnings = vec![
        IoInLoopWarning {
            function: "read_file".to_string(),
            io_call: "std::fs::read".to_string(),
            location: CodeLocation::new("".into(), 5, 9),
        },
        IoInLoopWarning {
            function: "write_data".to_string(),
            io_call: "std::fs::write".to_string(),
            location: CodeLocation::new("".into(), 10, 5),
        },
    ];
    let metrics = CodeMetrics::new(5).with_io_in_loops(warnings);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

// #56 T2 — abstention (ADR-0010/ADR-0014 §4): a synthesis line, never
// per-line detail — abstention must not become a pseudo-warning.
#[test]
fn write_console_shows_unclassifiable_io_in_loops_count() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5).with_unclassifiable_io_in_loops_count(2);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Appels en boucle non classifiables: 2"),
        "expected the unclassifiable synthesis line, got: {}",
        output
    );
}

#[test]
fn write_console_shows_pattern_name() {
    let writer = ConsoleReportWriter::new();
    let warning = ComplexityWarning {
        pattern: WarningPattern::QuadraticLoop,
        severity: WarningSeverity::Critical,
        function: "process_data".to_string(),
        location: CodeLocation::new("src/lib.rs".into(), 42, 1),
        message: "boucle quadratique détectée".to_string(),
        suggestion: "utiliser un HashMap".to_string(),
    };
    let metrics = CodeMetrics::new(5).with_warnings(vec![warning]);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("[CRITICAL][QuadraticLoop]"),
        "expected [CRITICAL][QuadraticLoop] in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_per_file_warnings() {
    let writer = ConsoleReportWriter::new();
    let warning = ComplexityWarning {
        pattern: WarningPattern::NestedLoops,
        severity: WarningSeverity::Warning,
        function: "search".to_string(),
        location: CodeLocation::new("src/search.rs".into(), 15, 1),
        message: "boucles imbriquées".to_string(),
        suggestion: "extraire la logique".to_string(),
    };
    let metrics = CodeMetrics::new(5).with_warnings(vec![warning]);
    let files = vec![(path("src/search.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("NestedLoops"),
        "expected NestedLoops in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_per_file_io_in_loops() {
    let writer = ConsoleReportWriter::new();
    let io_warning = IoInLoopWarning {
        function: "read_file".to_string(),
        io_call: "std::fs::read".to_string(),
        location: CodeLocation::new("src/reader.rs".into(), 10, 5),
    };
    let metrics = CodeMetrics::new(5).with_io_in_loops(vec![io_warning]);
    let files = vec![(path("src/reader.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("I/O dans boucle"),
        "expected I/O warning in output, got: {}",
        output
    );
}

#[test]
fn write_project_report_shows_unclassifiable_io_in_loops_total() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::new(5).with_unclassifiable_io_in_loops_count(2),
        ),
        (
            path("b.rs"),
            CodeMetrics::new(3).with_unclassifiable_io_in_loops_count(1),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        output.contains("Appels en boucle non classifiables (total): 3"),
        "expected the aggregate unclassifiable synthesis line, got: {}",
        output
    );
}

// #36 — the central acceptance criterion for the whole ticket: the tool
// must never render `0` for a metric it could not measure. `0` reads as
// "free", which is a lie.
#[test]
fn write_stress_test_shows_na_not_zero_when_unmeasurable() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
        1,
        1,
        None,
    );
    let impact = Measurement::Unmeasurable(UnmeasurableReason::NoSampler);
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("Temps CPU: 0 ms") && !output.contains("Mémoire: 0.0 MB"),
        "must never render a bare 0 for an unmeasured metric, got: {}",
        output
    );
    assert!(
        output.contains("Temps CPU: n/a") && output.contains("Mémoire: n/a"),
        "expected n/a for unmeasured metrics, got: {}",
        output
    );
}

#[test]
fn write_stress_test_shows_real_numbers_when_measured() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Available(1200),
        Measurement::Available(8192),
        42,
        50,
        None,
    );
    let impact = Measurement::Available(EconomicImpact::new(33.3, 8192 * 1024, 34.1, "low"));
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(output.contains("Temps CPU: 1200 ms"), "got: {}", output);
    assert!(!output.contains("n/a"), "got: {}", output);
}

// #39 — a 0-test run must render the reason, never a confident cost
// figure. This is the console-writer mirror of
// reactive_analyzer_zero_tests_yields_unmeasurable_no_tests_executed:
// the writer already renders Unmeasurable(reason) as "n/a (reason)" for
// every field (#36 machinery), so it needs zero code changes once the
// hexagon returns NoTestsExecuted — this test proves that.
#[test]
fn write_stress_test_shows_no_tests_executed_instead_of_a_cost() {
    let writer = ConsoleReportWriter::new();
    let run = StressTestRun::new(
        1500,
        Measurement::Available(1200),
        Measurement::Available(8192),
        0,
        0,
        None,
    );
    let impact = Measurement::Unmeasurable(UnmeasurableReason::NoTestsExecuted);
    let mut buf = Vec::new();
    writer.write_stress_test_to(&mut buf, &run, &impact);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("aucun test exécuté"),
        "expected the no-tests-executed reason in output, got: {}",
        output
    );
    assert!(
        !output.contains("Coût total: $") && !output.contains("Coût total: 0"),
        "must never render a confident cost figure for a 0-test run, got: {}",
        output
    );
}

#[test]
fn write_project_report_no_warnings_does_not_show_section() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5); // no warnings, no io_in_loops
    let files = vec![(path("src/clean.rs"), metrics)];
    let deps = vec![];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();
    assert!(
        !output.contains("avertissements:"),
        "should not show warnings section when no warnings, got: {}",
        output
    );
    assert!(
        !output.contains("I/O dans boucles:"),
        "should not show I/O section when no io_in_loops, got: {}",
        output
    );
}

// D3 (#50 slice S4), test case 21 — console project report must surface
// unmeasurable files as their own section, with path and reason, not
// silently omit them.
#[test]
fn write_project_report_shows_non_mesures_section_with_path_and_reason() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(vec![UnmeasurableFile {
            path: path("src/bad.rs"),
            reason: UnmeasurableReason::SourceUnparseable,
        }]);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("=== Fichiers NON MESURÉS (1) ==="),
        "expected a NON MESURÉS section header with the count, got: {}",
        output
    );
    assert!(
        output.contains("src/bad.rs"),
        "expected the unmeasurable file's path in the section, got: {}",
        output
    );
    assert!(
        output.contains("code source non analysable"),
        "expected the human-readable reason in the section, got: {}",
        output
    );
}

#[test]
fn write_project_report_no_unmeasurable_files_does_not_show_section() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("NON MESURÉS"),
        "should not show the NON MESURÉS section when there are no unmeasurable files, got: {}",
        output
    );
}

// US8 slice 1 — console surface: a breach must print a human-readable
// warning (AC3), via the ONE shared renderer (AD-3). AC6: no threshold
// evaluated at all (graph.threshold_report() == None) leaves the report
// byte-for-byte unchanged from before US8 — the exact prior behavior.
//
// Test List:
// 1. no threshold_report attached at all -> no warning section (AC6)
// 2. threshold_report attached but no breach -> still no warning section
// 3. threshold_report with a breach -> warning section naming the metric,
//    limit, actual value, and excess (AD-3's "by how much")

// #8 (found while writing the e2e single-file test) — write_console_to (the
// single-file surface) never got the threshold banner, only
// write_project_report_to did. Same shared-renderer wiring, single-file
// twin.

#[test]
fn write_console_breaching_threshold_report_shows_warning_with_the_numbers() {
    let writer = ConsoleReportWriter::new();
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000015), None);
    let metrics = CodeMetrics::new(5).with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("SEUIL") && output.contains("ÉNERGIE"),
        "a breach must print a warning section, got: {}",
        output
    );
}

#[test]
fn write_console_without_a_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let mut buf = Vec::new();
    writer.write_console_to(&mut buf, &metrics);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "no threshold was ever evaluated, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_without_a_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "no threshold was ever evaluated, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_non_breaching_threshold_report_shows_no_warning() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let thresholds = AlertThresholds::new(Some(1.0), None).unwrap();
    let report = thresholds.evaluate(Some(0.0000065), None);
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        !output.contains("SEUIL"),
        "threshold was evaluated but not breached, must not print a warning: {}",
        output
    );
}

#[test]
fn write_project_report_breaching_threshold_report_shows_warning_with_the_numbers() {
    let writer = ConsoleReportWriter::new();
    let files = vec![(path("src/good.rs"), CodeMetrics::new(5))];
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000015), None);
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_threshold_report(report);
    let mut buf = Vec::new();
    writer.write_project_report_to(&mut buf, &graph);
    let output = String::from_utf8(buf).unwrap();

    assert!(
        output.contains("SEUIL"),
        "a breach must print a warning section, got: {}",
        output
    );
    assert!(
        output.contains("ÉNERGIE"),
        "the warning must name the breached metric, got: {}",
        output
    );
}
