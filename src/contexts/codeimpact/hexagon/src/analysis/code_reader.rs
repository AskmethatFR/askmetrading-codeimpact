use super::analysis_target::AnalysisTarget;
use super::errors::AnalysisError;

pub trait CodeReader: Send + Sync {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;
}
