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
/// not a bare `syn::parse_file`. This is a regression gate for that wiring
/// end-to-end, through the REAL subprocess: a legitimately deep (but
/// non-pathological) nested-mod source must still parse correctly all the
/// way through the real canary and the parent's re-parse-and-extract,
/// with the qualified function name reflecting every mod level walked by
/// `collect_functions` — proving that walk actually ran inside the
/// canary's bounded thread and not just `syn::parse_file`.
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
