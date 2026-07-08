// Test List for CodeMetrics:
// 1. new(0) -> complexity=0, level="low"     (minimum)
// 2. new(10) -> complexity=10, level="low"   (boundary: low ceiling)
// 3. new(11) -> complexity=11, level="moderate" (boundary: moderate floor)
// 4. new(20) -> complexity=20, level="moderate" (boundary: moderate ceiling)
// 5. new(21) -> complexity=21, level="high"   (boundary: high floor)
// 6. new(40) -> complexity=40, level="high"   (boundary: high ceiling)
// 7. new(41) -> complexity=41, level="critical" (boundary: critical floor)
// 8. new(100) -> complexity=100, level="critical" (far above critical)
// 9. cyclomatic_complexity() returns the stored value

use codeimpact_hexagon::domain_model::code_metrics::CodeMetrics;

#[test]
fn complexity_0_is_low() {
    let metrics = CodeMetrics::new(0);
    assert_eq!(metrics.cyclomatic_complexity(), 0);
    assert_eq!(metrics.complexity_level(), "low");
}

#[test]
fn complexity_10_is_low() {
    let metrics = CodeMetrics::new(10);
    assert_eq!(metrics.complexity_level(), "low");
}

#[test]
fn complexity_11_is_moderate() {
    let metrics = CodeMetrics::new(11);
    assert_eq!(metrics.complexity_level(), "moderate");
}

#[test]
fn complexity_20_is_moderate() {
    let metrics = CodeMetrics::new(20);
    assert_eq!(metrics.complexity_level(), "moderate");
}

#[test]
fn complexity_21_is_high() {
    let metrics = CodeMetrics::new(21);
    assert_eq!(metrics.complexity_level(), "high");
}

#[test]
fn complexity_40_is_high() {
    let metrics = CodeMetrics::new(40);
    assert_eq!(metrics.complexity_level(), "high");
}

#[test]
fn complexity_41_is_critical() {
    let metrics = CodeMetrics::new(41);
    assert_eq!(metrics.complexity_level(), "critical");
}

#[test]
fn complexity_100_is_critical() {
    let metrics = CodeMetrics::new(100);
    assert_eq!(metrics.complexity_level(), "critical");
}

#[test]
fn cyclomatic_complexity_returns_stored_value() {
    let metrics = CodeMetrics::new(7);
    assert_eq!(metrics.cyclomatic_complexity(), 7);
}
