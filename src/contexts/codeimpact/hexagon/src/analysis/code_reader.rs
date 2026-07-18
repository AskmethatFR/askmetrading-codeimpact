use std::path::{Path, PathBuf};

use super::analysis_target::AnalysisTarget;
use super::errors::AnalysisError;

pub trait CodeReader: Send + Sync {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;

    /// Lists every file under `dir` whose extension (no leading dot) is one
    /// of `extensions` — language-agnostic (US14 L3): the port no longer
    /// knows "Rust", it only filters on whatever extension set the caller
    /// passes. The composition root (`RunAnalysis`) supplies `&["rs"]` to
    /// preserve today's behavior exactly.
    fn list_source_files(
        &self,
        dir: &Path,
        extensions: &[&str],
    ) -> Result<Vec<PathBuf>, AnalysisError>;
}
