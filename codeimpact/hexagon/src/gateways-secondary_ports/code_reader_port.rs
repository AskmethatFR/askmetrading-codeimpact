use crate::domain_model::analysis_target::AnalysisTarget;
use crate::domain_model::errors::AnalysisError;

/// Port secondaire — lecture du code source à analyser.
pub trait CodeReaderPort {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;
}
