use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::DependencyContext;
use codeimpact_hexagon::analysis::ParsedFunction;
use std::path::PathBuf;

pub struct CodeParserStub {
    result: Result<Vec<ParsedFunction>, AnalysisError>,
    resolved_dependencies: Option<Result<Vec<PathBuf>, AnalysisError>>,
    failing_when_source_contains: Option<(String, AnalysisError)>,
}

impl CodeParserStub {
    pub fn new(result: Result<Vec<ParsedFunction>, AnalysisError>) -> Self {
        Self {
            result,
            resolved_dependencies: None,
            failing_when_source_contains: None,
        }
    }

    pub fn with_functions(functions: Vec<ParsedFunction>) -> Self {
        Self {
            result: Ok(functions),
            resolved_dependencies: None,
            failing_when_source_contains: None,
        }
    }

    pub fn with_resolved_dependencies(mut self, deps: Result<Vec<PathBuf>, AnalysisError>) -> Self {
        self.resolved_dependencies = Some(deps);
        self
    }

    /// Makes `parse` fail with `err` for any source containing `marker`,
    /// otherwise returning the stub's normal result — lets a single stub
    /// distinguish a "good" file from a "bad" one by content, the way a
    /// real parser would (used to test per-file parse-failure handling in a
    /// project with a mix of valid and invalid files).
    pub fn failing_when_source_contains(mut self, marker: &str, err: AnalysisError) -> Self {
        self.failing_when_source_contains = Some((marker.to_string(), err));
        self
    }
}

impl CodeParser for CodeParserStub {
    /// Every existing call site doubles a Rust-context `CodeParser` (this
    /// stub predates US16's multi-language port delta) — no test exercises
    /// `language()`/`capabilities()` through a use case in T2 (human-
    /// approved Q1: deferred to T3), so a fixed Rust default keeps this
    /// double at its established shape instead of growing a builder no
    /// test needs yet (cc-yagni).
    fn language(&self) -> codeimpact_hexagon::analysis::Language {
        codeimpact_hexagon::analysis::Language::Rust
    }

    fn capabilities(&self) -> codeimpact_hexagon::analysis::LanguageCapabilities {
        codeimpact_hexagon::analysis::LanguageCapabilities::all_supported(self.language())
    }

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        if let Some((marker, err)) = &self.failing_when_source_contains {
            if source.contains(marker.as_str()) {
                return Err(err.clone());
            }
        }
        self.result.clone()
    }

    fn resolve_dependencies(
        &self,
        _source: &str,
        _ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        match &self.resolved_dependencies {
            Some(result) => result.clone(),
            None => Ok(vec![]),
        }
    }
}
