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
//     Runs at levels=32 (#52 follow-up): `compute_depth` is now memoized,
//     so the exponential re-walk that forced the level=18 cap is gone.
// 21. call_chain_depth_stays_correct_when_a_diamond_funnels_into_a_cycle →
//     #52 follow-up: memoizing `compute_depth` must not change a single
//     hand-computed depth — cycle members (short-circuit to depth 1),
//     a linear chain into the cycle, AND a diamond that reaches the same
//     cycle member via two different callers (the shared-subtree case the
//     memo cache must serve identically to both callers).
// 22. deep_diamond_chain_compute_depth_completes_quickly → #52: pins the
//     PERFORMANCE characteristic memoization fixes. Un-memoized, this
//     exact fixture shape at levels=32 takes minutes (2^32 re-walks); a
//     wall-clock assertion would be brittle, so instead the computation
//     runs on a background thread and the test bounds the WAIT via
//     `recv_timeout` — generous enough to never flake on a loaded CI
//     runner, yet far below what an un-memoized run would need.

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
    // levels was capped at 18 (not 32) until #52: `build()` also calls
    // `compute_depth` for every node to compute `max_call_depth`, and
    // `compute_depth` had the SAME unmemoized-shared-DAG blind spot this
    // ticket originally fixed for `transitive` (it re-walked each diamond
    // branch from scratch instead of caching by node), so it re-grew
    // exponentially on this exact fixture shape and made levels=32 take
    // minutes. `compute_depth` is now memoized (see
    // `deep_diamond_chain_compute_depth_completes_quickly` for the
    // dedicated performance pin), so levels=32 — the level at which the
    // OLD sum-over-paths formula would have overflowed `u32` — is back and
    // runs in milliseconds.
    let levels = 32;
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

// === 21. Diamond funneling into a cycle: memoization must not change a
//         single hand-computed depth ===
#[test]
fn call_chain_depth_stays_correct_when_a_diamond_funnels_into_a_cycle() {
    // a <-> b: a two-node cycle. `compute_depth` short-circuits a cycle
    // member to depth 1 (it never looks past the cycle boundary) — that is
    // pre-existing, unchanged behavior this test pins.
    //   depth(a) = depth(b) = 1
    // c1 and c2 both call straight into the cycle (through "a"), so the
    // memo cache for "a" — computed once — must be reused identically for
    // BOTH callers, not recomputed or poisoned on the second lookup:
    //   depth(c1) = depth(c2) = 1 + depth(a) = 2
    // "top" reaches the cycle via two DIFFERENT paths (a diamond over c1/
    // c2), exercising the exact shared-subtree shape memoization must
    // serve correctly:
    //   depth(top) = 1 + max(depth(c1), depth(c2)) = 3
    let fns = vec![
        make_fn("a", 1, vec!["b"]),
        make_fn("b", 1, vec!["a"]),
        make_fn("c1", 1, vec!["a"]),
        make_fn("c2", 1, vec!["a"]),
        make_fn("top", 1, vec!["c1", "c2"]),
    ];
    let graph = CallGraph::build(&fns);

    assert!(graph.has_cycle("a"));
    assert!(graph.has_cycle("b"));
    assert!(!graph.has_cycle("c1"));
    assert!(!graph.has_cycle("c2"));
    assert!(!graph.has_cycle("top"));

    assert_eq!(graph.call_chain_depth("a"), 1);
    assert_eq!(graph.call_chain_depth("b"), 1);
    assert_eq!(graph.call_chain_depth("c1"), 2);
    assert_eq!(graph.call_chain_depth("c2"), 2);
    assert_eq!(graph.call_chain_depth("top"), 3);
    assert_eq!(graph.max_call_depth(), 3);
}

// === 22. Deep diamond chain — compute_depth must be memoized ===
#[test]
fn deep_diamond_chain_compute_depth_completes_quickly() {
    // Reproduces the #52 performance defect: `compute_depth` re-walked the
    // same subtree once per incoming path with no cache, so a diamond
    // stacked `levels` deep costs O(2^levels). At levels=18 this measured
    // ~3.5s; at levels=32 the UNMEMOIZED code would take on the order of
    // MINUTES. A literal wall-clock assertion on that magnitude is brittle
    // (flaky under CI load) AND impractical to observe directly in a test
    // run, so instead the computation is handed to a background thread and
    // the test bounds the WAIT via `recv_timeout` — the same idiom already
    // used in this codebase for exactly this class of problem (see
    // `cargo_test_runner.rs`'s `run_with_timeout` tests). The bound below
    // is generous (the memoized version finishes in low single-digit
    // milliseconds) so it cannot flake, yet it is nowhere near what an
    // un-memoized run of this fixture would need.
    let levels = 32;
    let fns = diamond_chain(levels);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let graph = CallGraph::build(&fns);
        let _ = tx.send(graph.max_call_depth());
    });

    let depth = rx.recv_timeout(std::time::Duration::from_secs(5)).expect(
        "compute_depth must be memoized: an unmemoized diamond chain at \
             levels=32 takes minutes, not seconds",
    );

    // depth(f_i) = 2*i + 1 (f0 = 1, each level adds a1/b1 hop then f1's own
    // hop): the topmost f is always the deepest node in this fixture shape.
    assert_eq!(depth, 2 * levels + 1);
}
