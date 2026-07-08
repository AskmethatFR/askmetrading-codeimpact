use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_readers::code_reader_stub::CodeReaderStub;
use codeimpact_secondaries::gateways::report_writers::report_writer_stub::SharedReportWriterStub;

fn make_target(path: &str) -> AnalysisTarget {
    AnalysisTarget::new(PathBuf::from(path), TargetType::File)
}

#[test]
fn analyze_valid_file_writes_metrics() {
    let mut reader = CodeReaderStub::new();
    reader.add_source(
        PathBuf::from("test.rs"),
        "fn test() { if x > 0 { } }".into(),
    );
    let writer = SharedReportWriterStub::new();
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer.clone()));

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
    let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer));

    let result = use_case.handle(
        &make_target("nonexistent.rs"),
        &[AnalysisRule::CyclomaticComplexity],
    );
    match result {
        Err(AnalysisError::IoError(_)) => {}
        _ => panic!("expected IoError, got {:?}", result),
    }
}
