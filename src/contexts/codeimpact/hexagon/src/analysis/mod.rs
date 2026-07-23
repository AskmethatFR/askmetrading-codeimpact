pub mod alert_thresholds;
mod analysis_config;
mod analysis_rule;
mod analysis_target;
pub mod call_graph;
pub mod code_location;
pub mod code_metrics;
pub mod code_parser;
mod code_reader;
pub mod complexity_detector;
mod config_reader;
pub mod ecological_impact;
pub mod economic_impact;
mod errors;
pub mod file_consumption_graph;
mod file_filter;
pub mod gated_output;
pub mod io_classification;
pub mod io_in_loop_warning;
pub mod io_in_loops_detector;
pub mod language;
pub mod language_capabilities;
mod measurement;
mod output_format;
pub mod parser_registry;
pub mod proactive_analyzer;
pub mod reactive_analyzer;
mod report_writer;
mod run_analysis;
mod run_stress_test;
pub mod source_guard;
pub mod stress_test_run;

pub use alert_thresholds::{
    AlertThresholds, BreachedMetric, ThresholdBreach, ThresholdError, ThresholdReport,
};
pub use analysis_config::{AnalysisConfig, AnalysisConfigError};
pub use analysis_rule::AnalysisRule;
pub use analysis_target::{AnalysisTarget, TargetType};
pub use call_graph::CallGraph;
pub use code_location::CodeLocation;
pub use code_metrics::{complexity_level_for, CodeMetrics, FunctionDetail};
pub use code_parser::{CodeParser, DependencyContext, LoopCall, ParsedFunction};
pub use code_reader::CodeReader;
pub use complexity_detector::{
    ComplexityDetector, ComplexityWarning, DetectionConfig, WarningPattern, WarningSeverity,
};
pub use config_reader::ConfigReaderPort;
pub use ecological_impact::{EcologicalImpact, EcologicalImpactEstimator, EfficiencyClass};
pub use economic_impact::{EconomicImpact, EconomicImpactEstimator};
pub use errors::AnalysisError;
pub use file_consumption_graph::{
    FileConsumptionGraph, FileDependency, ProjectMetrics, UnmeasurableFile,
};
pub use file_filter::{FileFilter, FileFilterError};
pub use gated_output::GatedOutput;
pub use io_classification::IoClassification;
pub use io_in_loop_warning::IoInLoopWarning;
pub use io_in_loops_detector::IoInLoopsDetector;
pub use language::Language;
pub use language_capabilities::{AggregateMetricSupport, LanguageCapabilities, MetricSupport};
pub use output_format::OutputFormat;
pub use parser_registry::ParserRegistry;
pub use proactive_analyzer::analyze;
pub use reactive_analyzer::ReactiveAnalyzer;
pub use report_writer::ReportWriter;
pub use run_analysis::RunAnalysis;
pub use run_stress_test::RunStressTest;
pub use source_guard::{
    check_admissible, check_project_admissible, MAX_MEASURABLE_SOURCE_BYTES,
    MAX_PROJECT_SOURCE_BYTES,
};
pub use stress_test_run::{Measurement, StressTestRun, TestRunnerPort, UnmeasurableReason};
