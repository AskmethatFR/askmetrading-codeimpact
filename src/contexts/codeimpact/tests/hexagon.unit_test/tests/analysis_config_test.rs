use codeimpact_hexagon::analysis::{
    AlertThresholds, AnalysisConfig, AnalysisConfigError, FileFilter,
};

// Test List (US31 — AnalysisConfig is pure composition of two
// already-validated VOs, no invariant of its own beyond field storage):
//
// 1. defaults() -> AlertThresholds::none() + FileFilter::unrestricted()
//    (D4: reproduces today's behavior byte-for-byte)
// 2. new() stores and exposes exactly the thresholds/filter given
//
// Test List (US16 T4.3 — additive io_signature_prefixes field):
// 3. defaults() -> io_signature_prefixes() is empty
// 4. with_io_signature_prefixes() stores and exposes exactly what was given
//
// Test List (retry #1, Security MEDIUM — ioSignatures is unbounded, glob-DoS
// class, mirrors FileFilter's MAX_PATTERN_COUNT/MAX_PATTERN_LENGTH):
// 5. over MAX_IO_SIGNATURE_COUNT prefixes -> Err(TooManyIoSignaturePrefixes)
// 6. one prefix over MAX_IO_SIGNATURE_LENGTH chars -> Err(IoSignaturePrefixTooLong)
// 7. exactly at both caps -> Ok (boundary, not off-by-one)
//
// Test List (US16 T5, Q2 — sourceRoots wiring):
// 8. defaults() -> source_roots() is empty (absent -> defer to run_analysis's
//    own project_root fallback, D4-style byte-for-byte with pre-T5 behavior)
// 9. with_source_roots() overrides the default, new() itself still starts empty

#[test]
fn defaults_has_no_thresholds_and_unrestricted_filter() {
    let config = AnalysisConfig::defaults();

    assert_eq!(config.thresholds(), &AlertThresholds::none());
    assert_eq!(config.file_filter(), &FileFilter::unrestricted());
    assert!(config.io_signature_prefixes().is_empty());
}

#[test]
fn with_io_signature_prefixes_stores_and_exposes_exactly_what_was_given() {
    let config = AnalysisConfig::defaults()
        .with_io_signature_prefixes(vec!["MyIoWrapper.".to_string()])
        .unwrap();

    assert_eq!(
        config.io_signature_prefixes(),
        &["MyIoWrapper.".to_string()]
    );
}

#[test]
fn too_many_io_signature_prefixes_is_rejected() {
    let prefixes: Vec<String> = (0..257).map(|i| format!("P{}.", i)).collect();

    let result = AnalysisConfig::defaults().with_io_signature_prefixes(prefixes);

    assert!(
        matches!(
            result,
            Err(AnalysisConfigError::TooManyIoSignaturePrefixes(257))
        ),
        "expected TooManyIoSignaturePrefixes(257), got {:?}",
        result
    );
}

#[test]
fn an_over_length_io_signature_prefix_is_rejected() {
    let too_long = "A".repeat(257);

    let result = AnalysisConfig::defaults().with_io_signature_prefixes(vec![too_long.clone()]);

    assert!(
        matches!(
            result,
            Err(AnalysisConfigError::IoSignaturePrefixTooLong(ref p)) if *p == too_long
        ),
        "expected IoSignaturePrefixTooLong, got {:?}",
        result
    );
}

#[test]
fn exactly_at_both_caps_is_accepted() {
    let prefixes: Vec<String> = (0..256).map(|i| format!("{:0>3}", i)).collect();
    let at_length_cap = "A".repeat(256);

    let result = AnalysisConfig::defaults().with_io_signature_prefixes(prefixes.clone());
    assert!(
        result.is_ok(),
        "256 prefixes must be accepted, got {:?}",
        result
    );

    let result = AnalysisConfig::defaults().with_io_signature_prefixes(vec![at_length_cap]);
    assert!(
        result.is_ok(),
        "a 256-char prefix must be accepted, got {:?}",
        result
    );
}

#[test]
fn new_exposes_the_given_thresholds_and_filter() {
    let thresholds = AlertThresholds::new(Some(1.0), None).unwrap();
    let filter = FileFilter::new(vec!["src/**".to_string()], vec![], true).unwrap();

    let config = AnalysisConfig::new(thresholds, filter.clone());

    assert_eq!(config.thresholds(), &thresholds);
    assert_eq!(config.file_filter(), &filter);
}

#[test]
fn defaults_has_empty_source_roots() {
    let config = AnalysisConfig::defaults();

    assert!(config.source_roots().is_empty());
}

#[test]
fn with_source_roots_overrides_the_default_empty_list() {
    let thresholds = AlertThresholds::none();
    let filter = FileFilter::unrestricted();

    let config = AnalysisConfig::new(thresholds, filter).with_source_roots(vec!["src".to_string()]);

    assert_eq!(config.source_roots(), &["src".to_string()]);
}
