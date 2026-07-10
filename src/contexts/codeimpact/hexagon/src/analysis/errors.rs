use std::fmt;

#[derive(Clone, Debug)]
pub enum AnalysisError {
    IoError(String),
    AnalysisFailed(String),
    UnsupportedTarget(String),
    TestRunnerError(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "{}", msg),
            Self::AnalysisFailed(msg) => write!(f, "analyse échouée: {}", msg),
            Self::UnsupportedTarget(msg) => write!(f, "cible non supportée: {}", msg),
            Self::TestRunnerError(msg) => write!(f, "test runner: {}", msg),
        }
    }
}

impl std::error::Error for AnalysisError {}
