use codeimpact_hexagon::analysis::CallGraph;
use codeimpact_hexagon::analysis::ParsedFunction;

// Test List:
// 1. empty_functions → direct=0, transitive=0, no cycles, max_depth=0
// 2. single_function_no_calls → transitive = direct
// 3. a_calls_b → transitive(A) = direct(A) + direct(B)
// 4. chain_a_b_c → transitive(A) = direct(A) + direct(B) + direct(C)
// 5. cycle_a_b → each cycle member's transitive = SUM of the whole SCC's
//    direct (comprehension semantics: to understand either member you read
//    both — #46/#49 arbitration §1, corrected from the old "direct only"
//    special case)
// 6. three_node_cycle → same: all three members' transitive = SUM of the
//    cycle's direct (3-way SCC)
// 7. cycle_with_shared_noncycle → cycle nodes + non-cycle node, non-cycle
//    node's transitive unaffected
// 8. unknown_function → returns 0
// 9. max_call_depth_empty → 0
// 10. max_call_depth_single → 1
// 11. max_call_depth_chain_3 → 3
// 12. has_cycle_true → cycle nodes return true
// 13. has_cycle_false → non-cycle nodes return false
// 14. call_chain_depth → depth from specific function
// 15. self_cycle_detected → function calling itself
// 16. transitive_total → sum of all transitive complexities
// 17. diamond_graph → shared callee `d` counted ONCE in transitive(a), not
//     once per incoming path (#46/#49 arbitration §0: the finding that
//     changes everything — dedup does not require a cycle, a plain diamond
//     with all-distinct names already double-counts under sum-over-paths)
// 18. cycle_with_branch → cycle node transitive is the reachable-set sum
//     (SCC members + non-cycle callees), not "direct + non-cycle callees
//     only" (the old special case, deleted)
// 19. double_call_counts_callee_once → f calls g TWICE in its `calls` list;
//     hidden(f) == direct(g), not 2×direct(g) (#46/#49 arbitration §0: the
//     "calls g twice" factor is only a special case of the general
//     reachable-set fix, not a separate bug requiring separate dedup code)
// 20. deep_diamond_chain_transitive_stays_bounded_by_total_direct →
//     non-regression: a diamond stacked 32 levels deep grows as 2^levels
//     under the OLD sum-over-paths formula and overflows u32 (Security
//     HIGH-2); the NEW reachable-set formula is bounded by the fixture's
//     total direct complexity BY CONSTRUCTION, so it can never overflow.
//     Must hold in `--release` too (Security HIGH-1's whole point: no
//     runtime guard survives release, so the metric itself must be honest).

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
        calls_in_loops: vec![],
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
    // Both in cycle → to understand EITHER member you must read the whole
    // SCC: transitive(a) = transitive(b) = direct(a) + direct(b) = 1 + 2 = 3.
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert_eq!(graph.transitive_of("a"), 3);
    assert_eq!(graph.transitive_of("b"), 3);
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
    // All three members share the same SCC: transitive = 1 + 2 + 3 = 6.
    assert_eq!(graph.transitive_of("a"), 6);
    assert_eq!(graph.transitive_of("b"), 6);
    assert_eq!(graph.transitive_of("c"), 6);
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
    // A and B in cycle, C not, and C is unreachable from the cycle (nobody
    // calls it) — so C's transitive is untouched by the cycle fix.
    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert!(!graph.has_cycle("c"));
    assert_eq!(graph.transitive_of("a"), 3); // 1 + 2 (whole SCC)
    assert_eq!(graph.transitive_of("b"), 3); // 2 + 1 (whole SCC)
    assert_eq!(graph.transitive_of("c"), 4); // leaf, unreachable from a/b
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

// === 17. Diamond: A→B, A→C, B→D, C→D — `d` reached via TWO paths, must be
//         counted ONCE (#46/#49 arbitration §0: this is the finding that
//         changes everything — no cycle, no repeated name, and it STILL
//         double-counts under the old sum-over-paths formula) ===
#[test]
fn diamond_graph_computes_correctly() {
    let fns = vec![
        make_fn("a", 1, vec!["b", "c"]),
        make_fn("b", 2, vec!["d"]),
        make_fn("c", 3, vec!["d"]),
        make_fn("d", 4, vec![]),
    ];
    let graph = CallGraph::build(&fns);
    assert_eq!(graph.transitive_of("d"), 4); // leaf, unaffected
    assert_eq!(graph.transitive_of("b"), 6); // 2 + 4, single path, unaffected
    assert_eq!(graph.transitive_of("c"), 7); // 3 + 4, single path, unaffected
                                              // reachable(a) \ {a} = {b, c, d} — `d` counted ONCE, not twice:
                                              // 1 + (direct(b)=2 + direct(c)=3 + direct(d)=4) = 1 + 9 = 10.
                                              // The OLD formula gave 14 (= 1 + transitive(b) + transitive(c),
                                              // double-charging `d`'s complexity once per incoming path).
    assert_eq!(graph.transitive_of("a"), 10);
    assert_eq!(graph.max_call_depth(), 3);
}

// === 18. Cycle with non-cycle branch: A→B, B→A, A→C — the cycle members'
//         transitive is the reachable-SET sum, not "direct + non-cycle
//         callees only" (the old ad-hoc special case) ===
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
    // reachable(a) \ {a} = {b, c}: 1 + (2 + 4) = 7.
    assert_eq!(graph.transitive_of("a"), 7);
    // reachable(b) \ {b} = {a, c} (b -> a -> c): 2 + (1 + 4) = 7.
    assert_eq!(graph.transitive_of("b"), 7);
    // C not in cycle, not reachable from anything reaching it → direct only.
    assert_eq!(graph.transitive_of("c"), 4);
}

// === 19. Double call: f calls g TWICE — g counted ONCE ===
#[test]
fn double_call_counts_callee_once() {
    let fns = vec![make_fn("f", 1, vec!["g", "g"]), make_fn("g", 4, vec![])];
    let graph = CallGraph::build(&fns);
    // hidden(f) must be direct(g) = 4, not 2 * direct(g) = 8.
    assert_eq!(graph.transitive_of("f"), 5); // 1 + 4, not 1 + 4 + 4 = 9
    assert_eq!(graph.transitive_of("g"), 4);
}

// === 20. Deep diamond chain — bounded by construction, never overflows ===
fn diamond_chain(levels: usize) -> Vec<ParsedFunction> {
    let mut fns = vec![make_fn("f0", 1, vec![])];
    for i in 1..=levels {
        let prev = format!("f{}", i - 1);
        let a = format!("a{}", i);
        let b = format!("b{}", i);
        let f = format!("f{}", i);
        fns.push(make_fn(&a, 1, vec![prev.as_str()]));
        fns.push(make_fn(&b, 1, vec![prev.as_str()]));
        fns.push(make_fn(&f, 1, vec![a.as_str(), b.as_str()]));
    }
    fns
}

#[test]
fn deep_diamond_chain_transitive_stays_bounded_by_total_direct() {
    // Under the OLD sum-over-paths formula, transitive of the top function
    // is 2^(levels+1) - 3 — it would exceed u32::MAX (4_294_967_295) at
    // levels=32 (2^33 - 3): a debug build panics on the overflow, a release
    // build wraps silently (Security HIGH-2). Under the NEW reachable-set
    // formula every function has direct=1 and the top function reaches
    // every other function exactly once, so its transitive is EXACTLY the
    // fixture's function count — bounded by construction, in debug AND in
    // release, at ANY depth (the bound is `Σ direct`, monotonic and
    // non-negative — it cannot depend on how deep the chain is).
    //
    // levels is capped at 18 (not 32) for this test's own runtime: `build()`
    // also calls the pre-existing, un-memoized `compute_depth` for every
    // node to compute `max_call_depth`, and `compute_depth` has the SAME
    // unmemoized-shared-DAG blind spot this ticket fixes for `transitive`
    // (it re-walks each diamond branch from scratch instead of caching by
    // node), so it re-grows exponentially on this exact fixture shape and
    // makes levels=32 take minutes. `compute_depth` is explicitly out of
    // scope for #46/#49 (tech spec: "Do NOT touch compute_depth") — this is
    // a latent, separate performance defect, not a correctness one; it does
    // not threaten the u32 bound proven here, but a real deeply-diamond
    // codebase could make analysis hang. Filed for a follow-up ticket
    // (compute_depth needs the same memoized-visited-set treatment
    // reachable_from now uses). levels=18 already shows the old formula
    // overshooting by three orders of magnitude (524_285 vs a correct 55)
    // while finishing in milliseconds; the u32-overflow magnitude at
    // levels=32 is a direct consequence of the same recurrence, proven by
    // the closed-form 2^(levels+1) - 3 above, not re-executed here.
    let levels = 18;
    let fns = diamond_chain(levels);
    let total_functions = fns.len() as u32;
    let graph = CallGraph::build(&fns);
    let top = format!("f{}", levels);

    assert_eq!(
        graph.transitive_of(&top),
        total_functions,
        "transitive({}) must equal the fixture's total function count ({}) — \
         every function is reachable exactly once, never per incoming path",
        top,
        total_functions
    );
}
