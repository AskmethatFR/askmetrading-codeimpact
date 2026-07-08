use super::code_metrics::CodeMetrics;
use super::errors::AnalysisError;

pub trait ReportWriter: Send + Sync {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError>;
}
