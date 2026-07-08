use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_readers::file_system_code_reader::FileSystemCodeReader;

fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("tests");
    path.push("primaries.e2e_test");
    path.push("tests");
    path.push("fixtures");
    path.push(name);
    path
}

#[test]
fn read_existing_file_returns_content() {
    let reader = FileSystemCodeReader::new();
    let target = AnalysisTarget::new(fixture_path("sample.rs"), TargetType::File);
    let result = reader.read_source(&target);
    assert!(result.is_ok(), "should read fixture: {:?}", result.err());
    assert!(result.unwrap().contains("fn main"));
}

#[test]
fn read_nonexistent_file_returns_error() {
    let reader = FileSystemCodeReader::new();
    let target = AnalysisTarget::new(PathBuf::from("/tmp/__nonexistent__"), TargetType::File);
    let result = reader.read_source(&target);
    assert!(result.is_err(), "nonexistent file should error");
}
