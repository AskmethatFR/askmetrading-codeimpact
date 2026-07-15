use std::fmt;

use super::measurement::UnmeasurableReason;

#[derive(Clone, Debug)]
pub enum AnalysisError {
    IoError(String),
    AnalysisFailed(String),
    UnsupportedTarget(String),
    TestRunnerError(String),
    /// The source was refused by `source_guard::check_admissible` before
    /// ever reaching the parser (#62) — too large to safely offer to the
    /// parser.
    Unmeasurable(UnmeasurableReason),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "{}", msg),
            Self::AnalysisFailed(msg) => write!(f, "analyse échouée: {}", msg),
            Self::UnsupportedTarget(msg) => write!(f, "cible non supportée: {}", msg),
            Self::TestRunnerError(msg) => write!(f, "test runner: {}", msg),
            Self::Unmeasurable(reason) => write!(f, "{}", reason),
        }
    }
}

impl std::error::Error for AnalysisError {}
