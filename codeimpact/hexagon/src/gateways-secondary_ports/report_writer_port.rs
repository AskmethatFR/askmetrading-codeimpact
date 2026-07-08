use crate::domain_model::code_metrics::CodeMetrics;
use crate::domain_model::errors::AnalysisError;

/// Port secondaire — écriture du rapport d'analyse.
pub trait ReportWriterPort {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError>;
}
