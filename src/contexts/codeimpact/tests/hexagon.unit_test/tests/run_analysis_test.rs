use std::path::PathBuf;

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
