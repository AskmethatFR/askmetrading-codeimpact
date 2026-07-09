use std::path::{Path, PathBuf};

use super::analysis_target::AnalysisTarget;
use super::errors::AnalysisError;

pub trait CodeReader: Send + Sync {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;
    fn list_rust_files(&self, dir: &Path) -> Result<Vec<PathBuf>, AnalysisError>;
}
