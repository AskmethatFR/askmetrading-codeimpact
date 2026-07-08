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

    fn detect_cycles(edges: &HashMap<String, Vec<String>>) -> HashSet<String> {
        // 0 = white (unvisited), 1 = grey (in current path), 2 = black (done)
        let mut color: HashMap<&str, u8> = HashMap::new();
        let mut in_cycle: HashSet<String> = HashSet::new();

        for name in edges.keys() {
            color.entry(name.as_str()).or_insert(0);
        }

        for name in edges.keys() {
            let name_str: &str = name;
            if color.get(name_str) == Some(&0) {
                let mut path: Vec<&str> = Vec::new();
                Self::dfs_cycle(name_str, edges, &mut color, &mut path, &mut in_cycle);
            }
        }

        in_cycle
    }

    fn dfs_cycle<'a>(
        node: &'a str,
        edges: &'a HashMap<String, Vec<String>>,
        color: &mut HashMap<&'a str, u8>,
        path: &mut Vec<&'a str>,
        in_cycle: &mut HashSet<String>,
    ) {
        color.insert(node, 1); // grey
        path.push(node);

        if let Some(callees) = edges.get(node) {
            for callee in callees {
                let callee: &str = callee.as_str();
                match color.get(callee).copied().unwrap_or(0) {
                    0 => {
                        Self::dfs_cycle(callee, edges, color, path, in_cycle);
                    }
                    1 => {
                        // Back-edge detected: mark all nodes from callee to node as cycle
                        let mut in_cycle_range = false;
                        for &n in path.iter() {
                            if n == callee {
                                in_cycle_range = true;
                            }
                            if in_cycle_range {
                                in_cycle.insert(n.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        path.pop();
        color.insert(node, 2); // black
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
            },
        ];
        let graph = CallGraph::build(&fns);
        let cycles = graph.functions_with_cycles();
        assert_eq!(cycles, vec!["a", "b"]);
    }
}
