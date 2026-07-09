use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Default)]
pub struct FileSystemCodeReader;

impl FileSystemCodeReader {
    pub fn new() -> Self {
        Self
    }
}

impl CodeReader for FileSystemCodeReader {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        let path = target.path();
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AnalysisError::IoError("fichier introuvable".to_string()))?;

        let metadata = std::fs::metadata(&canonical)
            .map_err(|_| AnalysisError::IoError("fichier introuvable".to_string()))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(AnalysisError::IoError(
                "fichier trop volumineux (max 10 Mo)".to_string(),
            ));
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

    fn list_rust_files(&self, dir: &Path) -> Result<Vec<PathBuf>, AnalysisError> {
        let canonical_root = std::fs::canonicalize(dir)
            .map_err(|_| AnalysisError::IoError("dossier introuvable".to_string()))?;

        let mut files = Vec::new();
        let walker = walkdir::WalkDir::new(&canonical_root)
            .follow_links(false)
            .max_depth(128)
            .into_iter()
            .filter_entry(|e| {
                e.file_name()
                    .to_str()
                    .map(|s| !s.starts_with('.') || s == ".")
                    .unwrap_or(false)
            });

        for entry in walker {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_file() {
                        let path = entry.path();
                        if path.extension().map_or(false, |ext| ext == "rs") {
                            match std::fs::metadata(path) {
                                Ok(meta) if meta.len() <= MAX_FILE_SIZE => {
                                    files.push(path.to_path_buf());
                                }
                                Ok(_) => {
                                    eprintln!(
                                        "Avertissement: fichier ignoré (trop volumineux): {}",
                                        path.file_name().unwrap_or_default().to_string_lossy()
                                    );
                                }
                                Err(_) => {
                                    eprintln!(
                                        "Avertissement: fichier ignoré (illisible): {}",
                                        path.file_name().unwrap_or_default().to_string_lossy()
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Avertissement: erreur d'accès: {}", e);
                }
            }
        }

        Ok(files)
    }
}
