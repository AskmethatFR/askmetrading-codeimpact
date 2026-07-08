use std::collections::HashMap;
use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;

#[derive(Default)]
pub struct CodeReaderStub {
    sources: HashMap<PathBuf, String>,
}

impl CodeReaderStub {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    pub fn add_source(&mut self, path: PathBuf, source: String) {
        self.sources.insert(path, source);
    }
}

impl CodeReader for CodeReaderStub {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        self.sources
            .get(target.path())
            .cloned()
            .ok_or_else(|| AnalysisError::IoError("fichier introuvable".to_string()))
    }
}
