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

fn fixtures_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("tests");
    path.push("primaries.e2e_test");
    path.push("tests");
    path.push("fixtures");
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

#[test]
fn list_rust_files_finds_rs_in_fixtures() {
    let reader = FileSystemCodeReader::new();
    let result = reader.list_rust_files(&fixtures_dir());
    assert!(result.is_ok(), "should list fixtures dir: {:?}", result.err());
    let files = result.unwrap();
    assert!(
        files.iter().any(|f| f.ends_with("sample.rs")),
        "should find sample.rs in {:?}",
        files
    );
}

#[test]
fn list_rust_files_skips_non_rs_files() {
    let reader = FileSystemCodeReader::new();
    // Use the e2e test directory which has Cargo.toml (non-.rs) and .rs files
    let mut e2e_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    e2e_dir.pop();
    e2e_dir.pop();
    e2e_dir.push("tests");
    e2e_dir.push("primaries.e2e_test");
    e2e_dir.push("tests");

    let result = reader.list_rust_files(&e2e_dir);
    assert!(result.is_ok(), "should list dir: {:?}", result.err());
    let files = result.unwrap();
    // Should find the fixture file
    assert!(
        files.iter().any(|f| f.ends_with("sample.rs")),
        "should find sample.rs"
    );
    // Should NOT find Cargo.toml
    assert!(
        !files.iter().any(|f| f.ends_with("Cargo.toml")),
        "should not include non-.rs files"
    );
}
