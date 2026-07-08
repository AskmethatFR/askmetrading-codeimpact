use codeimpact_hexagon::domain_model::analysis_target::AnalysisTarget;
use codeimpact_hexagon::domain_model::errors::AnalysisError;
use codeimpact_hexagon::gateways_secondary_ports::code_reader_port::CodeReaderPort;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Default)]
pub struct FileSystemCodeReader;

impl FileSystemCodeReader {
    pub fn new() -> Self {
        Self
    }
}

impl CodeReaderPort for FileSystemCodeReader {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        let path = target.path();
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AnalysisError::IoError("fichier introuvable".to_string()))?;

        let metadata = std::fs::metadata(&canonical)
            .map_err(|_| AnalysisError::IoError("fichier introuvable".to_string()))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(AnalysisError::IoError(format!(
                "fichier trop volumineux: {} (max 10 Mo)",
                metadata.len()
            )));
        }

        std::fs::read_to_string(&canonical).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AnalysisError::IoError("fichier introuvable".to_string())
            }
            std::io::ErrorKind::PermissionDenied => {
                AnalysisError::IoError("permission refusée".to_string())
            }
            _ => AnalysisError::IoError("erreur de lecture".to_string()),
        })
    }
}
