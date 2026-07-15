use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
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

// Test List:
// 1. handle_json returns a non-empty string for valid file
// 2. handle_json with nonexistent file returns error
// 3. handle_project_json returns a non-empty string for project target

#[test]
fn handle_json_returns_string_for_valid_file() {
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

    let result = use_case.handle_json(
        &make_target("test.rs"),
        &[AnalysisRule::CyclomaticComplexity],
    );

    assert!(result.is_ok(), "handle_json should succeed");
    let json = result.unwrap();
    assert!(!json.is_empty(), "JSON string should not be empty");
    assert!(json.contains("test.rs"), "JSON should contain target path");
    assert!(json.contains("codeimpact"), "JSON should contain tool name");
}

#[test]
fn handle_json_nonexistent_file_returns_error() {
    let reader = CodeReaderStub::new();
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result = use_case.handle_json(
        &make_target("nonexistent.rs"),
        &[AnalysisRule::CyclomaticComplexity],
    );

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::IoError(_)) => {}
        _ => panic!("expected IoError, got {:?}", result),
    }
}

#[test]
fn handle_project_json_returns_string() {
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
    }]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result =
        use_case.handle_project_json(&make_target("."), &[AnalysisRule::CyclomaticComplexity]);

    assert!(result.is_ok(), "handle_project_json should succeed");
    let json = result.unwrap();
    assert!(!json.is_empty(), "JSON string should not be empty");
    assert!(
        json.contains("project"),
        "project JSON should contain target_type project"
    );
}

#[test]
fn handle_project_json_empty_project_returns_error() {
    let reader = CodeReaderStub::new(); // no files added
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result =
        use_case.handle_project_json(&make_target("."), &[AnalysisRule::CyclomaticComplexity]);

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!(
            "expected AnalysisFailed for empty project, got {:?}",
            result
        ),
    }
}

// BLOCKER 2 (#50 QA retry 1) — build_project_graph's unmeasurable branches
// (behind handle_project_json) had no test at all. Mirrors the console-path
// pins in run_analysis_test.rs (handle_project_records_un{readable,parseable}
// _file_as_unmeasurable) one layer up, on the JSON path.

#[test]
fn handle_project_json_records_unreadable_file_as_unmeasurable_and_excludes_it_from_sums() {
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

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_json must pass the built graph through");
    let unmeasurable = graph.unmeasurable_files();
    assert_eq!(unmeasurable.len(), 1, "got {:?}", unmeasurable);
    assert_eq!(unmeasurable[0].path, PathBuf::from("src/bad.rs"));
    assert_eq!(unmeasurable[0].reason, UnmeasurableReason::SourceUnreadable);
    assert_eq!(
        graph.aggregated_metrics().unmeasurable_files,
        1,
        "aggregated_metrics must count it too"
    );
    assert_eq!(
        graph.aggregated_metrics().total_files,
        1,
        "the unreadable file must enter no sum — only good.rs counts as measured"
    );
}

#[test]
fn handle_project_json_records_unparseable_file_as_unmeasurable_and_excludes_it_from_sums() {
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
        codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed("parse error".to_string()),
    );
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()), Box::new(parser));

    let result = use_case.handle_project_json(
        &make_project_target("."),
        &[AnalysisRule::CyclomaticComplexity],
    );
    assert!(result.is_ok(), "got {:?}", result);

    let graph = writer.last_graph.lock().unwrap();
    let graph = graph
        .as_ref()
        .expect("write_project_json must pass the built graph through");
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
