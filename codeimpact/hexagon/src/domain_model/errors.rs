use std::fmt;

#[derive(Debug)]
pub enum AnalysisError {
    InvalidLocation(String),
    InvalidEconomic(String),
    InvalidEcological(String),
    IoError(String),
    AnalysisFailed(String),
    TestRunFailed(String),
    ReportFailed(String),
    UnsupportedTarget(String),
}

impl AnalysisError {
    pub fn invalid_location(msg: impl Into<String>) -> Self { Self::InvalidLocation(msg.into()) }
    pub fn invalid_economic(msg: impl Into<String>) -> Self { Self::InvalidEconomic(msg.into()) }
    pub fn invalid_ecological(msg: impl Into<String>) -> Self { Self::InvalidEcological(msg.into()) }
    pub fn io_error(msg: impl Into<String>) -> Self { Self::IoError(msg.into()) }
    pub fn analysis_failed(msg: impl Into<String>) -> Self { Self::AnalysisFailed(msg.into()) }
    pub fn test_run_failed(msg: impl Into<String>) -> Self { Self::TestRunFailed(msg.into()) }
    pub fn report_failed(msg: impl Into<String>) -> Self { Self::ReportFailed(msg.into()) }
    pub fn unsupported_target(msg: impl Into<String>) -> Self { Self::UnsupportedTarget(msg.into()) }
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLocation(msg) => write!(f, "invalid location: {msg}"),
            Self::InvalidEconomic(msg) => write!(f, "invalid economic data: {msg}"),
            Self::InvalidEcological(msg) => write!(f, "invalid ecological data: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::AnalysisFailed(msg) => write!(f, "analysis failed: {msg}"),
            Self::TestRunFailed(msg) => write!(f, "test run failed: {msg}"),
            Self::ReportFailed(msg) => write!(f, "report generation failed: {msg}"),
            Self::UnsupportedTarget(msg) => write!(f, "unsupported target: {msg}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<std::io::Error> for AnalysisError {
    fn from(e: std::io::Error) -> Self { Self::IoError(e.to_string()) }
}