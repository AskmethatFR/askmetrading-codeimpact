use super::call_graph::CallGraph;
use super::code_metrics::CodeMetrics;
use super::code_parser::ParsedFunction;

/// Economic impact of a code file: estimated CPU and memory costs.
///
/// Costs are derived from complexity metrics using heuristic formulas.
/// A file with complexity = 1 has near-zero cost.
#[derive(Clone, Debug, PartialEq)]
pub struct EconomicImpact {
    cpu_cost_microdollars: f64,
    memory_bytes: u64,
    total_cost_microdollars: f64,
    level: &'static str,
}

impl EconomicImpact {
    pub fn new(
        cpu_cost_microdollars: f64,
        memory_bytes: u64,
        total_cost_microdollars: f64,
        level: &'static str,
    ) -> Self {
        Self {
            cpu_cost_microdollars,
            memory_bytes,
            total_cost_microdollars,
            level,
        }
    }

    pub fn cpu_cost_microdollars(&self) -> f64 {
        self.cpu_cost_microdollars
    }

    pub fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }

    pub fn total_cost_microdollars(&self) -> f64 {
        self.total_cost_microdollars
    }

    pub fn level(&self) -> &'static str {
        self.level
    }
}

/// Compute the level string from a total cost value.
///
/// Thresholds: 0–10 = low, 11–20 = moderate, 21–40 = high, 41+ = critical.
pub fn compute_level(total_cost: f64) -> &'static str {
    if total_cost <= 10.0 {
        "low"
    } else if total_cost <= 20.0 {
        "moderate"
    } else if total_cost <= 40.0 {
        "high"
    } else {
        "critical"
    }
}

impl std::ops::Add for EconomicImpact {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let cpu = self.cpu_cost_microdollars + other.cpu_cost_microdollars;
        let mem = self.memory_bytes + other.memory_bytes;
        let total = self.total_cost_microdollars + other.total_cost_microdollars;
        Self::new(cpu, mem, total, compute_level(total))
    }
}

/// Domain service that estimates economic impact from complexity metrics.
///
/// Heuristics:
/// - CPU cost: (direct_complexity × CPU_COST_DIRECT_WEIGHT + transitive_complexity × CPU_COST_TRANSITIVE_WEIGHT
///   + max_call_depth × CPU_COST_DEPTH_WEIGHT + warnings × CPU_COST_WARNING_WEIGHT) μ$
/// - Memory: (direct_complexity × MEMORY_DIRECT_COST + hidden_complexity × MEMORY_HIDDEN_COST
///   + functions_with_loops × MEMORY_LOOP_COST) bytes
/// - Total: cpu_cost + memory_bytes × MEMORY_TO_TOTAL_RATIO (memory is cheap)
/// - Level: same thresholds as complexity (0–10=low, 11–20=moderate,
///   21–40=high, 41+=critical)
pub struct EconomicImpactEstimator;

impl EconomicImpactEstimator {
    /// Weight per unit of direct complexity for CPU cost (μ$).
    pub const CPU_COST_DIRECT_WEIGHT: f64 = 0.5;
    /// Weight per unit of transitive complexity for CPU cost (μ$).
    pub const CPU_COST_TRANSITIVE_WEIGHT: f64 = 0.3;
    /// Weight per level of call depth for CPU cost (μ$).
    pub const CPU_COST_DEPTH_WEIGHT: f64 = 1.0;
    /// Weight per warning for CPU cost (μ$).
    pub const CPU_COST_WARNING_WEIGHT: f64 = 2.0;
    /// Bytes per unit of direct complexity.
    pub const MEMORY_DIRECT_COST: u64 = 100;
    /// Bytes per unit of hidden (transitive - direct) complexity.
    pub const MEMORY_HIDDEN_COST: u64 = 200;
    /// Additional bytes per function containing a loop.
    pub const MEMORY_LOOP_COST: u64 = 1024;
    /// Ratio of memory cost to include in total cost (μ$ per byte).
    pub const MEMORY_TO_TOTAL_RATIO: f64 = 0.0001;

    pub fn estimate(
        metrics: &CodeMetrics,
        functions: &[ParsedFunction],
        _call_graph: &CallGraph,
    ) -> EconomicImpact {
        let direct = metrics.cyclomatic_complexity() as f64;
        let transitive = metrics.transitive_complexity() as f64;
        let max_depth = metrics.max_call_depth() as f64;
        let warnings_count = metrics.warnings().len() as f64;

        let cpu_cost = direct * Self::CPU_COST_DIRECT_WEIGHT
            + transitive * Self::CPU_COST_TRANSITIVE_WEIGHT
            + max_depth * Self::CPU_COST_DEPTH_WEIGHT
            + warnings_count * Self::CPU_COST_WARNING_WEIGHT;

        let hidden = metrics.hidden_complexity() as u64;
        let loops_count = functions.iter().filter(|f| f.has_loop).count() as u64;

        let memory = (metrics.cyclomatic_complexity() as u64) * Self::MEMORY_DIRECT_COST
            + hidden * Self::MEMORY_HIDDEN_COST
            + loops_count * Self::MEMORY_LOOP_COST;

        let total = cpu_cost + memory as f64 * Self::MEMORY_TO_TOTAL_RATIO;

        let level = compute_level(total);

        EconomicImpact::new(cpu_cost, memory, total, level)
    }
}

#[cfg(test)]
mod tests {
    use super::super::call_graph::CallGraph;
    use super::super::code_metrics::CodeMetrics;
    use super::super::code_parser::ParsedFunction;
    use super::*;

    // Test List:
    // 1. estimate_trivial → near-zero cost
    // 2. estimate_with_warnings → warnings add 2 μ$ each
    // 3. estimate_with_loops → loops add 1024 bytes each
    // 4. estimate_level_low → total ≤ 10
    // 5. estimate_level_moderate → total 11–20
    // 6. estimate_level_high → total 21–40
    // 7. estimate_level_critical → total > 40
    // 8. estimate_hidden_complexity → hidden adds 200 bytes per point

    fn make_fn(
        name: &str,
        decision_points: u32,
        has_loop: bool,
        calls: Vec<&str>,
    ) -> ParsedFunction {
        ParsedFunction {
            name: name.to_string(),
            start_line: 1,
            calls: calls.into_iter().map(String::from).collect(),
            has_loop,
            has_nested_loop: false,
            decision_points,
            depth: 0,
            match_arms: 0,
            calls_in_loops: vec![],
        }
    }

    #[test]
    fn estimate_trivial_near_zero() {
        let metrics = CodeMetrics::new(1);
        let fns = vec![make_fn("main", 1, false, vec![])];
        let graph = CallGraph::build(&fns);
        let impact = EconomicImpactEstimator::estimate(&metrics, &fns, &graph);
        assert!(impact.cpu_cost_microdollars() < 1.0);
        assert!(impact.memory_bytes() < 200);
        assert!(impact.total_cost_microdollars() < 1.0);
        assert_eq!(impact.level(), "low");
    }

    #[test]
    fn estimate_with_warnings() {
        let metrics = {
            use super::super::complexity_detector::{
                ComplexityWarning, WarningPattern, WarningSeverity,
            };
            CodeMetrics::new(5).with_warnings(vec![
                ComplexityWarning {
                    pattern: WarningPattern::DeepConditional,
                    severity: WarningSeverity::Warning,
                    function: "foo".into(),
                    message: "w".into(),
                    suggestion: "s".into(),
                },
                ComplexityWarning {
                    pattern: WarningPattern::NestedLoops,
                    severity: WarningSeverity::Warning,
                    function: "bar".into(),
                    message: "w".into(),
                    suggestion: "s".into(),
                },
            ])
        };
        let fns = vec![make_fn("main", 5, false, vec![])];
        let graph = CallGraph::build(&fns);
        let impact = EconomicImpactEstimator::estimate(&metrics, &fns, &graph);
        // cpu = 5*0.5 + 5*0.3 + 0*1.0 + 2*2.0 = 2.5 + 1.5 + 0.0 + 4.0 = 8.0
        assert!((impact.cpu_cost_microdollars() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn estimate_with_loops() {
        let metrics = CodeMetrics::new(5);
        let fns = vec![make_fn("main", 5, true, vec![])];
        let graph = CallGraph::build(&fns);
        let impact = EconomicImpactEstimator::estimate(&metrics, &fns, &graph);
        // cpu = 5*0.5 + 5*0.3 + 0*1.0 + 0*2.0 = 2.5 + 1.5 + 0.0 = 4.0
        // memory = 5*100 + 0*200 + 1*1024 = 500 + 0 + 1024 = 1524
        assert!((impact.cpu_cost_microdollars() - 4.0).abs() < 1e-9);
        assert_eq!(impact.memory_bytes(), 1524);
    }

    #[test]
    fn estimate_level_low() {
        assert_eq!(compute_level(0.0), "low");
        assert_eq!(compute_level(5.0), "low");
        assert_eq!(compute_level(10.0), "low");
    }

    #[test]
    fn estimate_level_moderate() {
        assert_eq!(compute_level(10.1), "moderate");
        assert_eq!(compute_level(15.0), "moderate");
        assert_eq!(compute_level(20.0), "moderate");
    }

    #[test]
    fn estimate_level_high() {
        assert_eq!(compute_level(20.1), "high");
        assert_eq!(compute_level(30.0), "high");
        assert_eq!(compute_level(40.0), "high");
    }

    #[test]
    fn estimate_level_critical() {
        assert_eq!(compute_level(40.1), "critical");
        assert_eq!(compute_level(50.0), "critical");
        assert_eq!(compute_level(100.0), "critical");
    }

    #[test]
    fn estimate_hidden_complexity() {
        // direct=5, transitive=20 → hidden=15 → 15*200 = 3000
        let metrics = CodeMetrics::with_call_graph(5, 20, 1, vec![], vec![]);
        let fns = vec![make_fn("main", 5, false, vec![])];
        let graph = CallGraph::build(&fns);
        let impact = EconomicImpactEstimator::estimate(&metrics, &fns, &graph);
        // memory = 5*100 + 15*200 + 0*1024 = 500 + 3000 = 3500
        assert_eq!(impact.memory_bytes(), 3500);
    }
}
