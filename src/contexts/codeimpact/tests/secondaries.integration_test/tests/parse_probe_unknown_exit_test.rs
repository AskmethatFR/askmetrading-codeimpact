use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_bin_built;

// Deliberately the only test in this file — it overrides the
// process-global `CODEIMPACT_PARSE_PROBE`, which would race against any
// other test in the same binary relying on the real probe.

/// #63 T3 — an unknown exit code (neither 0 nor 2) is refused end-to-end
/// through the real subprocess wiring, not just `verdict_from` in
/// isolation: proves `probe_source` actually reads the child's exit code
/// rather than, say, always treating a clean `wait()` as admissible.
#[test]
fn unknown_exit_code_is_treated_as_source_too_complex() {
    let exit_seven_probe = ensure_bin_built(
        "codeimpact_secondaries_integration_test",
        "codeimpact-exit-seven-probe",
    );
    std::env::set_var("CODEIMPACT_PARSE_PROBE", &exit_seven_probe);

    let result = SynCodeParser::new().parse("fn f() {}");

    std::env::remove_var("CODEIMPACT_PARSE_PROBE");

    match result {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
        other => panic!("expected Unmeasurable(SourceTooComplex), got {:?}", other),
    }
}
