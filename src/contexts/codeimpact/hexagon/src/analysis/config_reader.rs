use std::path::Path;

use super::alert_thresholds::AlertThresholds;
use super::errors::AnalysisError;

/// Reads alert-threshold configuration from a project config file (US8
/// slice 4, `.codeimpact.json`). Hexagon stays zero-dep (ADR-0001): the
/// port returns the already-validated domain type, all parsing/serde
/// concerns live behind the adapter (`ca-ports-adapters`, DIP).
pub trait ConfigReaderPort: Send + Sync {
    /// `explicit_path`, when `Some`, MUST be honored exactly — a missing or
    /// invalid explicit path is an error, never a silent fall-through to
    /// auto-discovery. When `None`, `search_dirs` are tried in order (e.g.
    /// the analysis target's directory, then the current directory); the
    /// first one containing a config file wins.
    ///
    /// `Ok(None)` means no config file was found at all — not an error
    /// (AC6: the config file is optional).
    fn read_thresholds(
        &self,
        explicit_path: Option<&Path>,
        search_dirs: &[&Path],
    ) -> Result<Option<AlertThresholds>, AnalysisError>;
}
