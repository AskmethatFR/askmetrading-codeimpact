use codeimpact_hexagon::analysis::{
    check_admissible, UnmeasurableReason, MAX_MEASURABLE_SOURCE_BYTES,
};

// ── Test List ──────────────────────────────────────────────────────────
// check_admissible:
//   1. source_at_size_bound_is_admissible — len == MAX bytes → Ok (boundary)
//   2. source_one_byte_over_size_bound_is_source_too_large — MAX+1 bytes →
//      Err(SourceTooLarge)
//   3. realistic_multi_function_source_is_admissible — normal file → Ok
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
