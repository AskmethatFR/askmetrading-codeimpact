use std::path::Path;

use super::analysis_config::AnalysisConfig;
use super::errors::AnalysisError;

/// Reads analysis configuration (alert thresholds, US8; file filter, US31)
/// from a project config file (`.codeimpact.json`). Hexagon stays zero-dep
/// (ADR-0001): the port returns the already-validated domain type, all
/// parsing/serde concerns live behind the adapter (`ca-ports-adapters`,
/// DIP).
pub trait ConfigReaderPort: Send + Sync {
    /// `explicit_path`, when `Some`, MUST be honored exactly — a missing or
    /// invalid explicit path is an error, never a silent fall-through to
    /// auto-discovery. When `None`, `search_dirs` are tried in order (e.g.
    /// the analysis target's directory, then the current directory); the
    /// first one containing a config file wins.
    ///
    /// `Ok(None)` means no config file was found at all — not an error
    /// (AC6: the config file is optional; D4: absent file means the caller
    /// falls back to `AnalysisConfig::defaults()`).
    fn read_config(
        &self,
        explicit_path: Option<&Path>,
        search_dirs: &[&Path],
    ) -> Result<Option<AnalysisConfig>, AnalysisError>;
}
