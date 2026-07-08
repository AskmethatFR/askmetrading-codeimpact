use std::fmt;

/// Erreurs d'analyse.
#[derive(Debug)]
pub enum AnalysisError {
    /// Erreur d'entrée/sortie.
    IoError(String),
    /// L'analyse a échoué pour une raison métier.
    AnalysisFailed(String),
    /// Type de cible non supporté.
    UnsupportedTarget(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisError::IoError(msg) => write!(f, "{}", msg),
            AnalysisError::AnalysisFailed(msg) => write!(f, "Analyse échouée: {}", msg),
            AnalysisError::UnsupportedTarget(msg) => write!(f, "Cible non supportée: {}", msg),
        }
    }
}

impl std::error::Error for AnalysisError {}
