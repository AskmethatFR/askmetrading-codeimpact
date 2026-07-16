use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_probe_built;

fn parser() -> SynCodeParser {
    ensure_probe_built();
    SynCodeParser::new()
}

/// Security finding (A04/CWE-354, retry 1): the single-entry verdict cache
/// must never let a SECOND, DIFFERENT source reuse the FIRST source's
/// verdict — that is exactly what a cache keyed by a non-cryptographic,
/// deterministic hash (the old `DefaultHasher`-based design) would do on a
/// 64-bit collision, which is precomputable offline against a fixed key.
/// The fix drops the hash entirely and keys the cache by full source
/// equality, so this is now true by construction — this test locks the
/// invariant in as a regression gate.
///
/// On ONE shared `SynCodeParser` instance: probe an admissible source
/// first (populates the cache), then immediately probe a DIFFERENT,
/// pathological source — it must get its OWN (refused) verdict, never the
/// first source's cached `Admissible`.
#[test]
fn cache_never_reuses_a_verdict_across_different_sources() {
    let parser = parser();

    let admissible_source = "fn f() { if true {} }";
    let pathological_source = nested_mods_source(1800);

    let first = parser.parse(admissible_source);
    assert!(
        first.is_ok(),
        "the admissible source should parse cleanly, got {:?}",
        first
    );

    let second = parser.parse(&pathological_source);
    match second {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
        other => panic!(
            "a different (pathological) source must be re-probed on its own \
             merits, not silently inherit the previous source's cached \
             Admissible verdict — got {:?}",
            other
        ),
    }
}

fn nested_mods_source(depth: usize) -> String {
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&format!("mod m{} {{\n", i));
    }
    src.push_str("fn f() {}\n");
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src
}
