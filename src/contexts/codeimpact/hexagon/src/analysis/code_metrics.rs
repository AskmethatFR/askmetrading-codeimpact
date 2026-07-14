use super::code_location::CodeLocation;
use super::complexity_detector::ComplexityWarning;
use super::ecological_impact::EcologicalImpact;
use super::economic_impact::EconomicImpact;
use super::io_in_loop_warning::IoInLoopWarning;

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDetail {
    pub name: String,
    pub location: CodeLocation,
    pub direct: u32,
    pub transitive: u32,
    pub call_depth: usize,
    pub in_cycle: bool,
}

impl FunctionDetail {
    /// Complexity hidden in this function's calls: transitive - direct.
    ///
    /// `transitive >= direct` holds by construction (`CallGraph::compute_transitive`
    /// sums non-negative `u32` contributions), so this is measured at the
    /// atom and never re-derived by subtracting two aggregates (ADR-0012).
    pub fn hidden(&self) -> u32 {
        debug_assert!(
            self.transitive >= self.direct,
            "call-graph invariant broken: transitive({}) < direct({}) for {}",
            self.transitive,
            self.direct,
            self.name
        );
        self.transitive.saturating_sub(self.direct)
    }
}

/// Complexity level thresholds, shared by `CodeMetrics::complexity_level()`
/// and the project-level JSON writer (`ProjectMetrics` carries no
/// `CodeMetrics` of its own, so it cannot call the method — see ADR-0012).
pub fn complexity_level_for(cyclomatic_complexity: u32) -> &'static str {
    match cyclomatic_complexity {
        0..=10 => "low",
        11..=20 => "moderate",
        21..=40 => "high",
        _ => "critical",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeMetrics {
    cyclomatic_complexity: u32,
    transitive_complexity: u32,
    max_call_depth: usize,
    functions_with_cycles: Vec<String>,
    function_details: Vec<FunctionDetail>,
    warnings: Vec<ComplexityWarning>,
    economic_impact: Option<EconomicImpact>,
    ecological_impact: Option<EcologicalImpact>,
    io_in_loops: Vec<IoInLoopWarning>,
}

impl CodeMetrics {
    pub fn new(cyclomatic_complexity: u32) -> Self {
        Self {
            cyclomatic_complexity,
            transitive_complexity: cyclomatic_complexity,
            max_call_depth: 0,
            functions_with_cycles: Vec::new(),
            function_details: Vec::new(),
            warnings: Vec::new(),
            economic_impact: None,
            ecological_impact: None,
            io_in_loops: Vec::new(),
        }
    }

    pub fn with_call_graph(
        cyclomatic_complexity: u32,
        transitive_complexity: u32,
        max_call_depth: usize,
        functions_with_cycles: Vec<String>,
        function_details: Vec<FunctionDetail>,
    ) -> Self {
        Self {
            cyclomatic_complexity,
            transitive_complexity,
            max_call_depth,
            functions_with_cycles,
            function_details,
            warnings: Vec::new(),
            economic_impact: None,
            ecological_impact: None,
            io_in_loops: Vec::new(),
        }
    }

    pub fn cyclomatic_complexity(&self) -> u32 {
        self.cyclomatic_complexity
    }

    pub fn transitive_complexity(&self) -> u32 {
        self.transitive_complexity
    }

    pub fn max_call_depth(&self) -> usize {
        self.max_call_depth
    }

    pub fn functions_with_cycles(&self) -> &[String] {
        &self.functions_with_cycles
    }

    pub fn function_details(&self) -> &[FunctionDetail] {
        &self.function_details
    }

    /// Hidden complexity = the SUM of each measured function's hidden
    /// complexity (`FunctionDetail::hidden()`), never a subtraction of the
    /// two file-level aggregates.
    ///
    /// `cyclomatic_complexity` counts a `+1` per FILE (`1 + Σ decision_points`)
    /// while `transitive_complexity` carries no such `+1` — they are not the
    /// same unit, so `transitive_complexity - cyclomatic_complexity` silently
    /// fabricates (or, worse, `saturating_sub`s away) a wrong number. Measuring
    /// at the function (where `transitive >= direct` holds by construction)
    /// and summing is the only formula that is correct by construction
    /// (ADR-0012).
    ///
    /// A `CodeMetrics` with no `function_details` (nothing was ever measured
    /// at the function level) reports `0`, never a fabricated non-zero value
    /// derived from the file-level aggregates (ADR-0010).
    pub fn hidden_complexity(&self) -> u32 {
        self.function_details.iter().map(FunctionDetail::hidden).sum()
    }

    pub fn complexity_level(&self) -> &'static str {
        complexity_level_for(self.cyclomatic_complexity)
    }

    pub fn warnings(&self) -> &[ComplexityWarning] {
        &self.warnings
    }

    pub fn with_warnings(mut self, warnings: Vec<ComplexityWarning>) -> Self {
        self.warnings = warnings;
        self
    }

    pub fn economic_impact(&self) -> Option<&EconomicImpact> {
        self.economic_impact.as_ref()
    }

    pub fn with_economic_impact(mut self, impact: EconomicImpact) -> Self {
        self.economic_impact = Some(impact);
        self
    }

    pub fn ecological_impact(&self) -> Option<&EcologicalImpact> {
        self.ecological_impact.as_ref()
    }

    pub fn with_ecological_impact(mut self, impact: EcologicalImpact) -> Self {
        self.ecological_impact = Some(impact);
        self
    }

    pub fn io_in_loops(&self) -> &[IoInLoopWarning] {
        &self.io_in_loops
    }

    pub fn with_io_in_loops(mut self, io_in_loops: Vec<IoInLoopWarning>) -> Self {
        self.io_in_loops = io_in_loops;
        self
    }

    pub fn with_function_details(mut self, function_details: Vec<FunctionDetail>) -> Self {
        self.function_details = function_details;
        self
    }
}
