/// Why a physical quantity (CPU time, memory) — or a file's source — could
/// not be measured.
///
/// There is deliberately no `f64`/`u64` default for "not measured" (#36):
/// a missing reading must be `Unmeasurable`, never a silent `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnmeasurableReason {
    /// No sampler (e.g. `/usr/bin/time`) was available, or the one that
    /// ran produced output that could not be parsed into a reading.
    NoSampler,
    /// The run exercised zero tests. A run with nothing executed has no
    /// honest cost to report, however well-sampled its process was (#39).
    NoTestsExecuted,
    /// The parser could not read the file's syntax (D3, #50): distinct from
    /// `complexity_level() == "none"` (parsed OK, zero functions) — this
    /// file was never even looked at successfully.
    SourceUnparseable,
    /// The file could not be read from disk at all (D3, #50).
    SourceUnreadable,
    /// The file's byte length exceeds `source_guard::MAX_MEASURABLE_SOURCE_BYTES`
    /// (#62): refused before the parser ever sees it, to cap worst-case RSS.
    SourceTooLarge,
    /// The file's nesting depth or a run of consecutive `&` exceeds
    /// `source_guard::MAX_MEASURABLE_NESTING_DEPTH` (#63): refused before the
    /// parser ever sees it, because a deep-enough recursive-descent parse
    /// aborts the whole process (uncatchable stack-overflow SIGABRT).
    SourceTooComplex,
}

impl std::fmt::Display for UnmeasurableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSampler => write!(f, "aucun outil de mesure disponible"),
            Self::NoTestsExecuted => write!(f, "aucun test exécuté"),
            Self::SourceUnparseable => write!(f, "code source non analysable"),
            Self::SourceUnreadable => write!(f, "fichier illisible"),
            Self::SourceTooLarge => write!(f, "code source trop volumineux pour être mesuré"),
            Self::SourceTooComplex => write!(f, "code source trop complexe (imbrication excessive)"),
        }
    }
}

/// A physical quantity that either was sampled, or explicitly was not.
///
/// Replaces the previous convention of defaulting to `0` when a measurement
/// tool was unavailable — `0` reads as "free", which is a lie (#36).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measurement<T> {
    Available(T),
    Unmeasurable(UnmeasurableReason),
}

impl<T> Measurement<T> {
    pub fn available(self) -> Option<T> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unmeasurable(_) => None,
        }
    }
}
