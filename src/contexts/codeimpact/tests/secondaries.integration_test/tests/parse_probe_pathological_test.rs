use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries_integration_test::support::ensure_probe_built;

fn parser() -> SynCodeParser {
    ensure_probe_built();
    SynCodeParser::new()
}

/// `mod m0 { mod m1 { ... fn f() {} ... } }` nested `depth` levels deep —
/// the #63 ticket's own repro (~1800 nested `mod` overflows `syn`'s
/// recursive-descent stack in DEBUG). Depths bumped with margin (retry 2,
/// Security informational): `cargo test --release` genuinely admits a
/// deeper recursion before overflowing (release optimizations shrink
/// stack frames) — 1800/2000/10000 parse cleanly under `--release`,
/// empirically confirmed to still overflow the probe's 16 MiB thread at
/// 30000/30000/60000 in BOTH profiles (verified directly against the real
/// `codeimpact-parse-probe` binary, debug and release, before landing
/// this bump).
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

/// `fn f(x: Vec<Vec<...i32...>>) {}` nested `depth` levels deep — cycle-1
/// bypass vector #1 (nested generic types survive a byte-level brace scan).
fn nested_vec_type_source(depth: usize) -> String {
    let mut ty = String::from("i32");
    for _ in 0..depth {
        ty = format!("Vec<{}>", ty);
    }
    format!("fn f(x: {}) {{}}\n", ty)
}

/// `let _ = !!!!...x;` with `depth` leading `!` — cycle-1 bypass vector #2
/// (chained unary negation survives the same byte-level scan).
fn nested_not_chain_source(depth: usize) -> String {
    let mut expr = String::from("x");
    for _ in 0..depth {
        expr = format!("!{}", expr);
    }
    format!("fn f() {{ let x = true; let _ = {}; }}\n", expr)
}

// ── Test List (#63, AC1/AC2) ───────────────────────────────────────────
// One behavior — a pathological source is refused as SourceTooComplex,
// never crashes the calling process — three fixtures, one parameterized
// cycle. Safe to call `SynCodeParser::parse` directly here (unlike before
// #63): any crash now happens in the isolated canary subprocess, not in
// this test process.

#[test]
fn pathological_sources_are_unmeasurable_source_too_complex() {
    let cases: [(&str, String); 3] = [
        ("nested_mods", nested_mods_source(30000)),
        ("nested_vec", nested_vec_type_source(30000)),
        ("nested_not", nested_not_chain_source(60000)),
    ];

    for (name, source) in &cases {
        let result = parser().parse(source);
        match result {
            Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
            other => panic!(
                "case {}: expected Unmeasurable(SourceTooComplex), got {:?}",
                name, other
            ),
        }
    }
}

#[test]
fn pathological_source_dependencies_are_also_unmeasurable_source_too_complex() {
    let source = nested_mods_source(30000);

    let result = parser().parse_file_dependencies(&source);

    match result {
        Err(AnalysisError::Unmeasurable(UnmeasurableReason::SourceTooComplex)) => {}
        other => panic!("expected Unmeasurable(SourceTooComplex), got {:?}", other),
    }
}
