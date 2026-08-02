use codeimpact_hexagon::analysis::{FileFilter, FileFilterError};

// Test List (US31, D1 — FileFilter is an autonomous VO, test-ddd-tactical
// Entry Gate: validation is a public invariant several adapters/use cases
// rely on, not an internal detail of a single use case):
//
// 1. unrestricted() -> empty include, gitignore off (D4: reproduces today's
//    behavior byte-for-byte for include/gitignore; exclude is NOT empty
//    any more — see #34 T2 below)
// 2. new() with valid patterns succeeds, getters return exactly what was
//    given, plus the unioned defaults on exclude (#34 T2)
// 3. (parametrized) each invalid pattern shape is rejected: empty, interior
//    NUL, absolute path, ".." component, over-length
// 4. the too-many-patterns cap is enforced independently of any single
//    pattern's own validity
// 5. the error names the offending pattern (Display)
// 6. (review-barrier retry 1, minor) exact-boundary "still accepted" pins:
//    a pattern of exactly 512 chars must SUCCEED — discriminates a `>=` vs
//    `>` mutant on the length cap, which the 513-rejection test above alone
//    cannot catch
//
// Test List (#34 T2 — DEFAULT_EXCLUDES, ddd-value-object: the invariant is
// enforced AT CONSTRUCTION so no caller — no-config path via unrestricted(),
// config-file path via new() — can bypass it):
// 7. unrestricted() carries every DEFAULT_EXCLUDES entry (per-entry checks,
//    never just "non-empty" — a dropped entry must fail this test)
// 8. new() with an empty exclude list still carries every DEFAULT_EXCLUDES
//    entry (per-entry checks)
// 9. new() unions a user-supplied exclude pattern WITH the defaults (user
//    pattern survives, every default entry is also present)
// 10. a user pattern that already equals a default exclude is not
//     duplicated in the union
// 11. MAX_PATTERN_COUNT is enforced on the UNION, not the user's list alone:
//     a user list leaving exactly enough room for the 5 defaults (251) is
//     accepted; the former exact-cap boundary (256 user patterns) now
//     legitimately fails once the defaults are unioned in
//
// Test List (#34 T2 follow-up — operator ruling, dogfooding this repo at
// full scale: `target/**` was ruled OUT in the original tech spec on the
// ground that it would change behavior for existing Rust projects; running
// the dogfood proof against the WHOLE repository rather than the ticket's
// named subtree showed `target/` — not `node_modules/` — is what actually
// blows MAX_WALK_ENTRIES for a built Rust repo, so the ruling now adds it):
// 12. `target/**` is present in DEFAULT_EXCLUDES too (same per-entry
//     discrimination bar as the other four — a test that stays green with
//     `target/**` dropped from the list is not good enough)
// 13. the MAX_PATTERN_COUNT boundary moves again with 5 defaults (251, not
//     252) — updated deliberately below, visible in the diff

#[test]
fn unrestricted_has_no_include_patterns_and_gitignore_off() {
    let filter = FileFilter::unrestricted();
    assert!(filter.include().is_empty());
    assert!(!filter.respect_gitignore());
}

#[test]
fn unrestricted_carries_every_default_exclude_entry() {
    let filter = FileFilter::unrestricted();

    assert!(
        filter.exclude().iter().any(|p| p == "node_modules/**"),
        "missing node_modules/**, got {:?}",
        filter.exclude()
    );
    assert!(
        filter.exclude().iter().any(|p| p == "**/node_modules/**"),
        "missing **/node_modules/**, got {:?}",
        filter.exclude()
    );
    assert!(
        filter.exclude().iter().any(|p| p == "dist/**"),
        "missing dist/**, got {:?}",
        filter.exclude()
    );
    assert!(
        filter.exclude().iter().any(|p| p == "**/*.min.js"),
        "missing **/*.min.js, got {:?}",
        filter.exclude()
    );
    assert!(
        filter.exclude().iter().any(|p| p == "target/**"),
        "missing target/** (operator ruling — dogfooding this repo at full \
         scale showed target/, not node_modules/, is what blows \
         MAX_WALK_ENTRIES for a built Rust repo), got {:?}",
        filter.exclude()
    );
    assert_eq!(
        filter.exclude().len(),
        5,
        "no unexpected extra entries, got {:?}",
        filter.exclude()
    );
}

#[test]
fn new_with_valid_patterns_exposes_include_and_unions_exclude_with_defaults() {
    let filter = FileFilter::new(
        vec!["src/**".to_string()],
        vec!["coverage/**".to_string()],
        true,
    )
    .expect("valid patterns must construct");

    assert_eq!(filter.include(), &["src/**".to_string()]);
    assert!(filter.exclude().iter().any(|p| p == "coverage/**"));
    assert!(filter.exclude().iter().any(|p| p == "node_modules/**"));
    assert!(filter.exclude().iter().any(|p| p == "**/node_modules/**"));
    assert!(filter.exclude().iter().any(|p| p == "dist/**"));
    assert!(filter.exclude().iter().any(|p| p == "**/*.min.js"));
    assert!(filter.exclude().iter().any(|p| p == "target/**"));
    assert_eq!(filter.exclude().len(), 6, "got {:?}", filter.exclude());
    assert!(filter.respect_gitignore());
}

#[test]
fn new_with_empty_exclude_still_carries_every_default_exclude_entry() {
    let filter = FileFilter::new(vec![], vec![], false).expect("empty lists must construct");

    assert!(filter.exclude().iter().any(|p| p == "node_modules/**"));
    assert!(filter.exclude().iter().any(|p| p == "**/node_modules/**"));
    assert!(filter.exclude().iter().any(|p| p == "dist/**"));
    assert!(filter.exclude().iter().any(|p| p == "**/*.min.js"));
    assert!(
        filter.exclude().iter().any(|p| p == "target/**"),
        "missing target/**, got {:?}",
        filter.exclude()
    );
    assert_eq!(filter.exclude().len(), 5, "got {:?}", filter.exclude());
}

#[test]
fn new_does_not_duplicate_a_user_pattern_that_already_equals_a_default_exclude() {
    let filter = FileFilter::new(vec![], vec!["dist/**".to_string()], false)
        .expect("valid pattern must construct");

    let dist_occurrences = filter.exclude().iter().filter(|p| *p == "dist/**").count();
    assert_eq!(
        dist_occurrences,
        1,
        "dist/** must appear exactly once, not duplicated with the default, got {:?}",
        filter.exclude()
    );
    assert_eq!(
        filter.exclude().len(),
        5,
        "the union must still total exactly the 5 DEFAULT_EXCLUDES entries \
         (the user's dist/** collapsed into the matching default), got {:?}",
        filter.exclude()
    );
}

#[test]
fn invalid_pattern_shapes_are_all_rejected() {
    let invalid_patterns = [
        "",
        "bad\0pattern",
        "/etc/passwd",
        "../etc/**",
        &"a".repeat(513),
    ];

    for pattern in invalid_patterns {
        let result = FileFilter::new(vec![pattern.to_string()], vec![], false);
        assert!(
            result.is_err(),
            "pattern {:?} must be rejected, got {:?}",
            pattern,
            result
        );
    }
}

#[test]
fn absolute_pattern_is_rejected_with_the_precise_variant() {
    let result = FileFilter::new(vec!["/etc/passwd".to_string()], vec![], false);
    match result {
        Err(FileFilterError::AbsolutePattern(p)) => assert_eq!(p, "/etc/passwd"),
        other => panic!("expected AbsolutePattern, got {:?}", other),
    }
}

#[test]
fn parent_traversal_pattern_is_rejected_with_the_precise_variant() {
    let result = FileFilter::new(vec![], vec!["../etc/**".to_string()], false);
    match result {
        Err(FileFilterError::ParentTraversalPattern(p)) => assert_eq!(p, "../etc/**"),
        other => panic!("expected ParentTraversalPattern, got {:?}", other),
    }
}

#[test]
fn too_many_patterns_is_rejected_even_when_each_pattern_is_individually_valid() {
    let include: Vec<String> = (0..257).map(|i| format!("src/mod_{}/**", i)).collect();
    let result = FileFilter::new(include, vec![], false);
    match result {
        // 257 user include patterns + 5 unioned DEFAULT_EXCLUDES = 262: the
        // cap is enforced on the UNION, not the user's list alone (#34 T2).
        Err(FileFilterError::TooManyPatterns(count)) => assert_eq!(count, 262),
        other => panic!("expected TooManyPatterns(262), got {:?}", other),
    }
}

#[test]
fn pattern_of_exactly_the_max_length_is_still_accepted() {
    let pattern = "a".repeat(512);
    let result = FileFilter::new(vec![pattern.clone()], vec![], false);
    assert!(
        result.is_ok(),
        "a 512-char pattern (the exact cap) must be accepted, got {:?}",
        result
    );
    assert_eq!(result.unwrap().include(), &[pattern]);
}

#[test]
fn user_pattern_count_leaving_exact_room_for_the_defaults_is_still_accepted() {
    // 256 (cap) - 5 (DEFAULT_EXCLUDES, now including target/**) = 251: the
    // union lands exactly at the cap, discriminating a `>=` vs `>` mutant
    // the same way the pre-#34 boundary test used to, but against the
    // UNIONED total (#34 T2, moved again by the operator's target/** ruling).
    let include: Vec<String> = (0..251).map(|i| format!("src/mod_{}/**", i)).collect();
    let result = FileFilter::new(include.clone(), vec![], false);
    assert!(
        result.is_ok(),
        "251 user patterns + 5 defaults = 256 (exact cap) must be accepted, got {:?}",
        result
    );
    assert_eq!(result.unwrap().include().len(), 251);
}

#[test]
fn a_user_pattern_count_at_the_former_exact_cap_now_fails_once_defaults_are_unioned() {
    // Pre-#34, 256 user patterns with no exclude sat exactly at the cap and
    // was accepted (see the old exactly_the_max_pattern_count_is_still_accepted
    // test). Folding DEFAULT_EXCLUDES into new() adds 5 more entries to the
    // union (now including target/**), pushing the total to 261 — this is
    // the exact regression the tech spec calls out and requires pinning
    // with a test.
    let include: Vec<String> = (0..256).map(|i| format!("src/mod_{}/**", i)).collect();
    let result = FileFilter::new(include, vec![], false);
    match result {
        Err(FileFilterError::TooManyPatterns(count)) => assert_eq!(count, 261),
        other => panic!(
            "256 user patterns + 5 unioned defaults must now exceed the cap \
             with TooManyPatterns(261), got {:?}",
            other
        ),
    }
}

#[test]
fn error_display_names_the_offending_pattern() {
    let err = FileFilter::new(vec!["/etc/passwd".to_string()], vec![], false).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("/etc/passwd"),
        "error message must name the offending pattern: {}",
        message
    );
}
