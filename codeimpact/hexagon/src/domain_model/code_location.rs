use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeLocation {
    file_path: PathBuf,
    line: usize,
    column: usize,
}

impl CodeLocation {
    pub fn new(file_path: PathBuf, line: usize, column: usize) -> Result<Self, AnalysisError> {
        if line == 0 {
            return Err(AnalysisError::invalid_location("line must be >= 1"));
        }
        if column == 0 {
            return Err(AnalysisError::invalid_location("column must be >= 1"));
        }
        let canonical = file_path.canonicalize().map_err(|e| {
            AnalysisError::invalid_location(format!("invalid file path: {e}"))
        })?;
        Ok(Self { file_path: canonical, line, column })
    }

    pub fn file_path(&self) -> &PathBuf { &self.file_path }
    pub fn line(&self) -> usize { self.line }
    pub fn column(&self) -> usize { self.column }
}

use crate::domain_model::errors::AnalysisError;