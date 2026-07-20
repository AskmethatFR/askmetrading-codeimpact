use super::alert_thresholds::AlertThresholds;
use super::file_filter::FileFilter;

/// Value Object (US31): the two independent knobs an analysis run is
/// configured by — alert thresholds (US8) and the file filter (US31).
/// Immutable, pure composition of two already-validated VOs — no
/// validation of its own to perform.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisConfig {
    thresholds: AlertThresholds,
    filter: FileFilter,
    /// User-configured confident C# I/O prefixes (US16 T4.3, ADR-0019's
    /// reserved `ioSignatures` key), additive to `TreeSitterCodeParser`'s
    /// base `File.`/`Directory.` table. Empty by default — reproduces
    /// T4.1/T4.2 behavior byte-for-byte when absent.
    io_signature_prefixes: Vec<String>,
}

impl AnalysisConfig {
    /// No thresholds configured, no file filtering (D4: absent config file
    /// reproduces today's behavior byte-for-byte).
    pub fn defaults() -> Self {
        Self {
            thresholds: AlertThresholds::none(),
            filter: FileFilter::unrestricted(),
            io_signature_prefixes: Vec::new(),
        }
    }

    pub fn new(thresholds: AlertThresholds, filter: FileFilter) -> Self {
        Self {
            thresholds,
            filter,
            io_signature_prefixes: Vec::new(),
        }
    }

    pub fn thresholds(&self) -> &AlertThresholds {
        &self.thresholds
    }

    pub fn file_filter(&self) -> &FileFilter {
        &self.filter
    }

    /// Builder-style override (mirrors `with_call_graph`/
    /// `with_economic_impact` elsewhere in this codebase) — additive, T4.3.
    pub fn with_io_signature_prefixes(self, _prefixes: Vec<String>) -> Self {
        // T4.3 scaffold: wired in the next step.
        self
    }

    pub fn io_signature_prefixes(&self) -> &[String] {
        &self.io_signature_prefixes
    }
}
