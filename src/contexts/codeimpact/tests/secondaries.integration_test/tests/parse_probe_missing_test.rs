use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;

// Deliberately the only test in this file: it overrides the process-global
// `CODEIMPACT_PARSE_PROBE` env var, which would race against any other test
// in the same binary relying on the real probe (cargo runs tests in one
// file across multiple threads of the same process by default). Isolating
// it in its own file/binary sidesteps the race without extra ceremony.
#[test]
fn missing_probe_binary_produces_a_noisy_analysis_failed_error() {
    std::env::set_var(
        "CODEIMPACT_PARSE_PROBE",
        "/nonexistent/codeimpact-parse-probe",
    );

    let result = SynCodeParser::new().parse("fn f() {}");

    std::env::remove_var("CODEIMPACT_PARSE_PROBE");

    match result {
        Err(AnalysisError::AnalysisFailed(msg)) => {
            assert!(
                msg.contains("CODEIMPACT_PARSE_PROBE"),
                "error should point at the override env var, got: {}",
                msg
            );
            assert!(
                !msg.contains("/nonexistent"),
                "error must not leak the configured path (ADR-0006): {}",
                msg
            );
        }
        other => panic!("expected a noisy AnalysisFailed, got {:?}", other),
    }
}
