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
