use std::collections::HashMap;
use std::path::PathBuf;

use codeimpact_hexagon::domain_model::analysis_target::AnalysisTarget;
use codeimpact_hexagon::domain_model::errors::AnalysisError;
use codeimpact_hexagon::gateways_secondary_ports::code_reader_port::CodeReaderPort;

/// Stub — lecteur de code en mémoire pour les tests.
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

impl CodeReaderPort for CodeReaderStub {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        self.sources.get(target.path()).cloned().ok_or_else(|| {
            AnalysisError::IoError(format!("Fichier introuvable: {}", target.path().display()))
        })
    }
}
