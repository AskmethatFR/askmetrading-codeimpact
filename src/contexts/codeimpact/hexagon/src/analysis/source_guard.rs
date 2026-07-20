use super::errors::AnalysisError;
use super::measurement::UnmeasurableReason;

/// Ceiling on a source file's byte length before it is even offered to the
/// parser. `syn::parse_file` builds an AST proportional to input size with
/// no back-pressure of its own — an unbounded read is an unbounded RSS
/// allocation. 1 MB sits far above any real Rust source file and far below
/// a size that could meaningfully threaten process memory.
pub const MAX_MEASURABLE_SOURCE_BYTES: usize = 1024 * 1024;

pub fn check_admissible(source: &str) -> Result<(), UnmeasurableReason> {
    if source.len() > MAX_MEASURABLE_SOURCE_BYTES {
        return Err(UnmeasurableReason::SourceTooLarge);
    }

    Ok(())
}

/// Ceiling on a WHOLE PROJECT's aggregate source size (US16 T5, Security
/// HIGH retry #1) — `MAX_MEASURABLE_SOURCE_BYTES` above bounds ONE file's
/// RSS; it says nothing about hundreds of near-the-cap files summing to a
/// multi-gigabyte aggregate once `run_analysis::read_all_sources`
/// accumulates every project file's text in memory at once (needed by
/// US16 T5's project-global namespace pre-pass). 100 MB is generous for
/// any legitimate source project's TEXT alone (comfortably above, e.g.,
/// the Linux kernel's own C sources) while closing the unbounded-
/// aggregate class outright — and, deliberately, small enough that a test
/// can construct an over-the-ceiling fixture without slowing the suite
/// the way a "few hundred MB" boundary fixture would.
pub const MAX_PROJECT_SOURCE_BYTES: usize = 100 * 1024 * 1024;

/// `check_admissible`'s aggregate twin (US16 T5) — `read_all_sources`
/// calls this after every file it reads, with the RUNNING total, so a
/// project that crosses the ceiling stops immediately (ADR-0010: an
/// honest hard error, never a silent truncation of the file list or a
/// best-effort partial scan pretending to be complete).
pub fn check_project_admissible(total_bytes: usize) -> Result<(), AnalysisError> {
    if total_bytes > MAX_PROJECT_SOURCE_BYTES {
        return Err(AnalysisError::AnalysisFailed(format!(
            "le projet dépasse la limite agrégée de code source ({} Mo) — \
             analyse interrompue avant d'accumuler tout le texte en mémoire",
            MAX_PROJECT_SOURCE_BYTES / (1024 * 1024)
        )));
    }

    Ok(())
}
