use codeimpact_hexagon::analysis::Language;

// Test List (US16 T2, step B):
//   1. from_extension maps every registered extension to its Language, and
//      an unregistered one to None — ONE behavior (extension→Language
//      lookup), three divergent rows, one parameterized cycle.
//   2. extensions() returns the extension set for each language (inverse
//      of #1) — a DIFFERENT behavior (what a language claims, not what an
//      extension resolves to), its own cycle.
//   3. display_name() is a human-readable, language-specific string.

#[test]
fn from_extension_maps_known_extensions_and_refuses_unknown_ones() {
    let cases = [
        ("rs", Some(Language::Rust)),
        ("cs", Some(Language::CSharp)),
        ("ts", Some(Language::TypeScript)),
        ("mts", Some(Language::TypeScript)),
        ("cts", Some(Language::TypeScript)),
        ("js", Some(Language::JavaScript)),
        ("mjs", Some(Language::JavaScript)),
        ("cjs", Some(Language::JavaScript)),
        ("jsx", Some(Language::JavaScript)),
        // A2 (human-approved ruling): `.tsx` is deliberately OUT of v1 —
        // it would require widening `CodeParser::parse` to receive the
        // path, deferred to issue #118.
        ("tsx", None),
        ("md", None),
    ];
    for (extension, expected) in cases {
        assert_eq!(
            Language::from_extension(extension),
            expected,
            "extension '{}'",
            extension
        );
    }
}

#[test]
fn extensions_returns_this_language_own_extension_set() {
    assert_eq!(Language::Rust.extensions(), &["rs"]);
    assert_eq!(Language::CSharp.extensions(), &["cs"]);
    assert_eq!(Language::TypeScript.extensions(), &["ts", "mts", "cts"]);
    assert_eq!(
        Language::JavaScript.extensions(),
        &["js", "mjs", "cjs", "jsx"]
    );
}

#[test]
fn display_name_is_human_readable_and_language_specific() {
    assert_eq!(Language::Rust.display_name(), "Rust");
    assert_eq!(Language::CSharp.display_name(), "C#");
    assert_eq!(Language::TypeScript.display_name(), "TypeScript");
    assert_eq!(Language::JavaScript.display_name(), "JavaScript");
}
