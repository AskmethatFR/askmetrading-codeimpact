use std::collections::{HashMap, HashSet};

use super::code_parser::ParsedFunction;

/// Builds and queries a call graph from parsed functions.
///
/// Computes transitive complexity (direct + sum of callees' transitive).
/// Detects call cycles to prevent infinite recursion.
#[derive(Clone, Debug)]
pub struct CallGraph {
    edges: HashMap<String, Vec<String>>,
    direct_complexity: HashMap<String, u32>,
    transitive_complexity: HashMap<String, u32>,
    cycle_nodes: HashSet<String>,
    max_depth: usize,
}

/// Working state for Tarjan's strongly-connected-components algorithm,
/// threaded through the recursive traversal in [`CallGraph::tarjan_scc`].
#[derive(Default)]
struct TarjanState<'a> {
    next_index: usize,
    index: HashMap<&'a str, usize>,
    lowlink: HashMap<&'a str, usize>,
    on_stack: HashSet<&'a str>,
    stack: Vec<&'a str>,
    in_cycle: HashSet<String>,
}

impl CallGraph {
    pub fn build(functions: &[ParsedFunction]) -> Self {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut direct_complexity: HashMap<String, u32> = HashMap::new();

        for f in functions {
            edges.insert(f.name.clone(), f.calls.clone());
            direct_complexity.insert(f.name.clone(), f.decision_points);
        }

        let cycle_nodes = Self::detect_cycles(&edges);
        let mut transitive_complexity: HashMap<String, u32> = HashMap::new();

        for name in edges.keys() {
            let tc = Self::compute_transitive(
                name,
                &edges,
                &direct_complexity,
                &cycle_nodes,
                &mut transitive_complexity,
            );
            transitive_complexity.insert(name.clone(), tc);
        }

        let max_depth = if edges.is_empty() {
            0
        } else {
            let mut max = 0usize;
            for name in edges.keys() {
                let mut visited = HashSet::new();
                let depth = Self::compute_depth(name, &edges, &cycle_nodes, &mut visited);
                max = max.max(depth);
            }
            max
        };

        Self {
            edges,
            direct_complexity,
            transitive_complexity,
            cycle_nodes,
            max_depth,
        }
    }

    /// Returns the direct complexity of a function.
    pub fn direct_of(&self, name: &str) -> u32 {
        self.direct_complexity.get(name).copied().unwrap_or(0)
    }

    /// Returns the transitive complexity of a function (direct + callees).
    pub fn transitive_of(&self, name: &str) -> u32 {
        self.transitive_complexity.get(name).copied().unwrap_or(0)
    }

    /// Whether the function is part of a call cycle.
    pub fn has_cycle(&self, name: &str) -> bool {
        self.cycle_nodes.contains(name)
    }

    /// Deepest call chain in the graph.
    pub fn max_call_depth(&self) -> usize {
        self.max_depth
    }

    /// Depth of call chain starting from this function.
    pub fn call_chain_depth(&self, name: &str) -> usize {
        if !self.edges.contains_key(name) {
            return 0;
        }
        let mut visited = HashSet::new();
        Self::compute_depth(name, &self.edges, &self.cycle_nodes, &mut visited)
    }

    /// Sum of all transitive complexities.
    pub fn transitive_total(&self) -> u32 {
        self.transitive_complexity.values().sum()
    }

    /// Returns list of functions in cycles.
    pub fn functions_with_cycles(&self) -> Vec<String> {
        let mut result: Vec<String> = self.cycle_nodes.iter().cloned().collect();
        result.sort();
        result
    }

    // --- Private helpers ---

    /// Computes which functions are part of a call cycle (self-recursion,
    /// mutual recursion, or a longer cycle) using Tarjan's strongly-connected
    /// components algorithm.
    ///
    /// A prior DFS-back-edge implementation walked `edges.keys()` — whose
    /// iteration order is randomized per process by `HashMap`'s `RandomState`
    /// — as the set of DFS roots. For a "confluence" graph (two distinct
    /// paths through different direct successors of a common ancestor that
    /// both converge on the same node before closing a cycle back to that
    /// ancestor), that DFS only follows the FIRST path to completion —
    /// coloring the shared node "done" — before starting the second path.
    /// The second path's back edge then targets an already-"done" node and
    /// is never observed, silently dropping that path's nodes from the
    /// result depending on which root ran first. Tarjan's SCC decomposition
    /// is a graph invariant: which nodes end up in the same
    /// strongly-connected component does not depend on where the traversal
    /// starts, so the result is correct and reproducible regardless of
    /// `HashMap` iteration order. Root iteration is still sorted below —
    /// not required for correctness, but it keeps the traversal itself
    /// reproducible for anyone stepping through it.
    fn detect_cycles(edges: &HashMap<String, Vec<String>>) -> HashSet<String> {
        let mut names: Vec<&str> = edges.keys().map(String::as_str).collect();
        names.sort_unstable();

        let mut state = TarjanState::default();
        for name in names {
            if !state.index.contains_key(name) {
                Self::tarjan_scc(name, edges, &mut state);
            }
        }
        state.in_cycle
    }

    fn tarjan_scc<'a>(
        node: &'a str,
        edges: &'a HashMap<String, Vec<String>>,
        state: &mut TarjanState<'a>,
    ) {
        let node_index = state.next_index;
        state.index.insert(node, node_index);
        state.lowlink.insert(node, node_index);
        state.next_index += 1;
        state.stack.push(node);
        state.on_stack.insert(node);

        let mut self_loop = false;

        if let Some(callees) = edges.get(node) {
            for callee in callees {
                let callee: &str = callee.as_str();
                if callee == node {
                    self_loop = true;
                }
                if !state.index.contains_key(callee) {
                    Self::tarjan_scc(callee, edges, state);
                    let callee_lowlink = state.lowlink[callee];
                    let node_lowlink = state.lowlink[node];
                    if callee_lowlink < node_lowlink {
                        state.lowlink.insert(node, callee_lowlink);
                    }
                } else if state.on_stack.contains(callee) {
                    let callee_index = state.index[callee];
                    let node_lowlink = state.lowlink[node];
                    if callee_index < node_lowlink {
                        state.lowlink.insert(node, callee_index);
                    }
                }
            }
        }

        if state.lowlink[node] == state.index[node] {
            let mut component: Vec<&str> = Vec::new();
            loop {
                let member = state.stack.pop().expect("SCC root must be on the stack");
                state.on_stack.remove(member);
                let is_root = member == node;
                component.push(member);
                if is_root {
                    break;
                }
            }
            if component.len() > 1 || self_loop {
                for member in component {
                    state.in_cycle.insert(member.to_string());
                }
            }
        }
    }

    fn compute_transitive(
        name: &str,
        edges: &HashMap<String, Vec<String>>,
        direct: &HashMap<String, u32>,
        cycle_nodes: &HashSet<String>,
        cache: &mut HashMap<String, u32>,
    ) -> u32 {
        if let Some(&cached) = cache.get(name) {
            return cached;
        }

        let direct_val = direct.get(name).copied().unwrap_or(0);

        // If in cycle, transitive = direct + non-cycle callees only
        if cycle_nodes.contains(name) {
            let mut total = direct_val;
            if let Some(callees) = edges.get(name) {
                for callee in callees {
                    if !cycle_nodes.contains(callee.as_str()) {
                        total +=
                            Self::compute_transitive(callee, edges, direct, cycle_nodes, cache);
                    }
                }
            }
            cache.insert(name.to_string(), total);
            return total;
        }

        let mut total = direct_val;
        if let Some(callees) = edges.get(name) {
            for callee in callees {
                total += Self::compute_transitive(callee, edges, direct, cycle_nodes, cache);
            }
        }

        cache.insert(name.to_string(), total);
        total
    }

    fn compute_depth(
        name: &str,
        edges: &HashMap<String, Vec<String>>,
        cycle_nodes: &HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> usize {
        if visited.contains(name) {
            return 0;
        }
        visited.insert(name.to_string());

        // If in cycle, depth stops at this node
        if cycle_nodes.contains(name) {
            visited.remove(name);
            return 1;
        }

        let max_child = edges
            .get(name)
            .map(|callees| {
                callees
                    .iter()
                    .map(|c| Self::compute_depth(c, edges, cycle_nodes, visited))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        visited.remove(name);
        1 + max_child
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List — detect_cycles / dfs_cycle determinism (#47 retry 1):
    // 1. cycle_detection_finds_all_nodes_in_a_confluence_graph — two distinct
    //    paths (through different direct successors of a common ancestor)
    //    converge on the same node, which then closes a cycle back to that
    //    ancestor. Every node on either path is structurally part of a cycle
    //    and must be reported, regardless of which node the (HashMap-ordered)
    //    root iteration visits first — the DFS-back-edge marking used before
    //    this fix only followed the FIRST path fully to completion (coloring
    //    the shared node black) before the second path started, so the
    //    second path's back edge was never observed and its nodes were
    //    silently dropped from cycle_nodes depending on root order.

    fn make_fn(name: &str, calls: Vec<&str>) -> ParsedFunction {
        ParsedFunction {
            name: name.to_string(),
            start_line: 1,
            calls: calls.into_iter().map(String::from).collect(),
            has_loop: false,
            has_nested_loop: false,
            decision_points: 1,
            depth: 0,
            match_arms: 0,
            calls_in_loops: vec![],
        }
    }

    #[test]
    fn cycle_detection_finds_all_nodes_in_a_confluence_graph() {
        // a -> b, a -> c, b -> d, c -> d, d -> a
        // Both "a -> b -> d -> a" and "a -> c -> d -> a" are genuine cycles;
        // a, b, c, d are ALL structurally part of some cycle.
        let fns = vec![
            make_fn("a", vec!["b", "c"]),
            make_fn("b", vec!["d"]),
            make_fn("c", vec!["d"]),
            make_fn("d", vec!["a"]),
        ];
        let graph = CallGraph::build(&fns);
        let cycles = graph.functions_with_cycles();
        assert_eq!(cycles, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn direct_of_known_function() {
        let fns = vec![ParsedFunction {
            name: "foo".into(),
            start_line: 1,
            calls: vec![],
            has_loop: false,
            has_nested_loop: false,
            decision_points: 5,
            depth: 0,
            match_arms: 0,
            calls_in_loops: vec![],
        }];
        let graph = CallGraph::build(&fns);
        assert_eq!(graph.direct_of("foo"), 5);
    }

    #[test]
    fn functions_with_cycles_returns_sorted() {
        let fns = vec![
            ParsedFunction {
                name: "b".into(),
                start_line: 1,
                calls: vec!["a".into()],
                has_loop: false,
                has_nested_loop: false,
                decision_points: 1,
                depth: 0,
                match_arms: 0,
                calls_in_loops: vec![],
            },
            ParsedFunction {
                name: "a".into(),
                start_line: 1,
                calls: vec!["b".into()],
                has_loop: false,
                has_nested_loop: false,
                decision_points: 1,
                depth: 0,
                match_arms: 0,
                calls_in_loops: vec![],
            },
        ];
        let graph = CallGraph::build(&fns);
        let cycles = graph.functions_with_cycles();
        assert_eq!(cycles, vec!["a", "b"]);
    }
}
