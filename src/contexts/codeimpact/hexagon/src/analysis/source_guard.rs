use super::measurement::UnmeasurableReason;

/// Ceiling on a source file's byte length before it is even offered to the
/// parser. `syn::parse_file` builds an AST proportional to input size with
/// no back-pressure of its own — an unbounded read is an unbounded RSS
/// allocation. 1 MB sits far above any real Rust source file and far below
/// a size that could meaningfully threaten process memory.
pub const MAX_MEASURABLE_SOURCE_BYTES: usize = 1024 * 1024;

/// Ceiling on nesting depth (braces/parens/brackets) and on a run of
/// consecutive `&` bytes, before a source is offered to the parser.
///
/// `syn::parse_file` recurses once per nesting level with no depth limit of
/// its own. Deeply enough nested input drives the recursive-descent parser
/// past its stack guard page, which the OS turns into SIGABRT — an abort is
/// not a `Result::Err` (`?` never observes it) and is NOT caught by
/// `std::panic::catch_unwind` (that hook only runs on unwinding panics, and
/// a guard-page overrun aborts instead of unwinding). A bounded-stack thread
/// does not help either: the abort takes down the whole process, thread
/// boundary or not. The only defense is refusing the input before `syn`
/// ever sees it. 256 sits roughly 7x below the observed crash floor
/// (~1800 nested `mod`, ~5000 consecutive `&`) and roughly 12x above real
/// code, which is a difference of nature, not a tuning knob.
pub const MAX_MEASURABLE_NESTING_DEPTH: usize = 256;

pub fn check_admissible(source: &str) -> Result<(), UnmeasurableReason> {
    if source.len() > MAX_MEASURABLE_SOURCE_BYTES {
        return Err(UnmeasurableReason::SourceTooLarge);
    }

    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut ampersand_run: usize = 0;
    let mut max_ampersand_run: usize = 0;
    for byte in source.bytes() {
        match byte {
            b'{' | b'(' | b'[' => {
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            b'}' | b')' | b']' => depth = depth.saturating_sub(1),
            b'&' => {
                ampersand_run += 1;
                max_ampersand_run = max_ampersand_run.max(ampersand_run);
            }
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => ampersand_run = 0,
        }
    }

    if max_depth > MAX_MEASURABLE_NESTING_DEPTH || max_ampersand_run > MAX_MEASURABLE_NESTING_DEPTH
    {
        return Err(UnmeasurableReason::SourceTooComplex);
    }

    Ok(())
}
