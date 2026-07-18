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
}

#[test]
fn display_name_is_human_readable_and_language_specific() {
    assert_eq!(Language::Rust.display_name(), "Rust");
    assert_eq!(Language::CSharp.display_name(), "C#");
}
