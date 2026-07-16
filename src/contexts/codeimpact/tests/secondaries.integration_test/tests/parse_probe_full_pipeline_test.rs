use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_probe_built;

fn parser() -> SynCodeParser {
    ensure_probe_built();
    SynCodeParser::new()
}

/// `mod m0 { mod m1 { ... fn f() {} ... } }` nested `depth` levels deep.
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

/// Security finding retry 1 (CWE-674) — the canary now runs the SAME
/// parse-and-walk pipeline `parse()` performs (`exercise_full_pipeline`),
/// not a bare `syn::parse_file` (see that function's doc comment for the
/// full rationale AND its honest limits).
///
/// Honest status (retry 2, Dev-B): this test does NOT prove the walk ran
/// inside the canary's bounded thread — Dev-B reverted the probe to a
/// bare `syn::parse_file`, rebuilt clean, and this test (and the rest of
/// the suite) stayed green, because the PARENT re-runs the walk
/// regardless of what the probe computed, and no depth differential
/// between bare-parse and the full pipeline was ever found (see
/// `exercise_full_pipeline`'s doc comment). What this test actually pins:
/// a legitimately deep (but non-pathological) nested-mod source still
/// parses correctly end-to-end — real canary, real parent re-parse — with
/// the qualified function name reflecting every mod level walked by
/// `collect_functions`. That is end-to-end correctness at depth, not
/// proof of the probe's internal wiring; the wiring itself is justified
/// architecturally, not by this (or any) reddening test.
#[test]
fn moderately_deep_nested_mods_still_extract_the_qualified_function_end_to_end() {
    let depth = 300;
    let source = nested_mods_source(depth);

    let functions = parser()
        .parse(&source)
        .expect("a legitimately deep source must still parse");

    assert_eq!(functions.len(), 1, "expected exactly one function");
    let expected_prefix = "m0::m1::m2::";
    assert!(
        functions[0].name.starts_with(expected_prefix),
        "expected the qualified mod path in the function name, got: {}",
        functions[0].name
    );
    assert!(
        functions[0].name.ends_with("::f"),
        "expected the function name to end with the qualified path, got: {}",
        functions[0].name
    );
}
