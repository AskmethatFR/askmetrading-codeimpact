use super::code_location::CodeLocation;
use super::complexity_detector::ComplexityWarning;
use super::ecological_impact::EcologicalImpact;
use super::economic_impact::EconomicImpact;
use super::io_in_loop_warning::IoInLoopWarning;

/// A single function's complexity measurement (#46/#49 arbitration §2):
/// `hidden` is MEASURED directly (`CallGraph::hidden_of`, the direct
/// complexity of every other function reachable from this one, each
/// counted once) and stored; `transitive` is DERIVED as `direct + hidden`,
/// never stored, so `transitive >= direct` holds by construction — in debug
/// AND in release, with no runtime guard. Fields are private and `new()` is
/// the only constructor: no caller, including a future FFI adapter, can
/// build a `FunctionDetail` whose transitive is less than its direct — the
/// illegal state this type used to `debug_assert!` against is now
/// unconstructible, which is why there is no assertion left to write.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDetail {
    name: String,
    location: CodeLocation,
    direct: u32,
    hidden: u32,
    call_depth: usize,
    in_cycle: bool,
}

impl FunctionDetail {
    pub fn new(
        name: String,
        location: CodeLocation,
        direct: u32,
        hidden: u32,
        call_depth: usize,
        in_cycle: bool,
    ) -> Self {
        Self {
            name,
            location,
            direct,
            hidden,
            call_depth,
            in_cycle,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn location(&self) -> &CodeLocation {
        &self.location
    }

    pub fn direct(&self) -> u32 {
        self.direct
    }

    /// Complexity hidden in this function's calls: measured directly,
    /// never re-derived by subtracting two aggregates (ADR-0012).
    pub fn hidden(&self) -> u32 {
        self.hidden
    }

    /// Direct + hidden, derived so it is bounded by construction — it can
    /// never fall below `direct()`.
    pub fn transitive(&self) -> u32 {
        self.direct.saturating_add(self.hidden)
    }

    pub fn call_depth(&self) -> usize {
        self.call_depth
    }

    pub fn in_cycle(&self) -> bool {
        self.in_cycle
    }

    /// Returns a copy with `location`'s file path replaced, keeping its
    /// line/col (used once the file being analyzed is known — see
    /// `run_analysis::set_file_paths`).
    pub fn with_location(self, path: String) -> Self {
        let location = CodeLocation::new(path, self.location.line(), self.location.col());
        Self { location, ..self }
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
    /// complexity (`FunctionDetail::hidden()`, itself measured from the
    /// call graph's reachable set — never a subtraction of two aggregates).
    ///
    /// `cyclomatic_complexity` counts a `+1` per FILE (`1 + Σ decision_points`)
    /// while `transitive_complexity` carries no such `+1` — they are not the
    /// same unit, so `transitive_complexity - cyclomatic_complexity` would
    /// silently fabricate a wrong number. Summing each function's measured
    /// `hidden()` is the only formula that is correct by construction
    /// (ADR-0012). The `saturating_add` fold is a dead net, not a strategy:
    /// with the reachable-set formula (#46/#49) this sum is bounded by the
    /// file's total direct complexity and cannot overflow in practice — it
    /// is here only for consistency with the same pattern already used in
    /// `FileConsumptionGraph::aggregated_metrics()`.
    ///
    /// A `CodeMetrics` with no `function_details` (nothing was ever measured
    /// at the function level) reports `0`, never a fabricated non-zero value
    /// derived from the file-level aggregates (ADR-0010).
    pub fn hidden_complexity(&self) -> u32 {
        self.function_details
            .iter()
            .fold(0u32, |acc, d| acc.saturating_add(d.hidden()))
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
