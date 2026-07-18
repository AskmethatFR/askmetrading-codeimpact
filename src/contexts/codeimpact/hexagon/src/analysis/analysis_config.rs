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
}

impl AnalysisConfig {
    /// No thresholds configured, no file filtering (D4: absent config file
    /// reproduces today's behavior byte-for-byte).
    pub fn defaults() -> Self {
        Self {
            thresholds: AlertThresholds::none(),
            filter: FileFilter::unrestricted(),
        }
    }

    pub fn new(thresholds: AlertThresholds, filter: FileFilter) -> Self {
        Self { thresholds, filter }
    }

    pub fn thresholds(&self) -> &AlertThresholds {
        &self.thresholds
    }

    pub fn file_filter(&self) -> &FileFilter {
        &self.filter
    }
}
