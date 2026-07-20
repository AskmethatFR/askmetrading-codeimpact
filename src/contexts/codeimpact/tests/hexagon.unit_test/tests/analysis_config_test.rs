use codeimpact_hexagon::analysis::{AlertThresholds, AnalysisConfig, FileFilter};

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

#[test]
fn defaults_has_no_thresholds_and_unrestricted_filter() {
    let config = AnalysisConfig::defaults();

    assert_eq!(config.thresholds(), &AlertThresholds::none());
    assert_eq!(config.file_filter(), &FileFilter::unrestricted());
    assert!(config.io_signature_prefixes().is_empty());
}

#[test]
fn with_io_signature_prefixes_stores_and_exposes_exactly_what_was_given() {
    let config =
        AnalysisConfig::defaults().with_io_signature_prefixes(vec!["MyIoWrapper.".to_string()]);

    assert_eq!(
        config.io_signature_prefixes(),
        &["MyIoWrapper.".to_string()]
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
