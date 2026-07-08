use codeimpact_hexagon::domain_model::analysis_target::{AnalysisTarget, TargetType};
use codeimpact_hexagon::gateways_secondary_ports::code_reader_port::CodeReaderPort;
use codeimpact_secondaries::gateways::code_readers::file_system_code_reader::FileSystemCodeReader;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures")
}

#[test]
fn file_system_code_reader_reads_existing_file() {
    let fixture_path = fixtures_dir().join("sample.rs");
    let target = AnalysisTarget::new(fixture_path.clone(), TargetType::File);
    let reader = FileSystemCodeReader::new();

    let result = reader.read_source(&target);

    assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    let source = result.unwrap();
    assert!(
        source.contains("fn calculate_fibonacci"),
        "Should contain function content"
    );
}

#[test]
fn file_system_code_reader_returns_error_for_nonexistent_file() {
    let target = AnalysisTarget::new(
        PathBuf::from("/tmp/nonexistent_abc123.rs"),
        TargetType::File,
    );
    let reader = FileSystemCodeReader::new();

    let result = reader.read_source(&target);

    assert!(result.is_err(), "Expected Err for nonexistent file");
}
