use codeimpact_hexagon::analysis::CallGraph;
use codeimpact_hexagon::analysis::ParsedFunction;

// Test List:
// 1. empty_functions → direct=0, transitive=0, no cycles, max_depth=0
// 2. single_function_no_calls → transitive = direct
// 3. a_calls_b → transitive(A) = direct(A) + direct(B)
// 4. chain_a_b_c → transitive(A) = direct(A) + direct(B) + direct(C)
// 5. cycle_a_b → A and B in cycle, no infinite loop
// 6. three_node_cycle → all in cycle
// 7. cycle_with_shared_noncycle → cycle nodes + non-cycle node
// 8. unknown_function → returns 0
// 9. max_call_depth_empty → 0
// 10. max_call_depth_single → 1
// 11. max_call_depth_chain_3 → 3
// 12. has_cycle_true → cycle nodes return true
// 13. has_cycle_false → non-cycle nodes return false
// 14. call_chain_depth → depth from specific function
// 15. self_cycle_detected → function calling itself
// 16. transitive_total → sum of all transitive complexities
// 17. diamond_graph → shared callee computed correctly
// 18. cycle_with_branch → cycle node transitive includes non-cycle callees

fn make_fn(name: &str, decision_points: u32, calls: Vec<&str>) -> ParsedFunction {
    ParsedFunction {
        name: name.to_string(),
        start_line: 1,
        calls: calls.into_iter().map(String::from).collect(),
        has_loop: false,
        has_nested_loop: false,
        decision_points,
        depth: 0,
        match_arms: 0,
    }
}

// === 1. Empty graph ===
#[test]
fn empty_graph_returns_zero() {
    let graph = CallGraph::build(&[]);
    assert_eq!(graph.direct_of("anything"), 0);
    assert_eq!(graph.transitive_of("anything"), 0);
    assert_eq!(graph.max_call_depth(), 0);
    assert!(!graph.has_cycle("anything"));
    assert_eq!(graph.call_chain_depth("anything"), 0);
}

// === 2. Single function, no calls ===
#[test]
fn single_function_transitive_equals_direct() {
    let fns = vec![make_fn("foo", 3, vec![])];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.direct_of("foo"), 3);
    assert_eq!(graph.transitive_of("foo"), 3);
    assert_eq!(graph.max_call_depth(), 1);
    assert!(!graph.has_cycle("foo"));
}

// === 3. A calls B ===
#[test]
fn a_calls_b_transitive_includes_callee() {
    let fns = vec![make_fn("a", 2, vec!["b"]), make_fn("b", 3, vec![])];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.direct_of("a"), 2);
    assert_eq!(graph.direct_of("b"), 3);
    assert_eq!(graph.transitive_of("b"), 3);
    assert_eq!(graph.transitive_of("a"), 5); // 2 + 3
}

// === 4. Chain A→B→C ===
#[test]
fn chain_transitive_sums_all() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 2, vec!["c"]),
        make_fn("c", 3, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.transitive_of("c"), 3);
    assert_eq!(graph.transitive_of("b"), 5); // 2 + 3
    assert_eq!(graph.transitive_of("a"), 6); // 1 + 5
}

// === 5. Cycle A→B→A ===
#[test]
fn cycle_detected_no_infinite_loop() {
    let fns = vec![make_fn("a", 1, vec!["b"]), make_fn("b", 2, vec!["a"])];
    let graph = CallGraph::build(&fns);
    // Both in cycle → transitive = direct only
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert_eq!(graph.transitive_of("a"), 1);
    assert_eq!(graph.transitive_of("b"), 2);
}

// === 6. Three-node cycle A→B→C→A ===
#[test]
fn three_node_cycle_all_marked() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 2, vec!["c"]),
        make_fn("c", 3, vec!["a"]),
    ];
    let graph = CallGraph::build(&fns);
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert!(graph.has_cycle("c"));
    assert_eq!(graph.transitive_of("a"), 1);
    assert_eq!(graph.transitive_of("b"), 2);
    assert_eq!(graph.transitive_of("c"), 3);
}

// === 7. Cycle with shared non-cycle node ===
#[test]
fn cycle_with_noncycle_callee_still_has_transitive() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 2, vec!["a"]),
        make_fn("c", 4, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    // A and B in cycle, C not
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert!(!graph.has_cycle("c"));
    assert_eq!(graph.transitive_of("a"), 1);
    assert_eq!(graph.transitive_of("b"), 2);
    assert_eq!(graph.transitive_of("c"), 4);
}

// === 8. Unknown function ===
#[test]
fn unknown_function_returns_zero() {
    let fns = vec![make_fn("foo", 3, vec![])];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.direct_of("nonexistent"), 0);
    assert_eq!(graph.transitive_of("nonexistent"), 0);
    assert_eq!(graph.call_chain_depth("nonexistent"), 0);
    assert!(!graph.has_cycle("nonexistent"));
}

// === 9. Max call depth - empty ===
#[test]
fn max_call_depth_empty_is_zero() {
    let graph = CallGraph::build(&[]);
    assert_eq!(graph.max_call_depth(), 0);
}

// === 10. Max call depth - single function ===
#[test]
fn max_call_depth_single_is_one() {
    let fns = vec![make_fn("foo", 1, vec![])];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.max_call_depth(), 1);
}

// === 11. Max call depth - chain of 3 ===
#[test]
fn max_call_depth_chain_counts_depth() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 1, vec!["c"]),
        make_fn("c", 1, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.max_call_depth(), 3);
}

// === 12. has_cycle true for cycle nodes ===
#[test]
fn cycle_nodes_identified() {
    let fns = vec![make_fn("a", 1, vec!["b"]), make_fn("b", 1, vec!["a"])];
    let graph = CallGraph::build(&fns);
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
}

// === 13. has_cycle false for non-cycle nodes ===
#[test]
fn non_cycle_nodes_not_flagged() {
    let fns = vec![make_fn("a", 1, vec!["b"]), make_fn("b", 1, vec![])];
    let graph = CallGraph::build(&fns);
    assert!(!graph.has_cycle("a"));
    assert!(!graph.has_cycle("b"));
}

// === 14. Call chain depth ===
#[test]
fn call_chain_depth_from_function() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 1, vec!["c"]),
        make_fn("c", 1, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.call_chain_depth("a"), 3);
    assert_eq!(graph.call_chain_depth("b"), 2);
    assert_eq!(graph.call_chain_depth("c"), 1);
}

// === 15. Self-cycle ===
#[test]
fn self_cycle_detected() {
    let fns = vec![make_fn("a", 1, vec!["a"])];
    let graph = CallGraph::build(&fns);
    assert!(graph.has_cycle("a"));
    assert_eq!(graph.transitive_of("a"), 1);
}

// === 16. transitive_total ===
#[test]
fn transitive_total_sums_all() {
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 2, vec!["c"]),
        make_fn("c", 3, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    // transitive_total = transitive(a) + transitive(b) + transitive(c)
    // = 6 + 5 + 3 = 14
    assert_eq!(graph.transitive_total(), 14);
}

// === 17. Diamond: A→B, A→C, B→D, C→D ===
#[test]
fn diamond_graph_computes_correctly() {
    let fns = vec![
        make_fn("a", 1, vec!["b", "c"]),
        make_fn("b", 2, vec!["d"]),
        make_fn("c", 3, vec!["d"]),
        make_fn("d", 4, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.transitive_of("d"), 4);
    assert_eq!(graph.transitive_of("b"), 6); // 2 + 4
    assert_eq!(graph.transitive_of("c"), 7); // 3 + 4
    assert_eq!(graph.transitive_of("a"), 14); // 1 + 6 + 7
    assert_eq!(graph.max_call_depth(), 3);
}

// === 18. Cycle with non-cycle branch: A→B, B→A, A→C ===
#[test]
fn cycle_with_branch_has_partial_transitive() {
    let fns = vec![
        make_fn("a", 1, vec!["b", "c"]),
        make_fn("b", 2, vec!["a"]),
        make_fn("c", 4, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert!(!graph.has_cycle("c"));
    // A in cycle → transitive = direct + non-cycle callees
    assert_eq!(graph.transitive_of("a"), 5); // 1 direct + 4 from c
    assert_eq!(graph.transitive_of("b"), 2); // 2 direct + 0 (a is cycle, skipped)
                                             // C not in cycle → transitive = direct
    assert_eq!(graph.transitive_of("c"), 4);
}
