mod analysis_rule;
mod analysis_target;
mod code_metrics;
pub mod code_parser;
mod code_reader;
mod errors;
pub mod proactive_analyzer;
mod report_writer;
mod run_analysis;

pub use analysis_rule::AnalysisRule;
pub use analysis_target::{AnalysisTarget, TargetType};
pub use code_metrics::CodeMetrics;
pub use code_parser::{CodeParser, ParsedFunction};
pub use code_reader::CodeReader;
pub use errors::AnalysisError;
pub use proactive_analyzer::analyze;
pub use report_writer::ReportWriter;
pub use run_analysis::RunAnalysis;
