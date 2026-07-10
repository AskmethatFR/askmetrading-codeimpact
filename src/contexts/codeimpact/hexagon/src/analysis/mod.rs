mod analysis_rule;
mod analysis_target;
pub mod call_graph;
pub mod code_location;
pub mod code_metrics;
pub mod code_parser;
mod code_reader;
pub mod complexity_detector;
pub mod ecological_impact;
pub mod economic_impact;
mod errors;
pub mod file_consumption_graph;
pub mod io_in_loop_warning;
pub mod io_in_loops_detector;
pub mod proactive_analyzer;
mod report_writer;
mod run_analysis;

pub use analysis_rule::AnalysisRule;
pub use analysis_target::{AnalysisTarget, TargetType};
pub use call_graph::CallGraph;
pub use code_location::CodeLocation;
pub use code_metrics::{CodeMetrics, FunctionDetail};
pub use code_parser::{CodeParser, ParsedFunction};
pub use code_reader::CodeReader;
pub use complexity_detector::{
    ComplexityDetector, ComplexityWarning, DetectionConfig, WarningPattern, WarningSeverity,
};
pub use ecological_impact::{EcologicalImpact, EcologicalImpactEstimator, EfficiencyClass};
pub use economic_impact::{EconomicImpact, EconomicImpactEstimator};
pub use errors::AnalysisError;
pub use file_consumption_graph::{
    resolve_file_dependency, FileConsumptionGraph, FileDependency, ProjectMetrics,
};
pub use io_in_loop_warning::IoInLoopWarning;
pub use io_in_loops_detector::IoInLoopsDetector;
pub use proactive_analyzer::analyze;
pub use report_writer::ReportWriter;
pub use run_analysis::RunAnalysis;
