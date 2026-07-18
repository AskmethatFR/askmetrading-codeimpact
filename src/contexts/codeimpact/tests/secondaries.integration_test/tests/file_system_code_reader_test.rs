use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
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
fn list_source_files_finds_rs_in_fixtures() {
    let reader = FileSystemCodeReader::new();
    let result = reader.list_source_files(&fixtures_dir(), &["rs"], &FileFilter::unrestricted());
    assert!(
        result.is_ok(),
        "should list fixtures dir: {:?}",
        result.err()
    );
    let files = result.unwrap();
    assert!(
        files.iter().any(|f| f.ends_with("sample.rs")),
        "should find sample.rs in {:?}",
        files
    );
}

#[test]
fn list_source_files_skips_files_outside_requested_extensions() {
    let reader = FileSystemCodeReader::new();
    // Use the e2e test directory which has Cargo.toml (non-.rs) and .rs files
    let mut e2e_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    e2e_dir.pop();
    e2e_dir.pop();
    e2e_dir.push("tests");
    e2e_dir.push("primaries.e2e_test");
    e2e_dir.push("tests");

    let result = reader.list_source_files(&e2e_dir, &["rs"], &FileFilter::unrestricted());
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

// US31 (#31) — FileFilter wiring into the real filesystem walk. D1: glob
// compilation happens HERE (the adapter), FileFilter itself carries only
// validated raw patterns. Slice 1 wires `exclude`; slice 2 wires `include`
// and the both-match-excluded-wins precedence; slice 3 wires
// `respect_gitignore`.
//
// Test List:
// 1. an exclude glob prunes matching files from the walk (slice 1)
// 2. an include glob restricts the walk to only matching files (slice 2)
// 3. a file matched by BOTH include and exclude is excluded (slice 2,
//    exclude wins)
// 4. respect_gitignore=true drops a file listed in a `.gitignore` sitting
//    in the walked directory (slice 3)
// 5. respect_gitignore=false (explicit) still lists a gitignored file
//    (slice 3)
// 6. an invalid glob syntax in the filter surfaces as an AnalysisError, not
//    a panic (AC4 — hostile config)

fn isolated_walk_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_walk_filter_test_{}_{}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated walk dir");
    dir
}

#[test]
fn exclude_glob_prunes_matching_files_from_the_walk() {
    let dir = isolated_walk_dir("exclude");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::create_dir_all(dir.join("generated")).unwrap();
    std::fs::write(dir.join("generated").join("drop.rs"), "fn drop_fn() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec!["generated/**".to_string()], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("keep.rs")),
        "keep.rs must still be listed, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("drop.rs")),
        "drop.rs must be excluded, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn include_glob_restricts_the_walk_to_matching_files() {
    let dir = isolated_walk_dir("include");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::write(dir.join("other.rs"), "fn other() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec!["src/**".to_string()], vec![], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("keep.rs")),
        "src/keep.rs must be listed, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("other.rs")),
        "other.rs is outside include, must be dropped, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_matched_by_both_include_and_exclude_is_excluded() {
    let dir = isolated_walk_dir("both_match");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("both.rs"), "fn both() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(
        vec!["src/**".to_string()],
        vec!["src/both.rs".to_string()],
        false,
    )
    .unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        !files.iter().any(|f| f.ends_with("both.rs")),
        "a file matched by both include and exclude must be excluded (exclude wins), got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn respect_gitignore_true_drops_a_gitignored_file() {
    let dir = isolated_walk_dir("gitignore_true");
    std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("ignored.rs"), "fn ignored() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec![], true).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("kept.rs")),
        "kept.rs must still be listed, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("ignored.rs")),
        "ignored.rs must be dropped when respect_gitignore is true, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn respect_gitignore_false_still_lists_a_gitignored_file() {
    let dir = isolated_walk_dir("gitignore_false");
    std::fs::write(dir.join(".gitignore"), "ignored.rs\n").unwrap();
    std::fs::write(dir.join("ignored.rs"), "fn ignored() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec![], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("ignored.rs")),
        "respect_gitignore=false must still list ignored.rs, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_glob_syntax_in_filter_errors_instead_of_panicking() {
    let dir = isolated_walk_dir("invalid_glob");
    std::fs::write(dir.join("a.rs"), "fn a() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    // `[` opens a character class that is never closed — invalid glob
    // syntax globset rejects at compile time.
    let filter = FileFilter::new(vec!["src/[".to_string()], vec![], false).unwrap();
    let result = reader.list_source_files(&dir, &["rs"], &filter);

    assert!(
        result.is_err(),
        "an invalid glob pattern must surface as an error, got {:?}",
        result
    );
    let _ = std::fs::remove_dir_all(&dir);
}
