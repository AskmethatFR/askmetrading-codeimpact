use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;

#[derive(Default)]
pub struct CodeReaderStub {
    sources: HashMap<PathBuf, String>,
    source_files: Vec<PathBuf>,
}

impl CodeReaderStub {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            source_files: Vec::new(),
        }
    }

    pub fn add_source(&mut self, path: PathBuf, source: String) {
        self.sources.insert(path, source);
    }

    pub fn add_source_file(&mut self, path: PathBuf) {
        self.source_files.push(path);
    }
}

impl CodeReader for CodeReaderStub {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        self.sources
            .get(target.path())
            .cloned()
            .ok_or_else(|| AnalysisError::IoError("fichier introuvable".to_string()))
    }

    fn list_source_files(
        &self,
        _dir: &Path,
        _extensions: &[&str],
        _filter: &FileFilter,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        Ok(self.source_files.clone())
    }
}
