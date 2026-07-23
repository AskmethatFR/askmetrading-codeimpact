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

// Ticket #96 (perf) — `exclude` moves from post-walk `globset` filtering to
// a walk-time `ignore::overrides::Override` (negated patterns only), so
// `ignore::WalkBuilder` PRUNES a matching subtree during descent instead of
// enumerating every entry underneath it and filtering afterward. Measured:
// excluding target/ (20.6k files) via exclude=["target/**"] with
// respectGitignore:false was ~34x slower than gitignore-based exclusion for
// the identical result set — because only gitignore-based exclusion
// pruned directory descent; exclude was post-walk only.
//
// `include` deliberately STAYS on the existing post-walk globset filter.
// Moving it to the same walk-time Override too would turn on Override's
// "whitelist mode" — per `ignore::dir::Ignore::matched`, ANY override match
// (whitelist or blacklist) short-circuits and skips gitignore entirely for
// that path. An include pattern matching a gitignored file would then
// resurrect it, a real regression. A negated-only Override (exclude alone)
// never enables whitelist mode, so it never bypasses gitignore for a path
// that doesn't match one of the exclude patterns — see
// `ignore::overrides::Override::matched`'s doc comment and the `ignore`
// crate's own `only_ignores` unit test.
//
// Most of the cases below characterize EXISTING behavior that must not
// regress during the migration (result-set identity), not new behavior —
// consistent with this being "pure perf, results stay identical to today".
// The one case that genuinely pins NEW behavior (and is expected to be red
// against the pre-#96 post-walk-only implementation) is the last one: a
// large, deeply-nested excluded subtree must be walked within a generous
// time budget, which only holds if the subtree is pruned during descent.
//
// Test List:
// 1. exclude prunes a deeply nested match, not just a top-level one
//    (regression net: override-based matching must behave like the old
//    globset `**` matching for nested paths)
// 2. exclude + include + respect_gitignore=true together: gitignore drops
//    what it always dropped, exclude drops what it always dropped,
//    independently (AND-composition unaffected by the migration)
// 3. an invalid glob syntax in `exclude` surfaces as an AnalysisError, not
//    a panic (AC4 — hostile config), pinning the NEW OverrideBuilder-based
//    validation path (mirrors the existing include-side coverage)
// 4. a large, deeply-nested excluded subtree is walked well within a
//    generous time ceiling — best-effort proof that the excluded subtree
//    is pruned during descent rather than fully enumerated and filtered
//    post-hoc (the direct regression alarm for the reported 34x slowdown)

#[test]
fn exclude_glob_prunes_a_deeply_nested_match() {
    let dir = isolated_walk_dir("exclude_nested");
    std::fs::create_dir_all(dir.join("a").join("b").join("c")).unwrap();
    std::fs::write(
        dir.join("a").join("b").join("c").join("drop.rs"),
        "fn drop_fn() {}",
    )
    .unwrap();
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec!["a/**".to_string()], false).unwrap();
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
        "a deeply nested file under an excluded path must still be excluded, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exclude_and_gitignore_compose_independently_with_include() {
    let dir = isolated_walk_dir("exclude_gitignore_include_compose");
    std::fs::write(dir.join(".gitignore"), "ignored_by_git.rs\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("kept.rs"), "fn kept() {}").unwrap();
    std::fs::write(dir.join("src").join("ignored_by_git.rs"), "fn g() {}").unwrap();
    std::fs::write(dir.join("src").join("excluded.rs"), "fn e() {}").unwrap();
    std::fs::write(dir.join("other.rs"), "fn other() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(
        vec!["src/**".to_string()],
        vec!["src/excluded.rs".to_string()],
        true,
    )
    .unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("kept.rs")),
        "kept.rs must be listed, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("ignored_by_git.rs")),
        "gitignore must still drop its own entry, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("excluded.rs")),
        "exclude must still drop its own entry, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("other.rs")),
        "other.rs is outside include, must stay dropped, got {:?}",
        files
    );
    assert_eq!(
        files.len(),
        1,
        "only kept.rs should survive, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_glob_syntax_in_exclude_errors_instead_of_panicking() {
    let dir = isolated_walk_dir("invalid_exclude_glob");
    std::fs::write(dir.join("a.rs"), "fn a() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    // `[` opens a character class that is never closed — invalid glob
    // syntax the `ignore` crate's OverrideBuilder rejects at build time.
    let filter = FileFilter::new(vec![], vec!["target/[".to_string()], false).unwrap();
    let result = reader.list_source_files(&dir, &["rs"], &filter);

    assert!(
        result.is_err(),
        "an invalid exclude glob pattern must surface as an error, got {:?}",
        result
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exclude_prunes_a_large_nested_subtree_relative_to_full_enumeration() {
    // Self-calibrating against THIS machine's speed (avoids a flaky fixed
    // millisecond ceiling across heterogeneous CI hardware): compare the
    // excluded walk against a full-enumeration walk of the SAME fixture. If
    // the excluded subtree is pruned during descent, excluding it must be
    // dramatically cheaper than fully enumerating it — if it is only
    // filtered post-walk (the pre-#96 bug), both walks pay the same
    // directory-descent cost and the ratio collapses to ~1.
    let dir = isolated_walk_dir("exclude_perf_smoke");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    let excluded_root = dir.join("target");
    for i in 0..20_000 {
        let sub = excluded_root.join(format!("d{i}"));
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f.rs"), "fn f() {}").unwrap();
    }

    let reader = FileSystemCodeReader::new();
    let exclude_filter = FileFilter::new(vec![], vec!["target/**".to_string()], false).unwrap();

    let excluded_start = std::time::Instant::now();
    let excluded_files = reader
        .list_source_files(&dir, &["rs"], &exclude_filter)
        .expect("excluded walk should succeed");
    let excluded_elapsed = excluded_start.elapsed();

    let full_start = std::time::Instant::now();
    let full_files = reader
        .list_source_files(&dir, &["rs"], &FileFilter::unrestricted())
        .expect("full walk should succeed");
    let full_elapsed = full_start.elapsed();

    assert!(
        excluded_files.iter().any(|f| f.ends_with("keep.rs")),
        "keep.rs must survive the exclude, got {:?}",
        excluded_files
    );
    assert_eq!(
        excluded_files.len(),
        1,
        "only keep.rs should survive the exclude, got {:?}",
        excluded_files
    );
    assert_eq!(
        full_files.len(),
        20_001,
        "the unrestricted walk must still enumerate every file (sanity check \
         on the fixture itself), got {} files",
        full_files.len()
    );
    assert!(
        excluded_elapsed * 3 < full_elapsed,
        "excluding a subtree with 20 000 nested directories must be pruned \
         during descent, not fully enumerated then filtered — excluded walk \
         took {:?}, full walk took {:?} (expected the excluded walk to be \
         well under a third of the full walk, generous margin below the \
         reported 34x, regression alarm for the reported slowdown)",
        excluded_elapsed,
        full_elapsed
    );
    let _ = std::fs::remove_dir_all(&dir);
}
