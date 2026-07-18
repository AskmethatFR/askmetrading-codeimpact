use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::DependencyContext;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_bin_built;
use std::path::PathBuf;

// Deliberately the only test in this file — it overrides the
// process-global `CODEIMPACT_PARSE_PROBE`/`PROBE_CALL_LOG` env vars, which
// would race against any other test in the same binary (cargo runs a
// file's tests across multiple threads of one process by default).

// ── Test List (#63 T2, single-entry verdict cache) ────────────────────
//   1. Same source, `parse` then `resolve_dependencies` back-to-back:
//      the probe is spawned once, not twice (cache hit).
//   2. A different source afterwards: the probe is spawned again (cache
//      correctly distinguishes by content, not just "has probed before").

#[test]
fn probe_verdict_is_cached_per_source_not_per_call() {
    let count_probe = ensure_bin_built(
        "codeimpact_secondaries_integration_test",
        "codeimpact-count-probe",
    );
    let call_log = std::env::temp_dir().join(format!(
        "codeimpact_probe_call_log_{}.txt",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&call_log);
    std::env::set_var("CODEIMPACT_PARSE_PROBE", &count_probe);
    std::env::set_var("PROBE_CALL_LOG", &call_log);

    let parser = SynCodeParser::new();
    let source = "fn f() {}";
    let ctx = DependencyContext::new(PathBuf::from("f.rs"), PathBuf::from("."), vec![]);

    parser.parse(source).expect("parse should succeed");
    parser
        .resolve_dependencies(source, &ctx)
        .expect("resolve_dependencies should succeed");

    let calls_after_same_source = std::fs::read_to_string(&call_log).unwrap_or_default();
    assert_eq!(
        calls_after_same_source.lines().count(),
        1,
        "the same source must probe only once (cache hit), log: {:?}",
        calls_after_same_source
    );

    parser.parse("fn g() {}").expect("parse should succeed");

    let calls_after_new_source = std::fs::read_to_string(&call_log).unwrap_or_default();
    assert_eq!(
        calls_after_new_source.lines().count(),
        2,
        "a different source must probe again (cache miss), log: {:?}",
        calls_after_new_source
    );

    std::env::remove_var("CODEIMPACT_PARSE_PROBE");
    std::env::remove_var("PROBE_CALL_LOG");
    let _ = std::fs::remove_file(&call_log);
}
