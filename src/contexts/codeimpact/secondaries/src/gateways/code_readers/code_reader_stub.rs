use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;

#[derive(Default)]
pub struct CodeReaderStub {
    sources: HashMap<PathBuf, String>,
    rust_files: Vec<PathBuf>,
}

impl CodeReaderStub {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            rust_files: Vec::new(),
        }
    }

    pub fn add_source(&mut self, path: PathBuf, source: String) {
        self.sources.insert(path, source);
    }

    pub fn add_rust_file(&mut self, path: PathBuf) {
        self.rust_files.push(path);
    }
}

impl CodeReader for CodeReaderStub {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        self.sources
            .get(target.path())
            .cloned()
            .ok_or_else(|| AnalysisError::IoError("fichier introuvable".to_string()))
    }

    fn list_rust_files(&self, _dir: &Path) -> Result<Vec<PathBuf>, AnalysisError> {
        Ok(self.rust_files.clone())
    }
}
