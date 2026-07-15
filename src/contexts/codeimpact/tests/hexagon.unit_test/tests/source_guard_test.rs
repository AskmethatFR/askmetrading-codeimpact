use codeimpact_hexagon::analysis::{
    check_admissible, UnmeasurableReason, MAX_MEASURABLE_NESTING_DEPTH,
    MAX_MEASURABLE_SOURCE_BYTES,
};

// ── Test List ──────────────────────────────────────────────────────────
// check_admissible:
//   1. source_at_size_bound_is_admissible — len == MAX bytes → Ok (boundary)
//   2. source_one_byte_over_size_bound_is_source_too_large — MAX+1 bytes →
//      Err(SourceTooLarge)
//   3. nesting_at_depth_bound_is_admissible — 256 `{` deep → Ok (boundary)
//   4. braces_one_level_over_bound_is_source_too_complex — 257 `{` →
//      Err(SourceTooComplex) — vector 1 (nested mod/blocks)
//   5. long_ampersand_run_over_bound_is_source_too_complex — 257 consecutive
//      `&` → Err(SourceTooComplex) — vector 2 (deep reference types); a
//      brace counter alone misses this, which is the discriminator
//   6. realistic_multi_function_source_is_admissible — normal file → Ok
//      (false-positive guard)

#[test]
fn source_at_size_bound_is_admissible() {
    let source = "a".repeat(MAX_MEASURABLE_SOURCE_BYTES);
    assert_eq!(check_admissible(&source), Ok(()));
}

#[test]
fn source_one_byte_over_size_bound_is_source_too_large() {
    let source = "a".repeat(MAX_MEASURABLE_SOURCE_BYTES + 1);
    assert_eq!(
        check_admissible(&source),
        Err(UnmeasurableReason::SourceTooLarge)
    );
}

#[test]
fn braces_one_level_over_bound_is_source_too_complex() {
    let source = "{".repeat(MAX_MEASURABLE_NESTING_DEPTH + 1);
    assert_eq!(
        check_admissible(&source),
        Err(UnmeasurableReason::SourceTooComplex)
    );
}

#[test]
fn nesting_at_depth_bound_is_admissible() {
    let source = "{".repeat(MAX_MEASURABLE_NESTING_DEPTH);
    assert_eq!(check_admissible(&source), Ok(()));
}

#[test]
fn long_ampersand_run_over_bound_is_source_too_complex() {
    let source = "&".repeat(MAX_MEASURABLE_NESTING_DEPTH + 1);
    assert_eq!(
        check_admissible(&source),
        Err(UnmeasurableReason::SourceTooComplex)
    );
}

#[test]
fn realistic_multi_function_source_is_admissible() {
    let source = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

struct Calculator {
    total: i32,
}

impl Calculator {
    fn new() -> Self {
        Self { total: 0 }
    }

    fn add(&mut self, value: i32) -> &mut Self {
        self.total += value;
        self
    }
}
"#;
    assert_eq!(check_admissible(source), Ok(()));
}
