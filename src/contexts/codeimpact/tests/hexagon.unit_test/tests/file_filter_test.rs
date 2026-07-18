use codeimpact_hexagon::analysis::{FileFilter, FileFilterError};

// Test List (US31, D1 — FileFilter is an autonomous VO, test-ddd-tactical
// Entry Gate: validation is a public invariant several adapters/use cases
// rely on, not an internal detail of a single use case):
//
// 1. unrestricted() -> empty include, empty exclude, gitignore off (D4:
//    reproduces today's behavior byte-for-byte)
// 2. new() with valid patterns succeeds, getters return exactly what was
//    given
// 3. (parametrized) each invalid pattern shape is rejected: empty, interior
//    NUL, absolute path, ".." component, over-length
// 4. the too-many-patterns cap is enforced independently of any single
//    pattern's own validity
// 5. the error names the offending pattern (Display)

#[test]
fn unrestricted_has_no_patterns_and_gitignore_off() {
    let filter = FileFilter::unrestricted();
    assert!(filter.include().is_empty());
    assert!(filter.exclude().is_empty());
    assert!(!filter.respect_gitignore());
}

#[test]
fn new_with_valid_patterns_exposes_them_via_getters() {
    let filter = FileFilter::new(
        vec!["src/**".to_string()],
        vec!["target/**".to_string()],
        true,
    )
    .expect("valid patterns must construct");

    assert_eq!(filter.include(), &["src/**".to_string()]);
    assert_eq!(filter.exclude(), &["target/**".to_string()]);
    assert!(filter.respect_gitignore());
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
        Err(FileFilterError::TooManyPatterns(count)) => assert_eq!(count, 257),
        other => panic!("expected TooManyPatterns, got {:?}", other),
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
