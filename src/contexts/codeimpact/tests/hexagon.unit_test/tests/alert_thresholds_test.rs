use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::BreachedMetric;
use codeimpact_hexagon::analysis::ThresholdError;

// Test List (US8, AD-1 — the pure domain gate; test-ddd-tactical Entry Gate:
// AlertThresholds::evaluate is autonomous, not an internal detail of a
// single use case, so it earns direct tests):
//
// Change request on issue #8: energy (kWh) replaces CPU cost (μ$) as the
// gate's first metric. Magnitudes below are grounded in a real measurement:
// `codeimpact analyze` on the e2e fixture `sample.rs` reports
// `ecological_impact.energy_joules() == 23.4`, i.e.
// `23.4 / KWH_TO_JOULES (3_600_000.0) == 0.0000065` kWh — the tests use
// 0.00001 (1e-5) as a limit just above that real single-file magnitude, so
// the boundary/breach numbers stay in a realistic range rather than an
// arbitrary round number picked out of thin air.
//
// 1. no thresholds configured -> evaluate never breaches, any metric value
// 2. energy below limit -> no breach
// 3. energy exactly at limit (boundary) -> no breach (`>` not `>=`)
// 4. energy above limit -> breach, correct metric/limit/actual/excess
// 5. co2 above limit -> breach
// 6. both metrics breaching -> report carries both breaches
// 7. (parametrized) absent metric never breaches even with a threshold
//    configured (ADR-0010 — a missing measurement is not a confident zero)
// 8. negative energy threshold is rejected
// 9. negative co2 threshold is rejected
// 10. (parametrized) non-finite (NaN/Infinity) thresholds are rejected
// 11. a zero threshold is a valid (maximally strict) construction

#[test]
fn no_thresholds_configured_never_breaches() {
    let thresholds = AlertThresholds::none();
    let report = thresholds.evaluate(Some(1_000_000.0), Some(1_000_000.0));
    assert!(!report.has_breach());
    assert!(report.breaches().is_empty());
}

#[test]
fn energy_below_limit_does_not_breach() {
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000005), None);
    assert!(!report.has_breach());
}

#[test]
fn energy_exactly_at_limit_does_not_breach() {
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.00001), None);
    assert!(
        !report.has_breach(),
        "exceeding must be strictly greater than the limit, not equal to it"
    );
}

#[test]
fn energy_above_limit_breaches_with_the_right_numbers() {
    let thresholds = AlertThresholds::new(Some(0.00001), None).unwrap();
    let report = thresholds.evaluate(Some(0.000015), None);
    assert!(report.has_breach());
    let breaches = report.breaches();
    assert_eq!(breaches.len(), 1);
    assert_eq!(breaches[0].metric(), BreachedMetric::Energy);
    assert_eq!(breaches[0].limit(), 0.00001);
    assert_eq!(breaches[0].actual(), 0.000015);
    assert!((breaches[0].excess() - 0.000005).abs() < 1e-12);
}

#[test]
fn co2_above_limit_breaches() {
    let thresholds = AlertThresholds::new(None, Some(20.0)).unwrap();
    let report = thresholds.evaluate(None, Some(30.0));
    assert!(report.has_breach());
    assert_eq!(report.breaches().len(), 1);
    assert_eq!(report.breaches()[0].metric(), BreachedMetric::Co2);
    assert_eq!(report.breaches()[0].excess(), 10.0);
}

#[test]
fn both_metrics_breaching_reports_both() {
    let thresholds = AlertThresholds::new(Some(0.00001), Some(20.0)).unwrap();
    let report = thresholds.evaluate(Some(0.000015), Some(30.0));
    assert_eq!(report.breaches().len(), 2);
}

#[test]
fn absent_metric_never_breaches_even_with_threshold_set() {
    let thresholds = AlertThresholds::new(Some(0.00001), Some(20.0)).unwrap();
    let rows: [(Option<f64>, Option<f64>); 2] = [
        (None, None),
        (None, Some(5.0)), // co2 present but under its own limit too
    ];
    for (energy_kwh, co2) in rows {
        let report = thresholds.evaluate(energy_kwh, co2);
        assert!(
            !report.has_breach(),
            "energy_kwh={:?} co2={:?} must not breach when the metric is absent",
            energy_kwh,
            co2
        );
    }
}

#[test]
fn negative_energy_threshold_is_rejected() {
    let err = AlertThresholds::new(Some(-1.0), None).unwrap_err();
    assert_eq!(err, ThresholdError::InvalidEnergyThreshold(-1.0));
}

#[test]
fn negative_co2_threshold_is_rejected() {
    let err = AlertThresholds::new(None, Some(-1.0)).unwrap_err();
    assert_eq!(err, ThresholdError::InvalidCo2Threshold(-1.0));
}

#[test]
fn non_finite_thresholds_are_rejected() {
    let cases: [(Option<f64>, Option<f64>); 4] = [
        (Some(f64::NAN), None),
        (Some(f64::INFINITY), None),
        (None, Some(f64::NAN)),
        (None, Some(f64::INFINITY)),
    ];
    for (energy_kwh, co2) in cases {
        let result = AlertThresholds::new(energy_kwh, co2);
        assert!(
            result.is_err(),
            "energy_kwh={:?} co2={:?} must be rejected",
            energy_kwh,
            co2
        );
    }
}

#[test]
fn zero_threshold_is_a_valid_maximally_strict_construction() {
    let result = AlertThresholds::new(Some(0.0), Some(0.0));
    assert!(result.is_ok());
}

// US8 slice 4 (AD-5) — AlertThresholds::from_sources: the pure domain
// merge behind `.codeimpact.json` + CLI override. Calling use case: main.rs
// merges the config-file-read thresholds with the CLI-parsed ones before
// passing the result into RunAnalysis.
//
// Test List:
// 1. cli value overrides the file value for a metric when both are set
// 2. file value is used when cli is None for that metric
// 3. both None -> merged value is None
// 4. energy and co2 merge independently (cli overrides one, file supplies the other)

#[test]
fn from_sources_cli_overrides_file_when_both_set() {
    let file = AlertThresholds::new(Some(0.000005), None).unwrap();
    let cli = AlertThresholds::new(Some(0.00001), None).unwrap();
    let merged = AlertThresholds::from_sources(file, cli);
    assert_eq!(merged.max_energy_kwh(), Some(0.00001));
}

#[test]
fn from_sources_falls_back_to_file_when_cli_absent() {
    let file = AlertThresholds::new(Some(0.000005), None).unwrap();
    let cli = AlertThresholds::none();
    let merged = AlertThresholds::from_sources(file, cli);
    assert_eq!(merged.max_energy_kwh(), Some(0.000005));
}

#[test]
fn from_sources_both_absent_stays_none() {
    let merged = AlertThresholds::from_sources(AlertThresholds::none(), AlertThresholds::none());
    assert_eq!(merged.max_energy_kwh(), None);
    assert_eq!(merged.max_co2_grams(), None);
}

#[test]
fn from_sources_merges_energy_and_co2_independently() {
    let file = AlertThresholds::new(Some(0.000005), Some(20.0)).unwrap();
    let cli = AlertThresholds::new(None, Some(99.0)).unwrap();
    let merged = AlertThresholds::from_sources(file, cli);
    assert_eq!(
        merged.max_energy_kwh(),
        Some(0.000005),
        "cli left energy unset, file value should carry through"
    );
    assert_eq!(
        merged.max_co2_grams(),
        Some(99.0),
        "cli overrides co2 even though file also set it"
    );
}
