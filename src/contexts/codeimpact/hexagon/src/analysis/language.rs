/// The set of source languages CodeImpact can analyze (US16). Pure data —
/// no parsing logic, no adapter concern lives here (ADR-0018: the hexagon
/// names the concept, the adapter names the syntax).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    CSharp,
}

impl Language {
    /// Maps a file extension (without the leading dot) to the `Language`
    /// that owns it — `None` for an extension no registered adapter claims.
    pub fn from_extension(extension: &str) -> Option<Language> {
        match extension {
            "rs" => Some(Language::Rust),
            "cs" => Some(Language::CSharp),
            _ => None,
        }
    }

    /// The file extensions this language is recognized by (without the
    /// leading dot) — the inverse of `from_extension`.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::CSharp => &["cs"],
        }
    }

    /// The human-readable name used in reports and error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::CSharp => "C#",
        }
    }
}
