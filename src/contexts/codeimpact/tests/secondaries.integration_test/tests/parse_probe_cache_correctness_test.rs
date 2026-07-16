use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_bin_built;

// Deliberately the only test in this file — it overrides the
// process-global `CODEIMPACT_PARSE_PROBE`, which would race against any
// other test in the same binary relying on the real probe.

/// Security finding (A04/CWE-354, retry 1): the single-entry verdict cache
/// must never let a SECOND, DIFFERENT source reuse the FIRST source's
/// verdict — that is exactly what a cache keyed by a non-cryptographic,
/// deterministic hash (the old `DefaultHasher`-based design) would do on a
/// 64-bit collision, precomputable offline against a fixed key. The fix
/// (already landed) drops the hash and keys the cache by full source
/// equality.
///
/// Security finding (retry 2): the ORIGINAL version of this test drove the
/// invariant through a real 1800-level nested-mod source relying on
/// `syn::parse_file`'s actual stack overflow — which SIGABRTs in debug but
/// parses cleanly under `cargo test --release` (bisected: release admits
/// depth ~3000, aborts ~5000+). That made the test's correctness depend
/// on `syn`'s real stack threshold at a fixed depth: brittle against a
/// compiler-opt or `syn`-version change, and silently different between
/// debug and release CI runs.
///
/// This version decouples the assertion from real recursion entirely: a
/// deterministic fake canary (`codeimpact-content-probe`) whose verdict
/// depends only on whether stdin contains a fixed marker string — never on
/// `syn`, stack depth, or build profile — proves the cache-equality logic
/// on its own terms, identically in debug and release.
#[test]
fn cache_never_reuses_a_verdict_across_different_sources() {
    let content_probe = ensure_bin_built(
        "codeimpact_secondaries_integration_test",
        "codeimpact-content-probe",
    );
    std::env::set_var("CODEIMPACT_PARSE_PROBE", &content_probe);

    let parser = SynCodeParser::new();

    let admissible_source = "fn f() {}";
    let refused_source = "fn g() { /* CODEIMPACT_TEST_REFUSE_MARKER */ }";

    let first = parser.parse(admissible_source);
    let second = parser.parse(refused_source);

    std::env::remove_var("CODEIMPACT_PARSE_PROBE");

    assert!(
        first.is_ok(),
        "the marker-free source should be admissible, got {:?}",
        first
    );
    match second {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
        other => panic!(
            "a different (marker-bearing) source must be re-probed on its \
             own merits, not silently inherit the previous source's cached \
             Admissible verdict — got {:?}",
            other
        ),
    }
}
