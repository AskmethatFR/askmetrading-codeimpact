// Test List for RunAnalysis:
// 1. Valid file -> reads source, analyzes, writes report, returns Ok
// 2. File not found -> CodeReader returns IoError, RunAnalysis propagates it

use std::cell::RefCell;
use std::rc::Rc;

use codeimpact_hexagon::domain_model::analysis_rule::AnalysisRule;
use codeimpact_hexagon::domain_model::analysis_target::{AnalysisTarget, TargetType};
use codeimpact_hexagon::domain_model::errors::AnalysisError;
use codeimpact_hexagon::use_cases_application_services::run_analysis::RunAnalysis;
use codeimpact_secondaries::gateways::code_readers::code_reader_stub::CodeReaderStub;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::{
    ReportWriterStub, SharedReportWriterStub,
};
use std::path::PathBuf;

#[test]
fn run_analysis_valid_file_success() {
    let target = AnalysisTarget::new(PathBuf::from("test.rs"), TargetType::File);
    let source = "fn hello() { let x = 1; }";
    let mut reader = CodeReaderStub::new();
    reader.add_source(target.path().to_path_buf(), source.to_string());
    let writer = SharedReportWriterStub(Rc::new(RefCell::new(ReportWriterStub::new())));
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()));

    let result = use_case.execute(&target, &[AnalysisRule::CyclomaticComplexity]);

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);

    let last_metrics = writer.0.borrow().last_metrics();
    assert!(last_metrics.is_some(), "Expected a report to be written");
    if let Some(metrics) = last_metrics {
        assert_eq!(metrics.cyclomatic_complexity(), 1);
        assert_eq!(metrics.complexity_level(), "low");
    }
}

#[test]
fn run_analysis_file_not_found_propagates_error() {
    let target = AnalysisTarget::new(PathBuf::from("nonexistent.rs"), TargetType::File);
    let reader = CodeReaderStub::new(); // no sources added
    let writer = SharedReportWriterStub(Rc::new(RefCell::new(ReportWriterStub::new())));
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()));

    let result = use_case.execute(&target, &[AnalysisRule::CyclomaticComplexity]);

    assert!(result.is_err(), "Expected Err, got Ok");
    match result {
        Err(AnalysisError::IoError(_)) => {}
        _ => panic!("Expected IoError, got {:?}", result),
    }
}

#[test]
fn run_analysis_analyzes_and_returns_metrics() {
    let target = AnalysisTarget::new(PathBuf::from("complex.rs"), TargetType::File);
    let source = "fn test() { if x > 0 { } else { } }";
    let mut reader = CodeReaderStub::new();
    reader.add_source(target.path().to_path_buf(), source.to_string());
    let writer = SharedReportWriterStub(Rc::new(RefCell::new(ReportWriterStub::new())));
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()));

    let result = use_case.execute(&target, &[AnalysisRule::CyclomaticComplexity]);

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let last_metrics = writer.0.borrow().last_metrics();
    assert!(last_metrics.is_some());
    if let Some(metrics) = last_metrics {
        assert_eq!(metrics.cyclomatic_complexity(), 2);
        assert_eq!(metrics.complexity_level(), "low");
    }
}
