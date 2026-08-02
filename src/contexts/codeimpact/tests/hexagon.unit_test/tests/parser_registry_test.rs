use std::path::Path;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::ParserRegistry;
use codeimpact_secondaries::gateways::code_parsers::code_parser_stub::CodeParserStub;

// Test List (US16 T2, step C):
//   1. parser_for returns the parser registered for a language, and None
//      when nothing was registered for it — one behavior (lookup), two
//      divergent rows, one cycle.
//   2. dispatch resolves a path's extension to a Language (Language::
//      from_extension) then delegates to parser_for — routes a known
//      extension to its parser, refuses an unknown one. A DIFFERENT
//      behavior from #1 (adds the extension->Language step), own cycle.
//   3. extensions() is the union of every registered language's own
//      extensions — its own behavior.
//
// Test List (US17 T1 — TypeScript/JavaScript registered like any other
// language, no registry change):
//   4. dispatch resolves .ts/.js/.jsx to their registered parser, and
//      refuses .tsx (A2 ruling: out of v1 scope) — one behavior (routing),
//      divergent rows, one cycle.
//   5. extensions() union includes the new TypeScript/JavaScript
//      extensions alongside the existing ones.

fn rust_parser() -> CodeParserStub {
    CodeParserStub::new(Err(AnalysisError::AnalysisFailed("rust-marker".into())))
}

fn csharp_parser() -> CodeParserStub {
    CodeParserStub::new(Err(AnalysisError::AnalysisFailed("csharp-marker".into())))
}

/// Identifies WHICH stub a `&dyn CodeParser` reference is, without relying
/// on any language-introspection method on the port (that lands in step E)
/// — behavioral fingerprinting via each stub's distinct `parse()` marker.
fn marker_of(parser: &dyn CodeParser) -> String {
    match parser.parse("") {
        Err(AnalysisError::AnalysisFailed(msg)) => msg,
        other => panic!("expected an AnalysisFailed marker, got {:?}", other),
    }
}

fn two_language_registry() -> ParserRegistry {
    ParserRegistry::new()
        .register(Language::Rust, Box::new(rust_parser()))
        .register(Language::CSharp, Box::new(csharp_parser()))
}

#[test]
fn parser_for_returns_the_registered_parser_or_none() {
    let registry = two_language_registry();

    assert_eq!(
        marker_of(registry.parser_for(Language::Rust).unwrap()),
        "rust-marker"
    );

    let rust_only = ParserRegistry::new().register(Language::Rust, Box::new(rust_parser()));
    assert!(rust_only.parser_for(Language::CSharp).is_none());
}

#[test]
fn dispatch_routes_by_extension_and_refuses_unknown_ones() {
    let registry = two_language_registry();

    assert_eq!(
        marker_of(registry.dispatch(Path::new("a.rs")).unwrap()),
        "rust-marker"
    );
    assert_eq!(
        marker_of(registry.dispatch(Path::new("a.cs")).unwrap()),
        "csharp-marker"
    );
    assert!(registry.dispatch(Path::new("a.md")).is_none());
}

#[test]
fn extensions_is_the_union_of_every_registered_language() {
    let registry = two_language_registry();

    let mut extensions = registry.extensions();
    extensions.sort();
    assert_eq!(extensions, vec!["cs", "rs"]);
}

fn typescript_parser() -> CodeParserStub {
    CodeParserStub::new(Err(AnalysisError::AnalysisFailed(
        "typescript-marker".into(),
    )))
}

fn javascript_parser() -> CodeParserStub {
    CodeParserStub::new(Err(AnalysisError::AnalysisFailed(
        "javascript-marker".into(),
    )))
}

fn four_language_registry() -> ParserRegistry {
    two_language_registry()
        .register(Language::TypeScript, Box::new(typescript_parser()))
        .register(Language::JavaScript, Box::new(javascript_parser()))
}
// @scenario: typescript-javascript-analysis/S2

#[test]
fn dispatch_routes_typescript_and_javascript_extensions_and_refuses_tsx() {
    let registry = four_language_registry();

    assert_eq!(
        marker_of(registry.dispatch(Path::new("a.ts")).unwrap()),
        "typescript-marker"
    );
    assert_eq!(
        marker_of(registry.dispatch(Path::new("a.js")).unwrap()),
        "javascript-marker"
    );
    assert_eq!(
        marker_of(registry.dispatch(Path::new("a.jsx")).unwrap()),
        "javascript-marker"
    );
    // A2 (human-approved ruling): `.tsx` is deliberately out of v1 scope.
    assert!(registry.dispatch(Path::new("a.tsx")).is_none());
}
// @scenario: typescript-javascript-analysis/S2

#[test]
fn extensions_union_includes_typescript_and_javascript_extensions() {
    let registry = four_language_registry();

    let mut extensions = registry.extensions();
    extensions.sort();
    assert_eq!(
        extensions,
        vec!["cjs", "cs", "cts", "js", "jsx", "mjs", "mts", "rs", "ts"]
    );
}
