use super::alert_thresholds::AlertThresholds;
use super::file_filter::FileFilter;

/// Retry #1 (Security MEDIUM, #33 T4): mirrors `FileFilter`'s
/// `MAX_PATTERN_COUNT` — an unbounded `ioSignatures` list let a 90,000-entry
/// config (still under FileSystemConfigReader's 1 MiB cap) inflate per-file
/// wall-clock 6x and trip a false `SourceTooComplex`.
const MAX_IO_SIGNATURE_COUNT: usize = 256;
/// Mirrors `FileFilter`'s `MAX_PATTERN_LENGTH`, scaled down: a confident I/O
/// prefix is a short qualified-name fragment (`"File."`, `"MyIoWrapper."`),
/// never a long payload.
const MAX_IO_SIGNATURE_LENGTH: usize = 256;

/// Rejected construction of `AnalysisConfig::with_io_signature_prefixes`
/// (retry #1, Security MEDIUM) — mirrors `FileFilterError`'s shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisConfigError {
    TooManyIoSignaturePrefixes(usize),
    IoSignaturePrefixTooLong(String),
}

impl std::fmt::Display for AnalysisConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyIoSignaturePrefixes(count) => write!(
                f,
                "trop de préfixes ioSignatures: {} (max {})",
                count, MAX_IO_SIGNATURE_COUNT
            ),
            Self::IoSignaturePrefixTooLong(prefix) => write!(
                f,
                "préfixe ioSignatures trop long (max {} caractères): {}",
                MAX_IO_SIGNATURE_LENGTH, prefix
            ),
        }
    }
}

impl std::error::Error for AnalysisConfigError {}

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
    /// Fallible since retry #1 (Security MEDIUM): validates count + per-
    /// entry length the same way `FileFilter::new` validates include/
    /// exclude, so the invariant holds regardless of which caller
    /// constructs this VO (ddd-value-object), not just the config reader.
    pub fn with_io_signature_prefixes(
        mut self,
        prefixes: Vec<String>,
    ) -> Result<Self, AnalysisConfigError> {
        if prefixes.len() > MAX_IO_SIGNATURE_COUNT {
            return Err(AnalysisConfigError::TooManyIoSignaturePrefixes(
                prefixes.len(),
            ));
        }
        if let Some(too_long) = prefixes.iter().find(|p| p.len() > MAX_IO_SIGNATURE_LENGTH) {
            return Err(AnalysisConfigError::IoSignaturePrefixTooLong(
                too_long.clone(),
            ));
        }

        self.io_signature_prefixes = prefixes;
        Ok(self)
    }

    pub fn io_signature_prefixes(&self) -> &[String] {
        &self.io_signature_prefixes
    }
}
