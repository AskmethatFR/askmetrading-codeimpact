use std::fmt;

#[derive(Debug)]
pub enum AnalysisError {
    IoError(String),
    AnalysisFailed(String),
    UnsupportedTarget(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "{}", msg),
            Self::AnalysisFailed(msg) => write!(f, "analyse échouée: {}", msg),
            Self::UnsupportedTarget(msg) => write!(f, "cible non supportée: {}", msg),
        }
    }
}

impl std::error::Error for AnalysisError {}
