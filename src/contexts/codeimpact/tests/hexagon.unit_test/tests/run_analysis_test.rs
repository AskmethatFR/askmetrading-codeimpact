use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::AnalysisConfig;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::ParserRegistry;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;
use codeimpact_secondaries::gateways::code_readers::code_reader_stub::CodeReaderStub;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::File)
}

fn make_project_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::Project)
}

#[test]
fn analysis_target_project_has_correct_type() {
    let target = make_project_target(".");
    assert_eq!(*target.target_type(), TargetType::Project);
}

#[test]
fn analyze_project_target_returns_ok() {
    let reader = CodeReaderStub::new();
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok(), "project target should return Ok(())");
}

#[test]
fn analyze_valid_file_writes_metrics() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(
        PathBuf::from("test.rs"),
        "fn test() { if x > 0 { } }".into(),
    );
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "test".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    use_case
        .handle(
            &make_target("test.rs"),
            &[AnalysisRule::CyclomaticComplexity],
            &AnalysisConfig::defaults(),
        )
        .expect("analysis should succeed");

    let metrics = writer.last_metrics.lock().unwrap();
    assert!(metrics.is_some());
    assert_eq!(metrics.as_ref().unwrap().cyclomatic_complexity(), 2);
}

#[test]
fn analyze_nonexistent_file_returns_error() {
    let reader = CodeReaderStub::new();
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_target("nonexistent.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    match result {
        Err(AnalysisError::IoError(_)) => {}
        _ => panic!("expected IoError, got {:?}", result),
    }
}

#[test]
fn list_source_files_returns_configured_files_from_stub() {
    let mut reader = CodeReaderStub::new();
    reader.add_source_file(PathBuf::from("src/main.rs"));
    reader.add_source_file(PathBuf::from("src/lib.rs"));
    let files = reader
        .list_source_files(&PathBuf::from("."), &["rs"], &FileFilter::unrestricted())
        .expect("should list files");
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("src/main.rs")));
    assert!(files.contains(&PathBuf::from("src/lib.rs")));
}

#[test]
fn list_source_files_returns_empty_when_none_configured() {
    let reader = CodeReaderStub::new();
    let files = reader
        .list_source_files(&PathBuf::from("."), &["rs"], &FileFilter::unrestricted())
        .expect("should list files");
    assert!(files.is_empty());
}

#[test]
fn analyze_project_target_writes_per_file_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source(PathBuf::from("src/lib.rs"), "fn lib() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));
    reader.add_source_file(PathBuf::from("src/lib.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "f".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(
        result.is_ok(),
        "project analysis should succeed: {:?}",
        result
    );

    let graph = writer.last_graph.lock().unwrap();
    assert!(
        graph.is_some(),
        "write_project_report should have been called"
    );
    let graph = graph.as_ref().unwrap();
    let metrics = graph.per_file_metrics();
    assert_eq!(metrics.len(), 2, "should report on 2 files");
    assert!(metrics.contains_key(&PathBuf::from("src/main.rs")));
    assert!(metrics.contains_key(&PathBuf::from("src/lib.rs")));
}

#[test]
fn parser_error_propagates_through_use_case() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("bad.rs"), "invalid rust code @@@".into());
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::new(Err(AnalysisError::AnalysisFailed(
        "parse error".to_string(),
    )));
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_target("bad.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    match result {
        Err(AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!("expected AnalysisFailed, got {:?}", result),
    }
}

#[test]
fn handle_project_continues_on_read_error() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs")); // no source — read_source fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(
        result.is_ok(),
        "project analysis should continue despite read errors"
    );

    let graph = writer.last_graph.lock().unwrap();
    assert!(
        graph.is_some(),
        "write_project_report should have been called"
    );
    let graph = graph.as_ref().unwrap();
    assert!(
        graph
            .per_file_metrics()
            .contains_key(&PathBuf::from("src/good.rs")),
        "good.rs should have metrics despite bad.rs read failure"
    );
}

#[test]
fn handle_project_continues_on_parse_error() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::new(Err(AnalysisError::AnalysisFailed(
        "parse error".to_string(),
    )));
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(
        result.is_ok(),
        "project analysis should continue despite parse errors"
    );

    let graph = writer.last_graph.lock().unwrap();
    assert!(
        graph.is_some(),
        "write_project_report should have been called"
    );
    let graph = graph.as_ref().unwrap();
    assert!(
        graph.per_file_metrics().is_empty(),
        "no metrics when all files fail to parse"
    );
}

#[test]
fn handle_project_continues_on_deps_parse_error() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "main".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
    .with_resolved_dependencies(Err(AnalysisError::AnalysisFailed("deps error".to_string())));

    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(
        result.is_ok(),
        "project analysis should continue despite deps parse errors"
    );

    let graph = writer.last_graph.lock().unwrap();
    assert!(
        graph.is_some(),
        "write_project_report should have been called"
    );
    let graph = graph.as_ref().unwrap();
    assert!(
        graph
            .per_file_metrics()
            .contains_key(&PathBuf::from("src/main.rs")),
        "main.rs should have metrics despite deps parse failure"
    );
}

// D3 (#50 slice S4) — handle_project used to silently DROP a file that
// failed to read or parse (eprintln! then nothing), undercounting
// total_files and hiding the failure from the report entirely (the exact
// ADR-0010 lie, one layer up: 0 files reported wrong is no better than 0
// cost reported wrong). It must now record an UnmeasurableFile instead.

#[test]
fn handle_project_records_unreadable_file_as_unmeasurable() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs")); // no source configured — read_source fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok());

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/bad.rs"));
    assert_eq!(unmeasurable[0].reason, UnmeasurableReason::SourceUnreadable);
    assert_eq!(
        graph.aggregated_metrics().unmeasurable_files,
        1,
        "aggregated_metrics must count it too"
    );
}

#[test]
fn handle_project_records_unparseable_file_as_unmeasurable() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source(PathBuf::from("src/bad.rs"), "@@@ not rust".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/bad.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "@@@",
        AnalysisError::AnalysisFailed("parse error".to_string()),
    );
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok());

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    assert!(
        graph
            .per_file_metrics()
            .contains_key(&PathBuf::from("src/good.rs")),
        "good.rs should still be measured"
    );
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/bad.rs"));
    assert_eq!(
        unmeasurable[0].reason,
        UnmeasurableReason::SourceUnparseable
    );
    let pm = graph.aggregated_metrics();
    assert_eq!(pm.total_files, 1, "only good.rs counts as measured");
    assert_eq!(pm.unmeasurable_files, 1);
}

// source_guard (#62) — a file refused by check_admissible carries its
// precise reason (SourceTooLarge) through to the report, instead of
// collapsing into the generic SourceUnparseable every other parse failure
// gets.

#[test]
fn project_with_oversized_file_marks_it_source_too_large() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source(PathBuf::from("src/huge.rs"), "OVERSIZED".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/huge.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "OVERSIZED",
        AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge),
    );
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok());

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    assert!(
        graph
            .per_file_metrics()
            .contains_key(&PathBuf::from("src/good.rs")),
        "good.rs should still be measured"
    );
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/huge.rs"));
    assert_eq!(unmeasurable[0].reason, UnmeasurableReason::SourceTooLarge);
    let pm = graph.aggregated_metrics();
    assert_eq!(pm.total_files, 1, "only good.rs counts as measured");
}

// build_project_graph (#62) is the shared path behind BOTH handle_project
// (console, pinned above) and handle_project_json/handle_project_html
// (--format json/html). Only the console twin was pinned; this mirrors it
// through handle_project_json so the JSON/HTML reason-mapping branch is
// pinned too.
#[test]
fn project_json_marks_oversized_file_source_too_large() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/good.rs"), "fn good() {}".into());
    reader.add_source(PathBuf::from("src/huge.rs"), "OVERSIZED".into());
    reader.add_source_file(PathBuf::from("src/good.rs"));
    reader.add_source_file(PathBuf::from("src/huge.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "OVERSIZED",
        AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge),
    );
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AnalysisConfig::defaults(),
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_json should have been called");
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/huge.rs"));
    assert_eq!(
        unmeasurable[0].reason,
        UnmeasurableReason::SourceTooLarge,
        "must be SourceTooLarge, not the generic SourceUnparseable fallback"
    );
    let pm = graph.aggregated_metrics();
    assert_eq!(pm.total_files, 1, "only good.rs counts as measured");
}

// US8 slice 1 — the calling use case for AlertThresholds::evaluate (AD-1):
// handle_project evaluates the project's aggregate energy/CO2 impact
// against the configured thresholds and attaches the outcome to the graph
// so the console writer can render it (AD-3). Change request on issue #8:
// energy replaces CPU cost as the gate's first metric.
//
// Test List:
// 1. a maximally strict threshold (0.0) breaches — any measured project has
//    a positive base cost (file-level "+1" complexity alone is nonzero)
// 2. a threshold high enough to never breach still attaches a report
//    (Some(..), not None) — evaluation happened, it simply found nothing

#[test]
fn handle_project_with_breached_energy_threshold_attaches_a_breaching_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &thresholds,
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    let report = graph
        .threshold_report()
        .expect("a threshold was configured, evaluate() must have run");
    assert!(
        report.has_breach(),
        "a zero kWh threshold must breach any measured project's positive base energy"
    );
}

#[test]
fn handle_project_within_threshold_still_attaches_a_non_breaching_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(1_000_000.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &thresholds,
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    let report = graph
        .threshold_report()
        .expect("a threshold was configured, evaluate() must have run even with no breach");
    assert!(
        !report.has_breach(),
        "the huge threshold must not be breached by a trivial project"
    );
}

// Review-barrier fix (Dev-B, energy-swap re-review, issue #8) — the
// energy_joules() / KWH_TO_JOULES conversion in gate_project was not
// pinned by any test: every prior threshold was either 0.0 (breaches on
// any positive value, converted or not) or 1_000_000.0 (never breaches
// either way) — neither straddles the ~3.6M gap between a raw-joule
// magnitude (a few J for a trivial file) and its correct kWh conversion
// (a few 1e-7 kWh). Dev-B proved this by deleting the division and
// watching the full suite stay green. 0.001 kWh sits strictly between
// those two magnitudes: with the correct conversion the tiny kWh value
// never breaches it; if the division were dropped (raw joules fed as
// kWh), the same threshold WOULD breach. That asymmetry is what makes
// this test discriminate — verified by literally deleting the division
// locally and watching this test go red before restoring it.
#[test]
fn handle_project_energy_threshold_discriminates_joules_from_kwh() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.001), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &thresholds,
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    let report = graph
        .threshold_report()
        .expect("a threshold was configured, evaluate() must have run");
    assert!(
        !report.has_breach(),
        "0.001 kWh must not breach on the correctly-converted tiny kWh value — it WOULD breach \
         if energy_joules() / KWH_TO_JOULES were dropped and raw joules were fed as kWh instead"
    );
}

// AC7 / ADR-0010 honesty, at the use-case level (the VO-level gate is
// already pinned directly in alert_thresholds_test.rs): when every file in
// the project failed to measure, aggregated_metrics().total_ecological_impact
// is None (D3, #50) — evaluate() must receive None, not a fabricated 0, and
// must therefore never report a breach even with a maximally strict
// threshold configured.
#[test]
fn handle_project_with_every_file_unmeasurable_never_breaches_despite_strict_threshold() {
    let mut reader = CodeReaderStub::new();
    reader.add_source_file(PathBuf::from("src/bad.rs")); // no source configured -> read fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.0), Some(0.0)).unwrap(),
        FileFilter::unrestricted(),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &thresholds,
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_report should have been called");
    assert_eq!(
        graph.aggregated_metrics().total_ecological_impact,
        None,
        "precondition: every file failed to measure, there is no aggregate to breach"
    );
    let report = graph
        .threshold_report()
        .expect("thresholds were configured, evaluate() must still have run");
    assert!(
        !report.has_breach(),
        "an absent (unmeasured) aggregate must never count as a breach, however strict the threshold"
    );
}

// US8 slice 2 (AD-4) — the exit-code DECISION is main.rs's job, but the
// domain must hand it the breach outcome directly on the return value: a
// caller with only Result<GatedOutput<()>, _> in hand (no graph reference)
// must still be able to answer "was there a breach".
#[test]
fn handle_project_return_value_carries_the_same_breach_outcome_as_the_graph() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let gated = use_case
        .handle(
            &make_project_target("."),
            &[AnalysisRule::CyclomaticComplexity],
            &thresholds,
        )
        .expect("analysis should succeed");

    assert!(
        gated.thresholds().has_breach(),
        "the return value must carry the breach without needing the graph"
    );
}

// US8 slice 3 (T3) — the single-file gate: handle() evaluates thresholds
// against the FILE's own economic/ecological impact (not the project
// aggregate) when the target is a single file.
#[test]
fn handle_single_file_with_breached_threshold_returns_a_breaching_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("test.rs"), "fn test() {}".into());
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.0), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let gated = use_case
        .handle(
            &make_target("test.rs"),
            &[AnalysisRule::CyclomaticComplexity],
            &thresholds,
        )
        .expect("analysis should succeed");

    assert!(
        gated.thresholds().has_breach(),
        "a zero kWh threshold must breach any measured file's positive base energy"
    );
    let metrics = writer.last_metrics.lock().unwrap();
    let metrics = metrics
        .as_ref()
        .expect("write_console must have been called");
    assert!(
        metrics
            .threshold_report()
            .expect("evaluate() must have run and attached a report")
            .has_breach(),
        "the report must also be attached to the metrics passed to the writer"
    );
}

// Review-barrier fix (Dev-B, energy-swap re-review, issue #8) — the
// single-file twin of gate_project's discrimination gap: gate_metrics
// carries its OWN copy of the energy_joules() / KWH_TO_JOULES conversion,
// unpinned for the same reason (every prior single-file threshold test
// used 0.0). Same 0.001 kWh discriminator, same asymmetry.
#[test]
fn handle_single_file_energy_threshold_discriminates_joules_from_kwh() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("test.rs"), "fn test() {}".into());
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );
    let thresholds = AnalysisConfig::new(
        AlertThresholds::new(Some(0.001), None).unwrap(),
        FileFilter::unrestricted(),
    );

    let gated = use_case
        .handle(
            &make_target("test.rs"),
            &[AnalysisRule::CyclomaticComplexity],
            &thresholds,
        )
        .expect("analysis should succeed");

    assert!(
        !gated.thresholds().has_breach(),
        "0.001 kWh must not breach on the correctly-converted tiny kWh value — it WOULD breach \
         if energy_joules() / KWH_TO_JOULES were dropped and raw joules were fed as kWh instead"
    );
}

// ── Test List (US16 T2, step F — ParserRegistry dispatch through RunAnalysis) ──
//   1. A project mixing .rs and .cs files dispatches each to its OWN
//      registered parser — both measured, neither mis-dispatched to the
//      other's parser.
//   2. A single-file target whose extension has no registered parser
//      (.md) is refused non-fatally (Unmeasurable(UnsupportedLanguage)),
//      never a panic, never silently parsed as Rust.
//   3. A project containing one file with no registered parser tolerates
//      it (added to unmeasurable_files, AC: one hostile/unsupported file
//      never kills the whole scan) — exercises the defensive branch in
//      handle_project's per-file loop directly (a stub CodeReader can
//      return files list_source_files' real extension filter would
//      normally have excluded already).

#[test]
fn handle_project_dispatches_each_file_to_its_own_language_parser() {
    let mut reader = CodeReaderStub::new();
    reader.add_source_file(PathBuf::from("a.rs"));
    reader.add_source(PathBuf::from("a.rs"), "fn rust_fn() {}".into());
    reader.add_source_file(PathBuf::from("b.cs"));
    reader.add_source(
        PathBuf::from("b.cs"),
        "class C { void CsharpFn() {} }".into(),
    );

    let rust_parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "rust_fn".into(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);
    let csharp_parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "CsharpFn".into(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);

    let writer = SharedReportWriterStub::new();
    let registry = ParserRegistry::new()
        .register(Language::Rust, Box::new(rust_parser))
        .register(Language::CSharp, Box::new(csharp_parser));
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), registry);

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
    );

    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let graph = writer.last_graph.lock().unwrap().clone().unwrap();
    assert_eq!(graph.files().len(), 2, "both languages must be measured");
    assert!(graph.unmeasurable_files().is_empty());
}

#[test]
fn handle_single_file_with_unsupported_extension_is_refused_non_fatally() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("notes.md"), "# not code".into());
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_target("notes.md"),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
    );

    match result {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::UnsupportedLanguage)) => {}
        other => panic!(
            "expected Unmeasurable(UnsupportedLanguage), got {:?}",
            other
        ),
    }
}

#[test]
fn handle_project_with_one_unsupported_file_still_measures_the_rest() {
    let mut reader = CodeReaderStub::new();
    reader.add_source_file(PathBuf::from("a.rs"));
    reader.add_source(PathBuf::from("a.rs"), "fn a() {}".into());
    reader.add_source_file(PathBuf::from("notes.md"));
    reader.add_source(PathBuf::from("notes.md"), "# not code".into());

    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "a".into(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 0,
        depth: 0,
        branch_arms: 0,
        calls_in_loops: vec![],
    }]);

    let writer = SharedReportWriterStub::new();
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer.clone()),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
    );

    assert!(
        result.is_ok(),
        "one unsupported file must not fail the whole scan: {:?}",
        result
    );
    let graph = writer.last_graph.lock().unwrap().clone().unwrap();
    assert_eq!(graph.files().len(), 1, "a.rs must still be measured");
    assert_eq!(graph.unmeasurable_files().len(), 1);
    assert_eq!(
        graph.unmeasurable_files()[0].reason,
        UnmeasurableReason::UnsupportedLanguage
    );
}

#[test]
fn handle_single_file_without_threshold_flags_shows_no_breach() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("test.rs"), "fn test() {}".into());
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(
        Box::new(reader),
        Box::new(writer),
        ParserRegistry::new().register(Language::Rust, Box::new(parser)),
    );

    let gated = use_case
        .handle(
            &make_target("test.rs"),
            &[AnalysisRule::CyclomaticComplexity],
            &AnalysisConfig::defaults(),
        )
        .expect("analysis should succeed");

    assert!(!gated.thresholds().has_breach());
}
