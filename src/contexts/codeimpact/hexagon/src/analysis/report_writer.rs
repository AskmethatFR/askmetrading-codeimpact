use super::code_metrics::CodeMetrics;
use super::economic_impact::EconomicImpact;
use super::errors::AnalysisError;
use super::file_consumption_graph::FileConsumptionGraph;
use super::stress_test_run::StressTestRun;

pub trait ReportWriter: Send + Sync {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError>;
    fn write_json(
        &self,
        metrics: &CodeMetrics,
        target: &str,
        target_type: &str,
    ) -> Result<String, AnalysisError>;
    fn write_project_report(
        &self,
        graph: &FileConsumptionGraph,
    ) -> Result<(), AnalysisError>;
    fn write_stress_test(
        &self,
        run: &StressTestRun,
        impact: &EconomicImpact,
    ) -> Result<(), AnalysisError>;
}
