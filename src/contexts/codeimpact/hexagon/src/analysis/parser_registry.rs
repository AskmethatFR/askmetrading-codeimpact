use std::collections::HashMap;
use std::path::Path;

use super::code_parser::CodeParser;
use super::language::Language;

/// Dispatches an analysis target to the `CodeParser` adapter that owns its
/// language (US16 T2) — the domain-service seam ADR-0018 opened: adding a
/// language is registering one more adapter here, never editing the
/// hexagon. `Send + Sync` follows automatically from `CodeParser: Send +
/// Sync` (the port's own bound).
pub struct ParserRegistry {
    parsers: HashMap<Language, Box<dyn CodeParser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    pub fn register(mut self, language: Language, parser: Box<dyn CodeParser>) -> Self {
        self.parsers.insert(language, parser);
        self
    }

    pub fn parser_for(&self, language: Language) -> Option<&dyn CodeParser> {
        self.parsers.get(&language).map(|parser| parser.as_ref())
    }

    /// Resolves `path`'s extension to a `Language` (`Language::
    /// from_extension`) then delegates to `parser_for` — `None` when the
    /// extension is unmapped OR no parser is registered for its language.
    pub fn dispatch(&self, path: &Path) -> Option<&dyn CodeParser> {
        let extension = path.extension()?.to_str()?;
        let language = Language::from_extension(extension)?;
        self.parser_for(language)
    }

    /// The union of every registered language's own extension set.
    pub fn extensions(&self) -> Vec<&'static str> {
        self.parsers
            .keys()
            .flat_map(|language| language.extensions().iter().copied())
            .collect()
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}
