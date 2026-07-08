use codeimpact_hexagon::analysis::CodeMetrics;

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
