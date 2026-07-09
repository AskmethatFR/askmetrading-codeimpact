use super::code_metrics::CodeMetrics;
use super::errors::AnalysisError;
use super::file_consumption_graph::FileConsumptionGraph;

pub trait ReportWriter: Send + Sync {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError>;
    fn write_project_report(
        &self,
        graph: &FileConsumptionGraph,
    ) -> Result<(), AnalysisError>;
}
