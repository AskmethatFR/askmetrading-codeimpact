use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FunctionDetail;

// Test List (hidden complexity — #46/#49, ADR-0012 follow-up):
// 1. hidden_complexity() is the SUM of per-function hidden(), never a
//    subtraction of the two file-level aggregates.
// 2. hidden_complexity() is 0 when no function was measured (ADR-0010: no
//    fabricated hidden complexity for data that was never collected).
// 3. FunctionDetail::hidden() returns the measured value directly.
// 4. FunctionDetail::transitive() is direct + hidden, derived.
//
// function_detail_hidden_panics_when_invariant_broken — DELETED (#46/#49
// arbitration §2, Security HIGH-1): it pinned a debug_assert! that is
// compiled OUT of a release binary (Security ran the exact test in
// --release and it did not panic), so the guard it exercised never
// protected a real user. Fields are now private and FunctionDetail::new()
// takes (direct, hidden) with transitive() = direct + hidden derived — no
// public constructor can build a FunctionDetail whose transitive is less
// than its direct, so the illegal state is unconstructible and there is
// nothing left for a runtime assertion to catch.

fn detail(name: &str, direct: u32, hidden: u32) -> FunctionDetail {
    FunctionDetail::new(
        name.to_string(),
        CodeLocation::new("a.rs".into(), 1, 1),
        direct,
        hidden,
        0,
        false,
    )
}

#[test]
fn hidden_complexity_is_the_sum_of_per_function_hidden() {
    let details = vec![detail("f1", 2, 3), detail("f2", 3, 0)];
    let m = CodeMetrics::with_call_graph(6, 8, 1, vec![], details);

    assert_eq!(m.hidden_complexity(), 3, "hidden = 3 + 0 = 3");
}

#[test]
fn hidden_is_zero_when_no_function_was_measured() {
    // File-level cc=6, tc=8 would give a non-zero value under the OLD
    // saturating_sub(transitive, cyclomatic) formula (8-6=2), but zero
    // functions were ever measured here — hidden complexity cannot be
    // reported for functions nobody looked at.
    let m = CodeMetrics::with_call_graph(6, 8, 1, vec![], vec![]);

    assert_eq!(m.hidden_complexity(), 0);
}

#[test]
fn function_detail_hidden_returns_the_measured_value() {
    let f = detail("f", 2, 3);

    assert_eq!(f.hidden(), 3);
}

#[test]
fn function_detail_transitive_is_direct_plus_hidden() {
    let f = detail("f", 2, 3);

    assert_eq!(f.transitive(), 5);
}

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
