use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_bin_built;

// Deliberately the only test in this file — it overrides the
// process-global `CODEIMPACT_PARSE_PROBE`, which would race against any
// other test in the same binary relying on the real probe. `#[ignore]`d
// (like this repo's other real-time test, cargo_test_runner_test.rs) since
// it genuinely waits out the 10s timeout — run explicitly in the
// slow-tests CI job, not on every `cargo test`.
//
// No timing assertion (ADR-0010's lesson: margins get walked past) — the
// kill-on-timeout is proven by its *outcome* (SourceTooComplex), not by
// asserting how long it took.
#[test]
#[ignore]
fn probe_that_never_exits_is_killed_and_treated_as_source_too_complex() {
    let sleep_probe = ensure_bin_built(
        "codeimpact_secondaries_integration_test",
        "codeimpact-sleep-probe",
    );
    std::env::set_var("CODEIMPACT_PARSE_PROBE", &sleep_probe);

    let result = SynCodeParser::new().parse("fn f() {}");

    std::env::remove_var("CODEIMPACT_PARSE_PROBE");

    match result {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
        other => panic!("expected Unmeasurable(SourceTooComplex), got {:?}", other),
    }
}
