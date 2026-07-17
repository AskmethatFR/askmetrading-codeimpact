use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::ParsedFunction;
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
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
        match_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    use_case
        .handle(
            &make_target("test.rs"),
            &[AnalysisRule::CyclomaticComplexity],
            &AlertThresholds::none(),
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
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result = use_case.handle(
        &make_target("nonexistent.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
    );
    match result {
        Err(AnalysisError::IoError(_)) => {}
        _ => panic!("expected IoError, got {:?}", result),
    }
}

#[test]
fn list_rust_files_returns_configured_files_from_stub() {
    let mut reader = CodeReaderStub::new();
    reader.add_rust_file(PathBuf::from("src/main.rs"));
    reader.add_rust_file(PathBuf::from("src/lib.rs"));
    let files = reader
        .list_rust_files(&PathBuf::from("."))
        .expect("should list files");
    assert_eq!(files.len(), 2);
    assert!(files.contains(&PathBuf::from("src/main.rs")));
    assert!(files.contains(&PathBuf::from("src/lib.rs")));
}

#[test]
fn list_rust_files_returns_empty_when_none_configured() {
    let reader = CodeReaderStub::new();
    let files = reader
        .list_rust_files(&PathBuf::from("."))
        .expect("should list files");
    assert!(files.is_empty());
}

#[test]
fn analyze_project_target_writes_per_file_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_source(PathBuf::from("src/lib.rs"), "fn lib() {}".into());
    reader.add_rust_file(PathBuf::from("src/main.rs"));
    reader.add_rust_file(PathBuf::from("src/lib.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "f".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result = use_case.handle(
        &make_target("bad.rs"),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/good.rs"));
    reader.add_rust_file(PathBuf::from("src/bad.rs")); // no source — read_source fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::new(Err(AnalysisError::AnalysisFailed(
        "parse error".to_string(),
    )));
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "main".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }])
    .with_deps(Err(AnalysisError::AnalysisFailed("deps error".to_string())));

    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/good.rs"));
    reader.add_rust_file(PathBuf::from("src/bad.rs")); // no source configured — read_source fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/good.rs"));
    reader.add_rust_file(PathBuf::from("src/bad.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "@@@",
        AnalysisError::AnalysisFailed("parse error".to_string()),
    );
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/good.rs"));
    reader.add_rust_file(PathBuf::from("src/huge.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "OVERSIZED",
        AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge),
    );
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
    reader.add_rust_file(PathBuf::from("src/good.rs"));
    reader.add_rust_file(PathBuf::from("src/huge.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![ParsedFunction {
        name: "good".to_string(),
        start_line: 1,
        calls: vec![],
        has_loop: false,
        has_nested_loop: false,
        decision_points: 1,
        depth: 0,
        match_arms: 0,
        calls_in_loops: vec![],
    }])
    .failing_when_source_contains(
        "OVERSIZED",
        AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooLarge),
    );
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
        &AlertThresholds::none(),
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
// handle_project evaluates the project's aggregate CPU/CO2 impact against
// the configured thresholds and attaches the outcome to the graph so the
// console writer can render it (AD-3).
//
// Test List:
// 1. a maximally strict threshold (0.0) breaches — any measured project has
//    a positive base cost (file-level "+1" complexity alone is nonzero)
// 2. a threshold high enough to never breach still attaches a report
//    (Some(..), not None) — evaluation happened, it simply found nothing

#[test]
fn handle_project_with_breached_cpu_threshold_attaches_a_breaching_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_rust_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));
    let thresholds = AlertThresholds::new(Some(0.0), None).unwrap();

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
        "a zero cpu threshold must breach any measured project's positive base cost"
    );
}

#[test]
fn handle_project_within_threshold_still_attaches_a_non_breaching_report() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(PathBuf::from("src/main.rs"), "fn main() {}".into());
    reader.add_rust_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));
    let thresholds = AlertThresholds::new(Some(1_000_000.0), None).unwrap();

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

// AC7 / ADR-0010 honesty, at the use-case level (the VO-level gate is
// already pinned directly in alert_thresholds_test.rs): when every file in
// the project failed to measure, aggregated_metrics().total_economic_impact
// is None (D3, #50) — evaluate() must receive None, not a fabricated 0, and
// must therefore never report a breach even with a maximally strict
// threshold configured.
#[test]
fn handle_project_with_every_file_unmeasurable_never_breaches_despite_strict_threshold() {
    let mut reader = CodeReaderStub::new();
    reader.add_rust_file(PathBuf::from("src/bad.rs")); // no source configured -> read fails

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));
    let thresholds = AlertThresholds::new(Some(0.0), Some(0.0)).unwrap();

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
        graph.aggregated_metrics().total_economic_impact,
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
    reader.add_rust_file(PathBuf::from("src/main.rs"));

    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));
    let thresholds = AlertThresholds::new(Some(0.0), None).unwrap();

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
