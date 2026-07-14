use codeimpact_hexagon::analysis::{
    CallGraph, ComplexityDetector, ComplexityWarning, DetectionConfig, LoopCall, ParsedFunction,
    WarningPattern, WarningSeverity,
};

// Test List:
// 1. quadratic_loop_detected — fn with loop calling fn with loop → Critical
// 2. nested_loops_detected — fn with has_nested_loop → Warning
// 3. deep_call_chain_detected — chain depth > max_call_depth → Warning
// 4. hidden_complexity_detected — callee transitive > caller direct * ratio → Warning
// 5. recursion_direct_detected — fn calling itself → Recursion, Warning (#47: not
//    Critical — bounded tree/graph descent is a normal pattern, not evidence of
//    an unbounded/dangerous recursion the detector has actually established)
// 6. recursion_indirect_detected — cycle A→B→A → Recursion, Warning (#47, same)
// 7. large_match_detected — match_arms > max_match_arms → Warning
// 8. deep_conditional_detected — depth > max_conditional_depth → Warning
// 9. clean_code_no_warnings — no pattern triggers
// 10. detection_config_defaults — sensible default values
// 11. quadratic_loop_skipped_when_callee_has_no_loop — no false positive
// 12. multiple_warnings_on_same_function — multiple patterns can fire
// 13. quadratic_loop_not_flagged_on_recursive_tree_descent (#47) — a
//     self-recursive fn (has_cycle) whose loop iterates its own children is a
//     tree descent (O(n)), not O(n²) — QuadraticLoop must NOT fire, Recursion must
// 14. quadratic_loop_not_flagged_when_caller_also_calls_recursive_helper (#47) —
//     a fn that calls a non-recursive loop-having helper AND, elsewhere, a
//     recursive tree-descending helper (models build_tree → sort_children +
//     aggregate) must NOT be flagged QuadraticLoop for the non-recursive call
// 15. quadratic_loop_still_detected_with_unrelated_recursion_elsewhere (#47) —
//     a genuine quadratic pair must still be flagged even when the SAME file
//     contains an unrelated recursive pair the caller never calls
// 16. quadratic_loop_not_flagged_for_sequential_non_nested_call (#47 retry 1) —
//     a call to a loop-having, non-cyclic callee that happens AFTER the
//     caller's loop (not nested inside it) must NOT be flagged: only nested
//     calls can make a loop quadratic
// 17. quadratic_loop_still_detected_despite_nested_cyclic_helper (#47 retry 1)
//     — a caller that nests calls to BOTH a genuine quadratic partner and an
//     unrelated cyclic helper (in the same loop) must still be flagged for
//     the genuine partner; excluding the whole function the moment ANY
//     nested callee is cyclic is the false negative both reviews reproduced

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

/// A nested call, as `detect_quadratic_loops` sees it. `is_io` is
/// irrelevant here — this detector only cares about the callee's name.
fn lc(name: &str, line: usize, col: usize) -> LoopCall {
    LoopCall {
        name: name.to_string(),
        line,
        col,
        is_io: false,
    }
}

// === 1. QuadraticLoop ===
#[test]
fn quadratic_loop_detected_with_nested_call() {
    let mut process_items = make_fn("process_items", 1, vec!["validate"], true, false, 0, 0);
    process_items.calls_in_loops = vec![lc("validate", 2, 5)];
    let fns = vec![
        process_items,
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
    // #47: Recursion is Warning, not Critical — see rationale at test #5.
    assert_eq!(rec[0].severity, WarningSeverity::Warning);
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
    // #47: Recursion is Warning, not Critical — see rationale at test #5.
    assert!(rec.iter().all(|w| w.severity == WarningSeverity::Warning));
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

// === 11. Quadratic loop false positive — nested callee has no loop ===
#[test]
fn quadratic_loop_skipped_when_nested_callee_has_no_loop() {
    let mut process_items = make_fn("process_items", 1, vec!["validate"], true, false, 0, 0);
    process_items.calls_in_loops = vec![lc("validate", 2, 5)];
    let fns = vec![
        process_items,
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

// === 16. Quadratic loop not flagged — sequential, non-nested call (#47 retry 1) ===
#[test]
fn quadratic_loop_not_flagged_for_sequential_non_nested_call() {
    // `caller` has a loop and calls `helper` (which also loops), but the call
    // is a SEQUENTIAL statement after the loop, not nested inside it —
    // calls_in_loops stays empty. `helper` is not even cyclic, so a naive
    // "exclude only cyclic callees, scoped to f.calls" fix would still wrongly
    // flag this: it has no notion of nesting at all.
    let caller = make_fn("caller", 1, vec!["helper"], true, false, 0, 0);
    let fns = vec![caller, make_fn("helper", 1, vec![], true, false, 0, 0)];
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

// === 13. QuadraticLoop not flagged on recursive tree descent (#47) ===
#[test]
fn quadratic_loop_not_flagged_on_recursive_tree_descent() {
    // Models view_model.rs::aggregate: a postorder recursion over a tree —
    // the loop iterates the node's children, the recursive call descends.
    // Each node is visited exactly once: O(n), not O(n²). The self-call is
    // nested inside the loop (e.g. `for child_id in &child_ids`), which is
    // exactly the shape that must be excluded — and only because the nested
    // callee is cyclic, not because it lacks a loop.
    let mut aggregate = make_fn("aggregate", 1, vec!["aggregate"], true, false, 0, 0);
    aggregate.calls_in_loops = vec![lc("aggregate", 2, 5)];
    let fns = vec![aggregate];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    assert!(!warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::QuadraticLoop)));
    assert!(warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::Recursion) && w.function == "aggregate"));
}

// === 14. QuadraticLoop not flagged when caller also drives a recursive helper (#47) ===
#[test]
fn quadratic_loop_not_flagged_when_caller_also_calls_recursive_helper() {
    // Models view_model.rs::build_tree: calls sort_children (a non-recursive
    // loop over the whole node set, bounded per-node work) and, separately,
    // aggregate (a recursive tree descent). build_tree's own loop and
    // sort_children's loop are sequential passes over the same tree, not
    // nested — and build_tree also drives a genuine recursive descent, which
    // is evidence it orchestrates a tree structure rather than nesting an
    // unrelated quadratic pass.
    let fns = vec![
        make_fn(
            "build_tree",
            1,
            vec!["sort_children", "aggregate"],
            true,
            false,
            0,
            0,
        ),
        make_fn("sort_children", 1, vec![], true, false, 0, 0),
        make_fn("aggregate", 1, vec!["aggregate"], true, false, 0, 0),
    ];
    let graph = CallGraph::build(&fns);
    let config = DetectionConfig::default();
    let warnings = ComplexityDetector::detect(&fns, &graph, &config);

    assert!(!warnings
        .iter()
        .any(|w| matches!(w.pattern, WarningPattern::QuadraticLoop) && w.function == "build_tree"));
}

// === 15. Genuine quadratic loop still detected despite unrelated recursion elsewhere (#47) ===
#[test]
fn quadratic_loop_still_detected_with_unrelated_recursion_elsewhere() {
    // Non-regression: the recursion-based exclusion must be scoped to the
    // caller's OWN calls — an unrelated recursive pair elsewhere in the same
    // file, that the caller never calls, must not suppress a genuine
    // quadratic pair.
    let mut process_items = make_fn("process_items", 1, vec!["validate"], true, false, 0, 0);
    process_items.calls_in_loops = vec![lc("validate", 2, 5)];
    let fns = vec![
        process_items,
        make_fn("validate", 1, vec![], true, false, 0, 0),
        make_fn("a", 1, vec!["b"], false, false, 0, 0),
        make_fn("b", 1, vec!["a"], false, false, 0, 0),
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
}

// === 17. Quadratic loop still detected despite a nested cyclic helper (#47 retry 1) ===
#[test]
fn quadratic_loop_still_detected_despite_nested_cyclic_helper() {
    // `process_items` nests calls to BOTH a genuine quadratic partner
    // (`validate_loop`) and an unrelated recursive helper
    // (`self_recursive_helper`) inside the SAME loop. The recursive helper
    // must be excluded on its own merits (it is cyclic); the genuine partner
    // must still trigger the warning. The over-broad "any nested call is
    // cyclic -> skip the whole function" fix fails this: it drops the check
    // entirely the moment any nested callee is cyclic, including callees
    // unrelated to the recursion.
    let mut process_items = make_fn(
        "process_items",
        1,
        vec!["validate_loop", "self_recursive_helper"],
        true,
        false,
        0,
        0,
    );
    process_items.calls_in_loops =
        vec![lc("validate_loop", 2, 5), lc("self_recursive_helper", 3, 5)];
    let fns = vec![
        process_items,
        make_fn("validate_loop", 1, vec![], true, false, 0, 0),
        make_fn(
            "self_recursive_helper",
            1,
            vec!["self_recursive_helper"],
            true,
            false,
            0,
            0,
        ),
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
    assert!(quad[0].message.contains("validate_loop"));
}
