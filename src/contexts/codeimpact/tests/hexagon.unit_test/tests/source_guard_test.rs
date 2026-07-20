use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::{
    check_admissible, check_project_admissible, UnmeasurableReason, MAX_MEASURABLE_SOURCE_BYTES,
    MAX_PROJECT_SOURCE_BYTES,
};

// ── Test List ──────────────────────────────────────────────────────────
// check_admissible:
//   1. source_at_size_bound_is_admissible — len == MAX bytes → Ok (boundary)
//   2. source_one_byte_over_size_bound_is_source_too_large — MAX+1 bytes →
//      Err(SourceTooLarge)
//   3. realistic_multi_function_source_is_admissible — normal file → Ok
//      (false-positive guard)
//
// check_project_admissible (US16 T5, Security HIGH retry #1 — an
// aggregate ceiling on `read_all_sources`'s accumulated project text,
// distinct from the per-FILE `check_admissible` above):
//   4. total_at_project_size_bound_is_admissible — boundary
//   5. total_one_byte_over_project_size_bound_is_refused — MAX+1 → Err

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

#[test]
fn total_at_project_size_bound_is_admissible() {
    assert!(check_project_admissible(MAX_PROJECT_SOURCE_BYTES).is_ok());
}

#[test]
fn total_one_byte_over_project_size_bound_is_refused() {
    match check_project_admissible(MAX_PROJECT_SOURCE_BYTES + 1) {
        Err(AnalysisError::AnalysisFailed(_)) => {}
        other => panic!("expected Err(AnalysisFailed(_)), got {:?}", other),
    }
}
