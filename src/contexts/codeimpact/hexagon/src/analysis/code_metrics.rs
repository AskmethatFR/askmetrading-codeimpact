use super::alert_thresholds::ThresholdReport;
use super::code_location::CodeLocation;
use super::complexity_detector::ComplexityWarning;
use super::ecological_impact::EcologicalImpact;
use super::economic_impact::EconomicImpact;
use super::io_in_loop_warning::IoInLoopWarning;
use super::language_capabilities::LanguageCapabilities;

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
    /// Count of loop-nested calls whose receiver could not be classified at
    /// all (#56 T2, `IoClassification::Unknown`) — an aggregate signal only
    /// (ADR-0010/ADR-0014 §4): abstention is reported as a NUMBER, never a
    /// per-line pseudo-warning. `0` is an honest, meaningful answer, not an
    /// omitted signal.
    unclassifiable_io_in_loops_count: usize,
    /// This file's threshold-breach outcome (US8 T3) — `None` when no
    /// calling use case ever evaluated thresholds against it, mirroring
    /// `FileConsumptionGraph::threshold_report`.
    threshold_report: Option<ThresholdReport>,
    /// What the parser that produced this file's metrics can honestly
    /// measure (US16 T3, #33) — `None` when no calling use case ever
    /// attached one (mirrors `economic_impact`/`ecological_impact`
    /// /`threshold_report`'s "no evaluation ran" convention).
    capabilities: Option<LanguageCapabilities>,
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
            unclassifiable_io_in_loops_count: 0,
            threshold_report: None,
            capabilities: None,
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
            unclassifiable_io_in_loops_count: 0,
            threshold_report: None,
            capabilities: None,
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

    /// Three states, not two (ADR-0010 follow-up, #50 D3): a file with zero
    /// measured functions has "nothing to report", never a fabricated
    /// `"low"` — reporting the file-level `+1` base complexity as "clean"
    /// would be the exact `0`-reads-as-free lie ADR-0010 already forbids,
    /// one layer up. This is distinct from a file that failed to parse or
    /// read at all (`UnmeasurableFile`, `FileConsumptionGraph`) — that file
    /// never reaches `CodeMetrics` in the first place.
    pub fn complexity_level(&self) -> &'static str {
        if self.function_details.is_empty() {
            return "none";
        }
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

    pub fn unclassifiable_io_in_loops_count(&self) -> usize {
        self.unclassifiable_io_in_loops_count
    }

    pub fn with_unclassifiable_io_in_loops_count(mut self, count: usize) -> Self {
        self.unclassifiable_io_in_loops_count = count;
        self
    }

    pub fn with_function_details(mut self, function_details: Vec<FunctionDetail>) -> Self {
        self.function_details = function_details;
        self
    }

    /// Attaches the outcome of evaluating this file's own economic/
    /// ecological impact against its configured alert thresholds (US8 T3) —
    /// builder style, mirroring `FileConsumptionGraph::with_threshold_report`.
    pub fn with_threshold_report(mut self, report: ThresholdReport) -> Self {
        self.threshold_report = Some(report);
        self
    }

    pub fn threshold_report(&self) -> Option<&ThresholdReport> {
        self.threshold_report.as_ref()
    }

    /// Attaches the parser's declared `LanguageCapabilities` (US16 T3,
    /// #33) — builder style, mirroring `with_economic_impact`/
    /// `with_ecological_impact`.
    pub fn with_capabilities(mut self, capabilities: LanguageCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    pub fn capabilities(&self) -> Option<&LanguageCapabilities> {
        self.capabilities.as_ref()
    }
}
