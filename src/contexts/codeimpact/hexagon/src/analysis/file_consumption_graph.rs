use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::alert_thresholds::ThresholdReport;
use super::code_metrics::{complexity_level_for, CodeMetrics};
use super::complexity_detector::WarningSeverity;
use super::ecological_impact::EcologicalImpact;
use super::economic_impact::EconomicImpact;
use super::errors::AnalysisError;
use super::measurement::UnmeasurableReason;

/// A dependency between two files: `from` depends on `to`.
#[derive(Clone, Debug, PartialEq)]
pub struct FileDependency {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// A file that was never successfully measured — its source could not be
/// read from disk, or could not be parsed (D3, #50). Distinct from
/// `CodeMetrics::complexity_level() == "none"` (the file WAS read and
/// parsed, it simply has zero functions): this file never reached
/// `CodeMetrics` at all, so it carries no numbers to enter any sum.
#[derive(Clone, Debug, PartialEq)]
pub struct UnmeasurableFile {
    pub path: PathBuf,
    pub reason: UnmeasurableReason,
}

/// Immutable value object representing the consumption graph of a project.
///
/// Nodes are the analyzed files, edges are dependencies between them.
/// Detects cycles and computes consumption chains.
#[derive(Clone, Debug)]
pub struct FileConsumptionGraph {
    files: Vec<PathBuf>,
    dependencies: Vec<FileDependency>,
    adjacency: HashMap<PathBuf, Vec<PathBuf>>,
    per_file_metrics: HashMap<PathBuf, CodeMetrics>,
    cycle_nodes: HashSet<PathBuf>,
    max_depth: usize,
    unmeasurable_files: Vec<UnmeasurableFile>,
    /// The project's threshold-breach outcome (US8) — `None` when no
    /// calling use case ever evaluated thresholds against this graph
    /// (distinct from `Some(report)` with an empty `breaches()`, which
    /// means thresholds WERE evaluated and none breached).
    threshold_report: Option<ThresholdReport>,
}

impl FileConsumptionGraph {
    /// Build a new graph from a list of files and their dependencies.
    ///
    /// Validates that every dependency's `from` and `to` exist in the file list.
    pub fn build(
        files: &[(PathBuf, CodeMetrics)],
        dependencies: Vec<FileDependency>,
    ) -> Result<Self, AnalysisError> {
        let file_set: HashSet<&PathBuf> = files.iter().map(|(p, _)| p).collect();

        // Validate that all dependency endpoints exist in the file list
        for dep in &dependencies {
            if !file_set.contains(&dep.from) {
                return Err(AnalysisError::AnalysisFailed(format!(
                    "fichier source '{}' introuvable dans la liste des fichiers",
                    dep.from.display()
                )));
            }
            if !file_set.contains(&dep.to) {
                return Err(AnalysisError::AnalysisFailed(format!(
                    "fichier destination '{}' introuvable dans la liste des fichiers",
                    dep.to.display()
                )));
            }
        }

        let file_list: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
        let per_file_metrics: HashMap<PathBuf, CodeMetrics> = files.iter().cloned().collect();

        // Build adjacency map: what each file depends on (owned)
        let mut adjacency: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for (path, _) in files {
            adjacency.entry(path.clone()).or_default();
        }
        for dep in &dependencies {
            adjacency
                .entry(dep.from.clone())
                .or_default()
                .push(dep.to.clone());
        }

        let cycle_nodes = Self::detect_cycles(&adjacency);

        let max_depth = if file_list.is_empty() {
            0
        } else {
            let mut max = 0usize;
            for path in &file_list {
                let depth = Self::compute_depth(path, &adjacency, &cycle_nodes);
                max = max.max(depth);
            }
            max
        };

        Ok(Self {
            files: file_list,
            dependencies,
            adjacency,
            per_file_metrics,
            cycle_nodes,
            max_depth,
            unmeasurable_files: Vec::new(),
            threshold_report: None,
        })
    }

    /// Attaches the files that could not be measured (failed to read or to
    /// parse) — builder style, consistent with `CodeMetrics::with_*` (D3,
    /// #50).
    pub fn with_unmeasurable_files(mut self, files: Vec<UnmeasurableFile>) -> Self {
        self.unmeasurable_files = files;
        self
    }

    /// Files that could not be measured — see `UnmeasurableFile`.
    pub fn unmeasurable_files(&self) -> &[UnmeasurableFile] {
        &self.unmeasurable_files
    }

    /// Attaches the outcome of evaluating this project's aggregate impact
    /// against its configured alert thresholds (US8, AD-3: the report
    /// travels to the writers on the data object, not via a new
    /// `ReportWriter` port method) — builder style, mirroring
    /// `with_unmeasurable_files`.
    pub fn with_threshold_report(mut self, report: ThresholdReport) -> Self {
        self.threshold_report = Some(report);
        self
    }

    /// The project's threshold-breach outcome, if a calling use case
    /// evaluated one — see `threshold_report` field docs.
    pub fn threshold_report(&self) -> Option<&ThresholdReport> {
        self.threshold_report.as_ref()
    }

    /// Returns the files in the graph.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Returns the consumption chain starting from `file` (includes `file` itself).
    ///
    /// The chain follows the dependency direction: if A depends on B,
    /// the chain includes A → B → (B's dependencies) → ...
    pub fn consumption_chain(&self, file: &Path) -> Vec<PathBuf> {
        if !self.per_file_metrics.contains_key(file) {
            return Vec::new();
        }

        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut in_path = HashSet::new();
        Self::dfs_chain(
            file,
            &self.adjacency,
            &mut visited,
            &mut in_path,
            &mut chain,
        );
        chain
    }

    /// Files that are part of at least one dependency cycle, sorted.
    pub fn files_with_cycles(&self) -> Vec<&PathBuf> {
        let mut result: Vec<&PathBuf> = self.cycle_nodes.iter().collect();
        result.sort();
        result
    }

    /// Per-file metrics map.
    pub fn per_file_metrics(&self) -> &HashMap<PathBuf, CodeMetrics> {
        &self.per_file_metrics
    }

    /// Aggregated project metrics (sum of all files).
    pub fn aggregated_metrics(&self) -> ProjectMetrics {
        let mut total_cc = 0u32;
        let mut total_tc = 0u32;
        let mut total_hidden = 0u32;
        let mut max_call_depth = 0usize;
        let mut total_warnings = 0usize;
        let mut critical_warnings = 0usize;
        let mut total_io_in_loops = 0usize;
        let mut total_unclassifiable_io_in_loops = 0usize;
        let mut hotspot_files = 0usize;

        for metrics in self.per_file_metrics.values() {
            total_cc = total_cc.saturating_add(metrics.cyclomatic_complexity());
            total_tc = total_tc.saturating_add(metrics.transitive_complexity());
            total_hidden = total_hidden.saturating_add(metrics.hidden_complexity());
            max_call_depth = max_call_depth.max(metrics.max_call_depth());
            total_warnings += metrics.warnings().len();
            critical_warnings += metrics
                .warnings()
                .iter()
                .filter(|w| w.severity == WarningSeverity::Critical)
                .count();
            total_io_in_loops += metrics.io_in_loops().len();
            total_unclassifiable_io_in_loops += metrics.unclassifiable_io_in_loops_count();
            if metrics.complexity_level() == "critical" {
                hotspot_files += 1;
            }
        }

        let total_economic_impact = self
            .per_file_metrics
            .values()
            .filter_map(|m| m.economic_impact().cloned())
            .reduce(|a, b| a + b);

        let total_ecological_impact = self
            .per_file_metrics
            .values()
            .filter_map(|m| m.ecological_impact().cloned())
            .reduce(|a, b| a + b);

        let mut files_with_cycles: Vec<PathBuf> = self.cycle_nodes.iter().cloned().collect();
        files_with_cycles.sort();

        ProjectMetrics {
            total_files: self.files.len(),
            total_cyclomatic_complexity: total_cc,
            total_transitive_complexity: total_tc,
            total_hidden_complexity: total_hidden,
            max_call_depth,
            files_with_cycles,
            total_warnings,
            critical_warnings,
            total_io_in_loops,
            total_unclassifiable_io_in_loops,
            hotspot_files,
            total_economic_impact,
            total_ecological_impact,
            unmeasurable_files: self.unmeasurable_files.len(),
            median_file_cyclomatic_complexity: median_cyclomatic_complexity(&self.per_file_metrics),
        }
    }

    /// Total number of dependency edges.
    pub fn total_dependencies(&self) -> usize {
        self.dependencies.len()
    }

    /// Deepest consumption chain in the graph.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    // ── Private helpers ──

    /// DFS 3-colors cycle detection.
    const COLOR_WHITE: u8 = 0; // unvisited
    const COLOR_GREY: u8 = 1; // in current path
    const COLOR_BLACK: u8 = 2; // done

    fn detect_cycles(adjacency: &HashMap<PathBuf, Vec<PathBuf>>) -> HashSet<PathBuf> {
        let mut color: HashMap<&Path, u8> = HashMap::new();
        let mut in_cycle: HashSet<PathBuf> = HashSet::new();

        for path in adjacency.keys() {
            color.entry(path.as_path()).or_insert(Self::COLOR_WHITE);
        }

        for path in adjacency.keys() {
            if color.get(path.as_path()) == Some(&Self::COLOR_WHITE) {
                let mut path_stack: Vec<&Path> = Vec::new();
                Self::dfs_cycle(
                    path.as_path(),
                    adjacency,
                    &mut color,
                    &mut path_stack,
                    &mut in_cycle,
                );
            }
        }

        in_cycle
    }

    fn dfs_cycle<'a>(
        node: &'a Path,
        adjacency: &'a HashMap<PathBuf, Vec<PathBuf>>,
        color: &mut HashMap<&'a Path, u8>,
        path_stack: &mut Vec<&'a Path>,
        in_cycle: &mut HashSet<PathBuf>,
    ) {
        color.insert(node, Self::COLOR_GREY);
        path_stack.push(node);

        if let Some(callees) = adjacency.get(node) {
            for callee in callees {
                let callee_path: &Path = callee.as_path();
                match color.get(callee_path).copied().unwrap_or(Self::COLOR_WHITE) {
                    Self::COLOR_WHITE => {
                        Self::dfs_cycle(callee_path, adjacency, color, path_stack, in_cycle);
                    }
                    Self::COLOR_GREY => {
                        // Back-edge: mark all nodes from callee to node as cycle
                        let mut in_cycle_range = false;
                        for &n in path_stack.iter() {
                            if n == callee_path {
                                in_cycle_range = true;
                            }
                            if in_cycle_range {
                                in_cycle.insert(n.to_path_buf());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        path_stack.pop();
        color.insert(node, Self::COLOR_BLACK);
    }

    /// DFS to compute the consumption chain.
    ///
    /// Uses `visited` for globally-completed nodes and `in_path` for
    /// current-path cycle detection. When a cycle is found, appends the
    /// cycle-closing node so the caller sees the full cycle path.
    fn dfs_chain(
        node: &Path,
        adjacency: &HashMap<PathBuf, Vec<PathBuf>>,
        visited: &mut HashSet<PathBuf>,
        in_path: &mut HashSet<PathBuf>,
        chain: &mut Vec<PathBuf>,
    ) {
        if visited.contains(node) {
            return;
        }

        if in_path.contains(node) {
            // Cycle detected: close the cycle by re-appending this node
            chain.push(node.to_path_buf());
            return;
        }

        in_path.insert(node.to_path_buf());

        if adjacency.contains_key(node) {
            chain.push(node.to_path_buf());
        }

        // Recurse into dependencies
        if let Some(callees) = adjacency.get(node) {
            for callee in callees {
                Self::dfs_chain(callee.as_path(), adjacency, visited, in_path, chain);
            }
        }

        in_path.remove(node);
        visited.insert(node.to_path_buf());
    }

    /// Compute the depth of the longest chain starting from this node.
    fn compute_depth(
        node: &Path,
        adjacency: &HashMap<PathBuf, Vec<PathBuf>>,
        cycle_nodes: &HashSet<PathBuf>,
    ) -> usize {
        // If in cycle, depth stops at this node
        if cycle_nodes.contains(node) {
            return 1;
        }

        let max_child = adjacency
            .get(node)
            .map(|callees| {
                callees
                    .iter()
                    .map(|c| Self::compute_depth(c.as_path(), adjacency, cycle_nodes))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        1 + max_child
    }
}

/// Aggregated metrics for an entire project.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectMetrics {
    pub total_files: usize,
    pub total_cyclomatic_complexity: u32,
    pub total_transitive_complexity: u32,
    /// Sum of each file's `CodeMetrics::hidden_complexity()` (itself the sum
    /// of its functions' per-function hidden complexity) — additive at the
    /// atom, never `max(0, ΣT - ΣC)` nor `Σ max(0, Tᵢ - Cᵢ)` (ADR-0012).
    pub total_hidden_complexity: u32,
    pub max_call_depth: usize,
    pub files_with_cycles: Vec<PathBuf>,
    /// Total `ComplexityWarning` count across all files. `IoInLoopWarning`
    /// has no severity and is never folded in here (ubiquitous language: an
    /// I/O-in-loop is not a "complexity warning") — see `total_io_in_loops`.
    pub total_warnings: usize,
    /// `total_warnings`' subset with `WarningSeverity::Critical`.
    pub critical_warnings: usize,
    /// Total `IoInLoopWarning` count across all files — its own category.
    pub total_io_in_loops: usize,
    /// Sum of each file's `CodeMetrics::unclassifiable_io_in_loops_count()`
    /// (#56 T2) — calls whose receiver could not be classified at all
    /// (`IoClassification::Unknown`). An aggregate signal only (ADR-0010/
    /// ADR-0014 §4): abstention is a NUMBER, never a per-line pseudo-warning,
    /// and it must reach the project surface, not just the per-file one.
    pub total_unclassifiable_io_in_loops: usize,
    /// Number of files whose `complexity_level()` is `"critical"`.
    pub hotspot_files: usize,
    pub total_economic_impact: Option<EconomicImpact>,
    pub total_ecological_impact: Option<EcologicalImpact>,
    /// Count of files that could not be measured (failed to read or parse)
    /// — see `FileConsumptionGraph::unmeasurable_files()` for the list.
    /// `total_files` keeps its existing meaning (MEASURED files only): this
    /// is a separate counter, not folded into it (D3, #50).
    pub unmeasurable_files: usize,
    /// Median of MEASURED files' `cyclomatic_complexity()` — the number
    /// `complexity_level()` judges, not `total_cyclomatic_complexity`. The
    /// total is off the per-file scale `complexity_level_for` was
    /// calibrated against (summing every file onto one file's scale reads
    /// "critical" for nearly any real project, ADR-0010); the median stays
    /// on it, because it IS one file's value. Even file count -> the two
    /// middle values are averaged, round-half-up.
    pub median_file_cyclomatic_complexity: u32,
}

impl ProjectMetrics {
    /// The project's complexity level, judged on its median (typical) file
    /// — see `median_file_cyclomatic_complexity`. An empty project (no
    /// measured files) reads "none", mirroring
    /// `CodeMetrics::complexity_level()`'s own zero-function state, instead
    /// of the misleadingly clean "low" `complexity_level_for(0)` would give.
    pub fn complexity_level(&self) -> &'static str {
        if self.total_files == 0 {
            return "none";
        }
        complexity_level_for(self.median_file_cyclomatic_complexity)
    }
}

/// Median of `cyclomatic_complexity()` across `per_file_metrics` — plain
/// sort + index, no crate (hexagon stays zero-dep). Even count -> the two
/// middle values average, round-half-up via integer arithmetic.
fn median_cyclomatic_complexity(per_file_metrics: &HashMap<PathBuf, CodeMetrics>) -> u32 {
    let mut values: Vec<u32> = per_file_metrics
        .values()
        .map(|m| m.cyclomatic_complexity())
        .collect();
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        let lower = values[n / 2 - 1] as u64;
        let upper = values[n / 2] as u64;
        (lower + upper).div_ceil(2) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
        CodeMetrics::with_call_graph(cc, tc, 0, vec![], vec![])
    }

    fn path(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn build_empty() {
        let graph = FileConsumptionGraph::build(&[], vec![]).unwrap();
        assert_eq!(graph.total_dependencies(), 0);
        assert_eq!(graph.max_depth(), 0);
    }

    #[test]
    fn build_single_file() {
        let files = vec![(path("a.rs"), make_metrics(5, 5))];
        let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
        assert_eq!(graph.total_dependencies(), 0);
        assert_eq!(graph.max_depth(), 1);
    }

    #[test]
    fn chain_detected() {
        let files = vec![
            (path("a.rs"), make_metrics(1, 1)),
            (path("b.rs"), make_metrics(2, 2)),
            (path("c.rs"), make_metrics(3, 3)),
        ];
        let deps = vec![
            FileDependency {
                from: path("a.rs"),
                to: path("b.rs"),
            },
            FileDependency {
                from: path("b.rs"),
                to: path("c.rs"),
            },
        ];
        let graph = FileConsumptionGraph::build(&files, deps).unwrap();
        assert_eq!(graph.total_dependencies(), 2);
        assert_eq!(graph.max_depth(), 3);
    }

    #[test]
    fn cycle_detected() {
        let files = vec![
            (path("a.rs"), make_metrics(1, 1)),
            (path("b.rs"), make_metrics(2, 2)),
        ];
        let deps = vec![
            FileDependency {
                from: path("a.rs"),
                to: path("b.rs"),
            },
            FileDependency {
                from: path("b.rs"),
                to: path("a.rs"),
            },
        ];
        let graph = FileConsumptionGraph::build(&files, deps).unwrap();
        assert_eq!(graph.files_with_cycles().len(), 2);
    }

    #[test]
    fn missing_node_errors() {
        let files = vec![(path("a.rs"), make_metrics(1, 1))];
        let deps = vec![FileDependency {
            from: path("x.rs"),
            to: path("a.rs"),
        }];
        assert!(FileConsumptionGraph::build(&files, deps).is_err());
    }

    #[test]
    fn consumption_chain_cycle_shows_full_path() {
        let files = vec![
            (path("a.rs"), make_metrics(1, 1)),
            (path("b.rs"), make_metrics(2, 2)),
            (path("c.rs"), make_metrics(3, 3)),
        ];
        // A → B → C → A (3-node cycle)
        let deps = vec![
            FileDependency {
                from: path("a.rs"),
                to: path("b.rs"),
            },
            FileDependency {
                from: path("b.rs"),
                to: path("c.rs"),
            },
            FileDependency {
                from: path("c.rs"),
                to: path("a.rs"),
            },
        ];
        let graph = FileConsumptionGraph::build(&files, deps).unwrap();
        let chain = graph.consumption_chain(&path("a.rs"));
        assert_eq!(
            chain,
            vec![path("a.rs"), path("b.rs"), path("c.rs"), path("a.rs")]
        );
    }

    #[test]
    fn consumption_chain_2node_cycle_shows_full_path() {
        let files = vec![
            (path("a.rs"), make_metrics(1, 1)),
            (path("b.rs"), make_metrics(2, 2)),
        ];
        // A → B → A (2-node cycle)
        let deps = vec![
            FileDependency {
                from: path("a.rs"),
                to: path("b.rs"),
            },
            FileDependency {
                from: path("b.rs"),
                to: path("a.rs"),
            },
        ];
        let graph = FileConsumptionGraph::build(&files, deps).unwrap();
        let chain = graph.consumption_chain(&path("a.rs"));
        assert_eq!(chain, vec![path("a.rs"), path("b.rs"), path("a.rs")]);
    }

    #[test]
    fn aggregated_metrics_sum() {
        let files = vec![
            (path("a.rs"), make_metrics(5, 10)),
            (path("b.rs"), make_metrics(3, 7)),
        ];
        let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
        let pm = graph.aggregated_metrics();
        assert_eq!(pm.total_files, 2);
        assert_eq!(pm.total_cyclomatic_complexity, 8);
        assert_eq!(pm.total_transitive_complexity, 17);
    }

    #[test]
    fn aggregated_metrics_sums_unclassifiable_io_in_loops_across_files() {
        // #56 T2 — the project total is a per-file SUM, additive at the
        // atom, same shape as total_io_in_loops (ADR-0010/ADR-0014 §4: the
        // signal must reach the project surface too, not just per-file).
        let files = vec![
            (
                path("a.rs"),
                make_metrics(5, 10).with_unclassifiable_io_in_loops_count(2),
            ),
            (
                path("b.rs"),
                make_metrics(3, 7).with_unclassifiable_io_in_loops_count(1),
            ),
        ];
        let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
        let pm = graph.aggregated_metrics();
        assert_eq!(pm.total_unclassifiable_io_in_loops, 3);
    }
}
