use codeimpact_hexagon::analysis::{
    CallGraph, ComplexityDetector, ComplexityWarning, DetectionConfig, ParsedFunction,
    WarningPattern, WarningSeverity,
};

// Test List:
// 1. quadratic_loop_detected — fn with loop calling fn with loop → Critical
// 2. nested_loops_detected — fn with has_nested_loop → Warning
// 3. deep_call_chain_detected — chain depth > max_call_depth → Warning
// 4. hidden_complexity_detected — callee transitive > caller direct * ratio → Warning
// 5. recursion_direct_detected — fn calling itself → Critical
// 6. recursion_indirect_detected — cycle A→B→A → Critical
// 7. large_match_detected — match_arms > max_match_arms → Warning
// 8. deep_conditional_detected — depth > max_conditional_depth → Warning
// 9. clean_code_no_warnings — no pattern triggers
// 10. detection_config_defaults — sensible default values
// 11. quadratic_loop_skipped_when_callee_has_no_loop — no false positive
// 12. multiple_warnings_on_same_function — multiple patterns can fire

fn make_fn(
    name: &str,
    decision_points: u32,
    calls: Vec<&str>,
    has_loop: bool,
    has_nested_loop: bool,
    depth: u32,
    match_arms: u32,
) -> ParsedFunction {
    ParsedFunction {
        name: name.to_string(),
        start_line: 1,
        calls: calls.into_iter().map(String::from).collect(),
        has_loop,
        has_nested_loop,
        decision_points,
        depth,
        match_arms,
        calls_in_loops: vec![],
    }
}

// === 1. QuadraticLoop ===
#[test]
fn quadratic_loop_detected() {
    let fns = vec![
        make_fn("process_items", 1, vec!["validate"], true, false, 0, 0),
        make_fn("validate", 1, vec![], true, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let quad: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::QuadraticLoop))
        .collect();
    assert_eq!(quad.len(), 1);
    assert_eq!(quad[0].function, "process_items");
    assert_eq!(quad[0].severity, WarningSeverity::Critical);
    assert!(quad[0].message.contains("validate"));
}

// === 2. NestedLoops ===
#[test]
fn nested_loops_detected() {
    let fns = vec![make_fn("nested", 2, vec![], true, true, 0, 0)];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let nested: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::NestedLoops))
        .collect();
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].function, "nested");
    assert_eq!(nested[0].severity, WarningSeverity::Warning);
}

// === 3. DeepCallChain ===
#[test]
fn deep_call_chain_detected() {
    let fns = vec![
        make_fn("a", 1, vec!["b"], false, false, 0, 0),
        make_fn("b", 1, vec!["c"], false, false, 0, 0),
        make_fn("c", 1, vec!["d"], false, false, 0, 0),
        make_fn("d", 1, vec!["e"], false, false, 0, 0),
        make_fn("e", 1, vec!["f"], false, false, 0, 0),
        make_fn("f", 1, vec![], false, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    // max_call_depth=5 → a chain of 6 exceeds threshold
    let config = DetectionConfig {
        max_call_depth: 5,
        ..DetectionConfig::default()
    };
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let deep: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::DeepCallChain))
        .collect();
    assert_eq!(deep.len(), 1);
    assert_eq!(deep[0].function, "a");
    assert_eq!(deep[0].severity, WarningSeverity::Warning);
}

// === 4. HiddenComplexity ===
#[test]
fn hidden_complexity_detected() {
    let fns = vec![
        make_fn("simple", 1, vec!["complex"], false, false, 0, 0),
        make_fn("complex", 10, vec!["very_complex"], false, false, 0, 0),
        make_fn("very_complex", 10, vec![], false, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    // complex has transitive=20, simple has direct=1 → ratio=20 > 5
    let config = DetectionConfig {
        complexity_ratio: 5.0,
        ..DetectionConfig::default()
    };
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let hidden: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::HiddenComplexity))
        .collect();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].function, "simple");
    assert_eq!(hidden[0].severity, WarningSeverity::Warning);
}

// === 5. Recursion direct ===
#[test]
fn recursion_direct_detected() {
    let fns = vec![make_fn(
        "self_call",
        1,
        vec!["self_call"],
        false,
        false,
        0,
        0,
    )];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let rec: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::Recursion))
        .collect();
    assert_eq!(rec.len(), 1);
    assert_eq!(rec[0].function, "self_call");
    assert_eq!(rec[0].severity, WarningSeverity::Critical);
}

// === 6. Recursion indirect ===
#[test]
fn recursion_indirect_detected() {
    let fns = vec![
        make_fn("a", 1, vec!["b"], false, false, 0, 0),
        make_fn("b", 1, vec!["a"], false, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let rec: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::Recursion))
        .collect();
    assert_eq!(rec.len(), 2);
    assert!(rec.iter().any(|w| w.function == "a"));
    assert!(rec.iter().any(|w| w.function == "b"));
}

// === 7. LargeMatch ===
#[test]
fn large_match_detected() {
    let fns = vec![make_fn("handler", 1, vec![], false, false, 0, 15)];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig {
        max_match_arms: 10,
        ..DetectionConfig::default()
    };
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let large: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::LargeMatch))
        .collect();
    assert_eq!(large.len(), 1);
    assert_eq!(large[0].function, "handler");
    assert_eq!(large[0].severity, WarningSeverity::Warning);
}

// === 8. DeepConditional ===
#[test]
fn deep_conditional_detected() {
    let fns = vec![make_fn("deep_cond", 1, vec![], false, false, 7, 0)];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig {
        max_conditional_depth: 5,
        ..DetectionConfig::default()
    };
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let deep: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::DeepConditional))
        .collect();
    assert_eq!(deep.len(), 1);
    assert_eq!(deep[0].function, "deep_cond");
    assert_eq!(deep[0].severity, WarningSeverity::Warning);
}

// === 9. Clean code — no warnings ===
#[test]
fn clean_code_no_warnings() {
    let fns = vec![make_fn("clean", 1, vec![], false, false, 2, 3)];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);
    assert!(warnings.is_empty());
}

// === 10. DetectionConfig defaults ===
#[test]
fn detection_config_defaults() {
    let config = DetectionConfig::default();
    assert_eq!(config.max_call_depth, 5);
    assert!((config.complexity_ratio - 5.0).abs() < 1e-9);
    assert_eq!(config.max_match_arms, 10);
    assert_eq!(config.max_conditional_depth, 5);
}

// === 11. Quadratic loop false positive — callee has no loop ===
#[test]
fn quadratic_loop_skipped_when_callee_has_no_loop() {
    let fns = vec![
        make_fn("process_items", 1, vec!["validate"], true, false, 0, 0),
        make_fn("validate", 1, vec![], false, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    let quad: Vec<&ComplexityWarning> = warnings
        .iter()
        .filter(|w| matches!(w.pattern, WarningPattern::QuadraticLoop))
        .collect();
    assert!(quad.is_empty());
}

// === 12. Multiple warnings on same function ===
#[test]
fn multiple_warnings_on_same_function() {
    let fns = vec![make_fn("messy", 1, vec![], true, true, 7, 15)];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig {
        max_match_arms: 10,
        max_conditional_depth: 5,
        ..DetectionConfig::default()
    };
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    // Should have NestedLoops + LargeMatch + DeepConditional
    assert!(warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::NestedLoops)));
    assert!(warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::LargeMatch)));
    assert!(warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::DeepConditional)));
}
