use std::path::Path;
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

// Review-barrier retry 1 (QA CRITICAL) — `ignore::WalkBuilder` exposes FOUR
// independent ignore-source toggles (git_ignore, git_exclude, git_global,
// ignore), all defaulting to `true`. Gating only `git_ignore` on
// `respect_gitignore` left the other three ON unconditionally, silently
// dropping files even under `FileFilter::unrestricted()` — a regression
// against the pre-US31 `walkdir` behavior, which honored none of them.
//
// Test List:
// 1. a `.ignore` file must not drop a file when the filter is unrestricted
//    (no config file at all — D4)
// 2. a `.ignore` file must not drop a file when respect_gitignore is
//    explicitly false
// 3. a `.git/info/exclude` entry must not drop a file under the same two
//    conditions (git_exclude source)

#[test]
fn dot_ignore_file_does_not_drop_files_under_unrestricted_filter() {
    let dir = isolated_walk_dir("dot_ignore_unrestricted");
    std::fs::write(dir.join(".ignore"), "secret.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("secret.rs"), "fn secret() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let files = reader
        .list_source_files(&dir, &["rs"], &FileFilter::unrestricted())
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("secret.rs")),
        "a .ignore file must have NO effect under FileFilter::unrestricted() \
         (byte-identical to the pre-US31 walkdir walk), got {:?}",
        files
    );
    assert_eq!(files.len(), 2, "both files must be listed, got {:?}", files);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dot_ignore_file_does_not_drop_files_when_respect_gitignore_is_false() {
    let dir = isolated_walk_dir("dot_ignore_explicit_false");
    std::fs::write(dir.join(".ignore"), "secret.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("secret.rs"), "fn secret() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec![], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert_eq!(
        files.len(),
        2,
        "respect_gitignore=false must disable the .ignore source too, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn git_info_exclude_does_not_drop_files_under_unrestricted_filter() {
    let dir = isolated_walk_dir("git_exclude_unrestricted");
    std::fs::create_dir_all(dir.join(".git").join("info")).unwrap();
    std::fs::write(dir.join(".git").join("info").join("exclude"), "secret.rs\n").unwrap();
    std::fs::write(dir.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("secret.rs"), "fn secret() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let files = reader
        .list_source_files(&dir, &["rs"], &FileFilter::unrestricted())
        .expect("walk should succeed");

    assert_eq!(
        files.len(),
        2,
        "a .git/info/exclude entry must have NO effect under \
         FileFilter::unrestricted(), got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Review-barrier retry 1 (Security MEDIUM) — `.parents(true)` made the
// walker read .gitignore/.ignore from EVERY ancestor directory up to `/`.
// On a shared host, a party outside the analyzed directory could plant a
// .gitignore in a parent dir to hide source files and evade the --strict
// energy/CO2 gate (ADR-0017). The walker must never consult ignore state
// from outside the walked directory.

#[test]
fn gitignore_in_an_ancestor_directory_above_the_walk_root_has_zero_effect() {
    let parent = isolated_walk_dir("ancestor_gitignore_parent");
    let root = parent.join("walked_root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(parent.join(".gitignore"), "secret.rs\n").unwrap();
    std::fs::write(root.join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(root.join("secret.rs"), "fn secret() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec![], true).unwrap();
    let files = reader
        .list_source_files(&root, &["rs"], &filter)
        .expect("walk should succeed");

    assert_eq!(
        files.len(),
        2,
        "a .gitignore ABOVE the walk root must have zero effect on the file \
         list, even with respect_gitignore=true, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&parent);
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

// QA MEDIUM (US16 T5 retry #2) — `canonical_root` (introduced retry #1
// to fix the sourceRoots canonicalization mismatch, Security CRITICAL)
// had zero direct test coverage of its own: the Ok path was only
// exercised INDIRECTLY through other tests that happen to pass an
// existing dir, and the Err/fallback path was never exercised at all.
//
// Test List:
// 1. an existing directory -> canonical_root returns the SAME value as
//    std::fs::canonicalize (the Ok path)
// 2. a path that does not exist on disk -> canonical_root falls back to
//    the input UNCHANGED (identity) rather than propagating the error or
//    panicking — a mutation from `.unwrap_or_else(...)` to `.unwrap()`
//    must fail this test with a panic, not silently pass

#[test]
fn canonical_root_of_an_existing_dir_matches_std_fs_canonicalize() {
    let dir = isolated_walk_dir("canonical_root_existing");

    let reader = FileSystemCodeReader::new();
    let result = reader.canonical_root(&dir);

    assert_eq!(
        result,
        std::fs::canonicalize(&dir).expect("the temp dir must exist on disk")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn canonical_root_of_a_nonexistent_path_falls_back_to_identity() {
    let missing = PathBuf::from("/this/path/definitely/does/not/exist/__codeimpact_t5__");

    let reader = FileSystemCodeReader::new();
    let result = reader.canonical_root(&missing);

    assert_eq!(
        result, missing,
        "canonicalize must fail for a nonexistent path — the fallback must \
         return the input unchanged, not panic or propagate the error"
    );
}

// #95 (Security DoS residual) — `list_source_files` bounded recursion depth
// (MAX_WALK_DEPTH) and per-file size (MAX_FILE_SIZE) but not the TOTAL
// number of entries enumerated. A directory with many small files at
// shallow depth (a planted/generated tree, plausibly under
// respectGitignore:false) was fully enumerated with no early abort. A
// total-entries cap (`MAX_WALK_ENTRIES` in file_system_code_reader.rs, 50
// 000) now aborts early with an actionable error naming the limit —
// mirroring how MAX_FILE_SIZE surfaces "fichier trop volumineux (max 10
// Mo)".
//
// Test List:
// 1. a walk whose entry count exceeds the cap aborts with an Err naming
//    the limit, under BOTH respect_gitignore=false and =true (the cap
//    must not depend on the gitignore flag — one fixture, both flags)
// (below-cap -> normal Ok result is already covered by every test above:
// each walks a handful of fixture files, none anywhere near the cap)

fn populate_flat_files(dir: &Path, count: usize) {
    for i in 0..count {
        std::fs::write(dir.join(format!("f{i}.rs")), "").expect("create fixture file");
    }
}

#[test]
fn walk_exceeding_the_entry_cap_aborts_early_naming_the_limit_under_both_gitignore_modes() {
    let dir = isolated_walk_dir("entry_cap_exceeded");
    // MAX_WALK_ENTRIES (production) is 50_000 — one entry over it must
    // trip the guard.
    let over_cap_count = 50_001;
    populate_flat_files(&dir, over_cap_count);

    let reader = FileSystemCodeReader::new();

    for respect_gitignore in [false, true] {
        let filter = FileFilter::new(vec![], vec![], respect_gitignore).unwrap();
        let result = reader.list_source_files(&dir, &["rs"], &filter);

        assert!(
            result.is_err(),
            "walking {over_cap_count} files (over the entry cap) must abort \
             with an error (respect_gitignore={respect_gitignore}), got {:?}",
            result
        );
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("50000") && message.to_lowercase().contains("entr"),
            "the error must name the entries limit (respect_gitignore={respect_gitignore}), got: {message}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}
