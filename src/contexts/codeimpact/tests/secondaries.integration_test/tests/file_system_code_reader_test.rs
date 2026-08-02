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
    // Nested two levels deep (perf_fixture_subtree/dN/eM/), not flat: the
    // walk-time win comes from PRUNING descent past the first excluded
    // level, so the fixture must have enough sub-levels below that first
    // match for a regression (full recursive descent) to actually cost
    // extra directory reads. A flat "<subtree>/<20000 single-file dirs>"
    // shape under-measures this, since both walks pay the same single
    // readdir into the subtree either way.
    //
    // Deliberately NOT named "target" (#34 T2 follow-up): target/** is now
    // itself a DEFAULT_EXCLUDES entry, so FileFilter::unrestricted() below
    // — this test's "full enumeration" BASELINE — would silently exclude
    // it too, collapsing the comparison to 1 vs 1 instead of 1 vs 2501.
    let dir = isolated_walk_dir("exclude_perf_smoke");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    let excluded_root = dir.join("perf_fixture_subtree");
    for i in 0..50 {
        for j in 0..50 {
            let sub = excluded_root.join(format!("d{i}")).join(format!("e{j}"));
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("f.rs"), "fn f() {}").unwrap();
        }
    }

    let reader = FileSystemCodeReader::new();
    let exclude_filter =
        FileFilter::new(vec![], vec!["perf_fixture_subtree/**".to_string()], false).unwrap();

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
        2_501,
        "the unrestricted walk must still enumerate every file (sanity check \
         on the fixture itself), got {} files",
        full_files.len()
    );
    assert!(
        excluded_elapsed * 3 < full_elapsed,
        "excluding a subtree nested two levels deep (50x50 directories) must \
         be pruned during descent, not fully enumerated then filtered — \
         excluded walk took {:?}, full walk took {:?} (expected the excluded \
         walk to be well under a third of the full walk, generous margin \
         below the reported 34x, regression alarm for the reported \
         slowdown)",
        excluded_elapsed,
        full_elapsed
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Ticket #96, retry #1 (QA CRITICAL, reproduced) — the migration to
// walk-time `ignore::overrides::Override` pruning silently changed the glob
// DIALECT for `exclude`, not just its evaluation point. Pre-#96, `exclude`
// compiled via `globset::Glob::new` and matched the ENTIRE relative path
// (anchored/exact — ADR-0019 §4). `ignore`'s gitignore-line syntax differs
// on two points that both change RESULTS, not just performance:
//   1. a pattern with NO `/` matches the file's BASENAME AT ANY DEPTH
//      (gitignore semantics), not the literal full relative path (globset
//      semantics) — so a bare `"generated"` prunes the whole `generated/`
//      subtree under the new dialect, where the old one only ever matched a
//      top-level entry literally named "generated".
//   2. a single `*` NEVER crosses a `/` in gitignore-line syntax, but DOES
//      cross it in globset's default `Glob` (`literal_separator` defaults
//      to `false`) — so an anchored pattern like `"src/*.rs"` only pruned
//      *direct* children of `src/` under the new dialect, where the old one
//      matched the `*` as absorbing any nested path too.
//
// Fix: only patterns of the EXACT shape `<literal>/**` (a literal prefix —
// no `*`/`?`/`[`/`]`/`!`/`{`/`}` — followed by a trailing recursive `/**`)
// are dialect-safe for walk-time pruning: both `globset`'s own doc comment
// ("if the glob ends with /**, then it matches all sub-entries... but not
// foo") and the gitignore spec ("a trailing '/**' matches everything
// inside... with infinite depth") describe the IDENTICAL semantics for that
// one shape, and neither dialect's single-`*` behavior comes into play
// since the only wildcard used is the trailing `**`. Every other exclude
// pattern shape falls back to the pre-#96 post-walk `globset` match, which
// is byte-identical to before by construction (same code path, never
// migrated). Verified empirically against both crates (globset 0.4.19,
// ignore 0.4.31) before writing this fix.
//
// Test List (result-identity, not new behavior):
// 1. a bare-name exclude (no `/`) must NOT prune a subtree it would not
//    have matched pre-#96 (the QA-reproduced crux)
// 2. an anchored single-star exclude (`"src/*.rs"`) must still exclude a
//    file nested one level deeper than the star, exactly like the old
//    globset match did (same root cause as #1, same-shape sweep — the
//    single-`*`-crossing-`/` divergence, not just the bare-name one)
// 3. the `target/**` walk-time pruning win (perf motivation) must still
//    hold — already covered above by
//    `exclude_prunes_a_large_nested_subtree_relative_to_full_enumeration`,
//    which must stay green after this fix

#[test]
fn bare_name_exclude_does_not_prune_a_subtree_it_would_not_have_matched_pre_migration() {
    let dir = isolated_walk_dir("bare_name_dialect_parity");
    std::fs::write(dir.join("keep.rs"), "fn keep() {}").unwrap();
    std::fs::create_dir_all(dir.join("generated")).unwrap();
    std::fs::write(dir.join("generated").join("drop.rs"), "fn drop_fn() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    // Bare pattern, no `/` — pre-#96 `globset::Glob::new("generated")` only
    // matches a relative path that is LITERALLY "generated" (no wildcard to
    // expand), so it never matched the nested "generated/drop.rs". Both
    // files must survive, exactly as before #96.
    let filter = FileFilter::new(vec![], vec!["generated".to_string()], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("keep.rs")),
        "keep.rs must still be listed, got {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.ends_with("drop.rs")),
        "a bare-name exclude must NOT prune a nested file it would not have \
         matched under the pre-#96 anchored globset semantics (ADR-0019 §4) \
         — result set must stay identical to before the perf migration, got {:?}",
        files
    );
    assert_eq!(
        files.len(),
        2,
        "both files must survive an exclude pattern that never matched \
         either of them pre-#96, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn anchored_single_star_exclude_still_crosses_directories_like_pre_migration_globset() {
    let dir = isolated_walk_dir("anchored_single_star_dialect_parity");
    std::fs::create_dir_all(dir.join("src").join("sub")).unwrap();
    std::fs::write(dir.join("src").join("direct.rs"), "fn direct() {}").unwrap();
    std::fs::write(
        dir.join("src").join("sub").join("nested.rs"),
        "fn nested() {}",
    )
    .unwrap();
    std::fs::write(dir.join("other.rs"), "fn other() {}").unwrap();

    let reader = FileSystemCodeReader::new();
    // Anchored (contains `/`) but uses a single `*`, not `**` — pre-#96
    // `globset::Glob::new("src/*.rs")` matched the FULL relative path with
    // `literal_separator` defaulting to `false`, so `*` crossed the `/`
    // before `sub/` too: both direct.rs and sub/nested.rs were excluded.
    // gitignore-line syntax's single `*` never crosses `/`, so a naive
    // walk-time-only migration would wrongly keep sub/nested.rs.
    let filter = FileFilter::new(vec![], vec!["src/*.rs".to_string()], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["rs"], &filter)
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("other.rs")),
        "other.rs is outside src/, must survive, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("direct.rs")),
        "src/direct.rs must still be excluded, got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("nested.rs")),
        "src/sub/nested.rs must still be excluded — a single `*` crossed `/` \
         under the pre-#96 globset semantics (literal_separator=false), so \
         the migration must preserve that result even though gitignore-line \
         syntax's `*` would not cross `/` on its own, got {:?}",
        files
    );
    assert_eq!(
        files.len(),
        1,
        "only other.rs should survive, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
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

// #34 T2 (US17) — DEFAULT_EXCLUDES (FileFilter::unrestricted()/new(), #34 T2)
// wired through the real filesystem walk: `node_modules/` (any depth),
// `dist/` (root only, by ruling), and `*.min.js` (any depth) must never be
// analyzed, with or without a `.codeimpact.json`.
//
// Test List:
// 1. a project tree containing node_modules/, dist/, a *.min.js file, a
//    nested packages/x/node_modules/, and a real source file -> only the
//    real source file is listed, via FileFilter::unrestricted() (the
//    no-config path). `list_source_files` does not care whether a filter
//    came from unrestricted() or new() — both funnel through the same
//    exclude() getter — so this single adapter-level test is sufficient
//    proof that the union reaches the walk; new()'s own union behavior is
//    already pinned at the VO level in file_filter_test.rs.
// 2. the real proof of WHY this matters (MAX_WALK_ENTRIES, #34 T2 tech
//    spec): a NESTED node_modules/ (packages/x/node_modules/) holding more
//    files than MAX_WALK_ENTRIES must NOT abort the walk — only reachable
//    if `**/node_modules/**` is pruned at WALK TIME (is_dialect_safe_
//    prune_pattern's new `**/<literal>/**` shape), since a post-walk-only
//    fallback would still visit (and count) every file underneath it
//    before filtering, tripping the cap. This is the test that actually
//    discriminates the `**/<literal>/**` extension — test 1 above would
//    stay green even without it, since the post-walk `GlobSet` fallback
//    already matches `**/node_modules/**` correctly for a small fixture.

// @scenario: typescript-javascript-analysis/S5
#[test]
fn default_excludes_drop_node_modules_dist_and_minified_files_via_unrestricted_filter() {
    let dir = isolated_walk_dir("default_excludes");
    std::fs::create_dir_all(dir.join("node_modules")).unwrap();
    std::fs::write(dir.join("node_modules").join("a.js"), "var a=1;").unwrap();
    std::fs::create_dir_all(dir.join("dist")).unwrap();
    std::fs::write(dir.join("dist").join("b.js"), "var b=1;").unwrap();
    std::fs::write(dir.join("c.min.js"), "var c=1;").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("d.ts"), "const d = 1;").unwrap();
    std::fs::create_dir_all(dir.join("packages").join("x").join("node_modules")).unwrap();
    std::fs::write(
        dir.join("packages")
            .join("x")
            .join("node_modules")
            .join("e.js"),
        "var e=1;",
    )
    .unwrap();

    let reader = FileSystemCodeReader::new();
    let files = reader
        .list_source_files(&dir, &["js", "ts"], &FileFilter::unrestricted())
        .expect("walk should succeed");

    assert!(
        files.iter().any(|f| f.ends_with("d.ts")),
        "src/d.ts must survive, got {:?}",
        files
    );
    assert_eq!(
        files.len(),
        1,
        "only src/d.ts should survive the default excludes, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn populate_flat_files_with_extension(dir: &Path, count: usize, extension: &str) {
    for i in 0..count {
        std::fs::write(dir.join(format!("f{i}.{extension}")), "").expect("create fixture file");
    }
}

// @scenario: typescript-javascript-analysis/S5
#[test]
fn a_nested_node_modules_over_the_entry_cap_is_pruned_at_walk_time_not_counted() {
    let dir = isolated_walk_dir("nested_node_modules_over_cap");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("keep.ts"), "const keep = 1;").unwrap();
    let nested_node_modules = dir.join("packages").join("x").join("node_modules");
    std::fs::create_dir_all(&nested_node_modules).unwrap();
    // One entry OVER MAX_WALK_ENTRIES (50_000, production) — if this
    // subtree were only filtered post-walk (not pruned during descent),
    // the walker would still visit every one of these entries and abort
    // before src/keep.ts is ever reached.
    populate_flat_files_with_extension(&nested_node_modules, 50_001, "js");

    let reader = FileSystemCodeReader::new();
    let files = reader
        .list_source_files(&dir, &["js", "ts"], &FileFilter::unrestricted())
        .expect(
            "a nested node_modules/ over MAX_WALK_ENTRIES must be pruned at \
             walk time, not fully enumerated then filtered — the walk must \
             succeed",
        );

    assert!(
        files.iter().any(|f| f.ends_with("keep.ts")),
        "src/keep.ts must survive, got {:?}",
        files
    );
    assert_eq!(
        files.len(),
        1,
        "only src/keep.ts should survive, got {:?}",
        files
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Mutation-gate follow-up (#34 T2, blocking --in-diff): `cargo mutants`
// survived both `&&` -> `||` mutants inside `is_dialect_safe_prune_pattern`'s
// three-condition boolean chain (`!literal.is_empty() &&
// !literal.contains([...]) && literal.split('/').all(...)`), because every
// existing exclude pattern fed through it up to this point was EITHER a
// clean literal (all three conditions true either way — AND and OR agree)
// OR failed the earlier `strip_suffix("/**")` guard entirely (never reaches
// the chain). Neither shape can tell AND from OR.
//
// A pattern whose literal CONTAINS a wildcard char (so `contains(...)` is
// true, the middle conjunct is false) is required to discriminate — under
// `&&` it must be correctly rejected as dialect-unsafe and fall back to the
// post-walk `GlobSet`; under either mutated `||` it gets wrongly accepted as
// walk-time-safe. That misclassification is only OBSERVABLE (not just
// structurally different) when the two dialects actually disagree on the
// match for a real path — verified empirically (same two pinned crate
// versions, same method as the shape-equivalence proofs above):
// `*generated/**` against `a/generated/x.js` — globset's `*` crosses `/`
// (`literal_separator=false` by default), so the whole-path match ABSORBS
// `a/` before `generated` and excludes it; the walk-time `Override`
// (gitignore-line syntax) anchors the pattern component-by-component from
// the relative-path root, so `*generated` never aligns with the `a`
// component and `a/generated/x.js` survives instead. This is the SAME root
// cause already documented in `partition_exclude_patterns`'s doc comment
// (single `*` crossing `/` in globset but not gitignore-line syntax),
// applied to a LEADING star instead of a trailing one.
//
// Test List:
// 1. a literal containing a wildcard (`*generated/**`) must still exclude
//    via the post-walk `GlobSet` dialect (globset's `*` crossing `/`),
//    proving `is_dialect_safe_prune_pattern` correctly refused to route it
//    to the walk-time `Override` — kills both survived `&&`->`||` mutants
//    at once, since either one would wrongly keep a/generated/x.js instead

#[test]
fn wildcard_literal_before_the_trailing_star_star_uses_the_globset_fallback_not_walk_time_override()
{
    let dir = isolated_walk_dir("wildcard_literal_dialect_parity");
    std::fs::create_dir_all(dir.join("generated")).unwrap();
    std::fs::write(dir.join("generated").join("top.js"), "var x=1;").unwrap();
    std::fs::create_dir_all(dir.join("a").join("generated")).unwrap();
    std::fs::write(
        dir.join("a").join("generated").join("nested.js"),
        "var x=1;",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("gen2")).unwrap();
    std::fs::write(dir.join("gen2").join("kept.js"), "var x=1;").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("keep.ts"), "const keep = 1;").unwrap();

    let reader = FileSystemCodeReader::new();
    let filter = FileFilter::new(vec![], vec!["*generated/**".to_string()], false).unwrap();
    let files = reader
        .list_source_files(&dir, &["js", "ts"], &filter)
        .expect("walk should succeed");

    assert!(
        !files.iter().any(|f| f.ends_with("top.js")),
        "generated/top.js must be excluded (matches *generated literally), got {:?}",
        files
    );
    assert!(
        !files.iter().any(|f| f.ends_with("nested.js")),
        "a/generated/nested.js must be excluded too — globset's `*` crosses \
         `/` and absorbs the a/ prefix (the correct post-walk GlobSet \
         fallback dialect for a literal containing a wildcard). If \
         `is_dialect_safe_prune_pattern` wrongly routed this pattern to the \
         walk-time Override instead, gitignore-line anchoring would NOT \
         cross the a/ component and this file would wrongly survive, got {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.ends_with("kept.js")),
        "gen2/kept.js does not match *generated, must survive, got {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.ends_with("keep.ts")),
        "src/keep.ts must survive, got {:?}",
        files
    );
    assert_eq!(files.len(), 2, "got {:?}", files);
    let _ = std::fs::remove_dir_all(&dir);
}
