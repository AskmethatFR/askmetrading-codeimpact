use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FunctionDetail;

// Test List (hidden complexity — #46/#49, ADR-0012):
// 1. hidden_complexity() is the SUM of per-function (transitive - direct),
//    never a subtraction of the two file-level aggregates.
// 2. hidden_complexity() is 0 when no function was measured (ADR-0010: no
//    fabricated hidden complexity for data that was never collected), even
//    when the file-level transitive/cyclomatic totals would otherwise
//    suggest a non-zero hidden value under the old (wrong) formula.
// 3. FunctionDetail::hidden() is transitive - direct for that one function.
// 4. FunctionDetail::hidden() panics (debug_assert) when the call-graph
//    invariant (transitive >= direct) is broken — it must never silently
//    clamp a broken invariant to zero.

fn detail(name: &str, direct: u32, transitive: u32) -> FunctionDetail {
    FunctionDetail {
        name: name.to_string(),
        location: CodeLocation::new("a.rs".into(), 1, 1),
        direct,
        transitive,
        call_depth: 0,
        in_cycle: false,
    }
}

#[test]
fn hidden_complexity_is_the_sum_of_per_function_hidden() {
    let details = vec![detail("f1", 2, 5), detail("f2", 3, 3)];
    let m = CodeMetrics::with_call_graph(6, 8, 1, vec![], details);

    assert_eq!(m.hidden_complexity(), 3, "hidden = (5-2) + (3-3) = 3");
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
fn function_detail_hidden_is_transitive_minus_direct() {
    let f = detail("f", 2, 5);

    assert_eq!(f.hidden(), 3);
}

#[test]
#[should_panic]
fn function_detail_hidden_panics_when_invariant_broken() {
    let f = detail("broken", 5, 2); // transitive < direct: impossible by construction
    let _ = f.hidden();
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
