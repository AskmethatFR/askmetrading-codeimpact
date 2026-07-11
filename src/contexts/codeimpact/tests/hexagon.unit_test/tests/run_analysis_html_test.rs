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
    AnalysisTarget::new(PathBuf::from(path), TargetType::Project)
}

// Test List:
// 1. handle_project_html delegates to ReportWriter.write_html and returns its string
// 2. handle_project_html on an empty project (no files) returns AnalysisFailed

#[test]
fn handle_project_html_returns_writer_output_for_valid_project() {
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

    let result = use_case.handle_project_html(&make_target("."), &[AnalysisRule::CyclomaticComplexity]);

    assert!(result.is_ok(), "handle_project_html should succeed, got {:?}", result);
    let html = result.unwrap();
    assert!(!html.is_empty(), "html string should not be empty");
    assert_eq!(
        *writer.last_html.lock().unwrap(),
        Some(html),
        "handle_project_html must return exactly what the ReportWriter.write_html produced"
    );
    let captured_graph = writer.last_graph.lock().unwrap();
    assert!(
        captured_graph.is_some(),
        "handle_project_html must pass the built FileConsumptionGraph to write_html"
    );
    assert_eq!(captured_graph.as_ref().unwrap().files().len(), 1);
}

#[test]
fn handle_project_html_empty_project_returns_error() {
    let reader = CodeReaderStub::new(); // no files added
    let writer = SharedReportWriterStub::new();
    let parser = CodeParserStub::with_functions(vec![]);
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

    let result = use_case.handle_project_html(&make_target("."), &[AnalysisRule::CyclomaticComplexity]);

    match result {
        Err(codeimpact_hexagon::analysis::AnalysisError::AnalysisFailed(_)) => {}
        _ => panic!("expected AnalysisFailed for empty project, got {:?}", result),
    }
}
