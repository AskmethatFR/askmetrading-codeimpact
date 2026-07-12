use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;
use codeimpact_secondaries::gateways::code_readers::code_reader_stub::CodeReaderStub;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::File)
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
