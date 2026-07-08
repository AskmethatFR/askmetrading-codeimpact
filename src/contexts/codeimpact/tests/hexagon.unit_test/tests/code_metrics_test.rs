use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;

#[test]
fn complexity_0_is_low() {
    let m = CodeMetrics::new(0);
    assert_eq!(m.complexity_level(), "low");
}

#[test]
fn complexity_10_is_low() {
    let m = CodeMetrics::new(10);
    assert_eq!(m.complexity_level(), "low");
}

#[test]
fn complexity_11_is_moderate() {
    let m = CodeMetrics::new(11);
    assert_eq!(m.complexity_level(), "moderate");
}

#[test]
fn complexity_20_is_moderate() {
    let m = CodeMetrics::new(20);
    assert_eq!(m.complexity_level(), "moderate");
}

#[test]
fn complexity_21_is_high() {
    let m = CodeMetrics::new(21);
    assert_eq!(m.complexity_level(), "high");
}

#[test]
fn complexity_40_is_high() {
    let m = CodeMetrics::new(40);
    assert_eq!(m.complexity_level(), "high");
}

#[test]
fn complexity_41_is_critical() {
    let m = CodeMetrics::new(41);
    assert_eq!(m.complexity_level(), "critical");
}

#[test]
fn complexity_100_is_critical() {
    let m = CodeMetrics::new(100);
    assert_eq!(m.complexity_level(), "critical");
}

#[test]
fn getter_returns_stored_value() {
    let m = CodeMetrics::new(42);
    assert_eq!(m.cyclomatic_complexity(), 42);
}

#[test]
fn with_economic_impact_stores_it() {
    let impact = EconomicImpact::new(1.5, 200, 1.52, "low");
    let m = CodeMetrics::new(5).with_economic_impact(impact);
    let retrieved = m.economic_impact().expect("should have economic impact");
    assert!((retrieved.cpu_cost_microdollars() - 1.5).abs() < 1e-9);
    assert_eq!(retrieved.memory_bytes(), 200);
    assert_eq!(retrieved.level(), "low");
}

#[test]
fn economic_impact_none_by_default() {
    let m = CodeMetrics::new(5);
    assert!(m.economic_impact().is_none());
}
